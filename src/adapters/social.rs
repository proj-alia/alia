//! One RR snapshot and one source graph for all three distinct social layers.
use super::store::StoreError;
use crate::domain::{
    cognition::ModelContext,
    history::Event,
    social::{CurrentAppraisal, CurrentAttitude, CurrentRelation, SocialContext},
};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
enum Slot {
    Appraisal,
    Attitude,
    Relationship,
}
fn fields(row: &Value, names: &[&str]) -> Value {
    Value::Object(
        names
            .iter()
            .map(|name| ((*name).into(), row[*name].clone()))
            .collect(),
    )
}
async fn add(
    tx: &mut Tx<'_>,
    c: &mut ModelContext,
    graph: &str,
    slot: Slot,
    mut value: Value,
) -> Result<bool, StoreError> {
    let id: Uuid = serde_json::from_value(if matches!(slot, Slot::Relationship) {
        value["subject_id"].clone()
    } else {
        value["id"].clone()
    })?;
    let ids:Vec<Uuid>=sqlx::query_scalar(&format!("WITH RECURSIVE refs(source,target) AS ({graph}),owned_refs(source,target) AS (SELECT r.source,r.target FROM refs r JOIN objects s ON s.individual_id=$1 AND s.id=r.source JOIN objects t ON t.individual_id=$1 AND t.id=r.target),closure(id) AS (SELECT $2::uuid UNION SELECT r.source FROM closure c JOIN owned_refs r ON r.target=c.id) SELECT id FROM closure ORDER BY id LIMIT 1025"))
        .bind(c.individual_id).bind(id).fetch_all(&mut **tx).await?;
    if ids.len() > 1024 {
        return Ok(false);
    }
    let hidden:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM unnest($2::uuid[]) x(id) LEFT JOIN objects o ON o.individual_id=$1 AND o.id=x.id WHERE o.id IS NULL OR o.blocked) OR EXISTS(SELECT 1 FROM affective_sources s JOIN events e ON e.individual_id=s.individual_id AND e.id=s.event_id WHERE s.individual_id=$1 AND s.object_id=ANY($2) AND (e.source_kind='fictional_background' OR e.kind='decision.reviewed')) OR EXISTS(SELECT 1 FROM relationship_revision_events s JOIN events e ON e.individual_id=s.individual_id AND e.id=s.event_id WHERE s.individual_id=$1 AND s.revision_id=ANY($2) AND (e.source_kind='fictional_background' OR e.kind='decision.reviewed')) OR EXISTS(SELECT 1 FROM objects o WHERE o.individual_id=$1 AND o.id=ANY($2) AND ((o.kind IN ('affective_state','affective_attitude') AND NOT EXISTS(SELECT 1 FROM affective_sources s WHERE s.individual_id=$1 AND s.object_id=o.id)) OR (o.kind='relationship_revision' AND NOT EXISTS(SELECT 1 FROM relationship_revision_events s WHERE s.individual_id=$1 AND s.revision_id=o.id))))")
        .bind(c.individual_id).bind(&ids).fetch_one(&mut **tx).await?;
    if hidden {
        return Ok(false);
    }
    let events: Vec<Value> = sqlx::query_scalar(
        "SELECT to_jsonb(e) FROM events e WHERE individual_id=$1 AND id=ANY($2) ORDER BY sequence",
    )
    .bind(c.individual_id)
    .bind(&ids)
    .fetch_all(&mut **tx)
    .await?;
    let mut candidate = c.clone();
    let mut basis = Vec::new();
    for value in events {
        let e: Event = serde_json::from_value(value)?;
        basis.push(e.id);
        if !candidate.events.iter().any(|old| old.id == e.id) {
            candidate.events.push(e);
        }
    }
    candidate.input_manifest.extend(&ids);
    candidate.input_manifest.sort();
    candidate.input_manifest.dedup();
    candidate.events.sort_by_key(|e| e.sequence);
    value["provenance_ids"] = json!(ids);
    value["basis_event_ids"] = json!(basis);
    let selection = candidate
        .social_context
        .get_or_insert_with(SocialContext::default);
    match slot {
        Slot::Appraisal => {
            selection.appraisal = Some(serde_json::from_value::<CurrentAppraisal>(value)?)
        }
        Slot::Attitude => selection
            .attitudes
            .push(serde_json::from_value::<CurrentAttitude>(value)?),
        Slot::Relationship => selection
            .relationships
            .push(serde_json::from_value::<CurrentRelation>(value)?),
    }
    if !candidate
        .social_context
        .as_ref()
        .unwrap()
        .valid_for(&candidate)
    {
        return Err(StoreError::Invalid(
            "invalid social context or source bundle",
        ));
    }
    if serde_json::to_vec(&candidate)?.len() > 96 * 1024 {
        return Ok(false);
    }
    *c = candidate;
    Ok(true)
}
pub(super) async fn enrich_context(
    tx: &mut Tx<'_>,
    c: &mut ModelContext,
) -> Result<(), StoreError> {
    let graph = super::deletion::reference_graph_sql(tx).await?;
    let relationships:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('subject_id',r.subject_id,'version',r.version,'state',jsonb_build_object('stage',r.stage,'expectations',r.expectations,'boundaries',r.boundaries,'commitments',r.commitments,'unresolved',r.unresolved),'latest_revision_id',v.id,'latest_revision_version',v.after_version) FROM relationships r LEFT JOIN LATERAL (SELECT id,after_version FROM relationship_revisions WHERE individual_id=r.individual_id AND subject_id=r.subject_id ORDER BY after_version DESC,id DESC LIMIT 1) v ON true WHERE r.individual_id=$1 ORDER BY (r.subject_key='owner') DESC,r.subject_id LIMIT 5").bind(c.individual_id).fetch_all(&mut **tx).await?;
    let has_more = relationships.len() > 4;
    c.social_context = Some(SocialContext {
        has_more,
        ..Default::default()
    });
    for value in relationships.into_iter().take(4) {
        if !add(tx, c, &graph, Slot::Relationship, value).await? {
            c.social_context.as_mut().unwrap().has_more = true;
        }
    }
    let owner = c.relationship["subject_id"]
        .as_str()
        .and_then(|s| Uuid::parse_str(s).ok());
    if let Some(owner) = owner
        && !c
            .social_context
            .as_ref()
            .unwrap()
            .relationships
            .iter()
            .any(|r| r.subject_id == owner)
    {
        c.relationship = json!({});
        c.input_manifest.retain(|id| *id != owner);
    }
    let appraisal:Option<Value>=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('event_ids',ARRAY(SELECT event_id FROM affective_sources s WHERE s.individual_id=$1 AND s.object_id=a.id ORDER BY event_id)) FROM affective_appraisals a WHERE a.individual_id=$1 AND a.status='current' AND a.created_at<=$2 AND a.expires_at>$2 ORDER BY a.created_at DESC,a.id DESC LIMIT 1").bind(c.individual_id).bind(c.now).fetch_optional(&mut **tx).await?;
    if let Some(row) = appraisal {
        let draft = fields(
            &row,
            &[
                "event_ids",
                "goal_id",
                "subject_id",
                "valence",
                "arousal",
                "goal_relevance",
                "expectation_gap",
                "controllability",
                "uncertainty",
                "brief_basis",
            ],
        );
        let d: crate::domain::affect::AppraisalDraft = serde_json::from_value(draft.clone())?;
        let weight = crate::domain::affect::attention_weight(
            d.goal_relevance,
            d.expectation_gap,
            d.uncertainty,
        )
        .map_err(|_| StoreError::Invalid("appraisal weight"))?;
        let v = json!({"id":row["id"],"draft":draft,"created_at":row["created_at"],"expires_at":row["expires_at"],"attention_weight":weight});
        if !add(tx, c, &graph, Slot::Appraisal, v).await? {
            c.social_context.as_mut().unwrap().has_more = true;
        }
    }
    let attitudes:Vec<Value>=sqlx::query_scalar("SELECT to_jsonb(a)||jsonb_build_object('event_ids',ARRAY(SELECT event_id FROM affective_sources s WHERE s.individual_id=$1 AND s.object_id=a.id ORDER BY event_id)) FROM affective_attitudes a WHERE a.individual_id=$1 AND a.status='current' ORDER BY a.created_at DESC,a.id DESC LIMIT 5").bind(c.individual_id).fetch_all(&mut **tx).await?;
    c.social_context.as_mut().unwrap().has_more |= attitudes.len() > 4;
    for row in attitudes.into_iter().take(4) {
        let draft = fields(
            &row,
            &[
                "topic",
                "subject_id",
                "stance",
                "event_ids",
                "supersedes",
                "brief_basis",
            ],
        );
        let v = json!({"id":row["id"],"version":row["version"],"draft":draft});
        if !add(tx, c, &graph, Slot::Attitude, v).await? {
            c.social_context.as_mut().unwrap().has_more = true;
        }
    }
    crate::domain::social::order_attention(c);
    Ok(())
}
