//! Trial-only context authorization. Correlation JSON is checked against actual
//! P04 records; ordinary persona/source rules remain the canonical baseline.
use super::{
    development, development_context, development_measurements as measurements,
    scheduling::require_lease,
    store::{Store, StoreError},
};
use crate::{
    domain::{
        cognition::{CapabilityInfo, GoalView, ModelContext, RunMode},
        development::{DevelopmentCandidate, DevelopmentTarget},
        development_context::DevelopmentContext,
        development_trials::{TrialBinding, TrialPlan, TrialSide, TrialWork},
        history::Event,
    },
    runtime::{
        capability::{Capability, LOCAL_CAPABILITIES, LocalCapability},
        development_trials::plan_case_context,
        scheduling::{RunLease, TokenAllowance},
        web::{WEB_ID, web_info},
    },
};
use chrono::{DateTime, Utc};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;

/// Only runtime/storage callers choose the phase; it is never model JSON.
#[derive(Clone, Copy)]
pub enum TrialContextPhase {
    Reservation,
    Dispatch,
    Result,
}

fn equal(a: &ModelContext, b: &ModelContext) -> Result<bool, StoreError> {
    Ok(serde_json::to_value(a)? == serde_json::to_value(b)?)
}

/// Read the actual lease, then let the single existing lease validator enforce
/// lifecycle/state/privacy/configuration. This is a projection, not a second
/// lease policy or a model-supplied owner token.
async fn actual_lease_in(tx: &mut Tx<'_>, context: &ModelContext) -> Result<RunLease, StoreError> {
    let row: Option<serde_json::Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('individual_id',individual_id,'run_id',id,'owner_id',lease_owner,'generation',lease_generation,'expires_at',lease_expires_at,'deadline',deadline,'state_version',state_version,'privacy_epoch',privacy_epoch,'config_version',config_version,'mode',mode) FROM runs WHERE individual_id=$1 AND id=$2",
    ).bind(context.individual_id).bind(context.run_id).fetch_optional(&mut **tx).await?;
    let lease: RunLease = serde_json::from_value(row.ok_or(StoreError::NotFound)?)
        .map_err(|_| StoreError::StaleState)?;
    require_lease(tx, &lease).await?;
    Ok(lease)
}

/// A prebound digest proves which body was used, not whether its permissions
/// are true. Authenticate the complete capability projection against compiled
/// registrations and the actual runtime configuration in this transaction.
/// No capability identity/version/description is supplied by the trial plan.
async fn require_capabilities_in(
    tx: &mut Tx<'_>,
    context: &ModelContext,
) -> Result<(), StoreError> {
    let runtime: serde_json::Value =
        sqlx::query_scalar("SELECT runtime_config FROM individuals WHERE id=$1")
            .bind(context.individual_id)
            .fetch_optional(&mut **tx)
            .await?
            .ok_or(StoreError::NotInitialized)?;
    let enabled: Vec<String> = serde_json::from_value(runtime["capabilities"]["enabled"].clone())?;
    let hosts: Vec<String> =
        serde_json::from_value(runtime["capabilities"]["allowed_web_hosts"].clone())?;
    let mut actual = Vec::with_capacity(LOCAL_CAPABILITIES.len() + 1);
    for id in LOCAL_CAPABILITIES {
        let mut info = LocalCapability::resolve(id)
            .ok_or(StoreError::Invalid("unregistered local capability"))?
            .info();
        info.authorized = enabled.contains(&info.id);
        actual.push(info);
    }
    let mut web = web_info();
    web.authorized = enabled.iter().any(|id| id == WEB_ID) && !hosts.is_empty();
    web.description.push_str(&format!(
        " 当前允许主机：{}。",
        if hosts.is_empty() {
            "无".into()
        } else {
            hosts.join(", ")
        }
    ));
    actual.push(web);
    if serde_json::to_value(&actual)? != serde_json::to_value(&context.capabilities)? {
        return Err(StoreError::Invalid(
            "trial capabilities do not match current registry and permissions",
        ));
    }
    Ok(())
}

/// Source identity and availability alone do not authenticate a supplied body.
/// These are projections of existing immutable events/current goals; source
/// closure and permission policy remain with the original validators.
async fn require_input_rows_in(tx: &mut Tx<'_>, context: &ModelContext) -> Result<(), StoreError> {
    let mut seen = std::collections::HashSet::new();
    for event in &context.events {
        if !seen.insert(event.id) || !context.input_manifest.contains(&event.id) {
            return Err(StoreError::StaleState);
        }
        let actual: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE individual_id=$1 AND id=$2")
                .bind(context.individual_id)
                .bind(event.id)
                .fetch_optional(&mut **tx)
                .await?;
        let actual: Event = serde_json::from_value(actual.ok_or(StoreError::NotFound)?)?;
        if serde_json::to_value(&actual)? != serde_json::to_value(event)? {
            return Err(StoreError::StaleState);
        }
    }
    seen.clear();
    for goal in &context.goals {
        if !seen.insert(goal.id) || !context.input_manifest.contains(&goal.id) {
            return Err(StoreError::StaleState);
        }
        let actual: Option<serde_json::Value> =
            sqlx::query_scalar("SELECT to_jsonb(g) FROM goals g WHERE individual_id=$1 AND id=$2")
                .bind(context.individual_id)
                .bind(goal.id)
                .fetch_optional(&mut **tx)
                .await?;
        let actual: GoalView = serde_json::from_value(actual.ok_or(StoreError::NotFound)?)?;
        if serde_json::to_value(&actual)? != serde_json::to_value(goal)? {
            return Err(StoreError::StaleState);
        }
    }
    Ok(())
}

async fn ordinary_in(
    tx: &mut Tx<'_>,
    context: &ModelContext,
    candidate: &DevelopmentCandidate,
    side: TrialSide,
) -> Result<ModelContext, StoreError> {
    require_input_rows_in(tx, context).await?;
    let mut ordinary = context.clone();
    ordinary.trial_binding = None;
    if side == TrialSide::Trial
        && let Some(pointer) = candidate.before().target.persona_pointer()
    {
        *ordinary
            .persona
            .pointer_mut(pointer)
            .ok_or(StoreError::StaleState)? = candidate.before().value.clone();
    }
    // This checks the entire accepted persona and loads real current heads,
    // strategies/catalogs and ancestry for the *actual* evaluation run/goal.
    development_context::enrich_context(tx, &mut ordinary).await?;
    if let (Some(actual), Some(observed)) = (
        ordinary.development_context.as_mut(),
        context.development_context.as_ref(),
    ) {
        // Reuse Root's existing graph-growth/superset verification below. Do not
        // promote the evaluator's own outputs into a later side's input facts.
        actual.source_ids = observed.source_ids.clone();
    }
    development_context::require_context(tx, &ordinary).await?;
    Ok(ordinary)
}

async fn projection_base_in(
    tx: &mut Tx<'_>,
    mut ordinary: ModelContext,
    candidate: &DevelopmentCandidate,
) -> Result<ModelContext, StoreError> {
    if candidate.before().target == DevelopmentTarget::TaskStrategy
        && ordinary.development_context.is_none()
    {
        let (_, birth) = development::immutable_seed(tx, ordinary.individual_id).await?;
        ordinary.input_manifest.push(birth);
        ordinary.input_manifest.sort();
        ordinary.input_manifest.dedup();
        // An old Task may have a strategy while this fresh evaluation run has
        // no ordinary head. Both sides get the same real birth metadata; no
        // accepted head is invented for the new run.
        ordinary.development_context = Some(DevelopmentContext {
            seed_event_id: birth,
            heads: vec![],
            effective_task_strategy: None,
            source_ids: ordinary.input_manifest.clone(),
        });
    }
    Ok(ordinary)
}

async fn now_in(tx: &mut Tx<'_>) -> Result<DateTime<Utc>, StoreError> {
    Ok(sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?)
}

/// Called from Root's original reserve/dispatch/result transactions. P04 owns
/// the work/attempt SQL and real ledger attestation; this checks the sole allowed
/// transformation and the paired immutable input, at every phase.
pub(super) async fn require_context_in(
    tx: &mut Tx<'_>,
    context: &ModelContext,
    phase: TrialContextPhase,
) -> Result<(), StoreError> {
    let binding = context
        .trial_binding
        .as_ref()
        .ok_or(StoreError::Invalid("trial binding absent"))?;
    let lease = actual_lease_in(tx, context).await?;
    require_capabilities_in(tx, context).await?;
    if lease.mode != RunMode::Background
        || context.goal_review.is_some()
        || context.decision_review.is_some()
    {
        return Err(StoreError::Invalid("trial context mode"));
    }
    let plan = measurements::binding_plan_in(tx, &lease, binding).await?;
    let (development, _, _) =
        development::trial_candidate_in(tx, context.individual_id, plan.development_id).await?;
    let candidate = development.candidate();
    // Required P04 readonly exposure: the same owner/query used to load the
    // baseline in context_source_ids_in, including its immutable allowance.
    let (original, _) = measurements::request_context_in(tx, &lease, binding, binding.side)
        .await?
        .ok_or(StoreError::Invalid("trial request is not prebound"))?;
    if !equal(&original, context)? {
        return Err(StoreError::IdempotencyConflict);
    }
    let required = measurements::context_source_ids_in(tx, &lease, binding).await?;
    if required
        .iter()
        .any(|id| !context.input_manifest.contains(id))
        || context.input_manifest.len() > 4096
    {
        return Err(StoreError::StaleState);
    }
    let ordinary = ordinary_in(tx, context, candidate, binding.side).await?;
    if binding.side == TrialSide::Trial {
        let (baseline, _) =
            measurements::request_context_in(tx, &lease, binding, TrialSide::Baseline)
                .await?
                .ok_or(StoreError::Invalid("trial baseline is absent"))?;
        let mut baseline = ordinary_in(tx, &baseline, candidate, TrialSide::Baseline).await?;
        // Actual time and remaining accounting allowance can change between the
        // two calls. Input facts, scopes, permissions and selected sources cannot.
        baseline.now = ordinary.now;
        baseline.execution_budget = ordinary.execution_budget.clone();
        if !equal(&baseline, &ordinary)? {
            return Err(StoreError::StaleState);
        }
    }
    let projection = projection_base_in(tx, ordinary, candidate).await?;
    let expected = plan_case_context(
        &projection,
        candidate,
        &plan,
        binding,
        &lease,
        now_in(tx).await?,
    )
    .map_err(|_| {
        StoreError::Invalid(match phase {
            TrialContextPhase::Reservation => "invalid trial reservation context",
            TrialContextPhase::Dispatch => "invalid trial dispatch context",
            TrialContextPhase::Result => "invalid trial result context",
        })
    })?;
    if !equal(context, &expected)? {
        return Err(StoreError::StaleState);
    }
    Ok(())
}

/// Same Root reservation transaction, after it creates/reloads the real row.
/// Delegates ledger linkage to P04; no nested Store transaction or extra charge.
pub(super) async fn bind_reservation_in(
    tx: &mut Tx<'_>,
    lease: &RunLease,
    context: &ModelContext,
    request_key: Uuid,
    reservation_id: Uuid,
    allowance: TokenAllowance,
) -> Result<(), StoreError> {
    let binding = context
        .trial_binding
        .as_ref()
        .ok_or(StoreError::Invalid("trial binding absent"))?;
    if request_key != binding.attempt_id {
        return Err(StoreError::IdempotencyConflict);
    }
    if allowance.input_upper_bound == 0 || !(1..=8192).contains(&allowance.output_cap) {
        return Err(StoreError::Invalid("trial allowance"));
    }
    require_lease(tx, lease).await?;
    require_context_in(tx, context, TrialContextPhase::Reservation).await?;
    measurements::bind_reservation_in(tx, lease, reservation_id, context).await
}

/// Build the first request or reload its exact prebound body. Preparation does
/// not reserve/dispatch. A trial side derives facts from the recorded baseline,
/// never from a fresh window that might contain the baseline answer.
pub(crate) async fn prepare_case_context(
    store: &Store,
    lease: &RunLease,
    work: &TrialWork,
    binding: &TrialBinding,
    capabilities: Vec<CapabilityInfo>,
) -> Result<ModelContext, StoreError> {
    let fresh = store.model_context(lease.run_id, capabilities).await?;
    let mut tx = store.pool.begin().await?;
    sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
        .execute(&mut *tx)
        .await?;
    // A heartbeat renews the persisted lease without mutating the caller's
    // value. Never project using that old expiry after the first paid call.
    let actual = actual_lease_in(&mut tx, &fresh).await?;
    require_capabilities_in(&mut tx, &fresh).await?;
    if actual.owner_id != lease.owner_id || actual.generation != lease.generation {
        return Err(StoreError::LeaseLost);
    }
    let plan: TrialPlan = measurements::binding_plan_in(&mut tx, &actual, binding).await?;
    if plan != work.plan || work.evaluation_run_id != lease.run_id {
        return Err(StoreError::StaleState);
    }
    if let Some((original, _)) =
        measurements::request_context_in(&mut tx, lease, binding, binding.side).await?
    {
        require_context_in(&mut tx, &original, TrialContextPhase::Reservation).await?;
        tx.commit().await?;
        return Ok(original);
    }
    let (development, _, _) =
        development::trial_candidate_in(&mut tx, lease.individual_id, plan.development_id).await?;
    let candidate = development.candidate();
    let mut base = match binding.side {
        TrialSide::Baseline => fresh.clone(),
        TrialSide::Trial => {
            measurements::request_context_in(&mut tx, lease, binding, TrialSide::Baseline)
                .await?
                .ok_or(StoreError::Invalid("trial baseline is absent"))?
                .0
        }
    };
    base.now = fresh.now;
    base.execution_budget = fresh.execution_budget;
    base = ordinary_in(&mut tx, &base, candidate, TrialSide::Baseline).await?;
    base.input_manifest
        .extend(measurements::context_source_ids_in(&mut tx, lease, binding).await?);
    base.input_manifest.sort();
    base.input_manifest.dedup();
    if base.input_manifest.len() > 4096 {
        return Err(StoreError::Invalid("complete trial source bound"));
    }
    if let Some(view) = base.development_context.as_mut() {
        view.source_ids = base.input_manifest.clone();
    }
    development_context::require_context(&mut tx, &base).await?;
    let base = projection_base_in(&mut tx, base, candidate).await?;
    let context = plan_case_context(
        &base,
        candidate,
        &plan,
        binding,
        &actual,
        now_in(&mut tx).await?,
    )
    .map_err(|_| StoreError::Invalid("controlled trial projection"))?;
    if serde_json::to_vec(&context)?.len() > 96 * 1024 {
        return Err(StoreError::Invalid("complete trial context bound"));
    }
    tx.commit().await?;
    Ok(context)
}
