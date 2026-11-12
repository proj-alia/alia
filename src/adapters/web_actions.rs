use super::{scheduling::require_lease, store::*};
use crate::{
    domain::{
        cognition::{CapabilityInfo, ProposalError},
        history::IndividualState,
    },
    runtime::{capability::ActionReceipt, scheduling::RunLease, web::*},
};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

pub(crate) struct WebDispatch {
    pub target: WebTarget,
    pub action: Uuid,
    pub lease: RunLease,
    pub mode: CapabilityMode,
}
pub(crate) enum WebStart {
    Receipt(ActionReceipt),
    Dispatch(Box<WebDispatch>),
}
type WebRow = (
    String,
    String,
    Value,
    String,
    Option<Value>,
    Option<String>,
    Option<i64>,
    i32,
    Option<String>,
);

async fn replay_result(
    tx: &mut Transaction<'_, Postgres>,
    current: &IndividualState,
    lease: &RunLease,
    action: Uuid,
    old: Value,
    generation: Option<i64>,
) -> Result<ActionReceipt, StoreError> {
    let receipt: ActionReceipt = serde_json::from_value(old)?;
    if current.id != lease.individual_id
        || current.privacy_epoch != lease.privacy_epoch
        || generation != Some(lease.generation)
    {
        return Err(StoreError::StaleState);
    }
    let live:bool=sqlx::query_scalar("SELECT count(*)=2 FROM objects o WHERE o.individual_id=$1 AND o.id=ANY($2) AND NOT o.blocked AND NOT EXISTS(SELECT 1 FROM derivation_edges d JOIN objects s ON (s.individual_id,s.id)=(d.individual_id,d.source_id) WHERE d.individual_id=o.individual_id AND d.target_id=o.id AND s.blocked)").bind(current.id).bind(vec![action,receipt.result_event_id]).fetch_one(&mut **tx).await?;
    if !live {
        return Err(StoreError::StaleState);
    }
    Ok(receipt)
}

struct Observation<'a> {
    action: Uuid,
    target: Option<&'a WebTarget>,
    mode: CapabilityMode,
    outcome: &'a Result<WebDocument, WebFailure>,
    executed: bool,
}
async fn record(
    tx: &mut Transaction<'_, Postgres>,
    lease: &RunLease,
    observation: Observation<'_>,
) -> Result<ActionReceipt, StoreError> {
    let Observation {
        action,
        target,
        mode,
        outcome,
        executed,
    } = observation;
    require_lease(tx, lease).await?;
    let mut sources: Vec<Uuid> = sqlx::query_scalar("SELECT input_manifest FROM runs WHERE id=$1")
        .bind(lease.run_id)
        .fetch_one(&mut **tx)
        .await?;
    sources.push(action);
    sources.sort();
    sources.dedup();
    let (output, source_kind, status) = match outcome {
        Ok(document) => {
            document
                .validate()
                .map_err(|_| StoreError::Invalid("invalid web observation"))?;
            let url = target
                .ok_or(StoreError::Invalid("missing web target"))?
                .url();
            let id = Uuid::new_v4();
            insert_event(tx,lease.individual_id,id,"document.fetched","external_document","capability",json!({"url":url,"media_type":document.media_type,"http_status":document.http_status,"utf8_bytes":document.text.len(),"characters":document.text.chars().count(),"action_id":action,"execution_mode":mode}),lease.run_id,None).await?;
            // source_label is provenance, never interpreted as a path.
            sqlx::query("INSERT INTO documents(individual_id,event_id,title,source_label,body) VALUES($1,$2,$3,$4,$5)").bind(lease.individual_id).bind(id).bind(target.unwrap().host()).bind(url).bind(&document.text).execute(&mut **tx).await?;
            for source in &sources {
                edge(tx, lease.individual_id, *source, id).await?;
            }
            sources.push(id);
            let excerpt: String = document.text.chars().take(4096).collect();
            let more = document.text.chars().count() > 4096;
            (
                json!({"document_id":id,"url":url,"media_type":document.media_type,"http_status":document.http_status,"text":excerpt,"citation":crate::domain::completion::Citation::for_excerpt(id,0,&excerpt),"next_offset":if more{Some(4096)}else{None},"utf8_bytes":document.text.len(),"characters":document.text.chars().count()}),
                "external_document",
                "succeeded",
            )
        }
        Err(error) => (
            json!({"error":error.code,"http_status":error.http_status}),
            "direct_observation",
            "failed",
        ),
    };
    let result = Uuid::new_v4();
    let version = lease.state_version + 1;
    insert_event(tx,lease.individual_id,result,"tool.result",source_kind,"capability",json!({"action_id":action,"capability_id":WEB_ID,"capability_version":WEB_VERSION,"status":status,"executed":executed,"execution_mode":mode,"output":output}),lease.run_id,None).await?;
    for source in &sources {
        edge(tx, lease.individual_id, *source, result).await?;
        if *source != action {
            edge(tx, lease.individual_id, *source, action).await?;
        }
        if *source != lease.run_id {
            edge(tx, lease.individual_id, *source, lease.run_id).await?;
        }
    }
    sources.push(result);
    require_lease(tx, lease).await?;
    sqlx::query("UPDATE runs SET state_version=$2,input_manifest=$3 WHERE id=$1")
        .bind(lease.run_id)
        .bind(version)
        .bind(sources)
        .execute(&mut **tx)
        .await?;
    let receipt = ActionReceipt {
        action_id: action,
        result_event_id: result,
        status: status.into(),
        state_version: version,
        executed,
    };
    sqlx::query("UPDATE actions SET status=$3,result_event_id=$4,lease_generation=$5,receipt=$6,result_digest=$7,execution_mode=$8,finished_at=clock_timestamp() WHERE individual_id=$1 AND id=$2").bind(lease.individual_id).bind(action).bind(status).bind(result).bind(lease.generation).bind(serde_json::to_value(&receipt)?).bind(digest(outcome)?).bind(if mode==CapabilityMode::Live{"live"}else{"fixture"}).execute(&mut **tx).await?;
    advance(tx, lease.individual_id, version).await?;
    outbox(
        tx,
        lease.individual_id,
        "operation.updated",
        lease.run_id,
        version,
    )
    .await?;
    Ok(receipt)
}

impl Store {
    pub async fn capabilities(&self) -> Result<Vec<CapabilityInfo>, StoreError> {
        let mut capabilities = self.local_capabilities().await?;
        let state = self.snapshot().await?.ok_or(StoreError::NotInitialized)?;
        let mut info = web_info();
        info.authorized = state.runtime_config["capabilities"]["enabled"]
            .as_array()
            .is_some_and(|a| a.iter().any(|id| id.as_str() == Some(WEB_ID)));
        if state.runtime_config["capabilities"]["allowed_web_hosts"]
            .as_array()
            .is_none_or(|a| a.is_empty())
        {
            info.authorized = false;
        }
        let hosts: Vec<String> = serde_json::from_value(
            state.runtime_config["capabilities"]["allowed_web_hosts"].clone(),
        )?;
        info.description.push_str(&format!(
            " 当前允许主机：{}。",
            if hosts.is_empty() {
                "无".into()
            } else {
                hosts.join(", ")
            }
        ));
        capabilities.push(info);
        Ok(capabilities)
    }

    pub(crate) async fn begin_web_action(
        &self,
        lease: &RunLease,
        action: Uuid,
        mode: CapabilityMode,
    ) -> Result<WebStart, StoreError> {
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let row:Option<WebRow>=sqlx::query_as("SELECT capability_id,capability_version,arguments,status,receipt,result_digest,lease_generation,sequence,execution_mode FROM actions WHERE individual_id=$1 AND id=$2 AND run_id=$3").bind(lease.individual_id).bind(action).bind(lease.run_id).fetch_optional(&mut *tx).await?;
        let (cap, version, input, status, receipt, _, generation, sequence, _) =
            row.ok_or(StoreError::Invalid("action absent"))?;
        if cap != WEB_ID || version != WEB_VERSION {
            return Err(ProposalError::Capability.into());
        }
        if let Some(receipt) = receipt {
            return Ok(WebStart::Receipt(
                replay_result(&mut tx, &current, lease, action, receipt, generation).await?,
            ));
        }
        require_lease(&mut tx, lease).await?;
        if status != "planned" {
            return Err(StoreError::Invalid("web action already dispatched"));
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
        if !current.runtime_config["capabilities"]["enabled"]
            .as_array()
            .is_some_and(|a| a.iter().any(|id| id.as_str() == Some(WEB_ID)))
        {
            return Err(ProposalError::Capability.into());
        }
        let hosts: Vec<String> = serde_json::from_value(
            current.runtime_config["capabilities"]["allowed_web_hosts"].clone(),
        )?;
        let target = match WebTarget::from_input(&input, &hosts) {
            Ok(target) => target,
            Err(error) => {
                let receipt = record(
                    &mut tx,
                    lease,
                    Observation {
                        action,
                        target: None,
                        mode,
                        outcome: &Err(error),
                        executed: false,
                    },
                )
                .await?;
                tx.commit().await?;
                return Ok(WebStart::Receipt(receipt));
            }
        };
        let cap = current.runtime_config["autonomy"]["max_tool_calls_per_cycle"]
            .as_i64()
            .ok_or(StoreError::Invalid("tool limit"))?;
        let rows =
            sqlx::query("UPDATE runs SET tool_calls=tool_calls+1 WHERE id=$1 AND tool_calls<$2")
                .bind(lease.run_id)
                .bind(cap)
                .execute(&mut *tx)
                .await?;
        if rows.rows_affected() != 1 {
            return Err(StoreError::CallLimit);
        }
        sqlx::query("UPDATE actions SET status='executing',lease_generation=$3,execution_mode=$4,started_at=clock_timestamp() WHERE individual_id=$1 AND id=$2").bind(current.id).bind(action).bind(lease.generation).bind(if mode==CapabilityMode::Live{"live"}else{"fixture"}).execute(&mut *tx).await?;
        require_lease(&mut tx, lease).await?;
        tx.commit().await?;
        Ok(WebStart::Dispatch(Box::new(WebDispatch {
            target,
            action,
            lease: lease.clone(),
            mode,
        })))
    }

    pub(crate) async fn finish_web_action(
        &self,
        dispatch: &WebDispatch,
        outcome: &Result<WebDocument, WebFailure>,
    ) -> Result<ActionReceipt, StoreError> {
        let lease = &dispatch.lease;
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let row:Option<WebRow>=sqlx::query_as("SELECT capability_id,capability_version,arguments,status,receipt,result_digest,lease_generation,sequence,execution_mode FROM actions WHERE individual_id=$1 AND id=$2 AND run_id=$3").bind(lease.individual_id).bind(dispatch.action).bind(lease.run_id).fetch_optional(&mut *tx).await?;
        let (cap, version, _, status, receipt, hash, generation, _, mode) =
            row.ok_or(StoreError::Invalid("action absent"))?;
        if cap != WEB_ID
            || version != WEB_VERSION
            || generation != Some(lease.generation)
            || mode.as_deref()
                != Some(if dispatch.mode == CapabilityMode::Live {
                    "live"
                } else {
                    "fixture"
                })
        {
            return Err(StoreError::LeaseLost);
        }
        if let Some(receipt) = receipt {
            if hash != Some(digest(outcome)?) {
                return Err(StoreError::IdempotencyConflict);
            }
            return replay_result(
                &mut tx,
                &current,
                lease,
                dispatch.action,
                receipt,
                generation,
            )
            .await;
        }
        if status != "executing" {
            return Err(StoreError::LeaseLost);
        }
        let receipt = record(
            &mut tx,
            lease,
            Observation {
                action: dispatch.action,
                target: Some(&dispatch.target),
                mode: dispatch.mode,
                outcome,
                executed: true,
            },
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }
}
