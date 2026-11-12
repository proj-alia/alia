// Included by the registered production runtime module under cfg(test).
use super::{TrialRuntimeError, bound_case_context, trial_context};
use crate as alia;
use alia::domain::development_trials::{
    MEASUREMENT_VERSION, TrialBinding, TrialCase, TrialCaseRole, TrialCriterion, TrialPlan,
    TrialSide, TrialWork,
};
use alia::domain::{cognition::ModelContext, development::*, development_context::*};
use alia::runtime::scheduling::RunLease;
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use uuid::Uuid;

fn work_fixture(
    target: DevelopmentTarget,
    scope: Option<StrategyScope>,
) -> (
    ModelContext,
    DevelopmentCandidate,
    RunLease,
    TrialWork,
    DateTime<Utc>,
) {
    let (mut base, candidate, current, mut lease, now) = fixture(target, scope);
    // This is a separate accounting run, even for an original Task subject.
    lease.run_id = Uuid::new_v4();
    base.run_id = lease.run_id;
    base.current_goal_id = Some(Uuid::new_v4());
    if target == DevelopmentTarget::TaskStrategy {
        base.development_context.as_mut().unwrap().heads.clear();
    }
    let cases: Vec<TrialCase> = [
        ("first", TrialCaseRole::Scenario),
        ("second", TrialCaseRole::Scenario),
        ("counter", TrialCaseRole::CounterSearch),
    ]
    .into_iter()
    .map(|(context, role)| TrialCase {
        id: Uuid::new_v4(),
        context: context.into(),
        prompt: format!("Only the actual evaluation instruction for {context}"),
        role,
        criterion: TrialCriterion::JsonEquals {
            pointer: "/reply".into(),
            expected: json!("PRIVATE_EXPECTED_ANSWER_MUST_NOT_ENTER_MODEL_INPUT"),
        },
        source_event_ids: vec![base.current_event_id],
    })
    .collect();
    let trial_event = Uuid::new_v4();
    base.input_manifest.push(trial_event);
    base.events.push(alia::domain::history::Event {
        individual_id: base.individual_id,
        id: base.current_event_id,
        sequence: 1,
        kind: "development.evaluation_case".into(),
        source_kind: alia::domain::history::SourceKind::Generated,
        actor_id: "synthetic-controller".into(),
        occurred_at: now,
        recorded_at: now,
        payload: json!({"text":cases[0].prompt}),
        correlation_id: Uuid::new_v4(),
        causation_id: None,
    });
    let plan = TrialPlan {
        id: Uuid::new_v4(),
        individual_id: base.individual_id,
        development_id: candidate.id(),
        development_version: 2,
        draft: candidate.draft().clone(),
        proposer_id: candidate.proposer_id(),
        trial_event_id: trial_event,
        target: current,
        original_scope: candidate.before().scope.clone(),
        evaluator_id: Uuid::new_v4(),
        criterion_version: MEASUREMENT_VERSION.into(),
        cases_digest: "ab".repeat(32),
        cases: cases.clone(),
        deadline: now + Duration::minutes(30),
    };
    let binding = |side| TrialBinding {
        plan_id: plan.id,
        case_id: cases[0].id,
        side,
        attempt_id: Uuid::new_v4(),
        evaluation_run_id: lease.run_id,
        lease_generation: lease.generation,
    };
    let baseline = binding(TrialSide::Baseline);
    let trial = binding(TrialSide::Trial);
    let work = TrialWork {
        plan,
        case: cases[0].clone(),
        evaluation_run_id: lease.run_id,
        baseline,
        trial,
    };
    (base, candidate, lease, work, now)
}

#[test]
fn original_scope_and_real_evaluation_lease_are_separate_without_criteria_leakage() {
    for scope in [
        StrategyScope::Task(Uuid::new_v4()),
        StrategyScope::Goal(Uuid::new_v4()),
    ] {
        let (base, candidate, lease, work, now) =
            work_fixture(DevelopmentTarget::TaskStrategy, Some(scope.clone()));
        let before = serde_json::to_value(&base).unwrap();
        let baseline =
            bound_case_context(&base, &candidate, &work, &work.baseline, &lease, now).unwrap();
        let trial = bound_case_context(&base, &candidate, &work, &work.trial, &lease, now).unwrap();
        assert_eq!(trial.run_id, lease.run_id);
        assert_eq!(trial.current_goal_id, base.current_goal_id);
        assert_ne!(
            trial.run_id,
            match scope {
                StrategyScope::Task(id) | StrategyScope::Goal(id) => id,
            }
        );
        assert_eq!(baseline.persona, base.persona);
        assert_eq!(trial.persona, base.persona);
        assert_eq!(
            trial
                .development_context
                .as_ref()
                .unwrap()
                .effective_task_strategy
                .as_ref()
                .unwrap()
                .scope,
            scope
        );
        let mut expected = before.clone();
        expected["trial_binding"] = serde_json::to_value(&work.trial).unwrap();
        expected["development_context"]["effective_task_strategy"] =
            candidate.draft().proposed_value.clone();
        assert_eq!(serde_json::to_value(&trial).unwrap(), expected);
        assert!(
            !serde_json::to_string(&trial)
                .unwrap()
                .contains("PRIVATE_EXPECTED_ANSWER")
        );
        assert_eq!(serde_json::to_value(&base).unwrap(), before);
    }
}

#[test]
fn bound_persona_baseline_and_trial_only_change_the_target_and_opaque_binding() {
    let (base, candidate, lease, work, now) = work_fixture(DevelopmentTarget::Traits, None);
    let baseline =
        bound_case_context(&base, &candidate, &work, &work.baseline, &lease, now).unwrap();
    let trial = bound_case_context(&base, &candidate, &work, &work.trial, &lease, now).unwrap();
    let mut expected = serde_json::to_value(&base).unwrap();
    expected["trial_binding"] = serde_json::to_value(&work.baseline).unwrap();
    assert_eq!(serde_json::to_value(baseline).unwrap(), expected);
    expected["trial_binding"] = serde_json::to_value(&work.trial).unwrap();
    expected["persona"]["personality"]["traits"] = candidate.draft().proposed_value.clone();
    assert_eq!(serde_json::to_value(trial).unwrap(), expected);
}

#[test]
fn arbitrary_side_attempt_run_or_generation_cannot_relabel_a_case_request() {
    let (base, candidate, lease, work, now) = work_fixture(DevelopmentTarget::Traits, None);
    for mutate in [
        |b: &mut TrialBinding| b.plan_id = Uuid::new_v4(),
        |b: &mut TrialBinding| b.case_id = Uuid::new_v4(),
        |b: &mut TrialBinding| b.attempt_id = Uuid::new_v4(),
        |b: &mut TrialBinding| b.evaluation_run_id = Uuid::new_v4(),
        |b: &mut TrialBinding| b.lease_generation += 1,
    ] {
        let mut bad = work.trial.clone();
        mutate(&mut bad);
        assert!(matches!(
            bound_case_context(&base, &candidate, &work, &bad, &lease, now),
            Err(TrialRuntimeError::Binding)
        ));
    }
    let mut bad = work.clone();
    bad.case.prompt = "Substituted instruction".into();
    assert!(matches!(
        bound_case_context(&base, &candidate, &bad, &bad.trial, &lease, now),
        Err(TrialRuntimeError::Binding)
    ));
    let mut bad = base.clone();
    bad.events[0].payload = json!({"text":"Not the actual frozen case prompt"});
    assert!(matches!(
        bound_case_context(&bad, &candidate, &work, &work.trial, &lease, now),
        Err(TrialRuntimeError::Prompt)
    ));
}

fn fixture(
    target: DevelopmentTarget,
    scope: Option<StrategyScope>,
) -> (
    ModelContext,
    DevelopmentCandidate,
    TargetSnapshot,
    RunLease,
    DateTime<Utc>,
) {
    let now = Utc::now();
    let individual = Uuid::new_v4();
    let run = match scope {
        Some(StrategyScope::Task(id)) => id,
        _ => Uuid::new_v4(),
    };
    let goal = match scope {
        Some(StrategyScope::Goal(id)) => Some(id),
        _ => None,
    };
    let persona: Value =
        serde_json::from_slice(include_bytes!("../../config/persona.example.json")).unwrap();
    let value = target
        .persona_pointer()
        .map(|p| persona.pointer(p).unwrap().clone())
        .unwrap_or(Value::Null);
    let mut proposed = value.clone();
    match target {
        DevelopmentTarget::Traits => proposed["curiosity"] = json!(0.91),
        DevelopmentTarget::Values => proposed[0]["priority"] = json!(0.41),
        DevelopmentTarget::Expression => proposed["tone"] = json!("bounded trial tone"),
        DevelopmentTarget::CognitivePreferences => proposed
            .as_array_mut()
            .unwrap()
            .push(json!("trial-only habit")),
        DevelopmentTarget::TaskStrategy => {
            proposed = json!({"scope":scope,"description":"only this declared scope"})
        }
    }
    let current = TargetSnapshot {
        target,
        version: 3,
        head_revision_id: Some(Uuid::new_v4()),
        value,
        scope,
    };
    let draft = DevelopmentDraft {
        target,
        before_version: 3,
        proposed_value: proposed,
        proposed_description: "candidate only".into(),
        evidence_ids: vec![Uuid::new_v4()],
        counterevidence_ids: vec![Uuid::new_v4()],
        validation_plan: "actual independent cases required later".into(),
        deadline: now + Duration::hours(1),
    };
    let candidate = DevelopmentCandidate::prepare(
        Uuid::new_v4(),
        individual,
        Uuid::new_v4(),
        draft,
        &current,
        &persona,
        now - Duration::seconds(1),
    )
    .unwrap();
    let target_id = Uuid::new_v4();
    let birth = Uuid::new_v4();
    let event = Uuid::new_v4();
    let mut sources = vec![
        candidate.id(),
        target_id,
        birth,
        event,
        current.head_revision_id.unwrap(),
    ];
    sources.extend(candidate.draft().evidence_ids.iter().copied());
    sources.extend(candidate.draft().counterevidence_ids.iter().copied());
    if let Some(s) = &current.scope {
        sources.push(match s {
            StrategyScope::Task(id) | StrategyScope::Goal(id) => *id,
        });
    }
    let development = DevelopmentContext {
        seed_event_id: birth,
        heads: vec![CurrentDevelopmentHead {
            target_id,
            target,
            version: 3,
            revision_id: current.head_revision_id,
            scope: current.scope.clone(),
        }],
        effective_task_strategy: None,
        source_ids: sources.clone(),
    };
    let base:ModelContext=serde_json::from_value(json!({"individual_id":individual,"run_id":run,"state_version":8,"privacy_epoch":2,"config_version":1,"mode":"background","now":now,"persona":persona,"relationship":{},"current_event_id":event,"current_scope":"general","current_goal_id":goal,"events":[],"memories":[],"goals":[],"interests":[],"capabilities":[],"input_manifest":sources,"verified_goal_completions":[],"development_context":development})).unwrap();
    let lease = RunLease {
        individual_id: individual,
        run_id: run,
        owner_id: Uuid::new_v4(),
        generation: 1,
        expires_at: now + Duration::minutes(2),
        deadline: now + Duration::minutes(5),
        state_version: 8,
        privacy_epoch: 2,
        config_version: 1,
        mode: base.mode,
    };
    (base, candidate, current, lease, now)
}

#[test]
fn persona_projection_changes_only_the_candidate_path_and_never_the_baseline() {
    for target in [
        DevelopmentTarget::Traits,
        DevelopmentTarget::Values,
        DevelopmentTarget::Expression,
        DevelopmentTarget::CognitivePreferences,
    ] {
        let (base, candidate, current, lease, now) = fixture(target, None);
        let original = serde_json::to_value(&base).unwrap();
        let trial = trial_context(&base, &candidate, &current, &lease, now).unwrap();
        let mut expected = original.clone();
        *expected["persona"]
            .pointer_mut(target.persona_pointer().unwrap())
            .unwrap() = candidate.draft().proposed_value.clone();
        assert_eq!(serde_json::to_value(&trial).unwrap(), expected);
        assert_eq!(serde_json::to_value(&base).unwrap(), original);
        assert_eq!(
            trial.development_context.as_ref().unwrap().heads,
            base.development_context.as_ref().unwrap().heads
        );
    }
}

#[test]
fn task_and_goal_strategy_trials_remain_in_the_exact_scope() {
    for scope in [
        StrategyScope::Goal(Uuid::new_v4()),
        StrategyScope::Task(Uuid::new_v4()),
    ] {
        let (base, candidate, current, lease, now) =
            fixture(DevelopmentTarget::TaskStrategy, Some(scope));
        let trial = trial_context(&base, &candidate, &current, &lease, now).unwrap();
        let mut expected = serde_json::to_value(&base).unwrap();
        expected["development_context"]["effective_task_strategy"] =
            candidate.draft().proposed_value.clone();
        assert_eq!(serde_json::to_value(trial).unwrap(), expected);
        let mut another = base.clone();
        another.current_goal_id = Some(Uuid::new_v4());
        another.run_id = Uuid::new_v4();
        let mut another_lease = lease.clone();
        another_lease.run_id = another.run_id;
        assert!(matches!(
            trial_context(&another, &candidate, &current, &another_lease, now),
            Err(TrialRuntimeError::Scope)
        ));
    }
}

#[test]
fn a_goal_trial_cannot_replace_a_more_specific_accepted_task_strategy() {
    let (mut base, candidate, current, lease, now) = fixture(
        DevelopmentTarget::TaskStrategy,
        Some(StrategyScope::Goal(Uuid::new_v4())),
    );
    base.development_context
        .as_mut()
        .unwrap()
        .effective_task_strategy = Some(TaskStrategy {
        scope: StrategyScope::Task(base.run_id),
        description: "accepted task strategy".into(),
    });
    assert!(matches!(
        trial_context(&base, &candidate, &current, &lease, now),
        Err(TrialRuntimeError::Scope)
    ));
}

#[test]
fn stale_owner_snapshot_scope_and_target_heads_are_rejected() {
    let (base, candidate, current, lease, now) = fixture(DevelopmentTarget::Traits, None);
    for mutate in [
        |l: &mut RunLease| l.individual_id = Uuid::new_v4(),
        |l: &mut RunLease| l.run_id = Uuid::new_v4(),
        |l: &mut RunLease| l.owner_id = Uuid::nil(),
        |l: &mut RunLease| l.state_version += 1,
        |l: &mut RunLease| l.privacy_epoch += 1,
        |l: &mut RunLease| l.config_version += 1,
        |l: &mut RunLease| l.generation = 0,
    ] {
        let mut bad = lease.clone();
        mutate(&mut bad);
        assert!(matches!(
            trial_context(&base, &candidate, &current, &bad, now),
            Err(TrialRuntimeError::Stale)
        ));
    }
    for mutate in [
        |h: &mut TargetSnapshot| h.version += 1,
        |h: &mut TargetSnapshot| h.head_revision_id = Some(Uuid::new_v4()),
        |h: &mut TargetSnapshot| h.value = Value::Null,
    ] {
        let mut bad = current.clone();
        mutate(&mut bad);
        assert!(matches!(
            trial_context(&base, &candidate, &bad, &lease, now),
            Err(TrialRuntimeError::Stale)
        ));
    }
}

#[test]
fn missing_evidence_counterevidence_and_candidate_bindings_are_rejected() {
    let (base, candidate, current, lease, now) = fixture(DevelopmentTarget::Traits, None);
    for id in [
        candidate.id(),
        candidate.draft().evidence_ids[0],
        candidate.draft().counterevidence_ids[0],
        current.head_revision_id.unwrap(),
    ] {
        let mut bad = base.clone();
        bad.input_manifest.retain(|v| *v != id);
        assert!(matches!(
            trial_context(&bad, &candidate, &current, &lease, now),
            Err(TrialRuntimeError::Sources)
        ));
    }
    let mut ordinary = base.clone();
    ordinary.development_context = None;
    assert!(matches!(
        trial_context(&ordinary, &candidate, &current, &lease, now),
        Err(TrialRuntimeError::Sources)
    ));
}

#[test]
fn expiration_and_untracked_baseline_persona_changes_never_produce_a_trial_copy() {
    let (base, candidate, current, lease, now) = fixture(DevelopmentTarget::Traits, None);
    assert!(matches!(
        trial_context(&base, &candidate, &current, &lease, lease.expires_at),
        Err(TrialRuntimeError::Expired)
    ));
    let mut long = lease.clone();
    long.expires_at = candidate.draft().deadline + Duration::hours(1);
    long.deadline = long.expires_at;
    assert!(matches!(
        trial_context(
            &base,
            &candidate,
            &current,
            &long,
            candidate.draft().deadline
        ),
        Err(TrialRuntimeError::Expired)
    ));
    let mut bad = base.clone();
    bad.persona["personality"]["traits"]["curiosity"] = json!(0.02);
    assert!(matches!(
        trial_context(&bad, &candidate, &current, &lease, now),
        Err(TrialRuntimeError::Stale)
    ));
}
