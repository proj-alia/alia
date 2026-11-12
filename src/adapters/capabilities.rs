use super::{scheduling::require_lease, store::*};
use crate::{
    domain::{
        Lifecycle,
        cognition::{CapabilityInfo, ProposalError},
        retrieval::{RetrievalPage, RetrievalRequest},
    },
    runtime::{capability::*, scheduling::RunLease},
};
use chrono::Utc;
use serde_json::{Value, json};
use uuid::Uuid;

type ActionRow = (
    String,
    String,
    Value,
    String,
    Option<Value>,
    Option<i64>,
    i32,
);
struct PreparedSearch {
    arguments_digest: String,
    result: Result<RetrievalPage, &'static str>,
    started_at: chrono::DateTime<Utc>,
}

impl Store {
    pub async fn local_capabilities(&self) -> Result<Vec<CapabilityInfo>, StoreError> {
        let current = self.snapshot().await?.ok_or(StoreError::NotInitialized)?;
        Ok(LOCAL_CAPABILITIES
            .iter()
            .filter_map(|id| LocalCapability::resolve(id))
            .map(|cap| {
                let mut info = cap.info();
                info.authorized = current.runtime_config["capabilities"]["enabled"]
                    .as_array()
                    .is_some_and(|v| v.iter().any(|id| id.as_str() == Some(&info.id)));
                info
            })
            .collect())
    }

    /// Import is an owner operation, not an arbitrary filesystem capability.
    /// Body storage is separate from the small metadata event used in context.
    pub async fn import_document(
        &self,
        key: &str,
        title: &str,
        text: &str,
        source_label: &str,
    ) -> Result<ImportedDocument, StoreError> {
        if title.trim().is_empty()
            || title.len() > 512
            || text.trim().is_empty()
            || text.len() > 1048576
            || source_label.trim().is_empty()
            || source_label.len() > 512
            || [title, text, source_label].iter().any(|s| s.contains('\0'))
        {
            return Err(StoreError::Invalid("document input"));
        }
        let hash = digest(&json!({"title":title,"text":text,"source_label":source_label}))?;
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if super::goals::unavailable(&mut tx, current.id, &[current.id]).await? {
            return Err(StoreError::NotFound);
        }
        if let Some(receipt) =
            replay::<ImportedDocument>(&mut tx, "owner", "/documents", key, &hash).await?
        {
            let live:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM documents d JOIN objects o ON (o.individual_id,o.id)=(d.individual_id,d.event_id) WHERE d.individual_id=$1 AND d.event_id=$2 AND NOT o.blocked)").bind(current.id).bind(receipt.event_id).fetch_one(&mut *tx).await?;
            return if live {
                Ok(receipt)
            } else {
                Err(StoreError::StaleState)
            };
        }
        if current.lifecycle != Lifecycle::Active {
            return Err(StoreError::Inactive);
        }
        let id = Uuid::new_v4();
        let version = current.state_version + 1;
        insert_event(&mut tx,current.id,id,"document.imported","external_document","owner",json!({"title":title,"source_label":source_label,"utf8_bytes":text.len(),"characters":text.chars().count()}),id,Some(key)).await?;
        sqlx::query("INSERT INTO documents(individual_id,event_id,title,source_label,body) VALUES($1,$2,$3,$4,$5)").bind(current.id).bind(id).bind(title).bind(source_label).bind(text).execute(&mut *tx).await?;
        advance(&mut tx, current.id, version).await?;
        super::store::outbox(&mut tx, current.id, "state.changed", current.id, version).await?;
        let receipt = ImportedDocument {
            event_id: id,
            state_version: version,
        };
        save_receipt(&mut tx, "owner", "/documents", key, &hash, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    /// Encode before taking the write gate. The selected records, arguments,
    /// permissions and lease are checked again in the atomic result transaction.
    pub async fn execute_local_action(
        &self,
        lease: &RunLease,
        action: Uuid,
    ) -> Result<ActionReceipt, StoreError> {
        let prepared = self.prepare_memory_search(lease, action).await?;
        self.execute_prepared_local_action(lease, action, prepared.as_ref())
            .await
    }
    async fn prepare_memory_search(
        &self,
        lease: &RunLease,
        action: Uuid,
    ) -> Result<Option<PreparedSearch>, StoreError> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let row: Option<(String,String,Value,String,bool)> = sqlx::query_as("SELECT capability_id,capability_version,arguments,status,receipt IS NOT NULL FROM actions WHERE individual_id=$1 AND id=$2 AND run_id=$3")
            .bind(lease.individual_id).bind(action).bind(lease.run_id).fetch_optional(&mut *tx).await?;
        let Some((name, version, args, status, replay)) = row else {
            return Ok(None);
        };
        if name != "memory.search" || replay || status != "planned" {
            return Ok(None);
        }
        let Ok(LocalInput::SearchMemory(input)) = LocalCapability::SearchMemory.parse(&args) else {
            return Ok(None);
        };
        require_lease(&mut tx, lease).await?;
        let current = read_state(&mut tx)
            .await?
            .ok_or(StoreError::NotInitialized)?;
        if version != LocalCapability::SearchMemory.info().version
            || !current.runtime_config["capabilities"]["enabled"]
                .as_array()
                .is_some_and(|v| v.iter().any(|v| v == "memory.search"))
        {
            return Err(ProposalError::Capability.into());
        }
        let (trigger, manifest, calls): (Uuid, Vec<Uuid>, i32) = sqlx::query_as(
            "SELECT trigger_event_id,input_manifest,tool_calls FROM runs WHERE id=$1",
        )
        .bind(lease.run_id)
        .fetch_one(&mut *tx)
        .await?;
        if calls as i64
            >= current.runtime_config["autonomy"]["max_tool_calls_per_cycle"]
                .as_i64()
                .ok_or(StoreError::Invalid("tool limit"))?
        {
            return Err(StoreError::CallLimit);
        }
        let preceding: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM actions a JOIN actions current ON current.id=$2 WHERE a.run_id=$1 AND a.sequence<current.sequence AND a.status IN ('planned','executing','unknown'))").bind(lease.run_id).bind(action).fetch_one(&mut *tx).await?;
        if preceding {
            return Err(StoreError::Invalid("preceding action unfinished"));
        }
        let mut checked = manifest.clone();
        checked.extend([current.id, action, lease.run_id]);
        if super::goals::unavailable(&mut tx, current.id, &checked).await? {
            return Err(StoreError::StaleState);
        }
        let started_at = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        let arguments_digest = digest(&args)?;
        if input.person_id.is_some_and(|id| !manifest.contains(&id)) {
            return Ok(Some(PreparedSearch {
                arguments_digest,
                result: Err("person_not_in_context"),
                started_at,
            }));
        }
        let result = self
            .retrieve_memories(&RetrievalRequest {
                text: input.query,
                scope: Some(format!("event:{trigger}")),
                subject_id: None,
                person_id: input.person_id,
                person_name: input.person_name,
                at: Utc::now(),
                include_history: false,
                limit: input.limit,
            })
            .await;
        let result = match result {
            Ok(page) => Ok(page),
            Err(StoreError::RetrievalUnavailable) => Err("retrieval_unavailable"),
            Err(StoreError::NotFound) => Err("person_unavailable"),
            Err(StoreError::PersonAmbiguous) => Err("person_ambiguous"),
            Err(StoreError::Invalid(_)) => Err("invalid_input"),
            Err(e) => return Err(e),
        };
        Ok(Some(PreparedSearch {
            arguments_digest,
            result,
            started_at,
        }))
    }
    async fn execute_prepared_local_action(
        &self,
        lease: &RunLease,
        action: Uuid,
        prepared: Option<&PreparedSearch>,
    ) -> Result<ActionReceipt, StoreError> {
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let row:Option<ActionRow>=sqlx::query_as("SELECT capability_id,capability_version,arguments,status,receipt,lease_generation,sequence FROM actions WHERE individual_id=$1 AND id=$2 AND run_id=$3")
            .bind(current.id).bind(action).bind(lease.run_id).fetch_optional(&mut *tx).await?;
        let (name, version, args, status, old, generation, sequence) =
            row.ok_or(StoreError::Invalid("action absent"))?;
        if let Some(old) = old {
            let receipt: ActionReceipt = serde_json::from_value(old)?;
            if lease.individual_id != current.id
                || generation != Some(lease.generation)
                || lease.privacy_epoch != current.privacy_epoch
            {
                return Err(StoreError::StaleState);
            }
            let live:bool=sqlx::query_scalar("SELECT count(*)=2 FROM objects o WHERE o.individual_id=$1 AND o.id=ANY($2) AND NOT o.blocked AND NOT EXISTS(SELECT 1 FROM derivation_edges d JOIN objects s ON (s.individual_id,s.id)=(d.individual_id,d.source_id) WHERE d.individual_id=o.individual_id AND d.target_id=o.id AND s.blocked)")
                .bind(current.id).bind(vec![action,receipt.result_event_id]).fetch_one(&mut *tx).await?;
            return if live
                && !super::goals::unavailable(
                    &mut tx,
                    current.id,
                    &[current.id, action, receipt.result_event_id, lease.run_id],
                )
                .await?
            {
                Ok(receipt)
            } else {
                Err(StoreError::StaleState)
            };
        }
        require_lease(&mut tx, lease).await?;
        if status != "planned" {
            return Err(StoreError::Invalid("action is not planned"));
        }
        let live: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM objects WHERE individual_id=$1 AND id=$2 AND NOT blocked)",
        )
        .bind(current.id)
        .bind(action)
        .fetch_one(&mut *tx)
        .await?;
        if !live {
            return Err(StoreError::StaleState);
        }
        let preceding:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM actions WHERE run_id=$1 AND sequence<$2 AND status IN ('planned','executing','unknown'))").bind(lease.run_id).bind(sequence).fetch_one(&mut *tx).await?;
        if preceding {
            return Err(StoreError::Invalid("preceding action unfinished"));
        }
        let cap = LocalCapability::resolve(&name).ok_or(ProposalError::Capability)?;
        if version != cap.info().version
            || !current.runtime_config["capabilities"]["enabled"]
                .as_array()
                .is_some_and(|a| a.iter().any(|v| v.as_str() == Some(&name)))
        {
            return Err(ProposalError::Capability.into());
        }
        let parsed = cap.parse(&args);
        let executed = parsed.is_ok();
        if executed {
            let limit = current.runtime_config["autonomy"]["max_tool_calls_per_cycle"]
                .as_i64()
                .ok_or(StoreError::Invalid("tool limit"))?;
            let updated = sqlx::query(
                "UPDATE runs SET tool_calls=tool_calls+1 WHERE id=$1 AND tool_calls<$2",
            )
            .bind(lease.run_id)
            .bind(limit)
            .execute(&mut *tx)
            .await?;
            if updated.rows_affected() != 1 {
                return Err(StoreError::CallLimit);
            }
        }
        let mut sources: Vec<Uuid> =
            sqlx::query_scalar("SELECT input_manifest FROM runs WHERE id=$1")
                .bind(lease.run_id)
                .fetch_one(&mut *tx)
                .await?;
        sources.push(action);
        if executed {
            sqlx::query(
                "UPDATE actions SET started_at=COALESCE($3,clock_timestamp()) WHERE individual_id=$1 AND id=$2",
            )
            .bind(current.id)
            .bind(action)
            .bind(prepared.map(|p|p.started_at))
            .execute(&mut *tx)
            .await?;
        }
        let (output, source_kind, success) = match parsed {
            Err(_) => (
                json!({"error":"invalid_input"}),
                "direct_observation",
                false,
            ),
            Ok(LocalInput::WriteNote(p)) => {
                let mut citation_ids = Vec::new();
                let mut failure = None;
                for citation in &p.citations {
                    match super::completion::check_citation(
                        &mut tx,
                        current.id,
                        lease.run_id,
                        action,
                        citation,
                        &p.text,
                    )
                    .await?
                    {
                        Ok(ids) => citation_ids.extend(ids),
                        Err(reason) => {
                            failure = Some(reason);
                            break;
                        }
                    }
                }
                if let Some(reason) = failure {
                    let (code, instruction) = reason.feedback();
                    (
                        json!({"error":code,"instruction":instruction}),
                        "direct_observation",
                        false,
                    )
                } else {
                    sqlx::query(
                    "INSERT INTO notes(individual_id,action_id,title,body,citations) VALUES($1,$2,$3,$4,$5)",
                )
                .bind(current.id)
                .bind(action)
                .bind(&p.title)
                .bind(&p.text)
                .bind(json!(p.citations))
                .execute(&mut *tx)
                .await?;
                    sources.extend(citation_ids);
                    (
                        json!({"note_id":action,"title":p.title,"utf8_bytes":p.text.len(),"citations":p.citations}),
                        "direct_observation",
                        true,
                    )
                }
            }
            Ok(LocalInput::ReadDocument(p)) => {
                let document:Option<(String,String,String)>=sqlx::query_as("SELECT d.title,d.source_label,d.body FROM documents d JOIN objects o ON (o.individual_id,o.id)=(d.individual_id,d.event_id) WHERE d.individual_id=$1 AND d.event_id=$2 AND NOT o.blocked")
                    .bind(current.id).bind(p.document_id).fetch_optional(&mut *tx).await?;
                match document {
                    None => (
                        json!({"error":"document_unavailable"}),
                        "direct_observation",
                        false,
                    ),
                    Some((title, label, body)) => {
                        sources.push(p.document_id);
                        let length = body.chars().count();
                        if p.offset > length {
                            (
                                json!({"error":"offset_out_of_range"}),
                                "direct_observation",
                                false,
                            )
                        } else {
                            let text: String =
                                body.chars().skip(p.offset).take(p.max_chars).collect();
                            let end = p.offset + text.chars().count();
                            (
                                json!({"document_id":p.document_id,"title":title,"source_label":label,"text":text,"offset":p.offset,"citation":crate::domain::completion::Citation::for_excerpt(p.document_id,p.offset,&text),"next_offset":if end<length {Some(end)}else{None},"characters":length}),
                                "external_document",
                                true,
                            )
                        }
                    }
                }
            }
            Ok(LocalInput::SearchMemory(_)) => {
                let prepared = prepared.ok_or(StoreError::StaleState)?;
                if prepared.arguments_digest != digest(&args)? {
                    return Err(StoreError::StaleState);
                }
                match &prepared.result {
                    Err(code) => (
                        json!({"error":code,"instruction":if *code=="person_ambiguous" {"已保存的同名/别名对应多个人物，需澄清身份；不要选择其中一人、合并经历或声称没有相关记录。"} else {"检索未完成；不要声称已检查记忆或没有相关经历。"}}),
                        "direct_observation",
                        false,
                    ),
                    Ok(page) => {
                        let results = Store::validate_retrieval_page(
                            &mut tx,
                            page,
                            current.id,
                            current.state_version,
                            current.privacy_epoch,
                        )
                        .await?;
                        sources.extend(&page.person_evidence);
                        for memory in &results {
                            sources.push(memory.id);
                            sources.extend(&memory.source_event_ids);
                            sources.extend(memory.person_evidence_ids());
                        }
                        // A query result repeats stored interpretations; it is not new
                        // evidence that their contents happened to the individual.
                        (
                            json!({"items":results,"retrieval":page.summary().mode,"retrieval_details":page.summary()}),
                            "generated",
                            true,
                        )
                    }
                }
            }
        };
        let result = Uuid::new_v4();
        let new_version = current.state_version + 1;
        let status = if success { "succeeded" } else { "failed" };
        insert_event(&mut tx,current.id,result,"tool.result",source_kind,"capability",json!({"action_id":action,"capability_id":name,"capability_version":version,"status":status,"executed":executed,"output":output}),lease.run_id,None).await?;
        sources.sort();
        sources.dedup();
        for source in &sources {
            edge(&mut tx, current.id, *source, result).await?;
            if *source != action {
                edge(&mut tx, current.id, *source, action).await?;
            }
            if *source != lease.run_id {
                edge(&mut tx, current.id, *source, lease.run_id).await?;
            }
        }
        sources.push(result);
        require_lease(&mut tx, lease).await?;
        sqlx::query("UPDATE runs SET state_version=$2,input_manifest=$3 WHERE id=$1")
            .bind(lease.run_id)
            .bind(new_version)
            .bind(&sources)
            .execute(&mut *tx)
            .await?;
        let receipt = ActionReceipt {
            action_id: action,
            result_event_id: result,
            status: status.into(),
            state_version: new_version,
            executed,
        };
        sqlx::query("UPDATE actions SET status=$3,result_event_id=$4,lease_generation=$5,receipt=$6,finished_at=clock_timestamp() WHERE individual_id=$1 AND id=$2").bind(current.id).bind(action).bind(status).bind(result).bind(lease.generation).bind(serde_json::to_value(&receipt)?).execute(&mut *tx).await?;
        advance(&mut tx, current.id, new_version).await?;
        outbox(
            &mut tx,
            current.id,
            "operation.updated",
            lease.run_id,
            new_version,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }
}
