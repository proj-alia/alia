//! Transactional privacy projection for actual development rows. No gate/commit ownership.
use super::{
    deletion::reference_graph_sql,
    store::{StoreError, digest, object},
};
use crate::domain::{
    config::PersonaSeed,
    development::{DevelopmentTarget, StrategyScope, TaskStrategy},
};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use std::collections::HashSet;
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
fn invalid() -> StoreError {
    StoreError::Invalid("unprovable development privacy projection")
}
async fn available(tx: &mut Tx<'_>, individual: Uuid, ids: &[Uuid]) -> Result<bool, StoreError> {
    let graph = reference_graph_sql(tx).await?;
    let bad:bool=sqlx::query_scalar(&format!("WITH RECURSIVE refs(source,target) AS ({graph}), owned_refs(source,target) AS (SELECT r.source,r.target FROM refs r JOIN objects s ON s.individual_id=$1 AND s.id=r.source JOIN objects t ON t.individual_id=$1 AND t.id=r.target), ancestors(id) AS (SELECT unnest($2::uuid[]) UNION SELECT r.source FROM owned_refs r JOIN ancestors a ON a.id=r.target) SELECT EXISTS(SELECT 1 FROM ancestors a LEFT JOIN objects o ON o.individual_id=$1 AND o.id=a.id WHERE o.id IS NULL OR o.blocked)")).bind(individual).bind(ids).fetch_one(&mut **tx).await?;
    Ok(!bad)
}
#[derive(sqlx::FromRow)]
struct Head {
    id: Uuid,
    target: String,
    scope_kind: Option<String>,
    scope_id: Option<Uuid>,
    version: i64,
    value: Value,
    current_revision_id: Option<Uuid>,
}
async fn projected(tx: &mut Tx<'_>, individual: Uuid) -> Result<Value, StoreError> {
    let (seed, seed_digest): (Value, String) =
        sqlx::query_as("SELECT seed,seed_digest FROM individuals WHERE id=$1")
            .bind(individual)
            .fetch_one(&mut **tx)
            .await?;
    if digest(&seed)? != seed_digest {
        return Err(invalid());
    }
    PersonaSeed::validate(seed.clone()).map_err(|_| invalid())?;
    let birth:Option<(Uuid,Value,String,String)>=sqlx::query_as("SELECT id,payload,source_kind,actor_id FROM events WHERE individual_id=$1 AND kind='individual.created' ORDER BY sequence LIMIT 1").bind(individual).fetch_optional(&mut **tx).await?;
    let Some((birth, payload, kind, actor)) = birth else {
        return Err(invalid());
    };
    if payload.get("seed_digest").and_then(Value::as_str) != Some(seed_digest.as_str())
        || kind != "direct_observation"
        || actor != "system"
        || !available(tx, individual, &[birth]).await?
    {
        return Err(invalid());
    }

    let mut persona = seed.clone();
    let heads:Vec<Head>=sqlx::query_as("SELECT id,target,scope_kind,scope_id,version,value,current_revision_id FROM development_targets WHERE individual_id=$1 AND NOT provenance_removed ORDER BY target,scope_kind,scope_id,id LIMIT 100001").bind(individual).fetch_all(&mut **tx).await?;
    if heads.len() > 100000 {
        return Err(invalid());
    }
    let mut seen = HashSet::new();
    for h in heads {
        if !seen.insert((h.target.clone(), h.scope_kind.clone(), h.scope_id))
            || !available(tx, individual, &[h.id]).await?
        {
            return Err(invalid());
        }
        let target: DevelopmentTarget =
            serde_json::from_value(Value::String(h.target.clone())).map_err(|_| invalid())?;
        if let Some(revision) = h.current_revision_id {
            // The cached value is never trusted without the actual recorded application,
            // its candidate, step and independent validation/trial provenance.
            let proof:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM development_revisions r JOIN developments d ON (d.individual_id,d.id)=(r.individual_id,r.development_id) JOIN development_steps s ON s.individual_id=r.individual_id AND s.development_id=d.id AND s.event_id=r.event_id AND s.revision_id=r.id WHERE r.individual_id=$1 AND r.id=$2 AND r.target_id=$3 AND d.target_id=$3 AND r.after_version=$4 AND r.after_value=$5 AND NOT r.provenance_removed AND NOT d.provenance_removed AND ((r.action='accept' AND s.status='accepted' AND s.command='\"Accept\"'::jsonb AND d.status IN ('accepted','reverted')) OR (r.action='revert' AND s.status='reverted' AND s.command ? 'Revert' AND d.status='reverted')) AND EXISTS(SELECT 1 FROM development_steps t WHERE t.individual_id=$1 AND t.development_id=d.id AND t.status='trial' AND t.command ? 'StartTrial') AND EXISTS(SELECT 1 FROM development_steps v JOIN development_review_records rr ON rr.individual_id=v.individual_id AND rr.development_id=v.development_id AND rr.event_id=(v.command->'RecordValidation'->>'review_event_id')::uuid WHERE v.individual_id=$1 AND v.development_id=d.id AND v.validation_record IS NOT NULL AND v.command ? 'RecordValidation' AND rr.assessment=v.command->'RecordValidation'->'assessment' AND jsonb_typeof(v.command->'RecordValidation'->'source_event_ids')='array' AND jsonb_array_length(v.command->'RecordValidation'->'source_event_ids')>0 AND ARRAY(SELECT x.event_id FROM development_review_sources x WHERE x.individual_id=$1 AND x.review_event_id=rr.event_id ORDER BY x.event_id) = ARRAY(SELECT x::uuid FROM jsonb_array_elements_text(v.command->'RecordValidation'->'source_event_ids') x ORDER BY 1) AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements(rr.assessment->'observations') a LEFT JOIN development_observations o ON o.individual_id=$1 AND o.event_id=(a->>'result_event_id')::uuid WHERE o.event_id IS NULL OR o.trial_event_id IS DISTINCT FROM rr.trial_event_id OR o.role<>'scenario_result' OR o.case_id IS DISTINCT FROM (a->>'case_id')::uuid OR o.context IS DISTINCT FROM a->>'context' OR o.outcome IS DISTINCT FROM lower(a->>'outcome')) AND NOT EXISTS(SELECT 1 FROM jsonb_array_elements_text(rr.assessment->'counter_search_event_ids') a LEFT JOIN development_observations o ON o.individual_id=$1 AND o.event_id=a::uuid WHERE o.event_id IS NULL OR o.trial_event_id IS DISTINCT FROM rr.trial_event_id OR o.role<>'counter_search' OR o.outcome IS DISTINCT FROM 'passed') AND jsonb_typeof(rr.assessment->'observations')='array' AND jsonb_array_length(rr.assessment->'observations')>0 AND jsonb_typeof(rr.assessment->'counter_search_event_ids')='array' AND jsonb_array_length(rr.assessment->'counter_search_event_ids')>0) AND jsonb_typeof(d.draft->'evidence_ids')='array' AND jsonb_array_length(d.draft->'evidence_ids')>0 AND ARRAY(SELECT x.event_id FROM development_evidence x WHERE x.individual_id=$1 AND x.development_id=d.id AND x.role='evidence' ORDER BY x.event_id) = ARRAY(SELECT x::uuid FROM jsonb_array_elements_text(d.draft->'evidence_ids') x ORDER BY 1) AND ARRAY(SELECT x.event_id FROM development_evidence x WHERE x.individual_id=$1 AND x.development_id=d.id AND x.role='counterevidence' ORDER BY x.event_id) = ARRAY(SELECT x::uuid FROM jsonb_array_elements_text(d.draft->'counterevidence_ids') x ORDER BY 1))").bind(individual).bind(revision).bind(h.id).bind(h.version).bind(&h.value).fetch_one(&mut **tx).await?;
            if !proof || !available(tx, individual, &[revision]).await? {
                return Err(invalid());
            }
        } else {
            let baseline = target
                .persona_pointer()
                .and_then(|p| seed.pointer(p))
                .cloned()
                .unwrap_or(Value::Null);
            let baseline_version:i64=sqlx::query_scalar("SELECT greatest(coalesce((SELECT max(t.version) FROM development_targets t WHERE t.individual_id=$1 AND t.id<>$2 AND t.target=$3 AND t.scope_kind IS NOT DISTINCT FROM $4 AND t.scope_id IS NOT DISTINCT FROM $5),0),coalesce((SELECT max(r.after_version) FROM development_revisions r JOIN development_targets t ON (t.individual_id,t.id)=(r.individual_id,r.target_id) WHERE t.individual_id=$1 AND t.id<>$2 AND t.target=$3 AND t.scope_kind IS NOT DISTINCT FROM $4 AND t.scope_id IS NOT DISTINCT FROM $5),0))+1").bind(individual).bind(h.id).bind(&h.target).bind(&h.scope_kind).bind(h.scope_id).fetch_one(&mut **tx).await?;
            if h.version != baseline_version || h.value != baseline {
                return Err(invalid());
            }
            // A new generation baseline has no old revision pointer; historical maximum
            // is consumed by the command adapter, rather than restoring a private before.
        }
        if target == DevelopmentTarget::TaskStrategy {
            let scope = match (h.scope_kind.as_deref(), h.scope_id) {
                (Some("goal"), Some(id)) => StrategyScope::Goal(id),
                (Some("task"), Some(id)) => StrategyScope::Task(id),
                _ => return Err(invalid()),
            };
            if h.value.is_null() {
                if let Some(revision) = h.current_revision_id {
                    let reverted:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM development_revisions WHERE individual_id=$1 AND id=$2 AND action='revert' AND after_value='null')").bind(individual).bind(revision).fetch_one(&mut **tx).await?;
                    if !reverted {
                        return Err(invalid());
                    }
                }
                continue;
            }
            let strategy: TaskStrategy = serde_json::from_value(h.value).map_err(|_| invalid())?;
            if strategy.scope != scope
                || strategy.description.trim().is_empty()
                || strategy.description.chars().count() > 2000
                || strategy.description.contains('\0')
            {
                return Err(invalid());
            }
        } else {
            if h.scope_id.is_some() || h.scope_kind.is_some() {
                return Err(invalid());
            }
            *persona
                .pointer_mut(target.persona_pointer().ok_or_else(invalid)?)
                .ok_or_else(invalid)? = h.value;
        }
    }
    PersonaSeed::validate(persona.clone()).map_err(|_| invalid())?;
    Ok(persona)
}
pub(super) async fn verify_development_projection_in(
    tx: &mut Tx<'_>,
    individual: Uuid,
) -> Result<(), StoreError> {
    let expected = projected(tx, individual).await?;
    let actual: Value = sqlx::query_scalar("SELECT persona FROM individuals WHERE id=$1")
        .bind(individual)
        .fetch_one(&mut **tx)
        .await?;
    if actual != expected {
        return Err(invalid());
    }
    Ok(())
}
pub(super) async fn block_development_in(
    tx: &mut Tx<'_>,
    individual: Uuid,
    ids: &[Uuid],
) -> Result<(), StoreError> {
    for table in [
        "development_targets",
        "developments",
        "development_revisions",
    ] {
        sqlx::query(&format!(
            "UPDATE {table} SET provenance_removed=true WHERE individual_id=$1 AND id=ANY($2)"
        ))
        .bind(individual)
        .bind(ids)
        .execute(&mut **tx)
        .await?;
    }
    let value = projected(tx, individual).await?;
    sqlx::query("UPDATE individuals SET persona=$2 WHERE id=$1 AND persona IS DISTINCT FROM $2")
        .bind(individual)
        .bind(value)
        .execute(&mut **tx)
        .await?;
    renew_owner_relationship_in(tx, individual).await?;
    verify_development_projection_in(tx, individual).await
}
async fn renew_owner_relationship_in(tx: &mut Tx<'_>, individual: Uuid) -> Result<(), StoreError> {
    let blocked:Option<Uuid>=sqlx::query_scalar("SELECT r.subject_id FROM relationships r JOIN objects o ON (o.individual_id,o.id)=(r.individual_id,r.subject_id) WHERE r.individual_id=$1 AND r.subject_key='owner' AND o.blocked").bind(individual).fetch_optional(&mut **tx).await?;
    if let Some(old) = blocked {
        sqlx::query("UPDATE relationships SET subject_key='redacted:'||subject_id::text WHERE individual_id=$1 AND subject_id=$2").bind(individual).bind(old).execute(&mut **tx).await?;
        let new = Uuid::new_v4();
        object(tx, individual, new, "relationship").await?;
        sqlx::query(
            "INSERT INTO relationships(individual_id,subject_id,subject_key) VALUES($1,$2,'owner')",
        )
        .bind(individual)
        .bind(new)
        .execute(&mut **tx)
        .await?;
    }
    Ok(())
}
struct Rule {
    table: &'static str,
    condition: &'static str,
    set: &'static str,
    clean: &'static str,
}
const RULES: &[Rule] = &[
    Rule {
        table: "development_targets",
        condition: "t.id=ANY($2)",
        set: "provenance_removed=true,value='null',current_revision_id=NULL",
        clean: "provenance_removed AND value='null' AND current_revision_id IS NULL",
    },
    Rule {
        table: "developments",
        condition: "t.id=ANY($2) OR t.target_id=ANY($2)",
        set: "provenance_removed=true,draft='{}',before_value='null',before_head=NULL",
        clean: "provenance_removed AND draft='{}' AND before_value='null' AND before_head IS NULL",
    },
    Rule {
        table: "development_revisions",
        condition: "t.id=ANY($2) OR t.target_id=ANY($2) OR t.development_id=ANY($2)",
        set: "provenance_removed=true,before_value='null',after_value='null'",
        clean: "provenance_removed AND before_value='null' AND after_value='null'",
    },
    Rule {
        table: "development_steps",
        condition: "t.event_id=ANY($2) OR t.development_id=ANY($2) OR t.revision_id=ANY($2)",
        set: "command='{}',transition_record=NULL,validation_record=NULL",
        clean: "command='{}' AND transition_record IS NULL AND validation_record IS NULL",
    },
    Rule {
        table: "development_observations",
        condition: "t.event_id=ANY($2) OR t.verifier_id=ANY($2) OR t.trial_event_id=ANY($2)",
        set: "context=NULL,case_id=NULL",
        clean: "context IS NULL AND case_id IS NULL",
    },
    Rule {
        table: "development_review_records",
        condition: "t.event_id=ANY($2) OR t.development_id=ANY($2) OR t.trial_event_id=ANY($2) OR t.verifier_id=ANY($2)",
        set: "assessment='{}'",
        clean: "assessment='{}'",
    },
];
const ASSOCIATIONS: &[(&str, &str)] = &[
    (
        "development_evidence",
        "t.development_id=ANY($2) OR t.event_id=ANY($2)",
    ),
    (
        "development_dependencies",
        "t.object_id=ANY($2) OR t.source_id=ANY($2)",
    ),
    (
        "development_review_sources",
        "t.review_event_id=ANY($2) OR t.event_id=ANY($2)",
    ),
];
pub(super) async fn erase_development_in(
    tx: &mut Tx<'_>,
    individual: Uuid,
    ids: &[Uuid],
) -> Result<(), StoreError> {
    // The measurement owner scrubs its private copies in this same deletion
    // transaction. Keep opaque accounting associations for recovery; do not
    // create a second purge implementation or commit between the two domains.
    super::development_measurements::purge_development_trials_in(tx, individual, ids).await?;
    for rule in RULES {
        sqlx::query(&format!(
            "UPDATE {} t SET {} WHERE t.individual_id=$1 AND ({})",
            rule.table, rule.set, rule.condition
        ))
        .bind(individual)
        .bind(ids)
        .execute(&mut **tx)
        .await?;
    }
    for (table, condition) in ASSOCIATIONS {
        sqlx::query(&format!(
            "DELETE FROM {table} t WHERE t.individual_id=$1 AND ({condition})"
        ))
        .bind(individual)
        .bind(ids)
        .execute(&mut **tx)
        .await?;
    }
    verify_development_projection_in(tx, individual).await
}
pub(super) async fn development_privacy_clean_in(
    tx: &mut Tx<'_>,
    individual: Uuid,
    ids: &[Uuid],
) -> Result<bool, StoreError> {
    if super::development_measurements::reference_edges_in(tx)
        .await?
        .is_some()
    {
        // Independently verify every private trial copy before completing or
        // restoring a deletion. Retained UUIDs/hashes and original reservations
        // remain useful for replay and accounting, but cannot certify cleanup.
        let dirty: bool = sqlx::query_scalar(
            "WITH affected AS (SELECT p.id FROM development_trial_plans p \
             WHERE p.individual_id=$1 AND (p.id=ANY($2) OR p.development_id=ANY($2) \
             OR p.trial_event_id=ANY($2) OR EXISTS(SELECT 1 FROM development_trial_cases c \
             WHERE c.individual_id=$1 AND c.plan_id=p.id \
             AND (c.id=ANY($2) OR c.observation_event_id=ANY($2))) \
             OR EXISTS(SELECT 1 FROM development_trial_attempts a \
             WHERE a.individual_id=$1 AND a.plan_id=p.id \
             AND (a.id=ANY($2) OR a.result_event_id=ANY($2))))) \
             SELECT EXISTS(SELECT 1 FROM development_trial_plans p \
             WHERE p.individual_id=$1 AND p.id IN (SELECT id FROM affected) \
             AND (p.plan<>'{}' OR p.status<>'stale') \
             UNION ALL SELECT 1 FROM development_trial_cases c \
             WHERE c.individual_id=$1 AND c.plan_id IN (SELECT id FROM affected) \
             AND (c.case_data<>'{}' OR c.status<>'not_run') \
             UNION ALL SELECT 1 FROM development_trial_attempts a \
             WHERE a.individual_id=$1 AND a.plan_id IN (SELECT id FROM affected) \
             AND (a.context_json IS NOT NULL OR a.request_digest IS NOT NULL \
             OR a.input_bound IS NOT NULL OR a.output_cap IS NOT NULL \
             OR a.result IS NOT NULL OR a.state NOT IN ('unknown','not_run') \
             OR a.outcome<>'unverified'))",
        )
        .bind(individual)
        .bind(ids)
        .fetch_one(&mut **tx)
        .await?;
        if dirty {
            return Ok(false);
        }
    }
    for table in [
        "development_targets",
        "developments",
        "development_revisions",
    ] {
        let pending:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {table} WHERE individual_id=$1 AND id=ANY($2) AND NOT provenance_removed)")).bind(individual).bind(ids).fetch_one(&mut **tx).await?;
        if pending {
            return Ok(false);
        }
    }
    let expected = projected(tx, individual).await?;
    let actual: Value = sqlx::query_scalar("SELECT persona FROM individuals WHERE id=$1")
        .bind(individual)
        .fetch_one(&mut **tx)
        .await?;
    if actual != expected {
        return Ok(false);
    }
    for rule in RULES {
        let dirty:bool=sqlx::query_scalar(&format!("SELECT EXISTS(SELECT 1 FROM {} t WHERE t.individual_id=$1 AND ({}) AND NOT (({}) IS TRUE))",rule.table,rule.condition,rule.clean)).bind(individual).bind(ids).fetch_one(&mut **tx).await?;
        if dirty {
            return Ok(false);
        }
    }
    for (table, condition) in ASSOCIATIONS {
        let dirty: bool = sqlx::query_scalar(&format!(
            "SELECT EXISTS(SELECT 1 FROM {table} t WHERE t.individual_id=$1 AND ({condition}))"
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
