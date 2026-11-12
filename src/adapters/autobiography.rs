//! Trusted source-bound interpretations. No model or HTTP mutation entry point.
use super::{
    api::existing,
    store::{
        Store, StoreError, advance, digest, edge, insert_event, object, outbox, replay,
        save_receipt, state,
    },
};
use crate::domain::{
    Lifecycle,
    autobiography::{AutobiographyDraft, PreviousAutobiography},
    history::ExpectedState,
};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use std::collections::HashSet;
use uuid::Uuid;

type Tx<'a> = Transaction<'a, Postgres>;
const ROUTE: &str = "/internal/autobiography";
// Association tables are independent privacy barriers when a derivation edge is
// absent. UNION also terminates malformed predecessor/provenance cycles.
const LINKS: &str = r#"WITH RECURSIVE links(target,source) AS (
 SELECT target_id,source_id FROM derivation_edges WHERE individual_id=$1
 UNION SELECT id,supersedes FROM autobiographies WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT autobiography_id,event_id FROM autobiography_events WHERE individual_id=$1
 UNION SELECT autobiography_id,belief_id FROM autobiography_beliefs WHERE individual_id=$1
 UNION SELECT id,supersedes FROM beliefs WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT belief_id,event_id FROM belief_evidence WHERE individual_id=$1
) "#;
const BAD: &str = r#", bad(root) AS (
 SELECT c.root FROM closure c LEFT JOIN objects o ON o.individual_id=$1 AND o.id=c.id
 WHERE o.id IS NULL OR o.blocked
 UNION SELECT c.root FROM closure c JOIN autobiography_events a ON a.individual_id=$1 AND a.autobiography_id=c.id
 JOIN events e ON e.individual_id=$1 AND e.id=a.event_id
 WHERE e.source_kind='fictional_background' OR e.kind='decision.reviewed'
 UNION SELECT c.root FROM closure c JOIN belief_evidence b ON b.individual_id=$1 AND b.belief_id=c.id
 JOIN events e ON e.individual_id=$1 AND e.id=b.event_id
 WHERE e.source_kind='fictional_background' OR e.kind='decision.reviewed'
 UNION SELECT c.root FROM closure c JOIN autobiographies a ON a.individual_id=$1 AND a.id=c.id
 WHERE NOT EXISTS(SELECT 1 FROM autobiography_events e WHERE e.individual_id=$1 AND e.autobiography_id=a.id)
) "#;

async fn available(tx: &mut Tx<'_>, individual: Uuid, ids: &[Uuid]) -> Result<bool, StoreError> {
    sqlx::query_scalar(&format!("{LINKS}, closure(root,id) AS (SELECT id,id FROM unnest($2::uuid[]) AS t(id) UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id) {BAD} SELECT NOT EXISTS(SELECT 1 FROM bad)"))
  .bind(individual).bind(ids).fetch_one(&mut **tx).await.map_err(StoreError::from)
}

/// Include the entire lineage of selected stances even before the first history
/// interpretation exists. Otherwise a first interpretation could see a current
/// stance but miss the counterevidence behind its previous revisions.
async fn enrich_stance_basis(
    tx: &mut Tx<'_>,
    context: &mut crate::domain::cognition::ModelContext,
) -> Result<(), StoreError> {
    use crate::domain::{
        autobiography::{AutobiographyContext, HistoricalBelief},
        history::Event,
    };
    let roots: Vec<_> = context
        .selected_beliefs
        .iter()
        .flat_map(|s| &s.items)
        .map(|b| b.id)
        .collect();
    if roots.is_empty() {
        return Ok(());
    }
    let ids:Vec<Uuid> = sqlx::query_scalar("WITH RECURSIVE lineage(id) AS (SELECT id FROM beliefs WHERE individual_id=$1 AND id=ANY($2) UNION SELECT b.supersedes FROM beliefs b JOIN lineage l ON l.id=b.id WHERE b.individual_id=$1 AND b.supersedes IS NOT NULL) SELECT id FROM lineage UNION SELECT e.event_id FROM belief_evidence e JOIN lineage l ON l.id=e.belief_id WHERE e.individual_id=$1 ORDER BY id LIMIT 1025")
        .bind(context.individual_id).bind(&roots).fetch_all(&mut **tx).await?;
    let selection = context
        .autobiographical_context
        .get_or_insert(AutobiographyContext {
            entries: vec![],
            belief_basis: vec![],
            has_more: false,
        });
    if ids.len() > 1024 {
        selection.has_more = true;
        return Ok(());
    }
    if !available(tx, context.individual_id, &ids).await? {
        return Err(StoreError::StaleState);
    }
    let stances:Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',b.id,'version',b.version,'statement',b.statement,'scope',b.scope,'position',b.position,'brief_basis',b.brief_basis,'status',b.status,'supersedes',b.supersedes,'support_event_ids',ARRAY(SELECT event_id FROM belief_evidence e WHERE (e.individual_id,e.belief_id)=(b.individual_id,b.id) AND role='support' ORDER BY event_id),'counter_event_ids',ARRAY(SELECT event_id FROM belief_evidence e WHERE (e.individual_id,e.belief_id)=(b.individual_id,b.id) AND role='counter' ORDER BY event_id)) FROM beliefs b WHERE b.individual_id=$1 AND b.id=ANY($2) ORDER BY b.id LIMIT 65")
        .bind(context.individual_id).bind(&ids).fetch_all(&mut **tx).await?;
    if stances.len() > 64 {
        context.autobiographical_context.as_mut().unwrap().has_more = true;
        return Ok(());
    }
    let events:Vec<Value> = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE e.individual_id=$1 AND e.id=ANY($2) ORDER BY e.sequence")
        .bind(context.individual_id).bind(&ids).fetch_all(&mut **tx).await?;
    let mut candidate = context.clone();
    candidate.input_manifest.extend(ids);
    candidate.input_manifest.sort();
    candidate.input_manifest.dedup();
    for row in events {
        let event: Event = serde_json::from_value(row)?;
        if !candidate.events.iter().any(|e| e.id == event.id) {
            candidate.events.push(event);
        }
    }
    for row in stances {
        let belief: HistoricalBelief = serde_json::from_value(row)?;
        candidate
            .autobiographical_context
            .as_mut()
            .unwrap()
            .belief_basis
            .push(belief);
    }
    candidate.events.sort_by_key(|e| e.sequence);
    if !candidate
        .autobiographical_context
        .as_ref()
        .unwrap()
        .valid_for(&candidate)
    {
        return Err(StoreError::Invalid("incomplete history stance basis"));
    }
    if serde_json::to_vec(&candidate)?.len() <= 96 * 1024 {
        *context = candidate;
    } else {
        context.autobiographical_context.as_mut().unwrap().has_more = true;
    }
    Ok(())
}

/// Reuse the same RR snapshot as the rest of cognition. All linked historical
/// stances, counterevidence and predecessor sources travel together or not at all.
pub(super) async fn enrich_context(
    tx: &mut Tx<'_>,
    context: &mut crate::domain::cognition::ModelContext,
) -> Result<(), StoreError> {
    use crate::domain::{
        autobiography::{AutobiographyContext, CurrentAutobiography, HistoricalBelief},
        history::Event,
    };
    enrich_stance_basis(tx, context).await?;
    let owner = context.individual_id;
    let rows: Vec<Value> = sqlx::query_scalar(&format!(r#"{LINKS}, closure(root,id) AS (
      SELECT id,id FROM autobiographies WHERE individual_id=$1
      UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id
    ) {BAD} SELECT jsonb_build_object('id',a.id,'version',a.version,'topic',a.topic,
      'title',a.title,'interpretation',a.interpretation,'supersedes',a.supersedes,
      'event_ids',ARRAY(SELECT event_id FROM autobiography_events e WHERE e.individual_id=$1 AND e.autobiography_id=a.id ORDER BY event_id),
      'belief_ids',ARRAY(SELECT belief_id FROM autobiography_beliefs b WHERE b.individual_id=$1 AND b.autobiography_id=a.id ORDER BY belief_id),
      'provenance_ids','[]'::jsonb,'basis_event_ids','[]'::jsonb)
      FROM autobiographies a WHERE a.individual_id=$1 AND a.status='current'
       AND NOT EXISTS(SELECT 1 FROM bad WHERE root=a.id)
      ORDER BY a.created_at DESC,a.id DESC LIMIT 4"#))
        .bind(owner).fetch_all(&mut **tx).await?;
    if rows.is_empty() {
        return Ok(());
    }
    let selection = context
        .autobiographical_context
        .get_or_insert(AutobiographyContext {
            entries: vec![],
            belief_basis: vec![],
            has_more: rows.len() > 3,
        });
    selection.has_more |= rows.len() > 3;
    if serde_json::to_vec(context)?.len() > 96 * 1024 {
        return Err(StoreError::Invalid(
            "autobiography selection metadata exceeds budget",
        ));
    }
    for row in rows.into_iter().take(3) {
        let mut entry: CurrentAutobiography = serde_json::from_value(row)?;
        if !available(tx, owner, &[owner, entry.id]).await? {
            return Err(StoreError::StaleState);
        }
        let ids: Vec<Uuid> = sqlx::query_scalar(&format!(
            r#"{LINKS}, closure(id) AS (
          SELECT id FROM unnest($2::uuid[]) roots(id)
          UNION SELECT l.source FROM closure c JOIN links l ON l.target=c.id
        ) SELECT id FROM closure ORDER BY id LIMIT 1025"#
        ))
        .bind(owner)
        .bind([owner, entry.id].as_slice())
        .fetch_all(&mut **tx)
        .await?;
        if ids.len() > 1024 {
            context.autobiographical_context.as_mut().unwrap().has_more = true;
            continue;
        }
        let events: Vec<Value> = sqlx::query_scalar(
            "SELECT to_jsonb(e) FROM events e WHERE individual_id=$1 AND id=ANY($2) ORDER BY sequence"
        ).bind(owner).bind(&ids).fetch_all(&mut **tx).await?;
        let beliefs: Vec<Value> = sqlx::query_scalar(r#"SELECT jsonb_build_object(
          'id',b.id,'version',b.version,'statement',b.statement,'scope',b.scope,'position',b.position,
          'brief_basis',b.brief_basis,'status',b.status,'supersedes',b.supersedes,
          'support_event_ids',ARRAY(SELECT event_id FROM belief_evidence e WHERE (e.individual_id,e.belief_id)=(b.individual_id,b.id) AND role='support' ORDER BY event_id),
          'counter_event_ids',ARRAY(SELECT event_id FROM belief_evidence e WHERE (e.individual_id,e.belief_id)=(b.individual_id,b.id) AND role='counter' ORDER BY event_id))
          FROM beliefs b WHERE b.individual_id=$1 AND b.id=ANY($2) ORDER BY b.id LIMIT 65"#)
            .bind(owner).bind(&ids).fetch_all(&mut **tx).await?;
        if beliefs.len() > 64 {
            context.autobiographical_context.as_mut().unwrap().has_more = true;
            continue;
        }
        let mut candidate = context.clone();
        entry.provenance_ids = ids;
        for value in events {
            let event: Event = serde_json::from_value(value)?;
            entry.basis_event_ids.push(event.id);
            if !candidate.events.iter().any(|e| e.id == event.id) {
                candidate.events.push(event);
            }
        }
        candidate.input_manifest.extend(&entry.provenance_ids);
        let selection = candidate.autobiographical_context.as_mut().unwrap();
        for value in beliefs {
            let b: HistoricalBelief = serde_json::from_value(value)?;
            if !selection.belief_basis.iter().any(|old| old.id == b.id) {
                selection.belief_basis.push(b);
            }
        }
        selection.entries.push(entry);
        candidate.input_manifest.sort();
        candidate.input_manifest.dedup();
        candidate.events.sort_by_key(|e| e.sequence);
        let selection = candidate.autobiographical_context.as_ref().unwrap();
        if selection.belief_basis.len() > 64 {
            context.autobiographical_context.as_mut().unwrap().has_more = true;
        } else if !selection.valid_for(&candidate) {
            return Err(StoreError::Invalid("invalid autobiographical context"));
        } else if serde_json::to_vec(&candidate)?.len() <= 96 * 1024 {
            *context = candidate;
        } else {
            context.autobiographical_context.as_mut().unwrap().has_more = true;
        }
    }
    if !context
        .autobiographical_context
        .as_ref()
        .unwrap()
        .valid_for(context)
    {
        return Err(StoreError::Invalid("invalid autobiographical context"));
    }
    Ok(())
}

/// Return only an actual stored operation receipt, not an inferred experience.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AutobiographyReceipt {
    pub individual_id: Uuid,
    pub autobiography_id: Uuid,
    pub previous_autobiography_id: Option<Uuid>,
    pub event_id: Uuid,
    pub version: i64,
    pub autobiography_version: i64,
}

async fn validate_revision(
    tx: &mut Tx<'_>,
    individual: Uuid,
    input_manifest: &[Uuid],
    draft: &AutobiographyDraft,
    strict_manifest: bool,
) -> Result<crate::domain::autobiography::ValidatedAutobiography, StoreError> {
    let inputs: HashSet<_> = input_manifest.iter().copied().collect();
    let mut sources = input_manifest.to_vec();
    sources.push(individual);
    if !available(tx, individual, &sources).await? {
        return Err(StoreError::NotFound);
    }
    if draft
        .event_ids
        .iter()
        .chain(&draft.belief_ids)
        .chain(draft.supersedes.iter())
        .any(|id| !inputs.contains(id))
    {
        return Err(StoreError::Invalid(
            "autobiography reference outside manifest",
        ));
    }
    // No rejected candidate or fictional background may enter as experience,
    // including an otherwise unreferenced event in the supplied source manifest.
    let inspected = if strict_manifest {
        input_manifest
    } else {
        draft.event_ids.as_slice()
    };
    let forbidden:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE individual_id=$1 AND id=ANY($2) AND (source_kind='fictional_background' OR kind='decision.reviewed'))")
   .bind(individual).bind(inspected).fetch_one(&mut **tx).await?;
    if forbidden {
        return Err(StoreError::Invalid(
            "autobiography needs actual event records",
        ));
    }
    let actual:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM events WHERE individual_id=$1 AND id=ANY($2) AND source_kind<>'fictional_background' AND kind<>'decision.reviewed'")
   .bind(individual).bind(input_manifest).fetch_all(&mut **tx).await?;
    let beliefs: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM beliefs WHERE individual_id=$1 AND id=ANY($2)")
            .bind(individual)
            .bind(input_manifest)
            .fetch_all(&mut **tx)
            .await?;
    // Selected stances need their entire predecessor basis, never support alone.
    let basis:Vec<Uuid>=sqlx::query_scalar("WITH RECURSIVE lineage(id) AS (SELECT id FROM beliefs WHERE individual_id=$1 AND id=ANY($2) UNION SELECT b.supersedes FROM beliefs b JOIN lineage l ON l.id=b.id WHERE b.individual_id=$1 AND b.supersedes IS NOT NULL) SELECT id FROM lineage UNION SELECT e.event_id FROM belief_evidence e JOIN lineage l ON l.id=e.belief_id WHERE e.individual_id=$1")
   .bind(individual).bind(if strict_manifest { input_manifest } else { draft.belief_ids.as_slice() }).fetch_all(&mut **tx).await?;
    if basis.iter().any(|id| !inputs.contains(id)) {
        return Err(StoreError::Invalid("incomplete autobiography stance basis"));
    }
    let previous = if let Some(id) = draft.supersedes {
        let (topic, version, status): (String, i64, String) = sqlx::query_as(
            "SELECT topic,version,status FROM autobiographies WHERE individual_id=$1 AND id=$2",
        )
        .bind(individual)
        .bind(id)
        .fetch_optional(&mut **tx)
        .await?
        .ok_or(StoreError::NotFound)?;
        Some(PreviousAutobiography {
            individual_id: individual,
            id,
            topic,
            version,
            superseded: status != "current",
        })
    } else {
        None
    };
    let validated = draft
        .clone()
        .validate(
            individual,
            &actual.into_iter().collect(),
            &beliefs.into_iter().collect(),
            previous.as_ref(),
        )
        .map_err(|_| StoreError::Invalid("autobiography fields, source or predecessor"))?;
    if previous.is_none() {
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM autobiographies WHERE individual_id=$1 AND topic=$2 AND status='current')")
    .bind(individual).bind(&draft.topic).fetch_one(&mut **tx).await?;
        if exists {
            return Err(StoreError::Invalid(
                "autobiography topic requires current predecessor",
            ));
        }
    }
    Ok(validated)
}

async fn insert_revision(
    tx: &mut Tx<'_>,
    validated: &crate::domain::autobiography::ValidatedAutobiography,
    input_manifest: &[Uuid],
    version: i64,
    correlation: Option<Uuid>,
    key: Option<&str>,
) -> Result<(Uuid, Uuid), StoreError> {
    let individual = validated.individual_id();
    let draft = validated.draft();
    let autobiography = Uuid::new_v4();
    let event = Uuid::new_v4();
    if let Some((id, before)) = validated.expected_previous() {
        let changed=sqlx::query("UPDATE autobiographies SET status='superseded',version=version+1,superseded_at=clock_timestamp() WHERE individual_id=$1 AND id=$2 AND version=$3 AND status='current'")
    .bind(individual).bind(id).bind(before).execute(&mut **tx).await?;
        if changed.rows_affected() != 1 {
            return Err(StoreError::StaleState);
        }
        outbox(tx, individual, "autobiography.changed", id, version).await?;
    }
    object(tx, individual, autobiography, "autobiography").await?;
    sqlx::query("INSERT INTO autobiographies(individual_id,id,topic,title,interpretation,supersedes) VALUES($1,$2,$3,$4,$5,$6)")
   .bind(individual).bind(autobiography).bind(&draft.topic).bind(&draft.title).bind(&draft.interpretation).bind(draft.supersedes).execute(&mut **tx).await?;
    for id in &draft.event_ids {
        sqlx::query("INSERT INTO autobiography_events(individual_id,autobiography_id,event_id) VALUES($1,$2,$3)").bind(individual).bind(autobiography).bind(id).execute(&mut **tx).await?;
    }
    for id in &draft.belief_ids {
        sqlx::query("INSERT INTO autobiography_beliefs(individual_id,autobiography_id,belief_id) VALUES($1,$2,$3)").bind(individual).bind(autobiography).bind(id).execute(&mut **tx).await?;
    }
    insert_event(
        tx,
        individual,
        event,
        if draft.supersedes.is_some() {
            "autobiography.revised"
        } else {
            "autobiography.formed"
        },
        "inference",
        "individual",
        json!({"autobiography_id":autobiography,"topic":draft.topic,"supersedes":draft.supersedes}),
        correlation.unwrap_or(event),
        key,
    )
    .await?;
    for source in input_manifest {
        edge(tx, individual, *source, autobiography).await?;
        edge(tx, individual, *source, event).await?;
    }
    edge(tx, individual, event, autobiography).await?;
    outbox(
        tx,
        individual,
        "autobiography.changed",
        autobiography,
        version,
    )
    .await?;
    Ok((autobiography, event))
}

pub(super) async fn apply_model_changes(
    tx: &mut Tx<'_>,
    context: &crate::domain::cognition::ModelContext,
    changes: &[AutobiographyDraft],
    decision: Uuid,
    version: i64,
) -> Result<Vec<Uuid>, StoreError> {
    let mut ids = Vec::new();
    for draft in changes {
        let expected = draft
            .validate_context(context)
            .map_err(|_| StoreError::Invalid("autobiography context revision"))?;
        let validated = validate_revision(
            tx,
            context.individual_id,
            &context.input_manifest,
            draft,
            false,
        )
        .await?;
        if expected.expected_previous() != validated.expected_previous() {
            return Err(StoreError::StaleState);
        }
        let (id, event) = insert_revision(
            tx,
            &validated,
            &context.input_manifest,
            version,
            Some(context.run_id),
            None,
        )
        .await?;
        edge(tx, context.individual_id, decision, id).await?;
        edge(tx, context.individual_id, decision, event).await?;
        ids.push(id);
    }
    Ok(ids)
}

impl Store {
    pub async fn record_autobiography(
        &self,
        key: &str,
        expected: ExpectedState,
        input_manifest: &[Uuid],
        draft: &AutobiographyDraft,
    ) -> Result<AutobiographyReceipt, StoreError> {
        let inputs: HashSet<_> = input_manifest.iter().copied().collect();
        if inputs.is_empty()
            || inputs.len() != input_manifest.len()
            || inputs.contains(&Uuid::nil())
        {
            return Err(StoreError::Invalid("autobiography input manifest"));
        }
        let hash =
            digest(&json!({"expected":expected,"input_manifest":input_manifest,"draft":draft}))?;
        let mut tx = self.write().await?;
        if let Some(receipt) =
            replay::<AutobiographyReceipt>(&mut tx, "individual", ROUTE, key, &hash).await?
        {
            let individual = existing(&mut tx).await?;
            let stored:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM autobiographies a JOIN events e ON e.individual_id=a.individual_id AND e.id=$3 WHERE a.individual_id=$1 AND a.id=$2 AND e.kind IN ('autobiography.formed','autobiography.revised') AND e.source_kind='inference' AND e.payload->>'autobiography_id'=a.id::text)")
    .bind(individual).bind(receipt.autobiography_id).bind(receipt.event_id).fetch_one(&mut *tx).await?;
            let mut sources = input_manifest.to_vec();
            sources.extend([individual, receipt.autobiography_id, receipt.event_id]);
            if individual != receipt.individual_id
                || !stored
                || !available(&mut tx, individual, &sources).await?
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
        let validated = validate_revision(&mut tx, current.id, input_manifest, draft, true).await?;
        let version = current
            .state_version
            .checked_add(1)
            .ok_or(StoreError::Invalid("state version exhausted"))?;
        let (autobiography, event) = insert_revision(
            &mut tx,
            &validated,
            input_manifest,
            version,
            None,
            Some(key),
        )
        .await?;
        advance(&mut tx, current.id, version).await?;
        let receipt = AutobiographyReceipt {
            individual_id: current.id,
            autobiography_id: autobiography,
            previous_autobiography_id: draft.supersedes,
            event_id: event,
            version,
            autobiography_version: 1,
        };
        save_receipt(&mut tx, "individual", ROUTE, key, &hash, &receipt).await?;
        tx.commit().await?;
        Ok(receipt)
    }

    pub async fn autobiography(
        &self,
        include_history: bool,
        limit: u32,
    ) -> Result<Value, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("autobiography limit"));
        }
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let individual = existing(&mut tx).await?;
        if !available(&mut tx, individual, &[individual]).await? {
            return Err(StoreError::NotFound);
        }
        let mut items:Vec<Value>=sqlx::query_scalar(&format!(r#"{LINKS}, closure(root,id) AS (
    SELECT id,id FROM autobiographies WHERE individual_id=$1
    UNION SELECT c.root,l.source FROM closure c JOIN links l ON l.target=c.id
   ) {BAD} SELECT jsonb_build_object('id',a.id,'version',a.version,'topic',a.topic,'title',a.title,
    'interpretation',a.interpretation,'status',a.status,'supersedes',a.supersedes,
    'created_at',a.created_at,'superseded_at',a.superseded_at,
    'event_ids',ARRAY(SELECT event_id FROM autobiography_events e WHERE e.individual_id=$1 AND e.autobiography_id=a.id ORDER BY event_id),
    'belief_ids',ARRAY(SELECT belief_id FROM autobiography_beliefs b WHERE b.individual_id=$1 AND b.autobiography_id=a.id ORDER BY belief_id))
   FROM autobiographies a WHERE a.individual_id=$1 AND ($2 OR a.status='current')
    AND NOT EXISTS(SELECT 1 FROM bad WHERE root=a.id)
   ORDER BY a.created_at DESC,a.id DESC LIMIT $3"#))
   .bind(individual).bind(include_history).bind(i64::from(limit)+1).fetch_all(&mut *tx).await?;
        let has_more = items.len() > limit as usize;
        items.truncate(limit as usize);
        let (state_version, privacy_epoch, observed_at): (i64, i64, DateTime<Utc>) =
            sqlx::query_as(
                "SELECT state_version,privacy_epoch,clock_timestamp() FROM individuals WHERE id=$1",
            )
            .bind(individual)
            .fetch_one(&mut *tx)
            .await?;
        tx.commit().await?;
        Ok(
            json!({"individual_id":individual,"state_version":state_version,"privacy_epoch":privacy_epoch,"observed_at":observed_at,"items":items,"has_more":has_more}),
        )
    }
}
