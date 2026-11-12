//! One-snapshot persona projection and strong development provenance checks.
use super::{development, privacy_development, store::StoreError};
use crate::domain::{
    cognition::ModelContext,
    development::{DevelopmentTarget, StrategyScope, TaskStrategy},
    development_context::{CurrentDevelopmentHead, DevelopmentContext},
};
use serde_json::Value;
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
const MAX_SOURCES: i64 = 4096;

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

/// Also used at dispatch/replay: removed flags and actual FKs remain binding
/// when general derivation edges have been lost. No current-head comparison here:
/// a valid committed receipt may outlive a later legitimate personality revision.
pub(super) async fn require_sources(
    tx: &mut Tx<'_>,
    individual: Uuid,
    sources: &[Uuid],
) -> Result<(), StoreError> {
    if !development::available(tx, individual, sources).await? {
        return Err(StoreError::StaleState);
    }
    Ok(())
}

async fn read(
    tx: &mut Tx<'_>,
    individual: Uuid,
    run: Option<Uuid>,
    goal: Option<Uuid>,
    materialize_sources: bool,
) -> Result<(Value, Option<DevelopmentContext>, Vec<Uuid>), StoreError> {
    if let Some(run) = run {
        let actual: Option<Option<Uuid>> =
            sqlx::query_scalar("SELECT goal_id FROM runs WHERE individual_id=$1 AND id=$2")
                .bind(individual)
                .bind(run)
                .fetch_optional(&mut **tx)
                .await?;
        if actual != Some(goal) {
            return Err(StoreError::StaleState);
        }
    }
    privacy_development::verify_development_projection_in(tx, individual).await?;
    let (_, birth) = development::immutable_seed(tx, individual).await?;
    let persona: Value = sqlx::query_scalar("SELECT persona FROM individuals WHERE id=$1")
        .bind(individual)
        .fetch_one(&mut **tx)
        .await?;
    let rows:Vec<Head>=sqlx::query_as("SELECT id,target,scope_kind,scope_id,version,value,current_revision_id FROM development_targets WHERE individual_id=$1 AND NOT provenance_removed AND (target<>'task_strategy' OR (scope_kind='task' AND scope_id=$2) OR (scope_kind='goal' AND scope_id=$3)) ORDER BY target,scope_kind,scope_id,id LIMIT 7")
        .bind(individual).bind(run).bind(goal).fetch_all(&mut **tx).await?;
    if rows.len() > 6 {
        return Err(StoreError::Invalid("too many current development heads"));
    }
    let mut roots = vec![individual, birth];
    let mut heads = Vec::new();
    let (mut goal_strategy, mut task_strategy) = (None, None);
    for row in rows {
        let target: DevelopmentTarget = serde_json::from_value(Value::String(row.target))?;
        let scope = match (row.scope_kind.as_deref(), row.scope_id) {
            (None, None) if target != DevelopmentTarget::TaskStrategy => None,
            (Some("task"), Some(id)) if Some(id) == run => Some(StrategyScope::Task(id)),
            (Some("goal"), Some(id)) if Some(id) == goal => Some(StrategyScope::Goal(id)),
            _ => return Err(StoreError::Invalid("development context scope")),
        };
        if target == DevelopmentTarget::TaskStrategy && !row.value.is_null() {
            let strategy: TaskStrategy = serde_json::from_value(row.value)?;
            if Some(&strategy.scope) != scope.as_ref() {
                return Err(StoreError::Invalid("development context strategy scope"));
            }
            match strategy.scope {
                StrategyScope::Task(_) => task_strategy = Some(strategy),
                StrategyScope::Goal(_) => goal_strategy = Some(strategy),
            }
        }
        roots.push(row.id);
        roots.extend(row.current_revision_id);
        heads.push(CurrentDevelopmentHead {
            target_id: row.id,
            target,
            version: row.version,
            revision_id: row.current_revision_id,
            scope,
        });
    }
    let sources: Vec<Uuid> = if materialize_sources {
        sqlx::query_scalar(&format!(
        "{},closure(id) AS (SELECT unnest($2::uuid[]) UNION SELECT l.source FROM closure c JOIN links l ON l.target=c.id) SELECT id FROM closure ORDER BY id LIMIT $3",
        development::LINKS
    ))
    .bind(individual)
    .bind(roots)
    .bind(MAX_SOURCES + 1)
    .fetch_all(&mut **tx)
    .await?
    } else {
        roots.sort();
        roots.dedup();
        roots
    };
    if sources.len() as i64 > MAX_SOURCES {
        return Err(StoreError::Invalid(
            "development context provenance exceeds limit",
        ));
    }
    require_sources(tx, individual, &sources).await?;
    let view = if heads.is_empty() {
        None
    } else {
        Some(DevelopmentContext {
            seed_event_id: birth,
            heads,
            effective_task_strategy: task_strategy.or(goal_strategy),
            source_ids: sources.clone(),
        })
    };
    Ok((persona, view, sources))
}

pub(super) async fn enrich_context(
    tx: &mut Tx<'_>,
    context: &mut ModelContext,
) -> Result<(), StoreError> {
    let (persona, view, sources) = read(
        tx,
        context.individual_id,
        Some(context.run_id),
        context.current_goal_id,
        true,
    )
    .await?;
    if persona != context.persona {
        return Err(StoreError::StaleState);
    }
    context.development_context = view;
    context.development_targets = development::target_catalog_in(
        tx,
        context.individual_id,
        context.run_id,
        context.current_goal_id,
    )
    .await?;
    for target in &context.development_targets {
        context.input_manifest.extend(target.target_id);
        context.input_manifest.extend(target.head_revision_id);
        if let Some(StrategyScope::Task(id) | StrategyScope::Goal(id)) = target.scope {
            context.input_manifest.push(id);
        }
    }
    context.input_manifest.extend(sources);
    context.input_manifest.sort();
    context.input_manifest.dedup();
    if serde_json::to_vec(context)?.len() > 96 * 1024 {
        return Err(StoreError::Invalid(
            "complete development context exceeds limit",
        ));
    }
    Ok(())
}

pub(super) async fn require_context(
    tx: &mut Tx<'_>,
    context: &ModelContext,
) -> Result<(), StoreError> {
    require_context_at_phase(tx, context, false).await
}

pub(super) async fn require_author_context(
    tx: &mut Tx<'_>,
    context: &ModelContext,
) -> Result<(), StoreError> {
    require_context_at_phase(tx, context, true).await
}

async fn require_context_at_phase(
    tx: &mut Tx<'_>,
    context: &ModelContext,
    author_reservation: bool,
) -> Result<(), StoreError> {
    let catalog = development::target_catalog_in(
        tx,
        context.individual_id,
        context.run_id,
        context.current_goal_id,
    )
    .await?;
    if catalog != context.development_targets {
        return Err(StoreError::StaleState);
    }
    let (persona, mut expected, sources) = read(
        tx,
        context.individual_id,
        Some(context.run_id),
        context.current_goal_id,
        author_reservation,
    )
    .await?;
    if !author_reservation
        && let (Some(actual), Some(observed)) =
            (expected.as_mut(), context.development_context.as_ref())
    {
        // Reserve adds this request's manifest to its run. A scoped strategy
        // refers to that run, so later ancestors can legitimately grow. The
        // original observed list stays bound by the settled author digest;
        // recheck all actual graph edges without demanding that list change.
        let mut canonical = observed.source_ids.clone();
        canonical.sort();
        canonical.dedup();
        if canonical != observed.source_ids
            || canonical.len() as i64 > MAX_SOURCES
            || sources.iter().any(|id| !canonical.contains(id))
            || canonical
                .iter()
                .any(|id| !context.input_manifest.contains(id))
        {
            return Err(StoreError::StaleState);
        }
        actual.source_ids = canonical;
    }
    if persona != context.persona
        || expected != context.development_context
        || sources
            .iter()
            .any(|id| !context.input_manifest.contains(id))
    {
        return Err(StoreError::StaleState);
    }
    require_sources(tx, context.individual_id, &context.input_manifest).await?;
    Ok(())
}

pub(super) async fn require_public_persona(
    tx: &mut Tx<'_>,
    individual: Uuid,
) -> Result<(), StoreError> {
    // The public projection need not materialize the model's entire source list.
    // Validate its full graph without imposing the model packet's 4096-ID limit.
    privacy_development::verify_development_projection_in(tx, individual).await?;
    let (_, birth) = development::immutable_seed(tx, individual).await?;
    let mut heads: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM development_targets WHERE individual_id=$1 AND target<>'task_strategy' AND NOT provenance_removed ORDER BY target,id LIMIT 5",
    )
    .bind(individual)
    .fetch_all(&mut **tx)
    .await?;
    if heads.len() > 4 {
        return Err(StoreError::Invalid("too many current persona heads"));
    }
    heads.extend([individual, birth]);
    require_sources(tx, individual, &heads).await
}
