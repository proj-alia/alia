//! Durable deletion acceptance, opaque blocked tombstones and verified recovery.
//! Managed export/backup artifacts are not enabled by this online-only controller.
use super::privacy_development::{
    block_development_in, development_privacy_clean_in, erase_development_in,
    verify_development_projection_in,
};
use super::{
    deletion::{expand_deletion_objects, memory_deletion_closure_in},
    deletion_journal::{DeletionJournal, JournalAppend, JournalIntent},
    store::{Store, StoreError, advance, digest, insert_event, outbox, state},
};
use crate::domain::privacy::{DeletionProgress, DeletionReceipt, DeletionStatus};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{Postgres, Transaction};
use std::{collections::HashSet, sync::Arc};
use uuid::Uuid;

// Exact public column inventories independently applied to PostgreSQL: schema29
// and schema30's trial request/result storage with its adopted purge consumer.
// New persisted content still requires explicit cleanup rules; unfamiliar
// schemas cannot produce a completed deletion receipt.
const SCHEMA_DIGEST: &str = "65922f299c07dc39e37b085b1e0a20d057bef26c2ac8d2511d400602a41cfa44";
const SCHEMA30_DIGEST: &str = "b1d0736eb64657672a9eab87ca641611984993591e2e2bbbdc30102262aaefab";
const KINDS: &[&str] = &[
    "event",
    "memory",
    "goal",
    "run",
    "relationship",
    "action",
    "entity",
    "belief",
    "autobiography",
    "affective_state",
    "affective_attitude",
    "relationship_revision",
    "interest",
    "interest_revision",
    "development_target",
    "development",
    "development_revision",
    "development_verifier",
];
#[derive(sqlx::FromRow)]
struct Job {
    individual_id: Uuid,
    id: Uuid,
    target_id: Uuid,
    expected_memory_version: i64,
    idempotency_key_digest: String,
    request_digest: String,
    journal_sequence: i64,
    journal_digest: String,
    root_event_ids: Vec<Uuid>,
    object_ids: Vec<Uuid>,
    status: String,
    state_version: i64,
    privacy_epoch: i64,
}
fn hash_key(key: &str) -> Result<String, StoreError> {
    if key.is_empty() || key.len() > 128 || !key.bytes().all(|c| c.is_ascii_graphic()) {
        return Err(StoreError::Invalid("idempotency key"));
    }
    Ok(format!("{:x}", Sha256::digest(key.as_bytes())))
}
fn request_digest(key_digest: &str, target: Uuid, version: i64) -> Result<String, StoreError> {
    digest(
        &json!({"idempotency_key_digest":key_digest,"target_memory_id":target,"expected_memory_version":version}),
    )
}
fn status(value: &str) -> Result<DeletionStatus, StoreError> {
    match value {
        "blocked" => Ok(DeletionStatus::Blocked),
        "purging" => Ok(DeletionStatus::Purging),
        "retry_pending" => Ok(DeletionStatus::RetryPending),
        "completed" => Ok(DeletionStatus::Completed),
        _ => Err(StoreError::PrivacyUnavailable),
    }
}
fn receipt(job: &Job) -> Result<DeletionReceipt, StoreError> {
    Ok(DeletionReceipt {
        individual_id: job.individual_id,
        operation_id: job.id,
        target_id: job.target_id,
        state_version: job.state_version,
        privacy_epoch: job.privacy_epoch,
        status: status(&job.status)?,
    })
}
async fn schema_supported(tx: &mut Transaction<'_, Postgres>) -> Result<(), StoreError> {
    let inventory:Value=sqlx::query_scalar("SELECT jsonb_agg(jsonb_build_array(table_name,column_name,data_type,udt_name) ORDER BY table_name,ordinal_position) FROM information_schema.columns WHERE table_schema=current_schema() AND table_name<>'_sqlx_migrations'").fetch_one(&mut **tx).await?;
    let actual = digest(&inventory)?;
    if ![SCHEMA_DIGEST, SCHEMA30_DIGEST].contains(&actual.as_str()) {
        return Err(StoreError::Invalid("privacy schema unsupported"));
    }
    Ok(())
}
async fn kinds_supported(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    ids: &[Uuid],
) -> Result<(), StoreError> {
    let invalid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM objects WHERE id=ANY($2) AND (individual_id<>$1 OR NOT(kind=ANY($3))))").bind(individual).bind(ids).bind(KINDS).fetch_one(&mut **tx).await?;
    if invalid {
        return Err(StoreError::Invalid("privacy object kind unsupported"));
    }
    Ok(())
}
async fn disk_records(
    journal: Arc<DeletionJournal>,
    nonblocking: bool,
) -> Result<Vec<JournalIntent>, StoreError> {
    tokio::task::spawn_blocking(move || {
        if nonblocking {
            journal.try_records()
        } else {
            journal.records()
        }
    })
    .await
    .map_err(|_| StoreError::PrivacyUnavailable)?
    .map_err(|_| StoreError::PrivacyUnavailable)
}
async fn durable_append(
    journal: Arc<DeletionJournal>,
    intent: JournalIntent,
    nonblocking: bool,
) -> Result<JournalAppend, StoreError> {
    tokio::task::spawn_blocking(move || {
        if nonblocking {
            journal.try_append(&intent)
        } else {
            journal.append(&intent)
        }
    })
    .await
    .map_err(|_| StoreError::PrivacyUnavailable)?
    .map_err(|_| StoreError::PrivacyUnavailable)
}
async fn invalidate(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    ids: &[Uuid],
) -> Result<(), StoreError> {
    let runs:Vec<Uuid>=sqlx::query_scalar("UPDATE runs SET status='stale',lease_generation=lease_generation+1,lease_owner=NULL,lease_expires_at=NULL,error_code='privacy_erased',finished_at=COALESCE(finished_at,clock_timestamp()),retry_after=NULL WHERE individual_id=$1 AND id=ANY($2) AND status IN ('queued','running') RETURNING id").bind(individual).bind(ids).fetch_all(&mut **tx).await?;
    for run in runs {
        super::scheduling::close_pending(tx, run).await?;
    }
    sqlx::query("UPDATE actions SET status=CASE WHEN status='planned' THEN 'cancelled' ELSE 'unknown' END,finished_at=COALESCE(finished_at,clock_timestamp()) WHERE individual_id=$1 AND (id=ANY($2) OR run_id=ANY($2)) AND status IN ('planned','executing')").bind(individual).bind(ids).execute(&mut **tx).await?;
    sqlx::query("UPDATE outgoing_messages SET status='cancelled',cancellation_reason='privacy_erased' WHERE individual_id=$1 AND (draft_event_id=ANY($2) OR run_id=ANY($2)) AND status='pending'").bind(individual).bind(ids).execute(&mut **tx).await?;
    Ok(())
}
async fn block(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    ids: &[Uuid],
) -> Result<(i64, i64), StoreError> {
    kinds_supported(tx, individual, ids).await?;
    sqlx::query("UPDATE objects SET blocked=true WHERE individual_id=$1 AND id=ANY($2)")
        .bind(individual)
        .bind(ids)
        .execute(&mut **tx)
        .await?;
    block_development_in(tx, individual, ids).await?;
    let (version,epoch):(i64,i64)=sqlx::query_as("UPDATE individuals SET state_version=state_version+1,privacy_epoch=privacy_epoch+1,lease_generation=lease_generation+1 WHERE id=$1 RETURNING state_version,privacy_epoch").bind(individual).fetch_one(&mut **tx).await?;
    invalidate(tx, individual, ids).await?;
    outbox(tx, individual, "privacy.changed", individual, version).await?;
    Ok((version, epoch))
}
struct Rule {
    table: &'static str,
    condition: &'static str,
    set: Option<&'static str>,
    clean: Option<&'static str>,
}
const DIRECT: &str = "t.individual_id=$1 AND t.id=ANY($2)";
const RULES: &[Rule] = &[
    Rule {
        table: "events",
        condition: DIRECT,
        set: Some("kind='privacy.redacted',actor_id='redacted',payload='{}',idempotency_key=NULL"),
        clean: Some(
            "kind='privacy.redacted' AND actor_id='redacted' AND payload='{}' AND idempotency_key IS NULL",
        ),
    },
    Rule {
        table: "memories",
        condition: DIRECT,
        set: Some(
            "content='[redacted]',scope='[redacted]',validity='stale',embedding_version=NULL",
        ),
        clean: Some(
            "content='[redacted]' AND scope='[redacted]' AND validity='stale' AND embedding_version IS NULL",
        ),
    },
    Rule {
        table: "goals",
        condition: DIRECT,
        set: Some(
            "description='[redacted]',success_criterion='[redacted]',next_step='[redacted]',stop_condition='[redacted]',origin_ids='[]',completion=NULL,waiting_reason='privacy_erased',priority=50,due_at=NULL,resume_at=NULL,no_progress_cycles=0,waiting_since_sequence=NULL",
        ),
        clean: Some(
            "description='[redacted]' AND success_criterion='[redacted]' AND next_step='[redacted]' AND stop_condition='[redacted]' AND origin_ids='[]' AND completion IS NULL AND waiting_reason='privacy_erased' AND priority=50 AND due_at IS NULL AND resume_at IS NULL AND no_progress_cycles=0 AND waiting_since_sequence IS NULL",
        ),
    },
    Rule {
        table: "relationships",
        condition: "t.individual_id=$1 AND t.subject_id=ANY($2)",
        set: Some(
            "subject_key='redacted:'||subject_id::text,stage='redacted',expectations='[]',boundaries='[]',commitments='[]',unresolved='[]'",
        ),
        clean: Some(
            "subject_key='redacted:'||subject_id::text AND stage='redacted' AND expectations='[]' AND boundaries='[]' AND commitments='[]' AND unresolved='[]'",
        ),
    },
    Rule {
        table: "person_entities",
        condition: DIRECT,
        set: Some("external_key='redacted:'||id::text,label='[redacted]'"),
        clean: Some("external_key='redacted:'||id::text AND label='[redacted]'"),
    },
    Rule {
        table: "beliefs",
        condition: DIRECT,
        set: Some(
            "statement='[redacted]',scope='[redacted]',brief_basis='[redacted]',position='tentative',status='superseded',superseded_at=COALESCE(superseded_at,clock_timestamp())",
        ),
        clean: Some(
            "statement='[redacted]' AND scope='[redacted]' AND brief_basis='[redacted]' AND position='tentative' AND status='superseded' AND superseded_at IS NOT NULL",
        ),
    },
    Rule {
        table: "autobiographies",
        condition: DIRECT,
        set: Some(
            "topic='redacted.'||replace(id::text,'-',''),title='[redacted]',interpretation='[redacted]',status='superseded',superseded_at=COALESCE(superseded_at,clock_timestamp())",
        ),
        clean: Some(
            "topic='redacted.'||replace(id::text,'-','') AND title='[redacted]' AND interpretation='[redacted]' AND status='superseded' AND superseded_at IS NOT NULL",
        ),
    },
    Rule {
        table: "affective_appraisals",
        condition: DIRECT,
        set: Some(
            "brief_basis='[redacted]',valence=0,arousal=0,goal_relevance=0,expectation_gap=0,controllability=0,uncertainty=0,status='superseded'",
        ),
        clean: Some(
            "brief_basis='[redacted]' AND valence=0 AND arousal=0 AND goal_relevance=0 AND expectation_gap=0 AND controllability=0 AND uncertainty=0 AND status='superseded'",
        ),
    },
    Rule {
        table: "affective_attitudes",
        condition: DIRECT,
        set: Some(
            "topic='redacted.'||replace(id::text,'-',''),stance='[redacted]',brief_basis='[redacted]',status='superseded',superseded_at=COALESCE(superseded_at,clock_timestamp())",
        ),
        clean: Some(
            "topic='redacted.'||replace(id::text,'-','') AND stance='[redacted]' AND brief_basis='[redacted]' AND status='superseded' AND superseded_at IS NOT NULL",
        ),
    },
    Rule {
        table: "relationship_revisions",
        condition: DIRECT,
        set: Some("before_state='{}',after_state='{}',brief_basis='[redacted]'"),
        clean: Some("before_state='{}' AND after_state='{}' AND brief_basis='[redacted]'"),
    },
    Rule {
        table: "interests",
        condition: "t.individual_id=$1 AND t.object_id=ANY($2)",
        set: Some("description='[redacted]',status='retired'"),
        clean: Some("description='[redacted]' AND status='retired'"),
    },
    Rule {
        table: "interest_revisions",
        condition: DIRECT,
        set: Some(
            "before_description=NULL,before_status=NULL,after_description='[redacted]',after_status='retired',brief_basis='[redacted]'",
        ),
        clean: Some(
            "before_description IS NULL AND before_status IS NULL AND after_description='[redacted]' AND after_status='retired' AND brief_basis='[redacted]'",
        ),
    },
    Rule {
        table: "actions",
        condition: DIRECT,
        set: Some(
            "arguments='{}',receipt=NULL,result_digest=NULL,capability_id='privacy.redacted',capability_version='1'",
        ),
        clean: Some(
            "arguments='{}' AND receipt IS NULL AND result_digest IS NULL AND capability_id='privacy.redacted' AND capability_version='1'",
        ),
    },
    Rule {
        table: "runs",
        condition: DIRECT,
        set: Some("error_code='privacy_erased',retry_after=NULL"),
        clean: Some("error_code='privacy_erased' AND retry_after IS NULL"),
    },
    Rule {
        table: "decision_commits",
        condition: "t.individual_id=$1 AND (t.run_id=ANY($2) OR t.decision_event_id=ANY($2))",
        set: Some("receipt='{}'"),
        clean: Some("receipt='{}'"),
    },
    Rule {
        table: "decision_reviews",
        condition: "t.individual_id=$1 AND (t.run_id=ANY($2) OR t.event_id=ANY($2))",
        set: Some("assessment='{}',checker_version='privacy.redacted'"),
        clean: Some("assessment='{}' AND checker_version='privacy.redacted'"),
    },
    Rule {
        table: "goal_reviews",
        condition: "t.individual_id=$1 AND (t.goal_id=ANY($2) OR t.event_id=ANY($2))",
        set: Some("assessment='{}'"),
        clean: Some("assessment='{}'"),
    },
    Rule {
        table: "outgoing_messages",
        condition: "t.individual_id=$1 AND (t.run_id=ANY($2) OR t.draft_event_id=ANY($2) OR t.delivered_event_id=ANY($2))",
        set: Some("cancellation_reason='privacy_erased'"),
        clean: Some("cancellation_reason='privacy_erased'"),
    },
    Rule {
        table: "documents",
        condition: "t.individual_id=$1 AND t.event_id=ANY($2)",
        set: None,
        clean: None,
    },
    Rule {
        table: "notes",
        condition: "t.individual_id=$1 AND t.action_id=ANY($2)",
        set: None,
        clean: None,
    },
    Rule {
        table: "memory_vectors",
        condition: "t.individual_id=$1 AND t.memory_id=ANY($2)",
        set: None,
        clean: None,
    },
    Rule {
        table: "memory_vector_sets",
        condition: "t.individual_id=$1 AND t.memory_id=ANY($2)",
        set: None,
        clean: None,
    },
    Rule {
        table: "memory_index_jobs",
        condition: "t.individual_id=$1 AND t.memory_id=ANY($2)",
        set: None,
        clean: None,
    },
    Rule {
        table: "person_aliases",
        condition: "t.individual_id=$1 AND (t.entity_id=ANY($2) OR t.source_event_id=ANY($2) OR t.assertion_event_id=ANY($2))",
        set: None,
        clean: None,
    },
    Rule {
        table: "memory_person_links",
        condition: "t.individual_id=$1 AND (t.memory_id=ANY($2) OR t.entity_id=ANY($2) OR t.source_event_id=ANY($2) OR t.assertion_event_id=ANY($2))",
        set: None,
        clean: None,
    },
    Rule {
        table: "idempotency_records",
        condition: "EXISTS(SELECT 1 FROM jsonb_path_query(t.response,'$.**') x(value) WHERE x.value=ANY(SELECT to_jsonb(v::text) FROM unnest($2::uuid[]) v)) AND $1::uuid IS NOT NULL",
        set: None,
        clean: None,
    },
];
async fn clean(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    ids: &[Uuid],
) -> Result<bool, StoreError> {
    schema_supported(tx).await?;
    kinds_supported(tx, individual, ids).await?;
    if !development_privacy_clean_in(tx, individual, ids).await? {
        return Ok(false);
    }
    for rule in RULES {
        let condition = match rule.clean {
            Some(expected) => format!("{} AND NOT (({expected}) IS TRUE)", rule.condition),
            None => rule.condition.into(),
        };
        let dirty: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM {} t WHERE {condition})",
            rule.table
        ))
        .bind(individual)
        .bind(ids)
        .fetch_one(&mut **tx)
        .await?;
        if dirty {
            return Ok(false);
        }
    }
    Ok(true)
}
async fn erase(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    ids: &[Uuid],
) -> Result<(), StoreError> {
    schema_supported(tx).await?;
    kinds_supported(tx, individual, ids).await?;
    for rule in RULES {
        let sql = match (rule.set, rule.clean) {
            (Some(set), Some(expected)) => format!(
                "UPDATE {} t SET {set} WHERE {} AND NOT (({expected}) IS TRUE)",
                rule.table, rule.condition
            ),
            _ => format!("DELETE FROM {} t WHERE {}", rule.table, rule.condition),
        };
        sqlx::query(&sql)
            .bind(individual)
            .bind(ids)
            .execute(&mut **tx)
            .await?;
    }
    erase_development_in(tx, individual, ids).await?;
    if !clean(tx, individual, ids).await? {
        return Err(StoreError::PrivacyUnavailable);
    }
    Ok(())
}
impl Store {
    pub async fn begin_memory_deletion(
        &self,
        key: &str,
        target: Uuid,
        expected_memory_version: i64,
        journal: Arc<DeletionJournal>,
    ) -> Result<DeletionReceipt, StoreError> {
        let key_digest = hash_key(key)?;
        if target.is_nil() || expected_memory_version <= 0 {
            return Err(StoreError::Invalid("invalid memory deletion target"));
        }
        let request = request_digest(&key_digest, target, expected_memory_version)?;
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let previous: Option<Job> = sqlx::query_as(
            "SELECT * FROM privacy_operations WHERE individual_id=$1 AND idempotency_key_digest=$2",
        )
        .bind(current.id)
        .bind(&key_digest)
        .fetch_optional(&mut *tx)
        .await?;
        if let Some(job) = previous {
            if job.target_id != target
                || job.expected_memory_version != expected_memory_version
                || job.request_digest != request
            {
                return Err(StoreError::IdempotencyConflict);
            }
            let result = receipt(&job)?;
            tx.commit().await?;
            return Ok(result);
        }
        schema_supported(&mut tx).await?;
        let closure = memory_deletion_closure_in(&mut tx, target, expected_memory_version).await?;
        let records = match disk_records(journal.clone(), false).await {
            Ok(records) => records,
            Err(e) => {
                self.fence_privacy();
                return Err(e);
            }
        };
        if records
            .iter()
            .any(|r| r.idempotency_key_digest.as_deref() == Some(&key_digest))
        {
            self.fence_privacy();
            return Err(StoreError::PrivacyUnavailable);
        }
        let intent = JournalIntent {
            operation_id: Uuid::new_v4(),
            individual_id: current.id,
            target_memory_id: target,
            expected_memory_version,
            request_digest: request,
            root_event_ids: closure.root_event_ids,
            idempotency_key_digest: Some(key_digest),
        };
        self.fence_privacy();
        let append = durable_append(journal, intent.clone(), false).await?;
        let (version, epoch) = block(&mut tx, current.id, &closure.object_ids).await?;
        sqlx::query("INSERT INTO privacy_operations(individual_id,id,target_id,expected_memory_version,idempotency_key_digest,request_digest,journal_sequence,journal_digest,root_event_ids,object_ids,state_version,privacy_epoch) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
   .bind(current.id).bind(intent.operation_id).bind(target).bind(expected_memory_version).bind(&intent.idempotency_key_digest).bind(&intent.request_digest).bind(append.sequence as i64).bind(&append.record_digest).bind(&intent.root_event_ids).bind(&closure.object_ids).bind(version).bind(epoch).execute(&mut *tx).await?;
        insert_event(
            &mut tx,
            current.id,
            Uuid::new_v4(),
            "privacy.blocked",
            "direct_observation",
            "owner",
            json!({"operation_id":intent.operation_id,"target_id":target,"status":"blocked"}),
            intent.operation_id,
            None,
        )
        .await?;
        advance(&mut tx, current.id, version).await?;
        tx.commit().await?;
        self.set_privacy_ready(true);
        Ok(DeletionReceipt {
            individual_id: current.id,
            operation_id: intent.operation_id,
            target_id: target,
            state_version: version,
            privacy_epoch: epoch,
            status: DeletionStatus::Blocked,
        })
    }
    pub async fn privacy_operation(&self, id: Uuid) -> Result<DeletionProgress, StoreError> {
        let generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let view:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('individual_id',p.individual_id,'operation_id',p.id,'target_id',p.target_id,'state_version',p.state_version,'privacy_epoch',p.privacy_epoch,'status',p.status,'observed_at',statement_timestamp(),'last_error',p.last_error) FROM privacy_operations p JOIN individuals i ON i.id=p.individual_id WHERE i.singleton AND p.id=$1").bind(id).fetch_optional(&self.pool).await?;
        self.require_privacy_snapshot(generation)?;
        Ok(serde_json::from_value(view.ok_or(StoreError::NotFound)?)?)
    }
    pub async fn purge_privacy_operations(&self, limit: u32) -> Result<u32, StoreError> {
        if !(1..=16).contains(&limit) {
            return Err(StoreError::Invalid("invalid privacy purge limit"));
        }
        let mut tx = self.write_for_privacy_recovery().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let pending:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM privacy_operations WHERE individual_id=$1 AND status<>'completed' ORDER BY journal_sequence LIMIT $2").bind(current.id).bind(i64::from(limit)).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        let mut completed = 0;
        for id in pending {
            if self.purge_one(id).await? {
                completed += 1;
            }
        }
        Ok(completed)
    }
    async fn purge_one(&self, id: Uuid) -> Result<bool, StoreError> {
        let mut tx = self.write_for_privacy_recovery().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let job: Job = sqlx::query_as(
            "SELECT * FROM privacy_operations WHERE individual_id=$1 AND id=$2 FOR UPDATE",
        )
        .bind(current.id)
        .bind(id)
        .fetch_optional(&mut *tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        if job.status == "completed" {
            return Ok(false);
        }
        sqlx::query("UPDATE privacy_operations SET status='purging',last_error=NULL,updated_at=clock_timestamp() WHERE individual_id=$1 AND id=$2").bind(current.id).bind(id).execute(&mut *tx).await?;
        if tx.commit().await.is_err() {
            self.fence_privacy();
            return Err(StoreError::PrivacyUnavailable);
        }
        let result = self.erase_job(id).await;
        match result {
            Ok(completed) => Ok(completed),
            Err(error) => {
                let mut tx = self.write_for_privacy_recovery().await?;
                let static_error = if matches!(error, StoreError::Invalid(_)) {
                    "schema_or_kind_unsupported"
                } else {
                    "purge_failed"
                };
                sqlx::query("UPDATE privacy_operations SET status='retry_pending',last_error=$3,updated_at=clock_timestamp() WHERE individual_id=$1 AND id=$2 AND status<>'completed'").bind(current.id).bind(id).bind(static_error).execute(&mut *tx).await?;
                if tx.commit().await.is_err() {
                    self.fence_privacy();
                    return Err(StoreError::PrivacyUnavailable);
                }
                Ok(false)
            }
        }
    }
    async fn erase_job(&self, id: Uuid) -> Result<bool, StoreError> {
        let mut tx = self.write_for_privacy_recovery().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let job: Job = sqlx::query_as(
            "SELECT * FROM privacy_operations WHERE individual_id=$1 AND id=$2 FOR UPDATE",
        )
        .bind(current.id)
        .bind(id)
        .fetch_one(&mut *tx)
        .await?;
        if job.status == "completed" {
            tx.commit().await?;
            return Ok(false);
        }
        let mut seeds = job.object_ids.clone();
        seeds.extend(&job.root_event_ids);
        seeds.push(job.target_id);
        seeds.sort();
        seeds.dedup();
        let ids = expand_deletion_objects(&mut tx, current.id, &seeds, false).await?;
        let unblocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM objects WHERE individual_id=$1 AND id=ANY($2) AND NOT blocked)").bind(current.id).bind(&ids).fetch_one(&mut *tx).await?;
        let mut notification_version = current.state_version;
        if ids != job.object_ids || unblocked {
            let (version, epoch) = block(&mut tx, current.id, &ids).await?;
            notification_version = version;
            sqlx::query("UPDATE privacy_operations SET object_ids=$3,state_version=$4,privacy_epoch=$5 WHERE individual_id=$1 AND id=$2").bind(current.id).bind(id).bind(&ids).bind(version).bind(epoch).execute(&mut *tx).await?;
        }
        erase(&mut tx, current.id, &ids).await?;
        sqlx::query("UPDATE privacy_operations SET status='completed',last_error=NULL,updated_at=clock_timestamp() WHERE individual_id=$1 AND id=$2").bind(current.id).bind(id).execute(&mut *tx).await?;
        outbox(
            &mut tx,
            current.id,
            "privacy.changed",
            current.id,
            notification_version,
        )
        .await?;
        if tx.commit().await.is_err() {
            self.fence_privacy();
            return Err(StoreError::PrivacyUnavailable);
        }
        Ok(true)
    }
    pub async fn replay_deletion_journal(
        &self,
        journal: Arc<DeletionJournal>,
    ) -> Result<(), StoreError> {
        self.replay_deletion_journal_mode(journal, false).await
    }
    pub async fn try_replay_deletion_journal(
        &self,
        journal: Arc<DeletionJournal>,
    ) -> Result<(), StoreError> {
        self.replay_deletion_journal_mode(journal, true).await
    }
    async fn replay_deletion_journal_mode(
        &self,
        journal: Arc<DeletionJournal>,
        nonblocking: bool,
    ) -> Result<(), StoreError> {
        self.fence_privacy();
        let mut tx = self.write_for_privacy_recovery().await?;
        self.fence_privacy();
        let records = disk_records(journal.clone(), nonblocking).await?;
        let current = state(&mut tx).await?;
        let Some(current) = current else {
            if records.is_empty() {
                tx.commit().await?;
                self.set_privacy_ready(true);
                return Ok(());
            }
            return Err(StoreError::PrivacyUnavailable);
        };
        schema_supported(&mut tx).await?;
        let mut keys = HashSet::new();
        for intent in &records {
            let key = intent
                .idempotency_key_digest
                .as_deref()
                .ok_or(StoreError::PrivacyUnavailable)?;
            if intent.individual_id != current.id
                || !keys.insert(key)
                || request_digest(key, intent.target_memory_id, intent.expected_memory_version)?
                    != intent.request_digest
            {
                return Err(StoreError::PrivacyUnavailable);
            }
            let append = durable_append(journal.clone(), intent.clone(), nonblocking).await?;
            let previous: Option<Job> = sqlx::query_as(
                "SELECT * FROM privacy_operations WHERE individual_id=$1 AND id=$2 FOR UPDATE",
            )
            .bind(current.id)
            .bind(intent.operation_id)
            .fetch_optional(&mut *tx)
            .await?;
            if let Some(job) = &previous
                && (job.target_id != intent.target_memory_id
                    || job.expected_memory_version != intent.expected_memory_version
                    || job.idempotency_key_digest != key
                    || job.request_digest != intent.request_digest
                    || job.journal_sequence != append.sequence as i64
                    || job.journal_digest != append.record_digest
                    || job.root_event_ids != intent.root_event_ids)
            {
                return Err(StoreError::PrivacyUnavailable);
            }
            let mut seeds = intent.root_event_ids.clone();
            seeds.push(intent.target_memory_id);
            if let Some(job) = &previous {
                seeds.extend(&job.object_ids);
            }
            seeds.sort();
            seeds.dedup();
            let mut created = false;
            for (id, kind) in std::iter::once((intent.target_memory_id, "memory"))
                .chain(intent.root_event_ids.iter().map(|id| (*id, "event")))
            {
                let old: Option<String> =
                    sqlx::query_scalar("SELECT kind FROM objects WHERE individual_id=$1 AND id=$2")
                        .bind(current.id)
                        .bind(id)
                        .fetch_optional(&mut *tx)
                        .await?;
                match old {
                    Some(old) if old != kind => return Err(StoreError::PrivacyUnavailable),
                    Some(_) => {}
                    None => {
                        sqlx::query("INSERT INTO objects(individual_id,id,kind,blocked) VALUES($1,$2,$3,true)").bind(current.id).bind(id).bind(kind).execute(&mut *tx).await?;
                        created = true;
                    }
                }
            }
            let ids = expand_deletion_objects(&mut tx, current.id, &seeds, false).await?;
            let unblocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM objects WHERE individual_id=$1 AND id=ANY($2) AND NOT blocked)").bind(current.id).bind(&ids).fetch_one(&mut *tx).await?;
            let needs_block = match &previous {
                None => true,
                Some(job) => {
                    created
                        || unblocked
                        || job.object_ids != ids
                        || job.status == "completed" && !clean(&mut tx, current.id, &ids).await?
                }
            };
            if needs_block {
                let (version, epoch) = block(&mut tx, current.id, &ids).await?;
                if previous.is_none() {
                    sqlx::query("INSERT INTO privacy_operations(individual_id,id,target_id,expected_memory_version,idempotency_key_digest,request_digest,journal_sequence,journal_digest,root_event_ids,object_ids,state_version,privacy_epoch) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)").bind(current.id).bind(intent.operation_id).bind(intent.target_memory_id).bind(intent.expected_memory_version).bind(key).bind(&intent.request_digest).bind(append.sequence as i64).bind(&append.record_digest).bind(&intent.root_event_ids).bind(&ids).bind(version).bind(epoch).execute(&mut *tx).await?;
                    insert_event(&mut tx,current.id,Uuid::new_v4(),"privacy.blocked","direct_observation","recovery",json!({"operation_id":intent.operation_id,"target_id":intent.target_memory_id,"status":"blocked"}),intent.operation_id,None).await?;
                } else {
                    sqlx::query("UPDATE privacy_operations SET object_ids=$3,state_version=$4,privacy_epoch=$5,status='blocked',last_error=NULL,updated_at=clock_timestamp() WHERE individual_id=$1 AND id=$2").bind(current.id).bind(intent.operation_id).bind(&ids).bind(version).bind(epoch).execute(&mut *tx).await?;
                }
            }
        }
        let count: i64 =
            sqlx::query_scalar("SELECT count(*) FROM privacy_operations WHERE individual_id=$1")
                .bind(current.id)
                .fetch_one(&mut *tx)
                .await?;
        if count as usize != records.len() {
            return Err(StoreError::PrivacyUnavailable);
        }
        verify_development_projection_in(&mut tx, current.id).await?;
        tx.commit().await?;
        for _ in 0..records.len().div_ceil(16) + 1 {
            let remaining:i64=sqlx::query_scalar("SELECT count(*) FROM privacy_operations WHERE individual_id=$1 AND status<>'completed'").bind(current.id).fetch_one(&self.pool).await?;
            if remaining == 0 {
                self.set_privacy_ready(true);
                return Ok(());
            }
            if self.purge_privacy_operations(16).await? == 0 {
                return Err(StoreError::PrivacyUnavailable);
            }
        }
        Err(StoreError::PrivacyUnavailable)
    }
}
