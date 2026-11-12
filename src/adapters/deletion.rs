//! Conservative structural deletion preview. It never grants commit authorization or erases data.
use super::{
    api::{VISIBLE, existing},
    store::{Store, StoreError},
};
use serde::Serialize;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DeletionClosure {
    pub individual_id: Uuid,
    pub state_version: i64,
    pub privacy_epoch: i64,
    pub target_memory_id: Uuid,
    pub memory_version: i64,
    pub root_event_ids: Vec<Uuid>,
    pub object_ids: Vec<Uuid>,
}
// Reference direction is source -> dependent object, never owner -> all owned rows.
// Non-object payload tables (notes/documents/vectors/ledger) are represented by
// their owning action/event/memory/run. Subsequent purge must delete those rows too.
const REFERENCES: &str = r#"
 SELECT source_id,target_id FROM derivation_edges WHERE individual_id=$1
 UNION SELECT id,individual_id FROM events WHERE individual_id=$1 AND kind='individual.created'
 UNION SELECT event_id,memory_id FROM memory_sources WHERE individual_id=$1
 UNION SELECT supersedes,id FROM memories WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT event_id,subject_id FROM relationship_events WHERE individual_id=$1
 UNION SELECT unnest(input_manifest),id FROM runs WHERE individual_id=$1
 UNION SELECT trigger_event_id,id FROM runs WHERE individual_id=$1
 UNION SELECT goal_id,id FROM runs WHERE individual_id=$1 AND goal_id IS NOT NULL
 UNION SELECT run_id,id FROM actions WHERE individual_id=$1
 UNION SELECT decision_event_id,id FROM actions WHERE individual_id=$1
 UNION SELECT id,result_event_id FROM actions WHERE individual_id=$1 AND result_event_id IS NOT NULL
 UNION SELECT result_event_id,id FROM actions WHERE individual_id=$1 AND result_event_id IS NOT NULL
 UNION SELECT run_id,decision_event_id FROM decision_commits WHERE individual_id=$1
 UNION SELECT decision_event_id,run_id FROM decision_commits WHERE individual_id=$1
 UNION SELECT run_id,event_id FROM decision_reviews WHERE individual_id=$1
 UNION SELECT causation_id,id FROM events WHERE individual_id=$1 AND causation_id IS NOT NULL
 UNION SELECT correlation_id,id FROM events WHERE individual_id=$1
 UNION SELECT run_id,draft_event_id FROM outgoing_messages WHERE individual_id=$1
 UNION SELECT draft_event_id,delivered_event_id FROM outgoing_messages WHERE individual_id=$1 AND delivered_event_id IS NOT NULL
 UNION SELECT run_id,delivered_event_id FROM outgoing_messages WHERE individual_id=$1 AND delivered_event_id IS NOT NULL
 UNION SELECT source_event_id,run_id FROM discovery_occurrences WHERE individual_id=$1
 UNION SELECT formed_in_run,id FROM goals WHERE individual_id=$1 AND formed_in_run IS NOT NULL
 UNION SELECT event_id,goal_id FROM goal_progress WHERE individual_id=$1
 UNION SELECT goal_id,event_id FROM goal_reviews WHERE individual_id=$1
 UNION SELECT event_id,goal_id FROM goal_reviews WHERE individual_id=$1
 UNION SELECT run_id,goal_id FROM goal_cycles WHERE individual_id=$1
 UNION SELECT goal_id,run_id FROM goal_cycles WHERE individual_id=$1
 UNION SELECT event_id,belief_id FROM belief_evidence WHERE individual_id=$1
 UNION SELECT supersedes,id FROM beliefs WHERE individual_id=$1 AND supersedes IS NOT NULL
 UNION SELECT source_event_id,entity_id FROM person_aliases WHERE individual_id=$1
 UNION SELECT assertion_event_id,entity_id FROM person_aliases WHERE individual_id=$1
 UNION SELECT entity_id,memory_id FROM memory_person_links WHERE individual_id=$1
 UNION SELECT source_event_id,memory_id FROM memory_person_links WHERE individual_id=$1
 UNION SELECT assertion_event_id,memory_id FROM memory_person_links WHERE individual_id=$1
 UNION SELECT CASE WHEN x.value ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN x.value::uuid END,g.id FROM goals g CROSS JOIN LATERAL jsonb_array_elements_text(g.origin_ids) x(value)
   WHERE g.individual_id=$1 AND x.value ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
 UNION SELECT CASE WHEN c.value->>'document_id' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN (c.value->>'document_id')::uuid END,n.action_id FROM notes n CROSS JOIN LATERAL jsonb_array_elements(n.citations) c(value)
   WHERE n.individual_id=$1 AND c.value->>'document_id' ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
 UNION SELECT CASE WHEN x.value ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN x.value::uuid END,e.id FROM events e CROSS JOIN LATERAL jsonb_each_text(e.payload) x(key,value)
   WHERE e.individual_id=$1 AND x.key IN ('decision_id','goal_id','memory_id','previous_memory_id','belief_id','autobiography_id','supersedes','action_id','draft_event_id')
     AND x.value ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
 UNION SELECT CASE WHEN x.value ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$' THEN x.value::uuid END,e.id FROM events e CROSS JOIN LATERAL jsonb_each(e.payload) p(key,value)
   CROSS JOIN LATERAL jsonb_array_elements_text(CASE WHEN jsonb_typeof(p.value)='array' THEN p.value ELSE '[]'::jsonb END) x(value)
   WHERE e.individual_id=$1 AND p.key IN ('evidence_ids','source_event_ids','reply_evidence_ids')
     AND x.value ~* '^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$'
"#;
const AUTOBIOGRAPHY_REFERENCES: &str = r#"
 UNION SELECT event_id,autobiography_id FROM autobiography_events WHERE individual_id=$1
 UNION SELECT belief_id,autobiography_id FROM autobiography_beliefs WHERE individual_id=$1
 UNION SELECT supersedes,id FROM autobiographies WHERE individual_id=$1 AND supersedes IS NOT NULL
"#;
const AFFECT_REFERENCES: &str = r#"
 UNION SELECT event_id,object_id FROM affective_sources WHERE individual_id=$1
 UNION SELECT goal_id,id FROM affective_appraisals WHERE individual_id=$1 AND goal_id IS NOT NULL
 UNION SELECT subject_id,id FROM affective_appraisals WHERE individual_id=$1 AND subject_id IS NOT NULL
 UNION SELECT subject_id,id FROM affective_attitudes WHERE individual_id=$1 AND subject_id IS NOT NULL
 UNION SELECT supersedes,id FROM affective_attitudes WHERE individual_id=$1 AND supersedes IS NOT NULL
"#;
const RELATIONSHIP_REFERENCES: &str = r#"
 UNION SELECT event_id,revision_id FROM relationship_revision_events WHERE individual_id=$1
 UNION SELECT previous_revision_id,id FROM relationship_revisions WHERE individual_id=$1 AND previous_revision_id IS NOT NULL
 UNION SELECT subject_id,id FROM relationship_revisions WHERE individual_id=$1
 UNION SELECT id,subject_id FROM relationship_revisions WHERE individual_id=$1
"#;
const INTEREST_REFERENCES: &str = r#"
 UNION SELECT s.event_id,i.object_id FROM interest_sources s JOIN interests i ON (i.individual_id,i.id)=(s.individual_id,s.interest_id) WHERE s.individual_id=$1
 UNION SELECT r.event_id,r.id FROM interest_revisions r WHERE r.individual_id=$1
 UNION SELECT r.previous_revision_id,r.id FROM interest_revisions r WHERE r.individual_id=$1 AND r.previous_revision_id IS NOT NULL
 UNION SELECT i.object_id,r.id FROM interest_revisions r JOIN interests i ON (i.individual_id,i.id)=(r.individual_id,r.interest_id) WHERE r.individual_id=$1
 UNION SELECT r.id,i.object_id FROM interest_revisions r JOIN interests i ON (i.individual_id,i.id)=(r.individual_id,r.interest_id) WHERE r.individual_id=$1
 UNION SELECT i.object_id,g.id FROM goals g CROSS JOIN LATERAL jsonb_array_elements_text(g.origin_ids) x(value)
   JOIN interests i ON i.individual_id=g.individual_id AND i.id=x.value WHERE g.individual_id=$1
"#;
const DEVELOPMENT_REFERENCES: &str = r#"
 UNION SELECT scope_id,id FROM development_targets WHERE individual_id=$1 AND scope_id IS NOT NULL
 UNION SELECT current_revision_id,id FROM development_targets WHERE individual_id=$1 AND current_revision_id IS NOT NULL
 UNION SELECT target_id,id FROM developments WHERE individual_id=$1
 UNION SELECT proposer_id,id FROM developments WHERE individual_id=$1
 UNION SELECT created_event_id,id FROM developments WHERE individual_id=$1
 UNION SELECT before_head,id FROM developments WHERE individual_id=$1 AND before_head IS NOT NULL
 UNION SELECT event_id,development_id FROM development_evidence WHERE individual_id=$1
 UNION SELECT development_id,event_id FROM development_steps WHERE individual_id=$1
 UNION SELECT event_id,development_id FROM development_steps WHERE individual_id=$1
 UNION SELECT revision_id,event_id FROM development_steps WHERE individual_id=$1 AND revision_id IS NOT NULL
 UNION SELECT target_id,id FROM development_revisions WHERE individual_id=$1
 UNION SELECT development_id,id FROM development_revisions WHERE individual_id=$1
 UNION SELECT event_id,id FROM development_revisions WHERE individual_id=$1
 UNION SELECT previous_revision_id,id FROM development_revisions WHERE individual_id=$1 AND previous_revision_id IS NOT NULL
 UNION SELECT source_id,object_id FROM development_dependencies WHERE individual_id=$1
 UNION SELECT registration_event_id,id FROM development_verifiers WHERE individual_id=$1
 UNION SELECT verifier_id,event_id FROM development_observations WHERE individual_id=$1
 UNION SELECT trial_event_id,event_id FROM development_observations WHERE individual_id=$1 AND trial_event_id IS NOT NULL
 UNION SELECT development_id,event_id FROM development_review_records WHERE individual_id=$1
 UNION SELECT event_id,development_id FROM development_review_records WHERE individual_id=$1
 UNION SELECT trial_event_id,event_id FROM development_review_records WHERE individual_id=$1
 UNION SELECT verifier_id,event_id FROM development_review_records WHERE individual_id=$1
 UNION SELECT event_id,review_event_id FROM development_review_sources WHERE individual_id=$1
"#;

pub(super) async fn reference_graph_sql(
    tx: &mut Transaction<'_, Postgres>,
) -> Result<String, StoreError> {
    let history_tables:(bool,bool,bool)=sqlx::query_as("SELECT to_regclass('autobiographies') IS NOT NULL,to_regclass('autobiography_events') IS NOT NULL,to_regclass('autobiography_beliefs') IS NOT NULL").fetch_one(&mut **tx).await?;
    let history = match history_tables {
        (true, true, true) => AUTOBIOGRAPHY_REFERENCES,
        (false, false, false) => "",
        _ => {
            return Err(StoreError::Invalid(
                "incomplete autobiography deletion schema",
            ));
        }
    };
    let affect_tables:(bool,bool,bool)=sqlx::query_as("SELECT to_regclass('affective_appraisals') IS NOT NULL,to_regclass('affective_attitudes') IS NOT NULL,to_regclass('affective_sources') IS NOT NULL").fetch_one(&mut **tx).await?;
    let affect = match affect_tables {
        (true, true, true) => AFFECT_REFERENCES,
        (false, false, false) => "",
        _ => return Err(StoreError::Invalid("incomplete affective deletion schema")),
    };
    let relationship_tables:(bool,bool)=sqlx::query_as("SELECT to_regclass('relationship_revisions') IS NOT NULL,to_regclass('relationship_revision_events') IS NOT NULL").fetch_one(&mut **tx).await?;
    let relationships = match relationship_tables {
        (true, true) => RELATIONSHIP_REFERENCES,
        (false, false) => "",
        _ => {
            return Err(StoreError::Invalid(
                "incomplete relationship deletion schema",
            ));
        }
    };
    let interest_tables:(bool,bool,bool)=sqlx::query_as("SELECT to_regclass('interests') IS NOT NULL,to_regclass('interest_sources') IS NOT NULL,to_regclass('interest_revisions') IS NOT NULL").fetch_one(&mut **tx).await?;
    let interests = match interest_tables {
        (true, true, true) => INTEREST_REFERENCES,
        (false, false, false) => "",
        _ => return Err(StoreError::Invalid("incomplete interest deletion schema")),
    };
    let development_count:i64=sqlx::query_scalar("SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_name=ANY($1)").bind(&["development_targets","developments","development_evidence","development_steps","development_revisions","development_dependencies","development_verifiers","development_observations","development_review_records","development_review_sources"][..]).fetch_one(&mut **tx).await?;
    let development = match development_count {
        0 => "",
        10 => DEVELOPMENT_REFERENCES,
        _ => {
            return Err(StoreError::Invalid(
                "incomplete development provenance schema",
            ));
        }
    };
    let trial = match super::development_measurements::reference_edges_in(tx).await? {
        Some(edges) => format!(" UNION {edges}"),
        None => String::new(),
    };
    Ok(format!(
        "{REFERENCES}{history}{affect}{relationships}{interests}{development}{trial}"
    ))
}
pub(super) async fn expand_deletion_objects(
    tx: &mut Transaction<'_, Postgres>,
    individual: Uuid,
    seeds: &[Uuid],
    check_visibility: bool,
) -> Result<Vec<Uuid>, StoreError> {
    let graph = reference_graph_sql(tx).await?;
    let (object_ids,hidden_ancestor,ancestor_count):(Vec<Uuid>,bool,i64)=sqlx::query_as(&format!(r#"WITH RECURSIVE refs(source,target) AS ({graph}),
            owned_refs(source,target) AS (SELECT r.source,r.target FROM refs r
                JOIN objects s ON s.individual_id=$1 AND s.id=r.source
                JOIN objects t ON t.individual_id=$1 AND t.id=r.target),
            ancestors(id) AS (SELECT unnest($2::uuid[]) UNION SELECT r.source FROM owned_refs r JOIN ancestors a ON a.id=r.target),
            bounded_ancestors AS MATERIALIZED (SELECT id FROM ancestors LIMIT 10001),
            closure(id) AS (SELECT unnest($2::uuid[]) UNION SELECT r.target FROM owned_refs r JOIN closure c ON c.id=r.source)
            SELECT ARRAY(SELECT id FROM (SELECT id FROM closure LIMIT 10001) bounded ORDER BY id),
                EXISTS(SELECT 1 FROM bounded_ancestors a JOIN objects o ON o.individual_id=$1 AND o.id=a.id WHERE o.blocked),
                (SELECT count(*) FROM bounded_ancestors)"#)).bind(individual).bind(seeds).fetch_one(&mut **tx).await?;
    if ancestor_count > 10000 {
        return Err(StoreError::Invalid("memory deletion ancestry too large"));
    }
    if check_visibility && hidden_ancestor {
        return Err(StoreError::NotFound);
    }
    if object_ids.len() > 10000 {
        return Err(StoreError::Invalid("memory deletion closure too large"));
    }
    if object_ids.contains(&individual) {
        return Err(StoreError::Invalid("identity anchor deletion unsupported"));
    }
    Ok(object_ids)
}
pub(super) async fn memory_deletion_closure_in(
    tx: &mut Transaction<'_, Postgres>,
    target: Uuid,
    expected_memory_version: i64,
) -> Result<DeletionClosure, StoreError> {
    if target.is_nil() || expected_memory_version <= 0 {
        return Err(StoreError::Invalid("invalid memory deletion target"));
    }
    let individual = existing(tx).await?;
    let (state_version, privacy_epoch): (i64, i64) =
        sqlx::query_as("SELECT state_version,privacy_epoch FROM individuals WHERE id=$1")
            .bind(individual)
            .fetch_one(&mut **tx)
            .await?;
    let memory_version:Option<i64>=sqlx::query_scalar(&format!(r#"{VISIBLE}
            SELECT m.version FROM memories m WHERE m.individual_id=$1 AND m.id=$2
            AND NOT EXISTS(SELECT 1 FROM hidden h WHERE h.individual_id=m.individual_id AND h.id IN (m.individual_id,m.id))"#))
            .bind(individual).bind(target).fetch_optional(&mut **tx).await?;
    let memory_version = memory_version.ok_or(StoreError::NotFound)?;
    if memory_version != expected_memory_version {
        return Err(StoreError::StaleState);
    }
    let root_event_ids:Vec<Uuid>=sqlx::query_scalar("SELECT event_id FROM memory_sources WHERE individual_id=$1 AND memory_id=$2 ORDER BY event_id LIMIT 257").bind(individual).bind(target).fetch_all(&mut **tx).await?;
    if !(1..=256).contains(&root_event_ids.len()) {
        return Err(StoreError::Invalid("unsupported memory deletion sources"));
    }
    let visible:i64=sqlx::query_scalar(&format!(r#"{VISIBLE} SELECT count(*) FROM events e WHERE e.individual_id=$1 AND e.id=ANY($2)
            AND NOT EXISTS(SELECT 1 FROM hidden h WHERE (h.individual_id,h.id)=(e.individual_id,e.id))"#)).bind(individual).bind(&root_event_ids).fetch_one(&mut **tx).await?;
    if visible as usize != root_event_ids.len() {
        return Err(StoreError::NotFound);
    }
    let mut seeds = root_event_ids.clone();
    seeds.push(target);
    let object_ids = expand_deletion_objects(tx, individual, &seeds, true).await?;
    Ok(DeletionClosure {
        individual_id: individual,
        state_version,
        privacy_epoch,
        target_memory_id: target,
        memory_version,
        root_event_ids,
        object_ids,
    })
}
impl Store {
    pub async fn memory_deletion_closure(
        &self,
        target: Uuid,
        expected_memory_version: i64,
    ) -> Result<DeletionClosure, StoreError> {
        let generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        let result = memory_deletion_closure_in(&mut tx, target, expected_memory_version).await?;
        tx.commit().await?;
        self.require_privacy_snapshot(generation)?;
        Ok(result)
    }
}
