//! Trusted deterministic measurement persistence. No model/HTTP insertion path.
use super::{
    api::existing,
    development::{self, DevelopmentCommand, transition_in_tx, trial_candidate_in},
    scheduling::require_lease,
    store::{
        Store, StoreError, advance, digest, edge, insert_event, object, outbox, replay,
        save_receipt, state,
    },
};
use crate::{
    domain::{
        cognition::ModelContext, development::*, development_trials::*, history::ExpectedState,
    },
    runtime::{
        model::{ModelResult, ProviderMode},
        scheduling::{RunLease, TokenAllowance},
    },
};
use chrono::{DateTime, Utc};
use serde_json::{Value, json};
use sqlx::{Postgres, Transaction};
use uuid::Uuid;
type Tx<'a> = Transaction<'a, Postgres>;
// Private query shapes, never request/authorization DTOs.
type WorkIdentityRow = (
    Uuid,
    Uuid,
    String,
    Option<Uuid>,
    Option<i64>,
    Option<i64>,
    String,
);
type AttemptStatusRow = (
    Uuid,
    Uuid,
    String,
    Option<Uuid>,
    Option<i64>,
    String,
    Option<Uuid>,
    Option<Uuid>,
    String,
    Option<String>,
);
type CaseResultRow = (Uuid, String, String, Option<Value>, Option<String>);
type FinalAttemptRow = (Uuid, Value, Option<Uuid>, Option<String>, String);
type RecoveryRequestRow = (Value, String, i64, i32, Uuid, String);
type SavedRequestRow = (
    Option<String>,
    Uuid,
    Option<Value>,
    Option<i64>,
    Option<i32>,
    Option<Uuid>,
    Option<i64>,
);

const ROUTE: &str = "/internal/development-trials";

/// Actual FK edges, source→target, shared with the deletion controller.
const TRIAL_REFERENCES: &str = r#"
 SELECT development_id,id FROM development_trial_plans WHERE individual_id=$1
 UNION SELECT trial_event_id,id FROM development_trial_plans WHERE individual_id=$1
 UNION SELECT evaluator_id,id FROM development_trial_plans WHERE individual_id=$1
 UNION SELECT review_event_id,id FROM development_trial_plans WHERE individual_id=$1 AND review_event_id IS NOT NULL
 UNION SELECT unnest(input_manifest),id FROM development_trial_plans WHERE individual_id=$1
 UNION SELECT plan_id,id FROM development_trial_cases WHERE individual_id=$1
 UNION SELECT id,plan_id FROM development_trial_cases WHERE individual_id=$1
 UNION SELECT event_id,case_id FROM development_trial_case_sources WHERE individual_id=$1
 UNION SELECT evaluation_run_id,id FROM development_trial_cases WHERE individual_id=$1 AND evaluation_run_id IS NOT NULL
 UNION SELECT id,evaluation_run_id FROM development_trial_cases WHERE individual_id=$1 AND evaluation_run_id IS NOT NULL
 UNION SELECT id,observation_event_id FROM development_trial_cases WHERE individual_id=$1 AND observation_event_id IS NOT NULL
 UNION SELECT observation_event_id,id FROM development_trial_cases WHERE individual_id=$1 AND observation_event_id IS NOT NULL
 UNION SELECT case_id,id FROM development_trial_attempts WHERE individual_id=$1
 UNION SELECT id,case_id FROM development_trial_attempts WHERE individual_id=$1
 UNION SELECT evaluation_run_id,id FROM development_trial_attempts WHERE individual_id=$1 AND evaluation_run_id IS NOT NULL
 UNION SELECT unnest(request_sources),id FROM development_trial_attempts WHERE individual_id=$1
 UNION SELECT id,result_event_id FROM development_trial_attempts WHERE individual_id=$1 AND result_event_id IS NOT NULL
 UNION SELECT result_event_id,id FROM development_trial_attempts WHERE individual_id=$1 AND result_event_id IS NOT NULL"#;
pub(super) async fn reference_edges_in(
    tx: &mut Tx<'_>,
) -> Result<Option<&'static str>, StoreError> {
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM information_schema.tables WHERE table_schema='public' AND table_name=ANY($1)")
        .bind(&["development_measurement_programs","development_trial_plans","development_trial_cases","development_trial_attempts","development_trial_case_sources"][..]).fetch_one(&mut **tx).await?;
    match count {
        0 => Ok(None),
        5 => Ok(Some(TRIAL_REFERENCES)),
        _ => Err(StoreError::Invalid("incomplete development trial schema")),
    }
}
/// Uses actual owned work rows, never a caller-supplied context marker.
pub(super) async fn is_evaluation_run_in(
    tx: &mut Tx<'_>,
    individual: Uuid,
    run: Uuid,
) -> Result<bool, StoreError> {
    if reference_edges_in(tx).await?.is_none() {
        return Ok(false);
    }
    sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM development_trial_cases WHERE individual_id=$1 AND evaluation_run_id=$2)")
        .bind(individual).bind(run).fetch_one(&mut **tx).await.map_err(StoreError::from)
}

fn policy() -> Value {
    json!({"program":"json_comparison_v1","version":MEASUREMENT_VERSION,"max_cases":MAX_TRIAL_CASES,"targets":["persona.traits","persona.values","persona.expression","persona.cognitive_preferences","task_strategy"],"permits_tools":false,"certifies_semantics":false})
}
async fn commit(store: &Store, tx: Tx<'_>, generation: u64) -> Result<(), StoreError> {
    store.require_privacy_snapshot(generation)?;
    tx.commit().await?;
    store.require_privacy_snapshot(generation)
}
async fn dependencies(
    tx: &mut Tx<'_>,
    individual: Uuid,
    target: Uuid,
    sources: &[Uuid],
) -> Result<(), StoreError> {
    for source in sources {
        if *source != target {
            sqlx::query("INSERT INTO development_dependencies(individual_id,object_id,source_id) VALUES($1,$2,$3) ON CONFLICT DO NOTHING")
                .bind(individual).bind(target).bind(source).execute(&mut **tx).await?;
            edge(tx, individual, *source, target).await?;
        }
    }
    Ok(())
}
async fn program_in(tx: &mut Tx<'_>, individual: Uuid, evaluator: Uuid) -> Result<(), StoreError> {
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    if development::verifier(tx, individual, evaluator, now).await? != "measurement" {
        return Err(StoreError::Invalid(
            "development evaluator is not a program",
        ));
    }
    let actual:Option<(String,String,Value,String,bool)>=sqlx::query_as("SELECT p.program,p.version,p.policy,p.policy_digest,p.registration_event_id=v.registration_event_id FROM development_measurement_programs p JOIN development_verifiers v ON (v.individual_id,v.id)=(p.individual_id,p.verifier_id) WHERE p.individual_id=$1 AND p.verifier_id=$2")
        .bind(individual).bind(evaluator).fetch_optional(&mut **tx).await?;
    if actual
        != Some((
            "json_comparison_v1".into(),
            MEASUREMENT_VERSION.into(),
            policy(),
            digest(&policy())?,
            true,
        ))
    {
        return Err(StoreError::Invalid("untrusted development program policy"));
    }
    Ok(())
}
pub(super) async fn plan_in(
    tx: &mut Tx<'_>,
    individual: Uuid,
    id: Uuid,
    live: bool,
) -> Result<TrialPlan, StoreError> {
    let row:Option<(Value,String,Vec<Uuid>,i64,i64,String)>=sqlx::query_as("SELECT plan,cases_digest,input_manifest,privacy_epoch,config_version,status FROM development_trial_plans WHERE individual_id=$1 AND id=$2")
        .bind(individual).bind(id).fetch_optional(&mut **tx).await?;
    let (value, hash, sources, epoch, config, status) = row.ok_or(StoreError::NotFound)?;
    if !development::available(tx, individual, &[id]).await? {
        return Err(StoreError::NotFound);
    }
    let plan: TrialPlan = serde_json::from_value(value)?;
    if plan.id != id
        || plan.individual_id != individual
        || plan.cases_digest != hash
        || digest(&plan.cases)? != hash
    {
        return Err(StoreError::Invalid("stored development plan binding"));
    }
    program_in(tx, individual, plan.evaluator_id).await?;
    if !development::available(tx, individual, &sources).await? {
        return Err(StoreError::StaleState);
    }
    let cases:Vec<Value>=sqlx::query_scalar("SELECT case_data FROM development_trial_cases WHERE individual_id=$1 AND plan_id=$2 ORDER BY ordinal")
        .bind(individual).bind(id).fetch_all(&mut **tx).await?;
    if serde_json::to_value(&plan.cases)? != Value::Array(cases) {
        return Err(StoreError::Invalid("stored development case set"));
    }
    for case in &plan.cases {
        let actual:Vec<Uuid>=sqlx::query_scalar("SELECT event_id FROM development_trial_case_sources WHERE individual_id=$1 AND case_id=$2 ORDER BY event_id")
            .bind(individual).bind(case.id).fetch_all(&mut **tx).await?;
        let mut expected = case.source_event_ids.clone();
        expected.sort();
        if actual != expected || !development::available(tx, individual, &actual).await? {
            return Err(StoreError::StaleState);
        }
    }
    if live {
        let (actual_epoch, actual_config, lifecycle): (i64, i64, String) = sqlx::query_as(
            "SELECT privacy_epoch,config_version,lifecycle FROM individuals WHERE id=$1",
        )
        .bind(individual)
        .fetch_one(&mut **tx)
        .await?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut **tx)
            .await?;
        let (development, version, target) =
            trial_candidate_in(tx, individual, plan.development_id).await?;
        if status != "active"
            || now >= plan.deadline
            || epoch != actual_epoch
            || config != actual_config
            || lifecycle != "active"
            || development.status() != DevelopmentStatus::Trial
            || version != plan.development_version
            || target != plan.target
            || development.candidate().draft() != &plan.draft
            || development.candidate().proposer_id() != plan.proposer_id
            || target.scope != plan.original_scope
        {
            return Err(StoreError::StaleState);
        }
    }
    Ok(plan)
}

impl Store {
    pub async fn register_development_measurement(
        &self,
        key: &str,
        program: MeasurementProgram,
    ) -> Result<Uuid, StoreError> {
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        let hash = digest(&json!({"program":program,"policy":policy()}))?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if let Some(id) = replay::<Uuid>(
            &mut tx,
            "system",
            "/internal/development-measurements/register",
            key,
            &hash,
        )
        .await?
        {
            program_in(&mut tx, current.id, id).await?;
            self.require_privacy_snapshot(generation)?;
            return Ok(id);
        }
        if !matches!(
            current.lifecycle,
            crate::domain::Lifecycle::Active | crate::domain::Lifecycle::Paused
        ) {
            return Err(StoreError::Inactive);
        }
        let old:Option<Uuid>=sqlx::query_scalar("SELECT verifier_id FROM development_measurement_programs WHERE individual_id=$1 AND program='json_comparison_v1' AND version=$2")
            .bind(current.id).bind(program.version()).fetch_optional(&mut *tx).await?;
        let id = if let Some(id) = old {
            program_in(&mut tx, current.id, id).await?;
            id
        } else {
            let id = Uuid::new_v4();
            let event = Uuid::new_v4();
            object(&mut tx, current.id, id, "development_verifier").await?;
            insert_event(
                &mut tx,
                current.id,
                event,
                "development.verifier.registered",
                "direct_observation",
                "system",
                json!({"verifier_id":id,"kind":"measurement"}),
                id,
                Some(key),
            )
            .await?;
            sqlx::query("INSERT INTO development_verifiers(individual_id,id,kind,registration_event_id) VALUES($1,$2,'measurement',$3)")
                .bind(current.id).bind(id).bind(event).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO development_measurement_programs(individual_id,verifier_id,program,version,registration_event_id,policy,policy_digest) VALUES($1,$2,'json_comparison_v1',$3,$4,$5,$6)")
                .bind(current.id).bind(id).bind(program.version()).bind(event).bind(policy()).bind(digest(&policy())?).execute(&mut *tx).await?;
            let (_, birth) = development::immutable_seed(&mut tx, current.id).await?;
            dependencies(&mut tx, current.id, id, &[event, birth]).await?;
            id
        };
        save_receipt(
            &mut tx,
            "system",
            "/internal/development-measurements/register",
            key,
            &hash,
            &id,
        )
        .await?;
        commit(self, tx, generation).await?;
        Ok(id)
    }

    #[allow(clippy::too_many_arguments)]
    pub async fn freeze_development_trial_plan(
        &self,
        key: &str,
        expected: ExpectedState,
        input_manifest: &[Uuid],
        development_id: Uuid,
        expected_development_version: i64,
        evaluator_id: Uuid,
        route: ValidationRoute,
        draft: &TrialPlanDraft,
    ) -> Result<TrialPlan, StoreError> {
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        let hash = digest(
            &json!({"development_id":development_id,"version":expected_development_version,"evaluator_id":evaluator_id,"route":route,"draft":draft,"manifest":input_manifest}),
        )?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if let Some(old) = replay::<TrialPlan>(&mut tx, "system", ROUTE, key, &hash).await? {
            let actual = plan_in(&mut tx, current.id, old.id, false).await?;
            if actual != old {
                return Err(StoreError::StaleState);
            }
            self.require_privacy_snapshot(generation)?;
            return Ok(old);
        }
        if current.expected().version != expected.version
            || current.privacy_epoch != expected.privacy_epoch
        {
            return Err(StoreError::StaleState);
        }
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        draft
            .validate(now)
            .map_err(|_| StoreError::Invalid("development trial plan fields"))?;
        program_in(&mut tx, current.id, evaluator_id).await?;
        let (candidate, version, target) =
            trial_candidate_in(&mut tx, current.id, development_id).await?;
        if version != expected_development_version
            || candidate.status() != DevelopmentStatus::Proposed
            || draft.deadline > candidate.candidate().draft().deadline
            || evaluator_id == candidate.candidate().proposer_id()
        {
            return Err(StoreError::StaleState);
        }
        for case in &draft.cases {
            if !case
                .source_event_ids
                .iter()
                .all(|id| input_manifest.contains(id))
                || !development::available(&mut tx, current.id, &case.source_event_ids).await?
            {
                return Err(StoreError::StaleState);
            }
            let events: i64 = sqlx::query_scalar(
                "SELECT count(*) FROM events WHERE individual_id=$1 AND id=ANY($2)",
            )
            .bind(current.id)
            .bind(&case.source_event_ids)
            .fetch_one(&mut *tx)
            .await?;
            if events != case.source_event_ids.len() as i64 {
                return Err(StoreError::Invalid("trial case requires actual sources"));
            }
        }
        let mut trial_sources = candidate.candidate().draft().evidence_ids.clone();
        trial_sources.extend(&candidate.candidate().draft().counterevidence_ids);
        if let ValidationRoute::SignificantEvent { event_id } = &route {
            trial_sources.push(*event_id);
        }
        trial_sources.sort();
        trial_sources.dedup();
        let started = transition_in_tx(
            &mut tx,
            current.id,
            input_manifest,
            development_id,
            version,
            &DevelopmentCommand::StartTrial {
                route,
                source_event_ids: trial_sources,
            },
            Some(key),
        )
        .await?;
        let id = Uuid::new_v4();
        let hash_cases = digest(&draft.cases)?;
        let plan = TrialPlan {
            id,
            individual_id: current.id,
            development_id,
            development_version: started.development_version,
            draft: candidate.candidate().draft().clone(),
            proposer_id: candidate.candidate().proposer_id(),
            trial_event_id: started.event_id,
            original_scope: target.scope.clone(),
            target,
            evaluator_id,
            criterion_version: MEASUREMENT_VERSION.into(),
            cases_digest: hash_cases.clone(),
            cases: draft.cases.clone(),
            deadline: draft.deadline,
        };
        insert_event(&mut tx,current.id,id,"development.trial.plan.frozen","inference","system",json!({"development_id":development_id,"trial_event_id":started.event_id,"cases_digest":hash_cases,"criterion_version":MEASUREMENT_VERSION,"simulation_not_experience":true}),id,Some(key)).await?;
        sqlx::query("INSERT INTO development_trial_plans(individual_id,id,development_id,development_version,trial_event_id,evaluator_id,plan,input_manifest,cases_digest,privacy_epoch,config_version,deadline) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12)")
            .bind(current.id).bind(id).bind(development_id).bind(started.development_version).bind(started.event_id).bind(evaluator_id).bind(serde_json::to_value(&plan)?).bind(input_manifest).bind(hash_cases).bind(current.privacy_epoch).bind(current.config_version).bind(draft.deadline).execute(&mut *tx).await?;
        let mut refs = input_manifest.to_vec();
        refs.extend([development_id, started.event_id, evaluator_id]);
        dependencies(&mut tx, current.id, id, &refs).await?;
        for (ordinal, case) in plan.cases.iter().enumerate() {
            insert_event(&mut tx,current.id,case.id,"development.trial.case.frozen","inference","system",json!({"plan_id":id,"case_id":case.id,"text":case.prompt,"role":case.role,"criterion_digest":digest(&case.criterion)?,"simulation_not_experience":true}),id,None).await?;
            sqlx::query("INSERT INTO development_trial_cases(individual_id,id,plan_id,ordinal,case_data) VALUES($1,$2,$3,$4,$5)")
                .bind(current.id).bind(case.id).bind(id).bind(ordinal as i16).bind(serde_json::to_value(case)?).execute(&mut *tx).await?;
            let mut refs = case.source_event_ids.clone();
            refs.push(id);
            dependencies(&mut tx, current.id, case.id, &refs).await?;
            for source in &case.source_event_ids {
                sqlx::query("INSERT INTO development_trial_case_sources(individual_id,case_id,event_id) VALUES($1,$2,$3)").bind(current.id).bind(case.id).bind(source).execute(&mut *tx).await?;
            }
            for side in [TrialSide::Baseline, TrialSide::Trial] {
                let attempt = Uuid::new_v4();
                insert_event(
                    &mut tx,
                    current.id,
                    attempt,
                    "development.trial.attempt.created",
                    "inference",
                    "system",
                    json!({"plan_id":id,"case_id":case.id,"side":side,"not_executed":true}),
                    id,
                    None,
                )
                .await?;
                sqlx::query("INSERT INTO development_trial_attempts(individual_id,id,plan_id,case_id,side) VALUES($1,$2,$3,$4,$5)").bind(current.id).bind(attempt).bind(id).bind(case.id).bind(side.as_str()).execute(&mut *tx).await?;
                dependencies(&mut tx, current.id, attempt, &[case.id]).await?;
            }
        }
        let next = current
            .state_version
            .checked_add(1)
            .ok_or(StoreError::Invalid("state version exhausted"))?;
        outbox(
            &mut tx,
            current.id,
            "development.changed",
            development_id,
            next,
        )
        .await?;
        advance(&mut tx, current.id, next).await?;
        save_receipt(&mut tx, "system", ROUTE, key, &hash, &plan).await?;
        commit(self, tx, generation).await?;
        Ok(plan)
    }
}

impl Store {
    pub async fn pending_development_trial_cases(
        &self,
        limit: u32,
    ) -> Result<Vec<PendingTrialCase>, StoreError> {
        if !(1..=100).contains(&limit) {
            return Err(StoreError::Invalid("trial pending limit"));
        }
        let generation = self.privacy_generation();
        self.require_privacy_ready()?;
        let mut tx = self.pool.begin().await?;
        sqlx::query("SET TRANSACTION ISOLATION LEVEL REPEATABLE READ, READ ONLY")
            .execute(&mut *tx)
            .await?;
        if reference_edges_in(&mut tx).await?.is_none() {
            return Ok(vec![]);
        }
        let individual = existing(&mut tx).await?;
        let rows:Vec<(Uuid,Uuid,DateTime<Utc>)>=sqlx::query_as("SELECT c.plan_id,c.id,p.deadline FROM development_trial_cases c JOIN development_trial_plans p ON (p.individual_id,p.id)=(c.individual_id,c.plan_id) WHERE c.individual_id=$1 AND c.status='pending' AND p.status='active' AND p.deadline>clock_timestamp() ORDER BY p.deadline,p.id,c.ordinal LIMIT 100")
            .bind(individual).fetch_all(&mut *tx).await?;
        let mut pending = vec![];
        for (plan_id, case_id, deadline) in rows {
            match plan_in(&mut tx, individual, plan_id, true).await {
                Ok(_) => pending.push(PendingTrialCase {
                    plan_id,
                    case_id,
                    deadline,
                }),
                Err(StoreError::StaleState | StoreError::NotFound) => continue,
                Err(e) => return Err(e),
            }
            if pending.len() == limit as usize {
                break;
            }
        }
        tx.commit().await?;
        self.require_privacy_snapshot(generation)?;
        Ok(pending)
    }

    pub async fn enqueue_development_trial_case(
        &self,
        key: &str,
        plan_id: Uuid,
        case_id: Uuid,
    ) -> Result<Uuid, StoreError> {
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let hash = digest(&json!({"plan_id":plan_id,"case_id":case_id}))?;
        if let Some(run) = replay::<Uuid>(
            &mut tx,
            "system",
            "/internal/development-trials/enqueue",
            key,
            &hash,
        )
        .await?
        {
            plan_in(&mut tx, current.id, plan_id, false).await?;
            let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM development_trial_cases WHERE individual_id=$1 AND id=$2 AND plan_id=$3 AND evaluation_run_id=$4)")
                .bind(current.id).bind(case_id).bind(plan_id).bind(run).fetch_one(&mut *tx).await?;
            if !bound {
                return Err(StoreError::StaleState);
            }
            self.require_privacy_snapshot(generation)?;
            return Ok(run);
        }
        let plan = plan_in(&mut tx, current.id, plan_id, true).await?;
        let case = plan
            .cases
            .iter()
            .find(|c| c.id == case_id)
            .ok_or(StoreError::NotFound)?;
        let (paused, frozen): (bool, bool) =
            sqlx::query_as("SELECT autonomy_paused,budget_frozen FROM individuals WHERE id=$1")
                .bind(current.id)
                .fetch_one(&mut *tx)
                .await?;
        if paused || frozen || current.runtime_config["autonomy"]["enabled"] != true {
            return Err(StoreError::Inactive);
        }
        let goal = match plan.original_scope {
            Some(StrategyScope::Goal(id)) => Some(id),
            Some(StrategyScope::Task(id)) => {
                sqlx::query_scalar::<_, Option<Uuid>>(
                    "SELECT goal_id FROM runs WHERE individual_id=$1 AND id=$2",
                )
                .bind(current.id)
                .bind(id)
                .fetch_one(&mut *tx)
                .await?
            }
            None => None,
        };
        let busy:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM runs WHERE individual_id=$1 AND (status='running' OR (status='queued' AND mode='foreground') OR (status IN ('queued','running') AND goal_id=$2)))")
            .bind(current.id).bind(goal).fetch_one(&mut *tx).await?;
        if busy {
            return Err(StoreError::Invalid("trial waits for normal idle admission"));
        }
        let status:String=sqlx::query_scalar("SELECT status FROM development_trial_cases WHERE individual_id=$1 AND id=$2 AND plan_id=$3")
            .bind(current.id).bind(case_id).bind(plan_id).fetch_one(&mut *tx).await?;
        if status != "pending" {
            return Err(StoreError::StaleState);
        }
        let run = Uuid::new_v4();
        object(&mut tx, current.id, run, "run").await?;
        let mut sources = case.source_event_ids.clone();
        sources.extend([
            current.id,
            case_id,
            plan_id,
            plan.trial_event_id,
            plan.development_id,
        ]);
        sources.sort();
        sources.dedup();
        sqlx::query("INSERT INTO runs(individual_id,id,trigger_event_id,goal_id,mode,state_version,privacy_epoch,config_version,input_manifest) VALUES($1,$2,$3,$4,'background',$5,$6,$7,$8)")
            .bind(current.id).bind(run).bind(case_id).bind(goal).bind(current.state_version).bind(current.privacy_epoch).bind(current.config_version).bind(&sources).execute(&mut *tx).await?;
        sqlx::query("UPDATE development_trial_cases SET evaluation_run_id=$3,status='queued' WHERE individual_id=$1 AND id=$2")
            .bind(current.id).bind(case_id).bind(run).execute(&mut *tx).await?;
        sqlx::query("UPDATE development_trial_attempts SET evaluation_run_id=$3 WHERE individual_id=$1 AND case_id=$2")
            .bind(current.id).bind(case_id).bind(run).execute(&mut *tx).await?;
        dependencies(&mut tx, current.id, run, &sources).await?;
        outbox(
            &mut tx,
            current.id,
            "operation.updated",
            run,
            current.state_version,
        )
        .await?;
        save_receipt(
            &mut tx,
            "system",
            "/internal/development-trials/enqueue",
            key,
            &hash,
            &run,
        )
        .await?;
        commit(self, tx, generation).await?;
        Ok(run)
    }

    pub async fn claim_development_trial_work(
        &self,
        lease: &RunLease,
    ) -> Result<Option<TrialWork>, StoreError> {
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        if reference_edges_in(&mut tx).await?.is_none() {
            return Ok(None);
        }
        let row:Option<(Uuid,Uuid,Option<i64>)>=sqlx::query_as("SELECT plan_id,id,lease_generation FROM development_trial_cases WHERE individual_id=$1 AND evaluation_run_id=$2 AND status IN ('queued','claimed')")
            .bind(lease.individual_id).bind(lease.run_id).fetch_optional(&mut *tx).await?;
        let Some((plan_id, case_id, old_generation)) = row else {
            return Ok(None);
        };
        let plan = plan_in(&mut tx, lease.individual_id, plan_id, true).await?;
        if old_generation.is_some_and(|g| g != lease.generation) {
            let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM development_trial_attempts WHERE individual_id=$1 AND case_id=$2 AND request_digest IS NOT NULL)")
                .bind(lease.individual_id).bind(case_id).fetch_one(&mut *tx).await?;
            if bound {
                // The original scheduler has reclaimed this run and closed its
                // old accounting. Keep the old immutable request and evidence;
                // this lease may only finish the interrupted case, never retry.
                let outstanding:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM usage_reservations WHERE individual_id=$1 AND run_id=$2 AND status<>'settled')")
                    .bind(lease.individual_id).bind(lease.run_id).fetch_one(&mut *tx).await?;
                if outstanding {
                    return Err(StoreError::Invalid("trial accounting recovery incomplete"));
                }
                recover_attempts_in(&mut tx, &plan).await?;
                sqlx::query("UPDATE development_trial_attempts a SET state='not_run' WHERE a.individual_id=$1 AND a.case_id=$2 AND a.state IN ('pending','reserved') AND NOT EXISTS(SELECT 1 FROM usage_reservations u WHERE u.individual_id=a.individual_id AND u.run_id=a.evaluation_run_id AND u.request_key=a.id AND u.dispatched_at IS NOT NULL)")
                    .bind(lease.individual_id).bind(case_id).execute(&mut *tx).await?;
            }
        }
        sqlx::query("UPDATE development_trial_cases SET status='claimed',lease_generation=$3 WHERE individual_id=$1 AND id=$2")
            .bind(lease.individual_id).bind(case_id).bind(lease.generation).execute(&mut *tx).await?;
        sqlx::query("UPDATE development_trial_attempts SET lease_generation=$3 WHERE individual_id=$1 AND case_id=$2 AND request_digest IS NULL AND state='pending'")
            .bind(lease.individual_id).bind(case_id).bind(lease.generation).execute(&mut *tx).await?;
        let ids:Vec<(Uuid,String)>=sqlx::query_as("SELECT id,side FROM development_trial_attempts WHERE individual_id=$1 AND case_id=$2 ORDER BY side")
            .bind(lease.individual_id).bind(case_id).fetch_all(&mut *tx).await?;
        let binding = |side: TrialSide| -> Result<TrialBinding, StoreError> {
            let id = ids
                .iter()
                .find(|(_, s)| s == side.as_str())
                .ok_or(StoreError::Invalid("missing planned trial side"))?
                .0;
            Ok(TrialBinding {
                plan_id,
                case_id,
                side,
                attempt_id: id,
                evaluation_run_id: lease.run_id,
                lease_generation: lease.generation,
            })
        };
        let case = plan
            .cases
            .iter()
            .find(|c| c.id == case_id)
            .ok_or(StoreError::NotFound)?
            .clone();
        let work = TrialWork {
            baseline: binding(TrialSide::Baseline)?,
            trial: binding(TrialSide::Trial)?,
            plan,
            case,
            evaluation_run_id: lease.run_id,
        };
        commit(self, tx, generation).await?;
        Ok(Some(work))
    }
}

/// This proof validates work, lease and source identity; P02 separately validates
/// the only allowed base/trial context transformation before reserve/dispatch.
pub(super) async fn binding_plan_in(
    tx: &mut Tx<'_>,
    lease: &RunLease,
    binding: &TrialBinding,
) -> Result<TrialPlan, StoreError> {
    require_lease(tx, lease).await?;
    let actual:Option<WorkIdentityRow>=sqlx::query_as("SELECT a.plan_id,a.case_id,a.side,a.evaluation_run_id,a.lease_generation,c.lease_generation,c.status FROM development_trial_attempts a JOIN development_trial_cases c ON (c.individual_id,c.id)=(a.individual_id,a.case_id) WHERE a.individual_id=$1 AND a.id=$2")
        .bind(lease.individual_id).bind(binding.attempt_id).fetch_optional(&mut **tx).await?;
    let Some((plan, case, side, run, generation, case_generation, status)) = actual else {
        return Err(StoreError::NotFound);
    };
    if plan != binding.plan_id
        || case != binding.case_id
        || side != binding.side.as_str()
        || run != Some(lease.run_id)
        || generation != Some(lease.generation)
        || case_generation != generation
        || binding.evaluation_run_id != lease.run_id
        || binding.lease_generation != lease.generation
        || !matches!(status.as_str(), "claimed" | "finished" | "unknown")
    {
        return Err(StoreError::StaleState);
    }
    plan_in(tx, lease.individual_id, plan, true).await
}

pub(super) async fn context_source_ids_in(
    tx: &mut Tx<'_>,
    lease: &RunLease,
    binding: &TrialBinding,
) -> Result<Vec<Uuid>, StoreError> {
    binding_plan_in(tx, lease, binding).await?;
    let attempts: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM development_trial_attempts WHERE individual_id=$1 AND case_id=$2 ORDER BY side")
        .bind(lease.individual_id).bind(binding.case_id).fetch_all(&mut **tx).await?;
    if attempts.len() != 2 {
        return Err(StoreError::Invalid("missing planned trial sides"));
    }
    let mut roots = vec![
        lease.individual_id,
        lease.run_id,
        binding.plan_id,
        binding.case_id,
    ];
    roots.extend(attempts);
    if !development::available(tx, lease.individual_id, &roots).await? {
        return Err(StoreError::StaleState);
    }
    let original:Option<Value>=sqlx::query_scalar("SELECT context_json FROM development_trial_attempts WHERE individual_id=$1 AND case_id=$2 AND side='baseline'")
        .bind(lease.individual_id).bind(binding.case_id).fetch_one(&mut **tx).await?;
    if let Some(original) = original {
        // The paired comparison observes one frozen input. Own dispatch/results
        // can add causal ancestors, which are rechecked but not promoted to facts.
        let original: ModelContext = serde_json::from_value(original)?;
        let mut sources = original.input_manifest.clone();
        sources.sort();
        sources.dedup();
        if sources.len() > 4096
            || sources != original.input_manifest
            || roots.iter().any(|id| !sources.contains(id))
            || !development::available(tx, lease.individual_id, &sources).await?
        {
            return Err(StoreError::StaleState);
        }
        return Ok(sources);
    }
    let end = development::LINKS
        .rfind(')')
        .expect("static development CTE");
    let sources:Vec<Uuid>=sqlx::query_scalar(&format!("{} UNION SELECT target,source FROM ({TRIAL_REFERENCES}) actual_trial(source,target)), closure(id) AS (SELECT unnest($2::uuid[]) UNION SELECT l.source FROM closure c JOIN links l ON l.target=c.id) SELECT id FROM closure ORDER BY id LIMIT 4097",&development::LINKS[..end]))
        .bind(lease.individual_id).bind(roots).fetch_all(&mut **tx).await?;
    if sources.len() > 4096 || !development::available(tx, lease.individual_id, &sources).await? {
        return Err(StoreError::Invalid("complete trial source bound"));
    }
    Ok(sources)
}

async fn check_context_in(
    tx: &mut Tx<'_>,
    lease: &RunLease,
    binding: &TrialBinding,
    context: &ModelContext,
) -> Result<(), StoreError> {
    if context.trial_binding.as_ref() != Some(binding)
        || context.individual_id != lease.individual_id
        || context.run_id != lease.run_id
        || context.state_version != lease.state_version
        || context.privacy_epoch != lease.privacy_epoch
        || context.config_version != lease.config_version
        || context.mode != lease.mode
    {
        return Err(StoreError::StaleState);
    }
    let sources = context_source_ids_in(tx, lease, binding).await?;
    if sources.iter().any(|s| !context.input_manifest.contains(s))
        || serde_json::to_vec(context)?.len() > 96 * 1024
        || !development::available(tx, lease.individual_id, &context.input_manifest).await?
    {
        return Err(StoreError::StaleState);
    }
    Ok(())
}

/// The original reservation transaction calls this only after P02 validates the
/// controlled context. No transaction/receipt/charging is introduced here.
pub(super) async fn bind_reservation_in(
    tx: &mut Tx<'_>,
    lease: &RunLease,
    reservation: Uuid,
    context: &ModelContext,
) -> Result<(), StoreError> {
    let binding = context
        .trial_binding
        .as_ref()
        .ok_or(StoreError::Invalid("trial binding absent"))?;
    check_context_in(tx, lease, binding, context).await?;
    let row:Option<(String,i64,i32,i64,Uuid)>=sqlx::query_as("SELECT request_digest,input_bound,output_cap::integer,lease_generation,request_key FROM usage_reservations WHERE id=$1 AND individual_id=$2 AND run_id=$3")
        .bind(reservation).bind(lease.individual_id).bind(lease.run_id).fetch_optional(&mut **tx).await?;
    let (hash, input, output, generation, key) =
        row.ok_or(StoreError::Invalid("trial reservation absent"))?;
    let actual:Option<(String,i64,i32,Option<Uuid>)>=sqlx::query_as("SELECT request_digest,input_bound,output_cap,reservation_id FROM development_trial_attempts WHERE individual_id=$1 AND id=$2")
        .bind(lease.individual_id).bind(binding.attempt_id).fetch_optional(&mut **tx).await?;
    let (expected, expected_input, expected_output, old) = actual.ok_or(StoreError::NotFound)?;
    if key != binding.attempt_id
        || generation != lease.generation
        || expected != hash
        || input != expected_input
        || output != expected_output
        || old.is_some_and(|id| id != reservation)
        || digest(
            &json!({"context":context,"allowance":TokenAllowance{input_upper_bound:input as u64,output_cap:output as u32}}),
        )? != hash
    {
        return Err(StoreError::IdempotencyConflict);
    }
    sqlx::query("UPDATE development_trial_attempts SET reservation_id=$3,state=CASE WHEN state='pending' THEN 'reserved' ELSE state END WHERE individual_id=$1 AND id=$2")
        .bind(lease.individual_id).bind(binding.attempt_id).bind(reservation).execute(&mut **tx).await?;
    Ok(())
}

/// Only the actual runtime dispatch path supplies mode, in the same transaction
/// as the original dispatch marker. ModelResult cannot create this proof.
pub(crate) async fn attest_development_trial_dispatch_in(
    tx: &mut Tx<'_>,
    lease: &RunLease,
    reservation_id: Uuid,
    mode: ProviderMode,
) -> Result<(), StoreError> {
    let row:Option<(Value,Uuid)>=sqlx::query_as("SELECT a.context_json,a.id FROM development_trial_attempts a JOIN usage_reservations u ON u.id=a.reservation_id WHERE a.individual_id=$1 AND a.reservation_id=$2 AND a.evaluation_run_id=$3 AND u.individual_id=$1 AND u.run_id=$3 AND u.status='reserved' AND u.dispatched_at IS NULL")
        .bind(lease.individual_id).bind(reservation_id).bind(lease.run_id).fetch_optional(&mut **tx).await?;
    let (context, id) = row.ok_or(StoreError::Invalid("unbound trial dispatch"))?;
    let context: ModelContext = serde_json::from_value(context)?;
    bind_reservation_in(tx, lease, reservation_id, &context).await?;
    let mode = match mode {
        ProviderMode::Fixture => "fixture",
        ProviderMode::Live => "live",
    };
    let old: Option<String> = sqlx::query_scalar(
        "SELECT dispatch_mode FROM development_trial_attempts WHERE individual_id=$1 AND id=$2",
    )
    .bind(lease.individual_id)
    .bind(id)
    .fetch_one(&mut **tx)
    .await?;
    if old.as_ref().is_some_and(|v| v != mode) {
        return Err(StoreError::IdempotencyConflict);
    }
    sqlx::query("UPDATE development_trial_attempts SET dispatch_mode=$3,attested_at=coalesce(attested_at,clock_timestamp()),state='dispatched' WHERE individual_id=$1 AND id=$2")
        .bind(lease.individual_id).bind(id).bind(mode).execute(&mut **tx).await?;
    Ok(())
}

impl Store {
    pub async fn bind_development_trial_request(
        &self,
        lease: &RunLease,
        binding: &TrialBinding,
        context: &ModelContext,
        allowance: TokenAllowance,
    ) -> Result<Uuid, StoreError> {
        if allowance.input_upper_bound == 0
            || allowance.input_upper_bound > i64::MAX as u64
            || !(1..=8192).contains(&allowance.output_cap)
        {
            return Err(StoreError::Invalid("trial request allowance"));
        }
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        check_context_in(&mut tx, lease, binding, context).await?;
        if binding.side == TrialSide::Trial {
            let baseline: String = sqlx::query_scalar("SELECT state FROM development_trial_attempts WHERE individual_id=$1 AND case_id=$2 AND side='baseline'")
                .bind(lease.individual_id).bind(binding.case_id).fetch_one(&mut *tx).await?;
            if baseline != "recorded" {
                return Err(StoreError::Invalid("trial baseline is not recorded"));
            }
        }
        let hash = digest(&json!({"context":context,"allowance":allowance}))?;
        let old:Option<String>=sqlx::query_scalar("SELECT request_digest FROM development_trial_attempts WHERE individual_id=$1 AND id=$2")
            .bind(lease.individual_id).bind(binding.attempt_id).fetch_one(&mut *tx).await?;
        if let Some(old) = old {
            if old != hash {
                return Err(StoreError::IdempotencyConflict);
            }
            self.require_privacy_snapshot(generation)?;
            return Ok(binding.attempt_id);
        }
        let updated=sqlx::query("UPDATE development_trial_attempts SET request_digest=$3,context_json=$4,input_bound=$5,output_cap=$6,request_sources=$7 WHERE individual_id=$1 AND id=$2 AND state='pending'")
            .bind(lease.individual_id).bind(binding.attempt_id).bind(hash).bind(serde_json::to_value(context)?).bind(allowance.input_upper_bound as i64).bind(allowance.output_cap as i32).bind(&context.input_manifest).execute(&mut *tx).await?;
        if updated.rows_affected() != 1 {
            return Err(StoreError::Invalid("trial attempt cannot be rebound"));
        }
        dependencies(
            &mut tx,
            lease.individual_id,
            binding.attempt_id,
            &context.input_manifest,
        )
        .await?;
        commit(self, tx, generation).await?;
        Ok(binding.attempt_id)
    }
}

fn outcome_name(outcome: Outcome) -> &'static str {
    match outcome {
        Outcome::Passed => "passed",
        Outcome::Failed => "failed",
        Outcome::Unverified => "unverified",
    }
}
fn parse_outcome(value: &str) -> Result<Outcome, StoreError> {
    match value {
        "passed" => Ok(Outcome::Passed),
        "failed" => Ok(Outcome::Failed),
        "unverified" => Ok(Outcome::Unverified),
        _ => Err(StoreError::Invalid("trial outcome")),
    }
}
fn aggregate(outcomes: impl IntoIterator<Item = Outcome>) -> Outcome {
    let mut result = Outcome::Passed;
    let mut count = 0;
    for o in outcomes {
        count += 1;
        match o {
            Outcome::Failed => result = Outcome::Failed,
            Outcome::Unverified if result != Outcome::Failed => result = Outcome::Unverified,
            _ => {}
        }
    }
    if count == 0 {
        Outcome::Unverified
    } else {
        result
    }
}
async fn attempt_in(
    tx: &mut Tx<'_>,
    individual: Uuid,
    id: Uuid,
) -> Result<TrialAttempt, StoreError> {
    let (plan_id,case_id,side,run,generation,state,reservation,result,outcome,mode):AttemptStatusRow=
        sqlx::query_as("SELECT plan_id,case_id,side,evaluation_run_id,lease_generation,state,reservation_id,result_event_id,outcome,dispatch_mode FROM development_trial_attempts WHERE individual_id=$1 AND id=$2")
        .bind(individual).bind(id).fetch_one(&mut **tx).await?;
    let side = match side.as_str() {
        "baseline" => TrialSide::Baseline,
        "trial" => TrialSide::Trial,
        _ => return Err(StoreError::Invalid("trial side")),
    };
    let state = match state.as_str() {
        "pending" => TrialAttemptState::Pending,
        "reserved" => TrialAttemptState::Reserved,
        "dispatched" => TrialAttemptState::Dispatched,
        "recorded" => TrialAttemptState::Recorded,
        "unknown" => TrialAttemptState::Unknown,
        "not_run" => TrialAttemptState::NotRun,
        _ => return Err(StoreError::Invalid("trial attempt state")),
    };
    Ok(TrialAttempt {
        binding: TrialBinding {
            plan_id,
            case_id,
            side,
            attempt_id: id,
            evaluation_run_id: run.unwrap_or(Uuid::nil()),
            lease_generation: generation.unwrap_or(0),
        },
        state,
        reservation_id: reservation,
        result_event_id: result,
        outcome: parse_outcome(&outcome)?,
        production_measurement: state == TrialAttemptState::Recorded
            && mode.as_deref() == Some("live")
            && sqlx::query_scalar::<_, bool>("SELECT coalesce(result->'mode_matches'='true'::jsonb,false) FROM development_trial_attempts WHERE individual_id=$1 AND id=$2")
                .bind(individual).bind(id).fetch_one(&mut **tx).await?,
    })
}

async fn observation_in(
    tx: &mut Tx<'_>,
    plan: &TrialPlan,
    case: &TrialCase,
    close_missing: bool,
) -> Result<Option<(Uuid, Outcome)>, StoreError> {
    let old: Option<Uuid> = sqlx::query_scalar(
        "SELECT observation_event_id FROM development_trial_cases WHERE individual_id=$1 AND id=$2",
    )
    .bind(plan.individual_id)
    .bind(case.id)
    .fetch_one(&mut **tx)
    .await?;
    if let Some(id) = old {
        let outcome: String = sqlx::query_scalar(
            "SELECT outcome FROM development_observations WHERE individual_id=$1 AND event_id=$2",
        )
        .bind(plan.individual_id)
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
        return Ok(Some((id, parse_outcome(&outcome)?)));
    }
    let rows:Vec<CaseResultRow>=sqlx::query_as("SELECT id,side,state,result,dispatch_mode FROM development_trial_attempts WHERE individual_id=$1 AND case_id=$2 ORDER BY side")
        .bind(plan.individual_id).bind(case.id).fetch_all(&mut **tx).await?;
    if rows.len() != 2 {
        return Err(StoreError::Invalid("missing frozen trial sides"));
    }
    let done = rows
        .iter()
        .all(|(_, _, s, _, _)| matches!(s.as_str(), "recorded" | "unknown" | "not_run"));
    if !done && !close_missing {
        return Ok(None);
    }
    let usable = |side: &str| -> Option<&Value> {
        let (_, _, state, result, mode) = rows.iter().find(|(_, s, _, _, _)| s == side)?;
        if state != "recorded" || mode.as_deref() != Some("live") {
            return None;
        }
        let result = result.as_ref()?;
        if result["isolated"] != true || result["mode_matches"] != true {
            return None;
        }
        result.get("proposal")
    };
    let outcome = measure_case(case, usable("baseline"), usable("trial"));
    let id = Uuid::new_v4();
    let role = match case.role {
        TrialCaseRole::Scenario => "scenario_result",
        TrialCaseRole::CounterSearch => "counter_search",
    };
    insert_event(tx,plan.individual_id,id,"development.observed","direct_observation",&format!("development-verifier:{}",plan.evaluator_id),json!({"verifier_id":plan.evaluator_id,"trial_event_id":plan.trial_event_id,"role":role,"case_id":case.id,"context":case.context,"outcome":outcome_name(outcome)}),plan.id,None).await?;
    sqlx::query("INSERT INTO development_observations(individual_id,event_id,verifier_id,trial_event_id,role,case_id,context,outcome) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
        .bind(plan.individual_id).bind(id).bind(plan.evaluator_id).bind(plan.trial_event_id).bind(role).bind(case.id).bind(&case.context).bind(outcome_name(outcome)).execute(&mut **tx).await?;
    let mut refs = case.source_event_ids.clone();
    refs.extend([case.id, plan.id, plan.trial_event_id, plan.evaluator_id]);
    refs.extend(rows.iter().map(|r| r.0));
    dependencies(tx, plan.individual_id, id, &refs).await?;
    sqlx::query("UPDATE development_trial_cases SET observation_event_id=$3,status=CASE WHEN EXISTS(SELECT 1 FROM development_trial_attempts a WHERE a.individual_id=$1 AND a.case_id=$2 AND a.state='unknown') THEN 'unknown' ELSE 'finished' END WHERE individual_id=$1 AND id=$2")
        .bind(plan.individual_id).bind(case.id).bind(id).execute(&mut **tx).await?;
    if close_missing {
        sqlx::query("UPDATE development_trial_attempts SET state='not_run' WHERE individual_id=$1 AND case_id=$2 AND state='pending' AND reservation_id IS NULL")
        .bind(plan.individual_id).bind(case.id).execute(&mut **tx).await?;
    }
    Ok(Some((id, outcome)))
}

async fn recover_attempts_in(tx: &mut Tx<'_>, plan: &TrialPlan) -> Result<Vec<Uuid>, StoreError> {
    let individual = plan.individual_id;
    let rows:Vec<(Uuid,Option<Uuid>,Option<i64>,String)>=sqlx::query_as("SELECT id,evaluation_run_id,lease_generation,state FROM development_trial_attempts WHERE individual_id=$1 AND plan_id=$2 ORDER BY case_id,side")
            .bind(individual).bind(plan.id).fetch_all(&mut **tx).await?;
    for (id, run, generation, state) in &rows {
        if matches!(state.as_str(), "not_run" | "unknown") {
            continue;
        }
        if let Some(run) = run {
            let usage:Option<(Uuid,bool,String,i64)>=sqlx::query_as("SELECT id,dispatched_at IS NOT NULL,status,lease_generation FROM usage_reservations WHERE individual_id=$1 AND run_id=$2 AND request_key=$3")
                    .bind(individual).bind(run).bind(id).fetch_optional(&mut **tx).await?;
            if let Some((reservation, sent, status, actual_generation)) = usage {
                if Some(actual_generation) != *generation {
                    return Err(StoreError::StaleState);
                }
                let proof: bool = sqlx::query_scalar("SELECT a.request_digest=u.request_digest AND a.input_bound=u.input_bound AND a.output_cap=u.output_cap AND (a.reservation_id IS NULL OR a.reservation_id=u.id) FROM development_trial_attempts a JOIN usage_reservations u ON u.individual_id=a.individual_id AND u.id=$3 WHERE a.individual_id=$1 AND a.id=$2 AND a.context_json IS NOT NULL AND a.request_digest IS NOT NULL")
                        .bind(individual).bind(id).bind(reservation).fetch_optional(&mut **tx).await?.unwrap_or(false);
                if !proof {
                    return Err(StoreError::StaleState);
                }
                let (context,hash,input,output,case,side):RecoveryRequestRow=sqlx::query_as("SELECT context_json,request_digest,input_bound,output_cap,case_id,side FROM development_trial_attempts WHERE individual_id=$1 AND id=$2")
                        .bind(individual).bind(id).fetch_one(&mut **tx).await?;
                let context: ModelContext = serde_json::from_value(context)?;
                let binding = context
                    .trial_binding
                    .as_ref()
                    .ok_or(StoreError::StaleState)?;
                if input <= 0
                    || !(1..=8192).contains(&output)
                    || binding.plan_id != plan.id
                    || binding.case_id != case
                    || binding.attempt_id != *id
                    || binding.evaluation_run_id != *run
                    || binding.lease_generation != actual_generation
                    || binding.side.as_str() != side
                    || context.run_id != *run
                    || context.individual_id != individual
                    || digest(
                        &json!({"context":context,"allowance":TokenAllowance{input_upper_bound:input as u64,output_cap:output as u32}}),
                    )? != hash
                {
                    return Err(StoreError::StaleState);
                }
                if state == "recorded" {
                    if !sent || status != "settled" {
                        return Err(StoreError::StaleState);
                    }
                    continue;
                }
                let recovered = if sent {
                    "unknown"
                } else if status == "reserved" {
                    "reserved"
                } else {
                    "not_run"
                };
                sqlx::query("UPDATE development_trial_attempts SET reservation_id=$3,state=$4 WHERE individual_id=$1 AND id=$2")
                        .bind(individual).bind(id).bind(reservation).bind(recovered).execute(&mut **tx).await?;
            } else if state == "recorded" {
                return Err(StoreError::StaleState);
            }
        }
    }
    // A dispatched baseline with no recoverable result cannot authorize the
    // second call. Preserve the untouched side as not-run, without billing it.
    sqlx::query("UPDATE development_trial_attempts a SET state='not_run' WHERE a.individual_id=$1 AND a.plan_id=$2 AND a.side='trial' AND a.state='pending' AND a.reservation_id IS NULL AND a.request_digest IS NULL AND EXISTS(SELECT 1 FROM development_trial_attempts b WHERE b.individual_id=a.individual_id AND b.case_id=a.case_id AND b.side='baseline' AND b.state='unknown') AND NOT EXISTS(SELECT 1 FROM usage_reservations u WHERE u.individual_id=a.individual_id AND u.run_id=a.evaluation_run_id AND u.request_key=a.id)")
            .bind(individual).bind(plan.id).execute(&mut **tx).await?;
    Ok(rows.into_iter().map(|r| r.0).collect())
}

async fn close_queued_work_in(
    tx: &mut Tx<'_>,
    plan: &TrialPlan,
    notification_version: i64,
) -> Result<(), StoreError> {
    let runs:Vec<Uuid>=sqlx::query_scalar("SELECT r.id FROM development_trial_cases c JOIN runs r ON (r.individual_id,r.id)=(c.individual_id,c.evaluation_run_id) WHERE c.individual_id=$1 AND c.plan_id=$2 AND r.status<>'running'")
        .bind(plan.individual_id).bind(plan.id).fetch_all(&mut **tx).await?;
    for run in runs {
        super::scheduling::close_pending(tx, run).await?;
        let stopped=sqlx::query("UPDATE runs SET status='cancelled',lease_owner=NULL,lease_expires_at=NULL,finished_at=coalesce(finished_at,clock_timestamp()),error_code='development_trial_closed' WHERE individual_id=$1 AND id=$2 AND status='queued'")
            .bind(plan.individual_id).bind(run).execute(&mut **tx).await?;
        if stopped.rows_affected() == 1 {
            outbox(
                tx,
                plan.individual_id,
                "operation.updated",
                run,
                notification_version,
            )
            .await?;
        }
    }
    recover_attempts_in(tx, plan).await?;
    Ok(())
}

impl Store {
    pub async fn complete_development_trial_attempt(
        &self,
        lease: &RunLease,
        binding: &TrialBinding,
        reservation_id: Uuid,
        context: &ModelContext,
        result: &ModelResult,
    ) -> Result<TrialAttempt, StoreError> {
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        super::development_trial_context::require_context_in(
            &mut tx,
            context,
            super::development_trial_context::TrialContextPhase::Result,
        )
        .await?;
        check_context_in(&mut tx, lease, binding, context).await?;
        let plan = binding_plan_in(&mut tx, lease, binding).await?;
        let actual:Option<(String,i64,i64,i64,Uuid,String,bool)>=sqlx::query_as("SELECT request_digest,input_bound,output_cap,lease_generation,request_key,status,dispatched_at IS NOT NULL FROM usage_reservations WHERE individual_id=$1 AND run_id=$2 AND id=$3")
            .bind(lease.individual_id).bind(lease.run_id).bind(reservation_id).fetch_optional(&mut *tx).await?;
        let (request, input, output, stored_generation, key, status, dispatched) =
            actual.ok_or(StoreError::Invalid("trial accounting absent"))?;
        if key != binding.attempt_id
            || stored_generation != lease.generation
            || status != "settled"
            || !dispatched
            || digest(
                &json!({"context":context,"allowance":TokenAllowance{input_upper_bound:input as u64,output_cap:output as u32}}),
            )? != request
        {
            return Err(StoreError::StaleState);
        }
        let (bound,mode,expected,old):(Option<Uuid>,Option<String>,String,Option<Value>)=sqlx::query_as("SELECT reservation_id,dispatch_mode,request_digest,result FROM development_trial_attempts WHERE individual_id=$1 AND id=$2")
            .bind(lease.individual_id).bind(binding.attempt_id).fetch_one(&mut *tx).await?;
        if bound != Some(reservation_id) || expected != request || mode.is_none() {
            return Err(StoreError::Invalid("trial dispatch proof absent"));
        }
        let proposal = serde_json::to_value(&result.proposal)?;
        if serde_json::to_vec(&proposal)?.len() > 96 * 1024 {
            return Err(StoreError::Invalid("trial result bound"));
        }
        let isolated = [
            "memory_changes",
            "goal_changes",
            "tool_intents",
            "entity_changes",
            "belief_changes",
            "autobiography_changes",
            "appraisal_changes",
            "attitude_changes",
            "relationship_changes",
            "interest_changes",
        ]
        .iter()
        .all(|name| {
            proposal
                .get(*name)
                .is_none_or(|v| v.as_array().is_some_and(Vec::is_empty))
        }) && proposal.get("development").is_none_or(Value::is_null);
        let reported = match result.provider_mode {
            ProviderMode::Fixture => "fixture",
            ProviderMode::Live => "live",
        };
        let record = json!({"proposal":proposal,"isolated":isolated,"mode_matches":mode.as_deref()==Some(reported)});
        if let Some(old) = old {
            if old != record {
                return Err(StoreError::IdempotencyConflict);
            }
            let actual = attempt_in(&mut tx, lease.individual_id, binding.attempt_id).await?;
            self.require_privacy_snapshot(generation)?;
            return Ok(actual);
        }
        let event = Uuid::new_v4();
        insert_event(&mut tx,lease.individual_id,event,"development.trial.result","inference","system",json!({"plan_id":binding.plan_id,"case_id":binding.case_id,"side":binding.side,"reservation_id":reservation_id,"condition_only":true,"simulation_not_experience":true}),binding.plan_id,None).await?;
        let mut refs = context.input_manifest.clone();
        refs.push(binding.attempt_id);
        dependencies(&mut tx, lease.individual_id, event, &refs).await?;
        let production = mode.as_deref() == Some("live") && reported == "live";
        let case = plan
            .cases
            .iter()
            .find(|c| c.id == binding.case_id)
            .ok_or(StoreError::NotFound)?;
        let outcome = if !production {
            Outcome::Unverified
        } else if !isolated {
            Outcome::Failed
        } else if case.criterion.matches(&record["proposal"]) {
            Outcome::Passed
        } else {
            Outcome::Failed
        };
        sqlx::query("UPDATE development_trial_attempts SET result=$3,result_event_id=$4,state='recorded',outcome=$5 WHERE individual_id=$1 AND id=$2")
            .bind(lease.individual_id).bind(binding.attempt_id).bind(record).bind(event).bind(outcome_name(outcome)).execute(&mut *tx).await?;
        observation_in(&mut tx, &plan, case, false).await?;
        let actual = attempt_in(&mut tx, lease.individual_id, binding.attempt_id).await?;
        commit(self, tx, generation).await?;
        Ok(actual)
    }

    pub async fn recover_development_trial_work(
        &self,
        work_or_run_id: Uuid,
    ) -> Result<TrialWorkStatus, StoreError> {
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        let individual = existing(&mut tx).await?;
        let plan_id:Option<Uuid>=sqlx::query_scalar("SELECT id FROM development_trial_plans WHERE individual_id=$1 AND id=$2 UNION SELECT plan_id FROM development_trial_cases WHERE individual_id=$1 AND (id=$2 OR evaluation_run_id=$2) LIMIT 1")
            .bind(individual).bind(work_or_run_id).fetch_optional(&mut *tx).await?;
        let plan = plan_in(
            &mut tx,
            individual,
            plan_id.ok_or(StoreError::NotFound)?,
            false,
        )
        .await?;
        let rows = recover_attempts_in(&mut tx, &plan).await?;
        let mut attempts = vec![];
        for id in rows {
            attempts.push(attempt_in(&mut tx, individual, id).await?);
        }
        let mut outcomes = Vec::new();
        for case in &plan.cases {
            let observed: Option<String> = sqlx::query_scalar("SELECT o.outcome FROM development_trial_cases c JOIN development_observations o ON o.individual_id=c.individual_id AND o.event_id=c.observation_event_id WHERE c.individual_id=$1 AND c.id=$2")
                .bind(individual).bind(case.id).fetch_optional(&mut *tx).await?;
            outcomes.push(match observed {
                Some(v) => parse_outcome(&v)?,
                None => Outcome::Unverified,
            });
        }
        let outcome = aggregate(outcomes);
        commit(self, tx, generation).await?;
        Ok(TrialWorkStatus {
            plan_id: plan.id,
            attempts,
            outcome,
        })
    }

    pub async fn review_development_trial_plan(
        &self,
        key: &str,
        plan_id: Uuid,
    ) -> Result<TrialReviewStatus, StoreError> {
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        let hash = digest(&json!({"plan_id":plan_id,"program_version":MEASUREMENT_VERSION}))?;
        if let Some(saved) = replay::<TrialReviewStatus>(
            &mut tx,
            "system",
            "/internal/development-trials/review",
            key,
            &hash,
        )
        .await?
        {
            plan_in(&mut tx, current.id, plan_id, false).await?;
            if !development::available(&mut tx, current.id, &[saved.review_event_id]).await? {
                return Err(StoreError::NotFound);
            }
            self.require_privacy_snapshot(generation)?;
            return Ok(saved);
        }
        let status = review_plan_in(&mut tx, &current, key, plan_id).await?;
        commit(self, tx, generation).await?;
        Ok(status)
    }

    /// Deterministic idle maintenance. All evidence, original accounting and
    /// the domain transition are committed together; no model is called here.
    pub async fn expire_development_trial_plans(
        &self,
        limit: u32,
    ) -> Result<Vec<TrialReviewStatus>, StoreError> {
        if !(1..=20).contains(&limit) {
            return Err(StoreError::Invalid("trial expiry limit"));
        }
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        if reference_edges_in(&mut tx).await?.is_none() {
            return Ok(vec![]);
        }
        let individual = existing(&mut tx).await?;
        let running: bool = sqlx::query_scalar(
            "SELECT EXISTS(SELECT 1 FROM runs WHERE individual_id=$1 AND status='running')",
        )
        .bind(individual)
        .fetch_one(&mut *tx)
        .await?;
        if running {
            return Ok(vec![]);
        }
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM development_trial_plans WHERE individual_id=$1 AND status='active' AND deadline<=clock_timestamp() ORDER BY deadline,id LIMIT $2")
            .bind(individual).bind(limit as i64).fetch_all(&mut *tx).await?;
        let mut completed = vec![];
        for id in ids {
            let plan = match plan_in(&mut tx, individual, id, false).await {
                Ok(plan) => plan,
                Err(StoreError::NotFound | StoreError::StaleState) => continue,
                Err(error) => return Err(error),
            };
            let (candidate, version, target) =
                trial_candidate_in(&mut tx, individual, plan.development_id).await?;
            if candidate.status() != DevelopmentStatus::Trial
                || version != plan.development_version
                || target != plan.target
            {
                continue;
            }
            let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
            let next = current
                .state_version
                .checked_add(1)
                .ok_or(StoreError::Invalid("state version exhausted"))?;
            close_queued_work_in(&mut tx, &plan, next).await?;
            sqlx::query("UPDATE development_trial_attempts SET state='not_run' WHERE individual_id=$1 AND plan_id=$2 AND state IN ('pending','reserved')")
                .bind(individual).bind(id).execute(&mut *tx).await?;
            completed
                .push(review_plan_in(&mut tx, &current, &format!("trial-expire:{id}"), id).await?);
        }
        commit(self, tx, generation).await?;
        Ok(completed)
    }
}

async fn review_plan_in(
    tx: &mut Tx<'_>,
    current: &crate::domain::history::IndividualState,
    key: &str,
    plan_id: Uuid,
) -> Result<TrialReviewStatus, StoreError> {
    let hash = digest(&json!({"plan_id":plan_id,"program_version":MEASUREMENT_VERSION}))?;
    let plan = plan_in(tx, current.id, plan_id, false).await?;
    let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM development_trial_cases c JOIN runs r ON (r.individual_id,r.id)=(c.individual_id,c.evaluation_run_id) WHERE c.individual_id=$1 AND c.plan_id=$2 AND r.status='running')")
            .bind(current.id).bind(plan_id).fetch_one(&mut **tx).await?;
    if active {
        return Err(StoreError::Invalid(
            "trial result recovery must finish first",
        ));
    }
    let (development, version, target) =
        trial_candidate_in(tx, current.id, plan.development_id).await?;
    if development.status() != DevelopmentStatus::Trial
        || version != plan.development_version
        || target != plan.target
    {
        return Err(StoreError::StaleState);
    }
    close_queued_work_in(
        tx,
        &plan,
        current
            .state_version
            .checked_add(1)
            .ok_or(StoreError::Invalid("state version exhausted"))?,
    )
    .await?;
    let mut observations = vec![];
    let mut counter = vec![];
    let mut outcomes = vec![];
    let mut sources = plan.draft.evidence_ids.clone();
    sources.extend(&plan.draft.counterevidence_ids);
    for case in &plan.cases {
        let (event, outcome) = observation_in(tx, &plan, case, true)
            .await?
            .ok_or(StoreError::Invalid("missing planned observation"))?;
        sources.extend(&case.source_event_ids);
        sources.push(event);
        outcomes.push(outcome);
        match case.role {
            TrialCaseRole::Scenario => observations.push(ScenarioObservation {
                case_id: case.id,
                context: case.context.clone(),
                source_event_ids: case.source_event_ids.clone(),
                result_event_id: event,
                outcome,
            }),
            TrialCaseRole::CounterSearch => counter.push((event, outcome)),
        }
    }
    sources.sort();
    sources.dedup();
    let assessment = IndependentAssessment {
        review_id: Uuid::new_v4(),
        reviewer_id: plan.evaluator_id,
        origin: ReviewOrigin::IndependentMeasurement,
        reviewed_evidence_ids: plan.draft.evidence_ids.clone(),
        reviewed_counterevidence_ids: plan.draft.counterevidence_ids.clone(),
        counter_search_event_ids: counter.iter().map(|x| x.0).collect(),
        counter_review: aggregate(counter.iter().map(|x| x.1)),
        observations,
    };
    let event = Uuid::new_v4();
    insert_event(tx,current.id,event,"development.review.recorded","direct_observation",&format!("development-verifier:{}",plan.evaluator_id),json!({"development_id":plan.development_id,"trial_event_id":plan.trial_event_id,"review_id":assessment.review_id,"verifier_id":plan.evaluator_id}),plan.id,Some(key)).await?;
    sqlx::query("INSERT INTO development_review_records(individual_id,event_id,development_id,trial_event_id,verifier_id,assessment) VALUES($1,$2,$3,$4,$5,$6)")
            .bind(current.id).bind(event).bind(plan.development_id).bind(plan.trial_event_id).bind(plan.evaluator_id).bind(serde_json::to_value(&assessment)?).execute(&mut **tx).await?;
    for source in &sources {
        sqlx::query("INSERT INTO development_review_sources(individual_id,review_event_id,event_id) VALUES($1,$2,$3)").bind(current.id).bind(event).bind(source).execute(&mut **tx).await?;
    }
    let mut manifest: Vec<Uuid> = sqlx::query_scalar(
        "SELECT input_manifest FROM development_trial_plans WHERE individual_id=$1 AND id=$2",
    )
    .bind(current.id)
    .bind(plan.id)
    .fetch_one(&mut **tx)
    .await?;
    manifest.extend(&sources);
    let registration: Uuid = sqlx::query_scalar("SELECT registration_event_id FROM development_measurement_programs WHERE individual_id=$1 AND verifier_id=$2")
            .bind(current.id).bind(plan.evaluator_id).fetch_one(&mut **tx).await?;
    manifest.extend([
        plan.id,
        event,
        plan.evaluator_id,
        plan.trial_event_id,
        registration,
    ]);
    manifest.sort();
    manifest.dedup();
    dependencies(tx, current.id, event, &manifest).await?;
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let expired = now >= plan.draft.deadline;
    let window_closed = now >= plan.deadline;
    let command = if expired {
        DevelopmentCommand::Expire
    } else if window_closed {
        DevelopmentCommand::Reject {
            reason: "frozen measurement deadline reached without timely finalization".into(),
        }
    } else {
        DevelopmentCommand::RecordValidation {
            review_event_id: event,
            assessment,
            source_event_ids: sources,
        }
    };
    let validated = transition_in_tx(
        tx,
        current.id,
        &manifest,
        plan.development_id,
        version,
        &command,
        Some(key),
    )
    .await?;
    let outcome = aggregate(outcomes);
    sqlx::query("UPDATE development_trial_plans SET status=$3,review_event_id=$4,review_outcome=$5,review_development_version=$6 WHERE individual_id=$1 AND id=$2")
        .bind(current.id).bind(plan.id).bind(if expired||window_closed{"expired"}else{"reviewed"}).bind(event).bind(outcome_name(outcome)).bind(validated.development_version).execute(&mut **tx).await?;
    let next = current
        .state_version
        .checked_add(1)
        .ok_or(StoreError::Invalid("state version exhausted"))?;
    outbox(
        tx,
        current.id,
        "development.changed",
        plan.development_id,
        next,
    )
    .await?;
    advance(tx, current.id, next).await?;
    let status = TrialReviewStatus {
        plan_id: plan.id,
        review_event_id: event,
        outcome,
    };
    save_receipt(
        tx,
        "system",
        "/internal/development-trials/review",
        key,
        &hash,
        &status,
    )
    .await?;
    Ok(status)
}

#[cfg(test)]
#[allow(dead_code)]
mod tests {
    use crate as alia;
    use crate::adapters::store::StoreError;
    include!("../../tests/support/development_context_fixture.rs");
    use crate::domain::development_trials::*;
    async fn subject(
        pool: &PgPool,
    ) -> (
        Store,
        Uuid,
        DevelopmentReceipt,
        Uuid,
        TrialPlanDraft,
        Vec<Uuid>,
    ) {
        subject_with_deadline(pool, None).await
    }
    async fn subject_with_deadline(
        pool: &PgPool,
        lifetime: Option<Duration>,
    ) -> (
        Store,
        Uuid,
        DevelopmentReceipt,
        Uuid,
        TrialPlanDraft,
        Vec<Uuid>,
    ) {
        let s = setup(pool).await;
        let event = message(&s).await;
        let mut candidate = draft(event, DevelopmentTarget::Traits, traits(0.61));
        if let Some(lifetime) = lifetime {
            candidate.deadline = Utc::now() + lifetime;
        }
        let p = propose(&s, pool, &candidate).await;
        let evaluator = s
            .register_development_measurement(
                "actual-program",
                MeasurementProgram::JsonComparisonV1,
            )
            .await
            .unwrap();
        let cases = [
            ("first condition", TrialCaseRole::Scenario),
            ("second condition", TrialCaseRole::Scenario),
            ("frozen counter condition", TrialCaseRole::CounterSearch),
        ]
        .into_iter()
        .map(|(context, role)| TrialCase {
            id: Uuid::new_v4(),
            context: context.into(),
            prompt: format!("synthetic controlled scenario: {context}; never a shared life event"),
            role,
            criterion: TrialCriterion::JsonEquals {
                pointer: "/reply".into(),
                expected: json!("expected"),
            },
            source_event_ids: vec![event],
        })
        .collect();
        let plan = TrialPlanDraft {
            cases,
            deadline: candidate.deadline.min(Utc::now() + Duration::minutes(30)),
        };
        let input = refs(
            pool,
            p.development_id,
            &DevelopmentCommand::StartTrial {
                route: ValidationRoute::CrossScenarioHabit,
                source_event_ids: vec![event],
            },
        )
        .await;
        (s, event, p, evaluator, plan, input)
    }
    #[sqlx::test]
    #[ignore = "actual isolated PostgreSQL and real short deadline; no model"]
    async fn expired_review_finalization_reuses_actual_expiry_without_second_transition(
        pool: PgPool,
    ) {
        let (s, _, p, evaluator, draft, input) =
            subject_with_deadline(&pool, Some(Duration::seconds(8))).await;
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        let remaining = (plan.draft.deadline - Utc::now())
            .to_std()
            .unwrap_or_default();
        tokio::time::sleep(remaining + std::time::Duration::from_millis(25)).await;
        let review = s
            .review_development_trial_plan("expired-review", plan.id)
            .await
            .unwrap();
        assert_eq!(review.outcome, Outcome::Unverified);
        let initial = s.snapshot().await.unwrap().unwrap();
        let receipt = s
            .finalize_development_trial_plan("expired-final", plan.id, TrialDisposition::Expire)
            .await
            .unwrap();
        assert_eq!(receipt.status, "expired");
        assert_eq!(receipt.state_version, initial.state_version);
        assert_eq!(
            s.snapshot().await.unwrap().unwrap().state_version,
            initial.state_version
        );
        let steps: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM development_steps WHERE development_id=$1 AND status='expired'",
        )
        .bind(p.development_id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(steps, 1);
        let before = fingerprint(&pool).await;
        assert_eq!(
            s.finalize_development_trial_plan("expired-final", plan.id, TrialDisposition::Expire)
                .await
                .unwrap(),
            receipt
        );
        assert_eq!(fingerprint(&pool).await, before);
        assert!(
            s.finalize_development_trial_plan("expired-accept", plan.id, TrialDisposition::Accept)
                .await
                .is_err()
        );
    }
    async fn wait_for_deadline(deadline: chrono::DateTime<Utc>) {
        let remaining = (deadline - Utc::now()).to_std().unwrap_or_default();
        tokio::time::sleep(remaining + std::time::Duration::from_millis(25)).await;
    }
    #[sqlx::test]
    #[ignore = "isolated PostgreSQL and real short deadline, no model"]
    async fn idle_expiry_closes_never_queued_plan_once_without_charging(pool: PgPool) {
        let (s, _, p, evaluator, draft, input) =
            subject_with_deadline(&pool, Some(Duration::seconds(6))).await;
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        wait_for_deadline(plan.deadline).await;
        let initial = s.snapshot().await.unwrap().unwrap();
        let completed = s.expire_development_trial_plans(10).await.unwrap();
        assert_eq!(completed.len(), 1);
        assert_eq!(completed[0].outcome, Outcome::Unverified);
        let status: String = sqlx::query_scalar("SELECT status FROM developments WHERE id=$1")
            .bind(p.development_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "expired");
        let after = s.snapshot().await.unwrap().unwrap();
        assert_eq!(after.state_version, initial.state_version + 1);
        assert_eq!(after.persona, initial.persona);
        let calls: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(calls, 0);
        let before = fingerprint(&pool).await;
        assert!(
            s.expire_development_trial_plans(10)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(fingerprint(&pool).await, before);
        assert_eq!(
            s.finalize_development_trial_plan("expiry-receipt", plan.id, TrialDisposition::Expire)
                .await
                .unwrap()
                .status,
            "expired"
        );
    }
    #[sqlx::test]
    #[ignore = "isolated PostgreSQL and real short measurement window, no live model"]
    async fn shorter_window_rejects_queued_plan_atomically_without_expiring_candidate_early(
        pool: PgPool,
    ) {
        let (s, _, p, evaluator, mut draft, input) = subject(&pool).await;
        draft.deadline = Utc::now() + Duration::seconds(3);
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        finish_source_run(&s).await;
        let run = s
            .enqueue_development_trial_case("to-expire", plan.id, plan.cases[0].id)
            .await
            .unwrap();
        wait_for_deadline(plan.deadline).await;
        sqlx::raw_sql("CREATE FUNCTION fail_idle_expiry() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic idle expiry failure'; END $$; CREATE TRIGGER fail_idle_expiry BEFORE INSERT ON outbox FOR EACH ROW WHEN (NEW.event_type='development.changed') EXECUTE FUNCTION fail_idle_expiry();").execute(&pool).await.unwrap();
        let before = fingerprint(&pool).await;
        assert!(s.expire_development_trial_plans(1).await.is_err());
        assert_eq!(fingerprint(&pool).await, before);
        sqlx::query("DROP TRIGGER fail_idle_expiry ON outbox")
            .execute(&pool)
            .await
            .unwrap();
        let initial = s.snapshot().await.unwrap().unwrap();
        let usage_before: (i64, i64) = sqlx::query_as(
            "SELECT count(*),coalesce(sum(charged_tokens),0)::bigint FROM usage_reservations",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        let updates_before: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox WHERE resource_id=$1 AND event_type='operation.updated'",
        )
        .bind(run)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(s.expire_development_trial_plans(1).await.unwrap().len(), 1);
        let statuses: (String, String) = sqlx::query_as(
            "SELECT d.status,r.status FROM developments d JOIN runs r ON r.id=$2 WHERE d.id=$1",
        )
        .bind(p.development_id)
        .bind(run)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(statuses, ("rejected".into(), "cancelled".into()));
        let after = s.snapshot().await.unwrap().unwrap();
        assert_eq!(after.state_version, initial.state_version + 1);
        assert_eq!(after.persona, initial.persona);
        let usage_after: (i64, i64) = sqlx::query_as(
            "SELECT count(*),coalesce(sum(charged_tokens),0)::bigint FROM usage_reservations",
        )
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(usage_before, usage_after);
        let updates_after: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM outbox WHERE resource_id=$1 AND event_type='operation.updated'",
        )
        .bind(run)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(updates_after, updates_before + 1);
        let matrix:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM development_trial_attempts WHERE state='not_run'),(SELECT count(*) FROM development_observations WHERE outcome='unverified')").fetch_one(&pool).await.unwrap();
        assert_eq!(matrix, (6, 3));
        assert!(Utc::now() < plan.draft.deadline);
    }
    #[sqlx::test]
    #[ignore = "isolated PostgreSQL, real short deadline, synthetic dispatch marker only"]
    async fn idle_expiry_respects_running_lease_and_keeps_original_unknown_charge(pool: PgPool) {
        let (s, _, p, evaluator, draft, input) =
            subject_with_deadline(&pool, Some(Duration::seconds(8))).await;
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        finish_source_run(&s).await;
        let run = s
            .enqueue_development_trial_case("expiry-running", plan.id, plan.cases[0].id)
            .await
            .unwrap();
        let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
        let work = s
            .claim_development_trial_work(&lease)
            .await
            .unwrap()
            .unwrap();
        let context = crate::adapters::development_trial_context::prepare_case_context(
            &s,
            &lease,
            &work,
            &work.baseline,
            s.capabilities().await.unwrap(),
        )
        .await
        .unwrap();
        let allowance = crate::runtime::scheduling::TokenAllowance {
            input_upper_bound: 100,
            output_cap: 50,
        };
        let key = s
            .bind_development_trial_request(&lease, &work.baseline, &context, allowance)
            .await
            .unwrap();
        let reservation = s
            .reserve_model_call(&lease, key, &context, allowance)
            .await
            .unwrap();
        assert!(
            s.dispatch_model_call_with_context(
                &lease,
                reservation.id,
                &context,
                crate::runtime::model::ProviderMode::Fixture
            )
            .await
            .unwrap()
        );
        // Only the real ledger dispatch transaction is exercised; no provider
        // future or network operation runs. The missing result remains unknown.
        wait_for_deadline(plan.deadline).await;
        let before = fingerprint(&pool).await;
        assert!(
            s.expire_development_trial_plans(10)
                .await
                .unwrap()
                .is_empty()
        );
        assert_eq!(fingerprint(&pool).await, before);
        // Disposable process-loss fixture: expire only the lease, then use
        // canonical accounting recovery. The plan and lease clocks differ.
        sqlx::query(
            "UPDATE runs SET lease_expires_at=clock_timestamp()-interval '1 second' WHERE id=$1",
        )
        .bind(run)
        .execute(&pool)
        .await
        .unwrap();
        if let Some(reclaimed) = s.claim_run(Uuid::new_v4()).await.unwrap() {
            assert_eq!(reclaimed.run_id, run);
            s.finish_run(&reclaimed, crate::runtime::scheduling::RunOutcome::Failed)
                .await
                .unwrap();
        }
        let accounting: (String, i64, bool) = sqlx::query_as(
            "SELECT status,charged_tokens,usage_unknown FROM usage_reservations WHERE id=$1",
        )
        .bind(reservation.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(accounting, ("settled".into(), 150, true));
        assert_eq!(s.expire_development_trial_plans(10).await.unwrap().len(), 1);
        let after: (String, i64, bool) = sqlx::query_as(
            "SELECT status,charged_tokens,usage_unknown FROM usage_reservations WHERE id=$1",
        )
        .bind(reservation.id)
        .fetch_one(&pool)
        .await
        .unwrap();
        assert_eq!(after, accounting);
        let matrix:(i64,i64)=sqlx::query_as("SELECT count(*) FILTER(WHERE state='unknown'),count(*) FILTER(WHERE state='not_run') FROM development_trial_attempts WHERE plan_id=$1").bind(plan.id).fetch_one(&pool).await.unwrap();
        assert_eq!(matrix, (1, 5));
        let status: String = sqlx::query_scalar("SELECT status FROM developments WHERE id=$1")
            .bind(p.development_id)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "expired");
        let calls: i64 =
            sqlx::query_scalar("SELECT count(*) FROM usage_reservations WHERE run_id=$1")
                .bind(run)
                .fetch_one(&pool)
                .await
                .unwrap();
        assert_eq!(calls, 1);
    }
    #[sqlx::test]
    #[ignore = "isolated PostgreSQL, synthetic source completion only"]
    async fn reject_closes_only_mapped_queued_siblings_and_rolls_back_late_failure(pool: PgPool) {
        let (s, _, p, evaluator, draft, input) = subject(&pool).await;
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        finish_source_run(&s).await;
        let mut runs = vec![];
        for case in &plan.cases[..2] {
            runs.push(
                s.enqueue_development_trial_case(&format!("queued:{}", case.id), plan.id, case.id)
                    .await
                    .unwrap(),
            );
        }
        let unrelated = message(&s).await;
        let ordinary: Uuid = sqlx::query_scalar("SELECT id FROM runs WHERE trigger_event_id=$1")
            .bind(unrelated)
            .fetch_one(&pool)
            .await
            .unwrap();
        sqlx::raw_sql("CREATE FUNCTION fail_sibling_finalize() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic late finalize'; END $$; CREATE TRIGGER fail_sibling_finalize BEFORE INSERT ON outbox FOR EACH ROW WHEN (NEW.event_type='development.changed') EXECUTE FUNCTION fail_sibling_finalize();").execute(&pool).await.unwrap();
        let before = fingerprint(&pool).await;
        assert!(
            s.finalize_development_trial_plan(
                "reject-siblings",
                plan.id,
                TrialDisposition::Reject {
                    reason: "no independently verified result".into()
                }
            )
            .await
            .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
        sqlx::query("DROP TRIGGER fail_sibling_finalize ON outbox")
            .execute(&pool)
            .await
            .unwrap();
        let initial = s.snapshot().await.unwrap().unwrap();
        let receipt = s
            .finalize_development_trial_plan(
                "reject-siblings",
                plan.id,
                TrialDisposition::Reject {
                    reason: "no independently verified result".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(receipt.status, "rejected");
        assert_eq!(receipt.state_version, initial.state_version + 1);
        let cancelled:i64=sqlx::query_scalar("SELECT count(*) FROM runs WHERE id=ANY($1) AND status='cancelled' AND lease_owner IS NULL AND lease_expires_at IS NULL AND finished_at IS NOT NULL").bind(&runs).fetch_one(&pool).await.unwrap();
        assert_eq!(cancelled, 2);
        let status: String = sqlx::query_scalar("SELECT status FROM runs WHERE id=$1")
            .bind(ordinary)
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(status, "queued");
        let matrix:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM development_trial_attempts WHERE plan_id=$1 AND state='not_run'),(SELECT count(*) FROM development_trial_cases WHERE plan_id=$1 AND observation_event_id IS NOT NULL)").bind(plan.id).fetch_one(&pool).await.unwrap();
        assert_eq!(matrix, (6, 3));
        assert_eq!(
            s.snapshot().await.unwrap().unwrap().persona,
            initial.persona
        );
        let before = fingerprint(&pool).await;
        assert_eq!(
            s.finalize_development_trial_plan(
                "reject-siblings",
                plan.id,
                TrialDisposition::Reject {
                    reason: "no independently verified result".into()
                }
            )
            .await
            .unwrap(),
            receipt
        );
        assert_eq!(fingerprint(&pool).await, before);
    }
    #[sqlx::test]
    #[ignore = "isolated PostgreSQL; in-process live-mode protocol simulation, not provider/growth evidence"]
    async fn complete_comparison_matrix_accepts_once_through_real_accounting_and_domain_kernel(
        pool: PgPool,
    ) {
        use crate::runtime::{
            execution::accounted_call_with_request_key,
            model::{CallLimits, ModelResult, ProviderMode, TokenUsage},
            scheduling::TokenAllowance,
        };
        let (s, _, p, evaluator, draft, input) = subject(&pool).await;
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        finish_source_run(&s).await;
        let initial = s.snapshot().await.unwrap().unwrap();
        for case in &plan.cases {
            s.enqueue_development_trial_case(&format!("positive:{}", case.id), plan.id, case.id)
                .await
                .unwrap();
            let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
            let work = s
                .claim_development_trial_work(&lease)
                .await
                .unwrap()
                .unwrap();
            for binding in [&work.baseline, &work.trial] {
                let context = crate::adapters::development_trial_context::prepare_case_context(
                    &s,
                    &lease,
                    &work,
                    binding,
                    s.capabilities().await.unwrap(),
                )
                .await
                .unwrap();
                let allowance = TokenAllowance {
                    input_upper_bound: 100,
                    output_cap: 50,
                };
                let key = s
                    .bind_development_trial_request(&lease, binding, &context, allowance)
                    .await
                    .unwrap();
                let reply = if case.role == TrialCaseRole::Scenario
                    && binding.side == TrialSide::Baseline
                {
                    "baseline mismatch"
                } else {
                    "expected"
                };
                let proposal=crate::domain::cognition::DecisionProposal::parse(json!({"schema_version":1,"reply":reply,"reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"explicit in-process protocol test, not a real model result"}).to_string().as_bytes()).unwrap();
                // Exercise the trusted runtime ABI with an explicit test-only
                // future. This is not a vendor response or real growth trial.
                let (result, reservation) = accounted_call_with_request_key(
                    &s,
                    &lease,
                    &context,
                    key,
                    allowance,
                    CallLimits {
                        max_output_tokens: 50,
                        timeout_seconds: 10,
                    },
                    ProviderMode::Live,
                    async {
                        Ok(ModelResult {
                            proposal,
                            usage: Some(TokenUsage {
                                input: 1,
                                output: 1,
                                total: 2,
                            }),
                            provider_model: "explicit-inprocess-protocol-test-no-network".into(),
                            system_fingerprint: None,
                            provider_mode: ProviderMode::Live,
                            http: None,
                        })
                    },
                    |r| r.usage,
                )
                .await
                .unwrap();
                s.complete_development_trial_attempt(
                    &lease,
                    binding,
                    reservation,
                    &context,
                    &result,
                )
                .await
                .unwrap();
            }
            assert_eq!(
                s.finish_development_trial_work(&lease)
                    .await
                    .unwrap()
                    .outcome,
                Outcome::Passed
            );
        }
        let measured = s.snapshot().await.unwrap().unwrap();
        assert_eq!(measured.state_version, initial.state_version);
        assert_eq!(measured.persona, initial.persona);
        let review = s
            .review_development_trial_plan("positive-review", plan.id)
            .await
            .unwrap();
        assert_eq!(review.outcome, Outcome::Passed);
        let reviewed = s.snapshot().await.unwrap().unwrap();
        let receipt = s
            .finalize_development_trial_plan("positive-final", plan.id, TrialDisposition::Accept)
            .await
            .unwrap();
        assert_eq!(receipt.status, "accepted");
        assert!(receipt.revision_id.is_some());
        assert_eq!(receipt.state_version, reviewed.state_version + 1);
        let mut expected_persona = initial.persona.clone();
        expected_persona["personality"]["traits"] = traits(0.61);
        assert_eq!(
            s.snapshot().await.unwrap().unwrap().persona,
            expected_persona
        );
        let costs:(i64,i64)=sqlx::query_as("SELECT count(*),sum(u.charged_tokens)::bigint FROM usage_reservations u JOIN development_trial_cases c ON (c.individual_id,c.evaluation_run_id)=(u.individual_id,u.run_id) WHERE c.plan_id=$1 AND u.status='settled'").bind(plan.id).fetch_one(&pool).await.unwrap();
        assert_eq!(costs, (6, 12));
        let before = fingerprint(&pool).await;
        assert_eq!(
            s.finalize_development_trial_plan("positive-final", plan.id, TrialDisposition::Accept)
                .await
                .unwrap(),
            receipt
        );
        assert_eq!(fingerprint(&pool).await, before);
    }
    async fn freeze(
        s: &Store,
        p: &DevelopmentReceipt,
        evaluator: Uuid,
        draft: &TrialPlanDraft,
        input: &[Uuid],
    ) -> TrialPlan {
        s.freeze_development_trial_plan(
            "frozen-plan",
            s.snapshot().await.unwrap().unwrap().expected(),
            input,
            p.development_id,
            p.development_version,
            evaluator,
            ValidationRoute::CrossScenarioHabit,
            draft,
        )
        .await
        .unwrap()
    }
    async fn finish_source_run(s: &Store) {
        let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
        let provider=crate::runtime::model::FixtureProvider{response:json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"synthetic source run completion, not measurement"}).to_string()};
        let ready = crate::runtime::execution::model_step(
            s,
            &provider,
            &lease,
            vec![],
            crate::runtime::model::CallLimits::default(),
        )
        .await
        .unwrap();
        s.commit_decision(&lease, &ready, &[]).await.unwrap();
    }
    #[sqlx::test]
    #[ignore = "actual isolated schema30 PG, no network model"]
    async fn register_freeze_all_sides_and_replay_are_actual_atomic_records(pool: PgPool) {
        let (s, _, p, evaluator, draft, input) = subject(&pool).await;
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        let counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM development_trial_plans),(SELECT count(*) FROM development_trial_cases),(SELECT count(*) FROM development_trial_attempts)").fetch_one(&pool).await.unwrap();
        assert_eq!(counts, (1, 3, 6));
        assert_eq!(
            s.development(p.development_id).await.unwrap()["development"]["status"],
            "trial"
        );
        let before = fingerprint(&pool).await;
        assert_eq!(freeze(&s, &p, evaluator, &draft, &input).await, plan);
        assert_eq!(
            s.register_development_measurement(
                "actual-program",
                MeasurementProgram::JsonComparisonV1
            )
            .await
            .unwrap(),
            evaluator
        );
        assert_eq!(fingerprint(&pool).await, before);
        let leaked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM events WHERE kind LIKE 'development.trial.%' AND payload::text LIKE '%expected%')").fetch_one(&pool).await.unwrap();
        assert!(
            !leaked,
            "golden expected values belong only in private case rows"
        );
        let recovered = s.recover_development_trial_work(plan.id).await.unwrap();
        assert_eq!(recovered.attempts.len(), 6);
        assert_eq!(recovered.outcome, Outcome::Unverified);
        assert!(
            recovered
                .attempts
                .iter()
                .all(|a| a.state == TrialAttemptState::Pending && !a.production_measurement)
        );
    }
    #[sqlx::test]
    #[ignore = "actual isolated schema30 PG, no provider"]
    async fn invalid_or_late_failed_plan_never_leaves_partial_start_trial(pool: PgPool) {
        let (s, _, p, evaluator, draft, input) = subject(&pool).await;
        let mut invalid = draft.clone();
        invalid.cases.pop();
        let expected = s.snapshot().await.unwrap().unwrap().expected();
        let before = fingerprint(&pool).await;
        assert!(
            s.freeze_development_trial_plan(
                "invalid",
                expected,
                &input,
                p.development_id,
                p.development_version,
                evaluator,
                ValidationRoute::CrossScenarioHabit,
                &invalid
            )
            .await
            .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
        sqlx::raw_sql("CREATE FUNCTION trial_attempt_late_fail() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.side='trial' THEN RAISE EXCEPTION 'synthetic late failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER trial_attempt_late_fail BEFORE INSERT ON development_trial_attempts FOR EACH ROW EXECUTE FUNCTION trial_attempt_late_fail();").execute(&pool).await.unwrap();
        let before = fingerprint(&pool).await;
        assert!(
            s.freeze_development_trial_plan(
                "frozen-plan",
                expected,
                &input,
                p.development_id,
                p.development_version,
                evaluator,
                ValidationRoute::CrossScenarioHabit,
                &draft
            )
            .await
            .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
        sqlx::query("DROP TRIGGER trial_attempt_late_fail ON development_trial_attempts")
            .execute(&pool)
            .await
            .unwrap();
        freeze(&s, &p, evaluator, &draft, &input).await;
    }
    #[sqlx::test]
    #[ignore = "actual schema30 PG and explicitly synthetic source model"]
    async fn work_is_bound_to_real_background_claim_and_original_source_snapshot(pool: PgPool) {
        let (s, _, p, evaluator, draft, input) = subject(&pool).await;
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        assert!(
            s.enqueue_development_trial_case("queued", plan.id, plan.cases[0].id)
                .await
                .is_err(),
            "foreground source is still queued"
        );
        finish_source_run(&s).await;
        let run = s
            .enqueue_development_trial_case("queued", plan.id, plan.cases[0].id)
            .await
            .unwrap();
        let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
        assert_eq!(lease.run_id, run);
        assert_eq!(lease.mode, crate::domain::cognition::RunMode::Background);
        let work = s
            .claim_development_trial_work(&lease)
            .await
            .unwrap()
            .unwrap();
        assert_eq!(work.evaluation_run_id, run);
        assert_eq!(work.plan.original_scope, None);
        assert_ne!(work.baseline.attempt_id, work.trial.attempt_id);
        let mut tx = pool.begin().await.unwrap();
        assert!(
            super::is_evaluation_run_in(&mut tx, lease.individual_id, run)
                .await
                .unwrap()
        );
        let sources = super::context_source_ids_in(&mut tx, &lease, &work.baseline)
            .await
            .unwrap();
        assert!(
            sources.contains(&work.baseline.attempt_id) && sources.contains(&work.trial.attempt_id)
        );
        let mut forged = work.baseline.clone();
        forged.evaluation_run_id = Uuid::new_v4();
        assert!(
            super::binding_plan_in(&mut tx, &lease, &forged)
                .await
                .is_err()
        );
        tx.rollback().await.unwrap();
        let before = fingerprint(&pool).await;
        assert_eq!(
            s.enqueue_development_trial_case("queued", plan.id, plan.cases[0].id)
                .await
                .unwrap(),
            run
        );
        assert_eq!(fingerprint(&pool).await, before);
    }
    #[sqlx::test]
    #[ignore = "actual isolated PostgreSQL, no model"]
    async fn ordinary_claim_works_without_trial_schema_and_partial_schema_fails_closed(
        pool: PgPool,
    ) {
        let s = setup(&pool).await;
        message(&s).await;
        let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
        sqlx::raw_sql("DROP TABLE development_trial_case_sources,development_trial_attempts,development_trial_cases,development_trial_plans,development_measurement_programs CASCADE")
            .execute(&pool).await.unwrap();
        assert!(
            s.pending_development_trial_cases(10)
                .await
                .unwrap()
                .is_empty()
        );
        assert!(
            s.claim_development_trial_work(&lease)
                .await
                .unwrap()
                .is_none()
        );
        assert!(
            s.expire_development_trial_plans(1)
                .await
                .unwrap()
                .is_empty()
        );
        sqlx::query("CREATE TABLE development_trial_plans(id uuid)")
            .execute(&pool)
            .await
            .unwrap();
        assert!(matches!(
            s.pending_development_trial_cases(10).await,
            Err(StoreError::Invalid("incomplete development trial schema"))
        ));
        assert!(matches!(
            s.claim_development_trial_work(&lease).await,
            Err(StoreError::Invalid("incomplete development trial schema"))
        ));
        assert!(matches!(
            s.expire_development_trial_plans(1).await,
            Err(StoreError::Invalid("incomplete development trial schema"))
        ));
    }
    #[sqlx::test]
    #[ignore = "actual isolated PostgreSQL, no model"]
    async fn incomplete_work_cannot_finish_and_final_reject_is_atomic_and_stably_replayed(
        pool: PgPool,
    ) {
        let (s, _, p, evaluator, draft, input) = subject(&pool).await;
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        finish_source_run(&s).await;
        s.enqueue_development_trial_case("case", plan.id, plan.cases[0].id)
            .await
            .unwrap();
        let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
        s.claim_development_trial_work(&lease)
            .await
            .unwrap()
            .unwrap();
        let before = fingerprint(&pool).await;
        assert!(matches!(
            s.finish_development_trial_work(&lease).await,
            Err(StoreError::Invalid("trial attempt incomplete"))
        ));
        assert_eq!(fingerprint(&pool).await, before);
        assert!(
            s.finalize_development_trial_plan(
                "reject",
                plan.id,
                TrialDisposition::Reject {
                    reason: "insufficient measurements".into()
                }
            )
            .await
            .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
        // Explicit fault fixture closes only never-issued work; no result or
        // usage row is fabricated and no behavioral success is claimed.
        sqlx::query("UPDATE development_trial_attempts SET state='not_run' WHERE evaluation_run_id=$1 AND reservation_id IS NULL").bind(lease.run_id).execute(&pool).await.unwrap();
        let initial = s.snapshot().await.unwrap().unwrap();
        let result = s.finish_development_trial_work(&lease).await.unwrap();
        assert_eq!(result.outcome, Outcome::Unverified);
        let after = s.snapshot().await.unwrap().unwrap();
        assert_eq!(initial.state_version, after.state_version);
        assert_eq!(initial.persona, after.persona);
        sqlx::raw_sql("CREATE FUNCTION fail_trial_finalize_outbox() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN RAISE EXCEPTION 'synthetic final outbox failure'; END $$; CREATE TRIGGER fail_trial_finalize_outbox BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION fail_trial_finalize_outbox();").execute(&pool).await.unwrap();
        let before = fingerprint(&pool).await;
        assert!(
            s.finalize_development_trial_plan(
                "reject",
                plan.id,
                TrialDisposition::Reject {
                    reason: "insufficient measurements".into()
                }
            )
            .await
            .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
        sqlx::query("DROP TRIGGER fail_trial_finalize_outbox ON outbox")
            .execute(&pool)
            .await
            .unwrap();
        let receipt = s
            .finalize_development_trial_plan(
                "reject",
                plan.id,
                TrialDisposition::Reject {
                    reason: "insufficient measurements".into(),
                },
            )
            .await
            .unwrap();
        assert_eq!(receipt.status, "rejected");
        let before = fingerprint(&pool).await;
        assert_eq!(
            s.finalize_development_trial_plan(
                "reject",
                plan.id,
                TrialDisposition::Reject {
                    reason: "insufficient measurements".into()
                }
            )
            .await
            .unwrap(),
            receipt
        );
        assert_eq!(fingerprint(&pool).await, before);
        assert!(matches!(
            s.finalize_development_trial_plan(
                "reject",
                plan.id,
                TrialDisposition::Reject {
                    reason: "changed reason".into()
                }
            )
            .await,
            Err(StoreError::IdempotencyConflict)
        ));
    }
    #[sqlx::test]
    #[ignore = "actual isolated schema30 PG, no provider"]
    async fn every_missing_case_is_persisted_unverified_without_persona_acceptance(pool: PgPool) {
        let (s, _, p, evaluator, draft, input) = subject(&pool).await;
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        let before = s.snapshot().await.unwrap().unwrap();
        let review = s
            .review_development_trial_plan("review-all-missing", plan.id)
            .await
            .unwrap();
        assert_eq!(review.outcome, Outcome::Unverified);
        assert_eq!(s.snapshot().await.unwrap().unwrap().persona, before.persona);
        let matrix:(i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM development_trial_attempts WHERE state='not_run'),(SELECT count(*) FROM development_observations WHERE outcome='unverified')").fetch_one(&pool).await.unwrap();
        assert_eq!(matrix, (6, 3));
        assert!(
            s.transition_development(
                "forbidden-accept",
                s.snapshot().await.unwrap().unwrap().expected(),
                &refs(&pool, p.development_id, &DevelopmentCommand::Accept).await,
                p.development_id,
                plan.development_version + 1,
                &DevelopmentCommand::Accept
            )
            .await
            .is_err()
        );
        let before = fingerprint(&pool).await;
        assert_eq!(
            s.review_development_trial_plan("review-all-missing", plan.id)
                .await
                .unwrap(),
            review
        );
        assert_eq!(fingerprint(&pool).await, before);
    }
    #[sqlx::test]
    #[ignore = "actual isolated schema30 PG, no provider"]
    async fn fake_program_identity_and_missing_general_edges_do_not_bypass_sources(pool: PgPool) {
        let (s, event, p, evaluator, draft, input) = subject(&pool).await;
        let fake = registered(&pool).await;
        let before = fingerprint(&pool).await;
        assert!(
            s.freeze_development_trial_plan(
                "fake-program",
                s.snapshot().await.unwrap().unwrap().expected(),
                &input,
                p.development_id,
                p.development_version,
                fake,
                ValidationRoute::CrossScenarioHabit,
                &draft
            )
            .await
            .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        sqlx::query("DELETE FROM derivation_edges")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
            .bind(event)
            .execute(&pool)
            .await
            .unwrap();
        assert!(s.recover_development_trial_work(plan.id).await.is_err());
        let before = fingerprint(&pool).await;
        assert!(
            s.review_development_trial_plan("blocked", plan.id)
                .await
                .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
    }
    #[sqlx::test]
    #[ignore = "actual schema30 PG; singleton uniqueness removed only in isolated collision fixture"]
    async fn actual_trial_fk_edges_ignore_other_owner_uuid_collisions_without_general_edges(
        pool: PgPool,
    ) {
        let (s, _, p, evaluator, draft, input) = subject(&pool).await;
        let plan = freeze(&s, &p, evaluator, &draft, &input).await;
        let own = s.snapshot().await.unwrap().unwrap().id;
        let other = Uuid::new_v4();
        // Fault-injection only: product still supports exactly one active identity.
        sqlx::query("ALTER TABLE individuals DROP CONSTRAINT individuals_singleton_key")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO individuals(id,origin,seed,seed_digest,persona,runtime_config) SELECT $1,origin,seed,seed_digest,persona,runtime_config FROM individuals WHERE id=$2")
            .bind(other).bind(own).execute(&pool).await.unwrap();
        for table in [
            "objects",
            "events",
            "relationships",
            "development_targets",
            "development_verifiers",
            "developments",
            "development_steps",
            "development_evidence",
            "development_dependencies",
            "development_measurement_programs",
            "development_trial_plans",
            "development_trial_cases",
            "development_trial_attempts",
            "development_trial_case_sources",
        ] {
            let columns:Vec<String>=sqlx::query_scalar("SELECT column_name FROM information_schema.columns WHERE table_schema='public' AND table_name=$1 AND is_identity='NO' ORDER BY ordinal_position")
                .bind(table).fetch_all(&pool).await.unwrap();
            let names = columns
                .iter()
                .map(|n| format!("\"{n}\""))
                .collect::<Vec<_>>()
                .join(",");
            let values = columns
                .iter()
                .map(|n| {
                    if n == "individual_id" {
                        "$1".into()
                    } else {
                        format!("\"{n}\"")
                    }
                })
                .collect::<Vec<String>>()
                .join(",");
            sqlx::query(&format!(
                "INSERT INTO {table} ({names}) SELECT {values} FROM {table} WHERE individual_id=$2"
            ))
            .bind(other)
            .bind(own)
            .execute(&pool)
            .await
            .unwrap();
        }
        let collision = Uuid::new_v4();
        for (owner, blocked) in [(own, true), (other, false)] {
            sqlx::query(
                "INSERT INTO objects(individual_id,id,kind,blocked) VALUES($1,$2,'event',$3)",
            )
            .bind(owner)
            .bind(collision)
            .bind(blocked)
            .execute(&pool)
            .await
            .unwrap();
            sqlx::query("INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id) VALUES($1,$2,'synthetic.collision','direct_observation','system',clock_timestamp(),'{}',$2)")
                .bind(owner).bind(collision).execute(&pool).await.unwrap();
        }
        sqlx::query("INSERT INTO development_trial_case_sources(individual_id,case_id,event_id) VALUES($1,$2,$3)")
            .bind(other).bind(plan.cases[0].id).bind(collision).execute(&pool).await.unwrap();
        sqlx::query("DELETE FROM derivation_edges")
            .execute(&pool)
            .await
            .unwrap();
        let mut tx = pool.begin().await.unwrap();
        assert!(
            super::development::available(&mut tx, own, &[plan.cases[0].id])
                .await
                .unwrap()
        );
        let closure =
            crate::adapters::deletion::expand_deletion_objects(&mut tx, own, &[collision], false)
                .await
                .unwrap();
        assert!(
            !closure.contains(&plan.cases[0].id),
            "foreign owner edge must not delete own case"
        );
        tx.rollback().await.unwrap();
    }
}

/// P05 calls this inside its existing purge transaction after expanding actual
/// sources. Private plan/criterion/context/output copies are scrubbed; accounted
/// reservations, usage and opaque audit associations are never refunded/deleted.
pub(super) async fn purge_development_trials_in(
    tx: &mut Tx<'_>,
    individual: Uuid,
    ids: &[Uuid],
) -> Result<(), StoreError> {
    if reference_edges_in(tx).await?.is_none() {
        return Ok(());
    }
    let plans:Vec<Uuid>=sqlx::query_scalar("SELECT p.id FROM development_trial_plans p WHERE p.individual_id=$1 AND (p.id=ANY($2) OR p.development_id=ANY($2) OR p.trial_event_id=ANY($2) OR EXISTS(SELECT 1 FROM development_trial_cases c WHERE c.individual_id=$1 AND c.plan_id=p.id AND (c.id=ANY($2) OR c.observation_event_id=ANY($2))) OR EXISTS(SELECT 1 FROM development_trial_attempts a WHERE a.individual_id=$1 AND a.plan_id=p.id AND (a.id=ANY($2) OR a.result_event_id=ANY($2))))")
        .bind(individual).bind(ids).fetch_all(&mut **tx).await?;
    sqlx::query("UPDATE development_trial_attempts SET context_json=NULL,request_digest=NULL,input_bound=NULL,output_cap=NULL,result=NULL,state=CASE WHEN state IN ('dispatched','recorded','unknown') THEN 'unknown' ELSE 'not_run' END,outcome='unverified' WHERE individual_id=$1 AND plan_id=ANY($2)")
        .bind(individual).bind(&plans).execute(&mut **tx).await?;
    sqlx::query("UPDATE development_trial_cases SET case_data='{}',status='not_run' WHERE individual_id=$1 AND plan_id=ANY($2)")
        .bind(individual).bind(&plans).execute(&mut **tx).await?;
    sqlx::query("UPDATE development_trial_plans SET plan='{}',status='stale' WHERE individual_id=$1 AND id=ANY($2)")
        .bind(individual).bind(&plans).execute(&mut **tx).await?;
    Ok(())
}

/// Read the exact saved side within the already validated actual case. No new
/// context/clock/key is manufactured while recovering an accounted request.
pub(super) async fn request_context_in(
    tx: &mut Tx<'_>,
    lease: &RunLease,
    binding: &TrialBinding,
    side: TrialSide,
) -> Result<Option<(ModelContext, TokenAllowance)>, StoreError> {
    binding_plan_in(tx, lease, binding).await?;
    let (request_hash,id,context,input,output,run,generation):SavedRequestRow=
        sqlx::query_as("SELECT request_digest,id,context_json,input_bound,output_cap,evaluation_run_id,lease_generation FROM development_trial_attempts WHERE individual_id=$1 AND plan_id=$2 AND case_id=$3 AND side=$4")
            .bind(lease.individual_id).bind(binding.plan_id).bind(binding.case_id).bind(side.as_str()).fetch_one(&mut **tx).await?;
    if run != Some(lease.run_id) || generation != Some(lease.generation) {
        return Err(StoreError::StaleState);
    }
    let (request_hash, context, input, output) = match (request_hash, context, input, output) {
        (None, None, None, None) => return Ok(None),
        (Some(hash), Some(context), Some(input), Some(output))
            if input > 0 && (1..=8192).contains(&output) =>
        {
            (hash, context, input, output)
        }
        _ => return Err(StoreError::Invalid("partial saved trial request")),
    };
    let context: ModelContext = serde_json::from_value(context)?;
    let allowance = TokenAllowance {
        input_upper_bound: input as u64,
        output_cap: output as u32,
    };
    if digest(&json!({"context": context, "allowance": allowance}))? != request_hash {
        return Err(StoreError::StaleState);
    }
    let selected = TrialBinding {
        plan_id: binding.plan_id,
        case_id: binding.case_id,
        side,
        attempt_id: id,
        evaluation_run_id: lease.run_id,
        lease_generation: lease.generation,
    };
    check_context_in(tx, lease, &selected, &context).await?;
    Ok(Some((
        context,
        TokenAllowance {
            input_upper_bound: input as u64,
            output_cap: output as u32,
        },
    )))
}

async fn all_sources_in(
    tx: &mut Tx<'_>,
    individual: Uuid,
    roots: &[Uuid],
) -> Result<Vec<Uuid>, StoreError> {
    let end = development::LINKS
        .rfind(')')
        .expect("static development graph");
    let sources:Vec<Uuid>=sqlx::query_scalar(&format!("{} UNION SELECT target,source FROM ({TRIAL_REFERENCES}) actual_trial(source,target)), closure(id) AS (SELECT unnest($2::uuid[]) UNION SELECT l.source FROM closure c JOIN links l ON l.target=c.id) SELECT id FROM closure ORDER BY id LIMIT 4097",&development::LINKS[..end]))
        .bind(individual).bind(roots).fetch_all(&mut **tx).await?;
    if sources.len() > 4096 || !development::available(tx, individual, &sources).await? {
        return Err(StoreError::StaleState);
    }
    Ok(sources)
}

impl Store {
    pub async fn finish_development_trial_work(
        &self,
        lease: &RunLease,
    ) -> Result<TrialWorkStatus, StoreError> {
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        require_lease(&mut tx, lease).await?;
        let (plan_id,case_id,stored_generation):(Uuid,Uuid,Option<i64>)=sqlx::query_as("SELECT plan_id,id,lease_generation FROM development_trial_cases WHERE individual_id=$1 AND evaluation_run_id=$2")
            .bind(lease.individual_id).bind(lease.run_id).fetch_optional(&mut *tx).await?.ok_or(StoreError::NotFound)?;
        if stored_generation != Some(lease.generation) {
            return Err(StoreError::StaleState);
        }
        let plan = plan_in(&mut tx, lease.individual_id, plan_id, true).await?;
        let ids:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM development_trial_attempts WHERE individual_id=$1 AND case_id=$2 ORDER BY side")
            .bind(lease.individual_id).bind(case_id).fetch_all(&mut *tx).await?;
        if ids.len() != 2 {
            return Err(StoreError::Invalid("missing trial sides"));
        }
        let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM usage_reservations WHERE individual_id=$1 AND run_id=$2 AND status IN ('reserved','dispatched'))")
            .bind(lease.individual_id).bind(lease.run_id).fetch_one(&mut *tx).await?;
        if pending {
            return Err(StoreError::Invalid("trial accounting recovery incomplete"));
        }
        let mut attempts = vec![];
        for id in ids {
            let a = attempt_in(&mut tx, lease.individual_id, id).await?;
            if !matches!(
                a.state,
                TrialAttemptState::Recorded
                    | TrialAttemptState::Unknown
                    | TrialAttemptState::NotRun
            ) {
                return Err(StoreError::Invalid("trial attempt incomplete"));
            }
            attempts.push(a);
        }
        let case = plan
            .cases
            .iter()
            .find(|c| c.id == case_id)
            .ok_or(StoreError::NotFound)?;
        let (_, outcome) = observation_in(&mut tx, &plan, case, true)
            .await?
            .ok_or(StoreError::Invalid("trial observation missing"))?;
        sqlx::query("UPDATE runs SET status='committed',finished_at=clock_timestamp(),lease_owner=NULL,lease_expires_at=NULL WHERE individual_id=$1 AND id=$2")
            .bind(lease.individual_id).bind(lease.run_id).execute(&mut *tx).await?;
        outbox(
            &mut tx,
            lease.individual_id,
            "operation.updated",
            lease.run_id,
            lease.state_version,
        )
        .await?;
        commit(self, tx, generation).await?;
        Ok(TrialWorkStatus {
            plan_id,
            attempts,
            outcome,
        })
    }

    pub async fn finalize_development_trial_plan(
        &self,
        key: &str,
        plan_id: Uuid,
        disposition: TrialDisposition,
    ) -> Result<development::DevelopmentReceipt, StoreError> {
        let generation = self.privacy_generation();
        let mut tx = self.write().await?;
        let hash = digest(&json!({"plan_id":plan_id,"disposition":disposition}))?;
        let current = state(&mut tx).await?.ok_or(StoreError::NotInitialized)?;
        if let Some(receipt) = replay::<development::DevelopmentReceipt>(
            &mut tx,
            "system",
            "/internal/development-trials/finalize",
            key,
            &hash,
        )
        .await?
        {
            let plan = plan_in(&mut tx, current.id, plan_id, false).await?;
            if receipt.development_id != plan.development_id {
                return Err(StoreError::StaleState);
            }
            development::replay_receipt(&mut tx, &[plan.id], &receipt).await?;
            self.require_privacy_snapshot(generation)?;
            return Ok(receipt);
        }
        let plan = plan_in(&mut tx, current.id, plan_id, false).await?;
        let (candidate, version, target) =
            trial_candidate_in(&mut tx, current.id, plan.development_id).await?;
        let running:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM development_trial_cases c JOIN runs r ON (r.individual_id,r.id)=(c.individual_id,c.evaluation_run_id) WHERE c.individual_id=$1 AND c.plan_id=$2 AND r.status='running')")
            .bind(current.id).bind(plan.id).fetch_one(&mut *tx).await?;
        if running {
            return Err(StoreError::Invalid(
                "trial work must finish before finalization",
            ));
        }
        let already_expired = matches!(disposition, TrialDisposition::Expire)
            && candidate.status() == DevelopmentStatus::Expired;
        let notification_version = if already_expired {
            current.state_version
        } else {
            current
                .state_version
                .checked_add(1)
                .ok_or(StoreError::Invalid("state version exhausted"))?
        };
        close_queued_work_in(&mut tx, &plan, notification_version).await?;
        if matches!(disposition, TrialDisposition::Expire)
            && candidate.status() == DevelopmentStatus::Expired
        {
            let bound:bool=sqlx::query_scalar("SELECT status='expired' AND review_event_id IS NOT NULL AND review_development_version=$3 FROM development_trial_plans WHERE individual_id=$1 AND id=$2")
                .bind(current.id).bind(plan.id).bind(version).fetch_one(&mut *tx).await?;
            if !bound {
                return Err(StoreError::StaleState);
            }
            let event:Uuid=sqlx::query_scalar("SELECT event_id FROM development_steps WHERE individual_id=$1 AND development_id=$2 AND version=$3 AND status='expired'")
                .bind(current.id).bind(plan.development_id).bind(version).fetch_one(&mut *tx).await?;
            let receipt = development::DevelopmentReceipt {
                individual_id: current.id,
                development_id: plan.development_id,
                event_id: event,
                state_version: current.state_version,
                development_version: version,
                status: "expired".into(),
                revision_id: None,
            };
            development::replay_receipt(&mut tx, &[plan.id], &receipt).await?;
            save_receipt(
                &mut tx,
                "system",
                "/internal/development-trials/finalize",
                key,
                &hash,
                &receipt,
            )
            .await?;
            commit(self, tx, generation).await?;
            return Ok(receipt);
        }
        let command = match disposition {
            TrialDisposition::Accept => {
                let (status,review,review_version,epoch,config):(String,Option<Uuid>,Option<i64>,i64,i64)=sqlx::query_as("SELECT status,review_event_id,review_development_version,privacy_epoch,config_version FROM development_trial_plans WHERE individual_id=$1 AND id=$2")
                    .bind(current.id).bind(plan.id).fetch_one(&mut *tx).await?;
                let (paused, frozen): (bool, bool) = sqlx::query_as(
                    "SELECT autonomy_paused,budget_frozen FROM individuals WHERE id=$1",
                )
                .bind(current.id)
                .fetch_one(&mut *tx)
                .await?;
                if status != "reviewed"
                    || review.is_none()
                    || review_version != Some(version)
                    || target != plan.target
                    || epoch != current.privacy_epoch
                    || config != current.config_version
                    || sqlx::query_scalar::<_, bool>("SELECT clock_timestamp()>=$1")
                        .bind(plan.deadline)
                        .fetch_one(&mut *tx)
                        .await?
                    || candidate.status() != DevelopmentStatus::Trial
                    || current.lifecycle != crate::domain::Lifecycle::Active
                    || paused
                    || frozen
                    || current.runtime_config["autonomy"]["enabled"] != true
                {
                    return Err(StoreError::StaleState);
                }
                for case in &plan.cases {
                    let measured:Vec<CaseResultRow>=sqlx::query_as("SELECT id,side,state,result,dispatch_mode FROM development_trial_attempts WHERE individual_id=$1 AND case_id=$2")
                        .bind(current.id).bind(case.id).fetch_all(&mut *tx).await?;
                    let usable = |side: &str| -> Option<&Value> {
                        let (_, _, state, result, mode) = measured.iter().find(|r| r.1 == side)?;
                        let result = result.as_ref()?;
                        if state != "recorded"
                            || mode.as_deref() != Some("live")
                            || result["isolated"] != true
                            || result["mode_matches"] != true
                        {
                            return None;
                        }
                        result.get("proposal")
                    };
                    if measured.len() != 2
                        || measure_case(case, usable("baseline"), usable("trial"))
                            != Outcome::Passed
                    {
                        return Err(StoreError::Invalid(
                            "final trial results do not prove improvement",
                        ));
                    }
                    let (observation,observed):(Option<Uuid>,Option<String>)=sqlx::query_as("SELECT c.observation_event_id,o.outcome FROM development_trial_cases c LEFT JOIN development_observations o ON (o.individual_id,o.event_id)=(c.individual_id,c.observation_event_id) WHERE c.individual_id=$1 AND c.id=$2")
                        .bind(current.id).bind(case.id).fetch_one(&mut *tx).await?;
                    if observation.is_none() || observed.as_deref() != Some("passed") {
                        return Err(StoreError::Invalid(
                            "unverified or failed frozen trial case",
                        ));
                    }
                    let rows:Vec<FinalAttemptRow>=sqlx::query_as("SELECT id,context_json,reservation_id,dispatch_mode,state FROM development_trial_attempts WHERE individual_id=$1 AND case_id=$2")
                        .bind(current.id).bind(case.id).fetch_all(&mut *tx).await?;
                    if rows.len() != 2 {
                        return Err(StoreError::Invalid("missing final trial sides"));
                    }
                    for (attempt, context, reservation, mode, state) in rows {
                        if state != "recorded" || mode.as_deref() != Some("live") {
                            return Err(StoreError::Invalid("nonproduction trial measurement"));
                        }
                        let context: ModelContext = serde_json::from_value(context)?;
                        let binding = context
                            .trial_binding
                            .as_ref()
                            .ok_or(StoreError::Invalid("final trial request unbound"))?;
                        if binding.plan_id != plan.id
                            || binding.case_id != case.id
                            || binding.attempt_id != attempt
                            || context.individual_id != current.id
                            || context.run_id != binding.evaluation_run_id
                        {
                            return Err(StoreError::StaleState);
                        }
                        let (request,input,output,lease_generation,key):(String,i64,i64,i64,Uuid)=sqlx::query_as("SELECT request_digest,input_bound,output_cap,lease_generation,request_key FROM usage_reservations WHERE id=$1 AND individual_id=$2 AND run_id=$3 AND status='settled' AND dispatched_at IS NOT NULL")
                            .bind(reservation.ok_or(StoreError::Invalid("final trial accounting absent"))?).bind(current.id).bind(binding.evaluation_run_id).fetch_optional(&mut *tx).await?.ok_or(StoreError::Invalid("unsettled final trial accounting"))?;
                        if key != attempt
                            || lease_generation != binding.lease_generation
                            || digest(
                                &json!({"context":context,"allowance":TokenAllowance{input_upper_bound:input as u64,output_cap:output as u32}}),
                            )? != request
                        {
                            return Err(StoreError::StaleState);
                        }
                    }
                }
                DevelopmentCommand::Accept
            }
            TrialDisposition::Reject { reason } => DevelopmentCommand::Reject { reason },
            TrialDisposition::Expire => DevelopmentCommand::Expire,
        };
        for case in &plan.cases {
            observation_in(&mut tx, &plan, case, true).await?;
        }
        let manifest = all_sources_in(
            &mut tx,
            current.id,
            &[plan.id, plan.development_id, current.id],
        )
        .await?;
        let applied = transition_in_tx(
            &mut tx,
            current.id,
            &manifest,
            plan.development_id,
            version,
            &command,
            Some(key),
        )
        .await?;
        let next = current
            .state_version
            .checked_add(1)
            .ok_or(StoreError::Invalid("state version exhausted"))?;
        outbox(
            &mut tx,
            current.id,
            "development.changed",
            plan.development_id,
            next,
        )
        .await?;
        advance(&mut tx, current.id, next).await?;
        let receipt = development::DevelopmentReceipt {
            individual_id: current.id,
            development_id: plan.development_id,
            event_id: applied.event_id,
            state_version: next,
            development_version: applied.development_version,
            status: applied.status,
            revision_id: applied.revision_id,
        };
        save_receipt(
            &mut tx,
            "system",
            "/internal/development-trials/finalize",
            key,
            &hash,
            &receipt,
        )
        .await?;
        commit(self, tx, generation).await?;
        Ok(receipt)
    }
}
