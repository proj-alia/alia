//! Trusted cognition persistence. No model/HTTP mutation endpoint is enabled.
use super::{
    api::VISIBLE,
    goals::unavailable,
    store::{
        Store, StoreError, advance, digest, edge, insert_event, object, outbox, replay,
        save_receipt, state,
    },
};
use crate::domain::{
    Lifecycle,
    beliefs::{BeliefDraft, CurrentBelief, PreviousBelief},
    history::ExpectedState,
};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use std::collections::HashSet;
use uuid::Uuid;

/// Bounded current stances in the caller's repeatable-read snapshot. Complete
/// evidence bundles are loaded by context.rs before anything enters a request.
pub(super) async fn for_context(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    individual: Uuid,
    scope: &str,
) -> Result<Vec<CurrentBelief>, StoreError> {
    let rows: Vec<Value> = sqlx::query_scalar(&format!(
        "{VISIBLE} SELECT jsonb_build_object(
          'id',b.id,'version',b.version,'statement',b.statement,'scope',b.scope,
          'position',b.position,'brief_basis',b.brief_basis,
          'support_event_ids',ARRAY(SELECT event_id FROM belief_evidence e
            WHERE (e.individual_id,e.belief_id)=(b.individual_id,b.id) AND role='support' ORDER BY event_id),
          'counter_event_ids',ARRAY(SELECT event_id FROM belief_evidence e
            WHERE (e.individual_id,e.belief_id)=(b.individual_id,b.id) AND role='counter' ORDER BY event_id))
         FROM beliefs b WHERE b.individual_id=$1 AND b.status='current'
          AND b.scope IN ('general',$2)
          AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=$1 AND h.id IN ($1,b.id))
          AND NOT EXISTS(SELECT 1 FROM belief_evidence e JOIN hidden h
            ON (h.individual_id,h.id)=(e.individual_id,e.event_id)
            WHERE (e.individual_id,e.belief_id)=(b.individual_id,b.id))
         ORDER BY (b.scope=$2) DESC,b.created_at DESC,b.id DESC LIMIT 7"
    ))
    .bind(individual)
    .bind(scope)
    .fetch_all(&mut **tx)
    .await?;
    rows.into_iter()
        .map(|v| serde_json::from_value(v).map_err(StoreError::from))
        .collect()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BeliefReceipt {
    pub individual_id: Uuid,
    pub belief_id: Uuid,
    pub previous_belief_id: Option<Uuid>,
    pub event_id: Uuid,
    pub version: i64,
    pub belief_version: i64,
}

/// Apply one validated revision inside the caller's existing atomic decision.
async fn insert_revision(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    validated: &crate::domain::beliefs::ValidatedBelief,
    input_manifest: &[Uuid],
    version: i64,
    correlation: Option<Uuid>,
    key: Option<&str>,
) -> Result<(Uuid, Uuid), StoreError> {
    let individual = validated.individual_id();
    let draft = validated.draft();
    let belief = Uuid::new_v4();
    let event = Uuid::new_v4();
    if let Some((id, before)) = validated.expected_previous() {
        let changed=sqlx::query("UPDATE beliefs SET status='superseded',version=version+1,superseded_at=clock_timestamp() WHERE individual_id=$1 AND id=$2 AND version=$3 AND status='current'")
                .bind(individual).bind(id).bind(before).execute(&mut **tx).await?;
        if changed.rows_affected() != 1 {
            return Err(StoreError::StaleState);
        }
        outbox(tx, individual, "belief.changed", id, version).await?;
    }
    object(tx, individual, belief, "belief").await?;
    let position = serde_json::to_value(draft.position)?;
    sqlx::query("INSERT INTO beliefs(individual_id,id,statement,scope,position,brief_basis,supersedes) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(individual).bind(belief).bind(&draft.statement).bind(&draft.scope).bind(position.as_str().ok_or(StoreError::Invalid("belief position"))?)
            .bind(&draft.brief_basis).bind(draft.supersedes).execute(&mut **tx).await?;
    for (role, ids) in [
        ("support", &draft.support_event_ids),
        ("counter", &draft.counter_event_ids),
    ] {
        for id in ids {
            sqlx::query("INSERT INTO belief_evidence(individual_id,belief_id,event_id,role) VALUES($1,$2,$3,$4)")
                .bind(individual).bind(belief).bind(id).bind(role).execute(&mut **tx).await?;
        }
    }
    insert_event(
        tx,
        individual,
        event,
        if draft.supersedes.is_some() {
            "belief.revised"
        } else {
            "belief.formed"
        },
        "inference",
        "individual",
        json!({"belief_id":belief,"supersedes":draft.supersedes,"position":draft.position}),
        correlation.unwrap_or(event),
        key,
    )
    .await?;
    for source in input_manifest {
        edge(tx, individual, *source, belief).await?;
        edge(tx, individual, *source, event).await?;
    }
    edge(tx, individual, event, belief).await?;
    outbox(tx, individual, "belief.changed", belief, version).await?;
    Ok((belief, event))
}

pub(super) async fn apply_model_changes(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    context: &crate::domain::cognition::ModelContext,
    changes: &[BeliefDraft],
    decision: Uuid,
    version: i64,
) -> Result<Vec<Uuid>, StoreError> {
    let mut ids = Vec::new();
    for draft in changes {
        let validated = draft
            .validate_context(context)
            .map_err(|_| StoreError::Invalid("belief context revision"))?;
        let (belief, event) = insert_revision(
            tx,
            &validated,
            &context.input_manifest,
            version,
            Some(context.run_id),
            None,
        )
        .await?;
        edge(tx, context.individual_id, decision, event).await?;
        edge(tx, context.individual_id, decision, belief).await?;
        ids.push(belief);
    }
    Ok(ids)
}

impl Store {
    /// Validates ownership/provenance, not whether evidence semantically proves a claim.
    pub async fn record_belief(
        &self,
        key: &str,
        expected: ExpectedState,
        input_manifest: &[Uuid],
        draft: &BeliefDraft,
    ) -> Result<BeliefReceipt, StoreError> {
        let inputs: HashSet<_> = input_manifest.iter().copied().collect();
        if inputs.is_empty() || inputs.len() > 256 || inputs.len() != input_manifest.len() {
            return Err(StoreError::Invalid("belief input manifest"));
        }
        let hash =
            digest(&json!({"expected":expected,"input_manifest":input_manifest,"draft":draft}))?;
        let mut tx = self.write().await?;
        if let Some(receipt) =
            replay::<BeliefReceipt>(&mut tx, "individual", "/internal/beliefs", key, &hash).await?
        {
            if unavailable(
                &mut tx,
                receipt.individual_id,
                &[receipt.individual_id, receipt.belief_id, receipt.event_id],
            )
            .await?
            {
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
        let mut sources = vec![current.id];
        sources.extend_from_slice(input_manifest);
        if unavailable(&mut tx, current.id, &sources).await? {
            return Err(StoreError::NotFound);
        }
        let visible: Vec<Uuid> =
            sqlx::query_scalar("SELECT id FROM events WHERE individual_id=$1 AND id=ANY($2)")
                .bind(current.id)
                .bind(input_manifest)
                .fetch_all(&mut *tx)
                .await?;
        let previous = if let Some(id) = draft.supersedes {
            if !inputs.contains(&id) {
                return Err(StoreError::Invalid("previous belief outside context"));
            }
            let (version, scope, status): (i64, String, String) = sqlx::query_as(
                "SELECT version,scope,status FROM beliefs WHERE individual_id=$1 AND id=$2",
            )
            .bind(current.id)
            .bind(id)
            .fetch_optional(&mut *tx)
            .await?
            .ok_or(StoreError::NotFound)?;
            Some(PreviousBelief {
                individual_id: current.id,
                id,
                version,
                scope,
                superseded: status != "current",
            })
        } else {
            None
        };
        let validated = draft
            .clone()
            .validate(
                current.id,
                &visible.into_iter().collect(),
                previous.as_ref(),
            )
            .map_err(|_| StoreError::Invalid("belief evidence, scope or predecessor"))?;
        let version = current.state_version + 1;
        let (belief, event) = insert_revision(
            &mut tx,
            &validated,
            input_manifest,
            version,
            None,
            Some(key),
        )
        .await?;
        advance(&mut tx, current.id, version).await?;
        let receipt = BeliefReceipt {
            individual_id: current.id,
            belief_id: belief,
            previous_belief_id: draft.supersedes,
            event_id: event,
            version,
            belief_version: 1,
        };
        save_receipt(
            &mut tx,
            "individual",
            "/internal/beliefs",
            key,
            &hash,
            &receipt,
        )
        .await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn beliefs(&self, include_history: bool) -> Result<Vec<Value>, StoreError> {
        let rows=sqlx::query_scalar(&format!("{VISIBLE} SELECT to_jsonb(b) || jsonb_build_object('support_event_ids',ARRAY(SELECT event_id FROM belief_evidence e WHERE (e.individual_id,e.belief_id)=(b.individual_id,b.id) AND role='support' ORDER BY event_id),'counter_event_ids',ARRAY(SELECT event_id FROM belief_evidence e WHERE (e.individual_id,e.belief_id)=(b.individual_id,b.id) AND role='counter' ORDER BY event_id)) FROM beliefs b WHERE ($1 OR b.status='current') AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=b.individual_id AND h.id IN (b.individual_id,b.id)) ORDER BY b.created_at,b.id LIMIT 100"))
            .bind(include_history).fetch_all(&self.pool).await?;
        Ok(rows)
    }
}
