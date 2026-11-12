//! Transactional persistence. Caller-provided model data requires domain validation first.
use super::{api::VISIBLE, goals::unavailable};
use crate::domain::{
    Lifecycle,
    config::{PersonaSeed, RuntimeRules},
    history::*,
};
use chrono::Utc;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use sha2::{Digest, Sha256};
use sqlx::{PgPool, Postgres, Transaction};
use std::collections::HashSet;
use uuid::Uuid;

pub static MIGRATOR: sqlx::migrate::Migrator = sqlx::migrate!("./migrations");

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("storage failure")]
    Database(#[from] sqlx::Error),
    #[error("stored data failed validation")]
    Decode(#[from] serde_json::Error),
    #[error("individual has not been initialized")]
    NotInitialized,
    #[error("resource unavailable")]
    NotFound,
    #[error("individual already exists")]
    AlreadyInitialized,
    #[error("idempotency key reused with a different request")]
    IdempotencyConflict,
    #[error("state changed since the operation began")]
    StaleState,
    #[error("individual is not active")]
    Inactive,
    #[error("execution lease is no longer valid")]
    LeaseLost,
    #[error("model budget is exhausted or frozen")]
    BudgetUnavailable,
    #[error("cycle call limit is exhausted or a call is already outstanding")]
    CallLimit,
    #[error("decision proposal failed domain validation")]
    Proposal(#[from] crate::domain::cognition::ProposalError),
    #[error("memory retrieval encoder unavailable")]
    RetrievalUnavailable,
    #[error("person name refers to multiple records")]
    PersonAmbiguous,
    #[error("privacy recovery is required before further access")]
    PrivacyUnavailable,
    #[error("invalid input: {0}")]
    Invalid(&'static str),
}

#[derive(Clone)]
pub struct Store {
    pub(super) pool: PgPool,
    pub(super) encoder: Option<std::sync::Arc<dyn crate::runtime::indexing::MemoryEncoder>>,
    privacy_ready: std::sync::Arc<std::sync::atomic::AtomicBool>,
    privacy_generation: std::sync::Arc<std::sync::atomic::AtomicU64>,
}

pub(super) fn digest(value: &impl Serialize) -> Result<String, StoreError> {
    // serde_json's default map representation sorts keys. Object key order is not identity.
    let canonical = serde_json::to_value(value)?;
    Ok(format!(
        "{:x}",
        Sha256::digest(serde_json::to_vec(&canonical)?)
    ))
}

fn decode<T: DeserializeOwned>(value: Value) -> Result<T, StoreError> {
    Ok(serde_json::from_value(value)?)
}

impl Store {
    pub fn new(pool: PgPool) -> Self {
        Self {
            pool,
            encoder: None,
            privacy_ready: std::sync::Arc::new(std::sync::atomic::AtomicBool::new(true)),
            privacy_generation: std::sync::Arc::new(std::sync::atomic::AtomicU64::new(0)),
        }
    }

    pub fn with_memory_encoder(
        mut self,
        encoder: std::sync::Arc<dyn crate::runtime::indexing::MemoryEncoder>,
    ) -> Self {
        self.encoder = Some(encoder);
        self
    }
    pub fn semantic_enabled(&self) -> bool {
        self.encoder.is_some()
    }
    pub async fn migrate(&self) -> Result<(), sqlx::migrate::MigrateError> {
        MIGRATOR.run(&self.pool).await
    }

    // Short, per-database write gate also covers the pre-identity initialization race.
    // No model/network waits. Database-only capabilities commit here atomically.
    pub(super) async fn write(&self) -> Result<Transaction<'_, Postgres>, StoreError> {
        let generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let tx = self.write_for_privacy_recovery().await?;
        self.require_privacy_snapshot(generation)?;
        Ok(tx)
    }

    /// Only the durable-journal recovery path may write while access is fenced.
    pub(super) async fn write_for_privacy_recovery(
        &self,
    ) -> Result<Transaction<'_, Postgres>, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(1095518529)")
            .execute(&mut *tx)
            .await?;
        Ok(tx)
    }

    pub fn privacy_ready(&self) -> bool {
        self.privacy_ready
            .load(std::sync::atomic::Ordering::Acquire)
    }
    pub fn privacy_generation(&self) -> u64 {
        self.privacy_generation
            .load(std::sync::atomic::Ordering::Acquire)
    }
    /// Fail closed while an independently durable intent has an uncertain DB
    /// outcome. Reopening is reserved for the verified recovery coordinator.
    pub fn fence_privacy(&self) {
        self.set_privacy_ready(false);
    }
    pub(crate) fn set_privacy_ready(&self, ready: bool) {
        self.privacy_ready
            .store(ready, std::sync::atomic::Ordering::Release);
        if !ready {
            self.privacy_generation
                .fetch_add(1, std::sync::atomic::Ordering::AcqRel);
        }
    }
    pub(super) fn require_privacy_ready(&self) -> Result<(), StoreError> {
        if self.privacy_ready() {
            Ok(())
        } else {
            Err(StoreError::PrivacyUnavailable)
        }
    }
    pub(super) fn require_privacy_snapshot(&self, generation: u64) -> Result<(), StoreError> {
        if self.privacy_ready() && self.privacy_generation() == generation {
            Ok(())
        } else {
            Err(StoreError::PrivacyUnavailable)
        }
    }

    pub async fn snapshot(&self) -> Result<Option<IndividualState>, StoreError> {
        let generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let result =
            sqlx::query_scalar::<_, Value>("SELECT to_jsonb(i) FROM individuals i WHERE singleton")
                .fetch_optional(&self.pool)
                .await?
                .map(decode)
                .transpose()?;
        self.require_privacy_snapshot(generation)?;
        Ok(result)
    }

    /// Seeds must have passed the configuration validator before this method is called.
    /// This method never updates an existing individual's seed or current persona.
    pub async fn initialize(
        &self,
        key: &str,
        seed: &PersonaSeed,
        config: &RuntimeRules,
    ) -> Result<Initialization, StoreError> {
        let seed = seed.as_value();
        let config = config.as_value();
        let hash = digest(&json!({"persona":seed,"runtime":config}))?;
        let mut tx = self.write().await?;
        if let Some(receipt) = replay(&mut tx, "owner", "/initialize", key, &hash).await? {
            return Ok(receipt);
        }
        if state(&mut tx).await?.is_some() {
            return Err(StoreError::AlreadyInitialized);
        }
        let id = Uuid::new_v4();
        let birth = Uuid::new_v4();
        let seed_hash = digest(seed)?;
        sqlx::query("INSERT INTO individuals(id,origin,seed,seed_digest,persona,runtime_config) VALUES($1,$2,$3,$4,$3,$5)")
            .bind(id).bind(json!({"kind":"created","schema_version":1})).bind(seed).bind(&seed_hash).bind(config)
            .execute(&mut *tx).await?;
        object(&mut tx, id, id, "identity").await?;
        insert_event(
            &mut tx,
            id,
            birth,
            "individual.created",
            "direct_observation",
            "system",
            json!({"seed_digest":seed_hash}),
            birth,
            Some(key),
        )
        .await?;
        edge(&mut tx, id, birth, id).await?;
        super::interests::seed_interests(&mut tx, id, seed, birth).await?;
        let owner = Uuid::new_v4();
        object(&mut tx, id, owner, "relationship").await?;
        sqlx::query(
            "INSERT INTO relationships(individual_id,subject_id,subject_key) VALUES($1,$2,'owner')",
        )
        .bind(id)
        .bind(owner)
        .execute(&mut *tx)
        .await?;
        let receipt = Initialization {
            individual_id: id,
            genesis_event_id: birth,
            version: 1,
        };
        outbox(&mut tx, id, "state.changed", id, 1).await?;
        save_receipt(&mut tx, "owner", "/initialize", key, &hash, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn submit_message(
        &self,
        key: &str,
        text: &str,
        conversation_id: Option<Uuid>,
    ) -> Result<SubmittedMessage, StoreError> {
        if text.trim().is_empty() || text.len() > 16 * 1024 || text.contains('\0') {
            return Err(StoreError::Invalid("message size"));
        }
        let hash = digest(&json!({"text":text,"conversation_id":conversation_id}))?;
        let mut tx = self.write().await?;
        if let Some(receipt) = replay(&mut tx, "owner", "/messages", key, &hash).await? {
            return Ok(receipt);
        }
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if super::goals::unavailable(&mut tx, current.id, &[current.id]).await? {
            return Err(StoreError::NotFound);
        }
        if current.lifecycle != Lifecycle::Active {
            return Err(StoreError::Inactive);
        }
        let version = current.state_version + 1;
        let event = Uuid::new_v4();
        let run = Uuid::new_v4();
        insert_event(
            &mut tx,
            current.id,
            event,
            "message.received",
            "reported_claim",
            "owner",
            json!({"text":text,"conversation_id":conversation_id}),
            run,
            Some(key),
        )
        .await?;
        object(&mut tx, current.id, run, "run").await?;
        sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,input_manifest) VALUES($1,$2,$3,'foreground',$4,$5,$6,$7)")
            .bind(current.id).bind(run).bind(event).bind(version).bind(current.privacy_epoch)
            .bind(current.config_version).bind(vec![event]).execute(&mut *tx).await?;
        edge(&mut tx, current.id, event, run).await?;
        let owner: Uuid = sqlx::query_scalar("INSERT INTO relationship_events(individual_id,subject_id,event_id) SELECT individual_id,subject_id,$2 FROM relationships WHERE individual_id=$1 AND subject_key='owner' RETURNING subject_id")
            .bind(current.id).bind(event).fetch_one(&mut *tx).await?;
        edge(&mut tx, current.id, event, owner).await?;
        sqlx::query("UPDATE relationships SET version=version+1 WHERE individual_id=$1 AND subject_key='owner'")
            .bind(current.id).execute(&mut *tx).await?;
        advance(&mut tx, current.id, version).await?;
        outbox(&mut tx, current.id, "message.created", event, version).await?;
        outbox(&mut tx, current.id, "operation.updated", run, version).await?;
        let receipt = SubmittedMessage {
            individual_id: current.id,
            event_id: event,
            operation_id: run,
            version,
        };
        save_receipt(&mut tx, "owner", "/messages", key, &hash, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Storage-level atomic batch; the runtime must additionally validate semantics,
    /// lease/config version and permissions before calling the eventual proposal commit.
    pub async fn record_memories(
        &self,
        key: &str,
        expected: ExpectedState,
        input_manifest: &[Uuid],
        changes: &[NewMemory],
    ) -> Result<RecordedMemories, StoreError> {
        if changes.is_empty() || changes.len() > 6 {
            return Err(StoreError::Invalid("memory batch size"));
        }
        let inputs: HashSet<_> = input_manifest.iter().copied().collect();
        if inputs.is_empty() || inputs.len() != input_manifest.len() {
            return Err(StoreError::Invalid("input manifest"));
        }
        let hash = digest(
            &json!({"expected":expected,"input_manifest":input_manifest,"changes":changes}),
        )?;
        let mut tx = self.write().await?;
        if let Some(receipt) =
            replay::<RecordedMemories>(&mut tx, "system", "/internal/memories", key, &hash).await?
        {
            // Idempotency does not grant access to a revoked result. Recheck
            // provenance even though the original command was committed.
            let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
            let mut ids = vec![current.id, receipt.event_id];
            ids.extend_from_slice(&receipt.memory_ids);
            if unavailable(&mut tx, current.id, &ids).await? {
                return Err(StoreError::NotFound);
            }
            return Ok(receipt);
        }
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if current.state_version != expected.version
            || current.privacy_epoch != expected.privacy_epoch
        {
            return Err(StoreError::StaleState);
        }
        if current.lifecycle != Lifecycle::Active {
            return Err(StoreError::Inactive);
        }
        let mut source_ids = vec![current.id];
        source_ids.extend_from_slice(input_manifest);
        if unavailable(&mut tx, current.id, &source_ids).await? {
            return Err(StoreError::Invalid("unavailable input"));
        }
        let memory_ids = apply_memories(&mut tx, current.id, input_manifest, changes).await?;
        let version = current.state_version + 1;
        let event = Uuid::new_v4();
        insert_event(
            &mut tx,
            current.id,
            event,
            "memory.recorded",
            "generated",
            "system",
            json!({"memory_ids":memory_ids}),
            event,
            Some(key),
        )
        .await?;
        for source in input_manifest {
            edge(&mut tx, current.id, *source, event).await?;
        }
        for id in &memory_ids {
            outbox(&mut tx, current.id, "memory.changed", *id, version).await?;
        }
        advance(&mut tx, current.id, version).await?;
        let receipt = RecordedMemories {
            memory_ids,
            event_id: event,
            version,
        };
        save_receipt(
            &mut tx,
            "system",
            "/internal/memories",
            key,
            &hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn events(&self, after: i64, limit: u32) -> Result<Vec<Event>, StoreError> {
        let rows: Vec<Value> = sqlx::query_scalar(&format!("{VISIBLE} SELECT to_jsonb(e) FROM events e WHERE e.sequence>$1 AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=e.individual_id AND h.id IN (e.individual_id,e.id)) ORDER BY e.sequence LIMIT $2"))
            .bind(after).bind(i64::from(limit.clamp(1,100))).fetch_all(&self.pool).await?;
        rows.into_iter().map(decode).collect()
    }

    pub async fn memories(&self) -> Result<Vec<Memory>, StoreError> {
        let rows: Vec<Value> = sqlx::query_scalar(&format!("{VISIBLE} SELECT to_jsonb(m) || jsonb_build_object('source_event_ids',ARRAY(SELECT event_id FROM memory_sources s WHERE (s.individual_id,s.memory_id)=(m.individual_id,m.id) ORDER BY event_id)) FROM memories m WHERE NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=m.individual_id AND h.id IN (m.individual_id,m.id)) ORDER BY m.created_at,m.id LIMIT 100"))
            .fetch_all(&self.pool).await?;
        rows.into_iter().map(decode).collect()
    }
}

pub(super) async fn read_state(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Option<IndividualState>, StoreError> {
    sqlx::query_scalar::<_, Value>("SELECT to_jsonb(i) FROM individuals i WHERE singleton")
        .fetch_optional(&mut **tx)
        .await?
        .map(decode)
        .transpose()
}

pub(super) async fn state(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<Option<IndividualState>, StoreError> {
    sqlx::query_scalar::<_, Value>(
        "SELECT to_jsonb(i) FROM individuals i WHERE singleton FOR UPDATE",
    )
    .fetch_optional(&mut **tx)
    .await?
    .map(decode)
    .transpose()
}

pub(super) async fn replay<T: DeserializeOwned>(
    tx: &mut Transaction<'_, Postgres>,
    actor: &str,
    route: &str,
    key: &str,
    hash: &str,
) -> Result<Option<T>, StoreError> {
    if key.is_empty() || key.len() > 128 || !key.bytes().all(|c| c.is_ascii_graphic()) {
        return Err(StoreError::Invalid("idempotency key"));
    }
    let row: Option<(String,Value)> = sqlx::query_as("SELECT body_digest,response FROM idempotency_records WHERE actor_id=$1 AND route=$2 AND key=$3")
        .bind(actor).bind(route).bind(key).fetch_optional(&mut **tx).await?;
    match row {
        Some((stored, _)) if stored != hash => Err(StoreError::IdempotencyConflict),
        Some((_, response)) => Ok(Some(decode(response)?)),
        None => Ok(None),
    }
}

pub(super) async fn save_receipt(
    tx: &mut Transaction<'_, Postgres>,
    actor: &str,
    route: &str,
    key: &str,
    hash: &str,
    response: &impl Serialize,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO idempotency_records(actor_id,route,key,body_digest,response) VALUES($1,$2,$3,$4,$5)")
        .bind(actor).bind(route).bind(key).bind(hash).bind(serde_json::to_value(response)?).execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn object(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    id: Uuid,
    kind: &str,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO objects(individual_id,id,kind) VALUES($1,$2,$3)")
        .bind(individual)
        .bind(id)
        .bind(kind)
        .execute(&mut **tx)
        .await?;
    Ok(())
}

#[allow(clippy::too_many_arguments)]
pub(super) async fn insert_event(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    id: Uuid,
    kind: &str,
    source: &str,
    actor: &str,
    payload: Value,
    correlation: Uuid,
    key: Option<&str>,
) -> Result<(), StoreError> {
    object(tx, individual, id, "event").await?;
    sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id,idempotency_key) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
        .bind(individual).bind(id).bind(kind).bind(source).bind(actor).bind(Utc::now())
        .bind(payload).bind(correlation).bind(key).execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn edge(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    source: Uuid,
    target: Uuid,
) -> Result<(), StoreError> {
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
        .bind(individual).bind(source).bind(target).execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn outbox(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    kind: &str,
    id: Uuid,
    version: i64,
) -> Result<(), StoreError> {
    sqlx::query(
        "INSERT INTO outbox(individual_id,event_type,resource_id,version) VALUES($1,$2,$3,$4)",
    )
    .bind(individual)
    .bind(kind)
    .bind(id)
    .bind(version)
    .execute(&mut **tx)
    .await?;
    Ok(())
}

pub(super) async fn advance(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    version: i64,
) -> Result<(), StoreError> {
    sqlx::query("UPDATE individuals SET state_version=$2 WHERE id=$1")
        .bind(individual)
        .bind(version)
        .execute(&mut **tx)
        .await?;
    outbox(tx, individual, "state.changed", individual, version).await
}

pub(super) async fn apply_memories(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    input_manifest: &[Uuid],
    changes: &[NewMemory],
) -> Result<Vec<Uuid>, StoreError> {
    let inputs: HashSet<_> = input_manifest.iter().copied().collect();
    let mut memory_ids = Vec::new();
    for memory in changes {
        if memory.content.trim().is_empty()
            || memory.content.chars().count() > 2000
            || memory.scope.trim().is_empty()
            || memory.scope.chars().count() > 200
        {
            return Err(StoreError::Invalid("memory text or scope"));
        }
        let sources: HashSet<_> = memory.source_event_ids.iter().copied().collect();
        if sources.is_empty()
            || sources.len() > 32
            || sources.len() != memory.source_event_ids.len()
            || !sources.is_subset(&inputs)
        {
            return Err(StoreError::Invalid("memory sources outside context"));
        }
        let found: i64 =
            sqlx::query_scalar("SELECT count(*) FROM events WHERE individual_id=$1 AND id=ANY($2)")
                .bind(individual)
                .bind(&memory.source_event_ids)
                .fetch_one(&mut **tx)
                .await?;
        if found != sources.len() as i64 {
            return Err(StoreError::Invalid("source is not an event"));
        }
        if let Some(previous) = memory.supersedes {
            if !inputs.contains(&previous) {
                return Err(StoreError::Invalid("superseded memory outside context"));
            }
            let scope: Option<String> = sqlx::query_scalar("SELECT scope FROM memories WHERE individual_id=$1 AND id=$2 AND validity IN ('active','disputed','stale')")
                    .bind(individual).bind(previous).fetch_optional(&mut **tx).await?;
            if scope.as_deref() != Some(&memory.scope) {
                return Err(StoreError::Invalid("incompatible supersession scope"));
            }
            sqlx::query("UPDATE memories SET validity='superseded',version=version+1,valid_until=now() WHERE individual_id=$1 AND id=$2")
                    .bind(individual).bind(previous).execute(&mut **tx).await?;
        }
        let id = Uuid::new_v4();
        object(tx, individual, id, "memory").await?;
        sqlx::query("INSERT INTO memories(individual_id,id,kind,content,scope,supersedes) VALUES($1,$2,$3,$4,$5,$6)")
                .bind(individual).bind(id).bind(serde_json::to_value(&memory.kind)?.as_str().unwrap())
                .bind(&memory.content).bind(&memory.scope).bind(memory.supersedes).execute(&mut **tx).await?;
        for source in &memory.source_event_ids {
            sqlx::query(
                "INSERT INTO memory_sources(individual_id,memory_id,event_id) VALUES($1,$2,$3)",
            )
            .bind(individual)
            .bind(id)
            .bind(source)
            .execute(&mut **tx)
            .await?;
        }
        for source in input_manifest {
            edge(tx, individual, *source, id).await?;
        }
        memory_ids.push(id);
    }
    Ok(memory_ids)
}
