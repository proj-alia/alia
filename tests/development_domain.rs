//! Executable domain tests use explicit synthetic server evidence, not real reviews.
use alia::domain::development::*;
use chrono::{DateTime, Duration, Utc};
use serde_json::{Value, json};
use uuid::Uuid;
fn now() -> DateTime<Utc> {
    "2026-11-12T14:00:00Z".parse().unwrap()
}
fn persona() -> Value {
    serde_json::from_slice(include_bytes!("../config/persona.example.json")).unwrap()
}
fn draft(target: DevelopmentTarget, value: Value) -> DevelopmentDraft {
    DevelopmentDraft {
        target,
        before_version: 3,
        proposed_value: value,
        proposed_description: "依据实际经历重新安排认识和价值".into(),
        evidence_ids: vec![Uuid::new_v4()],
        counterevidence_ids: vec![Uuid::new_v4()],
        validation_plan: "先核查反证，再在独立场景检验实际行为与无关回归".into(),
        deadline: now() + Duration::days(7),
    }
}
fn candidate(
    target: DevelopmentTarget,
    value: Value,
) -> (DevelopmentCandidate, TargetSnapshot, Value) {
    let p = persona();
    let snapshot = TargetSnapshot {
        target,
        version: 3,
        head_revision_id: Some(Uuid::new_v4()),
        value: p
            .pointer(target.persona_pointer().unwrap())
            .unwrap()
            .clone(),
        scope: None,
    };
    let c = DevelopmentCandidate::prepare(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        draft(target, value),
        &snapshot,
        &p,
        now(),
    )
    .unwrap();
    (c, snapshot, p)
}
fn source(
    c: &DevelopmentCandidate,
    id: Uuid,
    class: SourceClass,
    time: DateTime<Utc>,
) -> ServerSource {
    ServerSource {
        id,
        individual_id: c.individual_id(),
        class,
        recorded_at: time,
    }
}
fn evidence(c: &DevelopmentCandidate, class: SourceClass) -> Vec<ServerSource> {
    c.draft()
        .evidence_ids
        .iter()
        .map(|id| source(c, *id, class, now() - Duration::hours(1)))
        .chain(c.draft().counterevidence_ids.iter().map(|id| {
            source(
                c,
                *id,
                SourceClass::ExternalReport,
                now() - Duration::minutes(1),
            )
        }))
        .collect()
}
fn trial(c: &DevelopmentCandidate, route: ValidationRoute, class: SourceClass) -> Development {
    let token = TrustedValidator::authorize_trial(c, evidence(c, class), route, now()).unwrap();
    Development::propose(c.clone())
        .start_trial(&token, now())
        .unwrap()
}
fn assessment(
    d: &Development,
    contexts: &[&str],
    outcome: Outcome,
) -> (IndependentAssessment, Vec<ServerSource>) {
    let c = d.candidate();
    let mut sources = d.trial_sources().to_vec();
    let search = Uuid::new_v4();
    sources.push(source(
        c,
        search,
        SourceClass::ObservedOutcome,
        now() + Duration::hours(1),
    ));
    let mut observations = vec![];
    for context in contexts {
        let result = Uuid::new_v4();
        sources.push(source(
            c,
            result,
            SourceClass::ObservedOutcome,
            now() + Duration::hours(1),
        ));
        observations.push(ScenarioObservation {
            case_id: Uuid::new_v4(),
            context: (*context).into(),
            source_event_ids: c.draft().evidence_ids.clone(),
            result_event_id: result,
            outcome,
        });
    }
    let input = IndependentAssessment {
        review_id: Uuid::new_v4(),
        reviewer_id: Uuid::new_v4(),
        origin: ReviewOrigin::IndependentMeasurement,
        reviewed_evidence_ids: c.draft().evidence_ids.clone(),
        reviewed_counterevidence_ids: c.draft().counterevidence_ids.clone(),
        counter_search_event_ids: vec![search],
        counter_review: Outcome::Passed,
        observations,
    };
    (input, sources)
}
fn validated(d: &Development, contexts: &[&str], outcome: Outcome) -> Development {
    let (a, s) = assessment(d, contexts, outcome);
    let token = TrustedValidator::review(d, a, s, now() + Duration::hours(2)).unwrap();
    d.record_validation(&token, now() + Duration::hours(2))
        .unwrap()
}
fn changed_traits() -> Value {
    let mut v = persona()["personality"]["traits"].clone();
    v["curiosity"] = json!(0.65);
    v
}

#[test]
fn significant_value_reordering_can_be_accepted_without_mechanical_three_trials() {
    let proposed = json!([{"id":"respect","description":"更重视对方明确表达的边界","priority":1.0},{"id":"truthfulness","description":"保留事实核查并调整价值排序","priority":0.6}]);
    let (c, snapshot, p) = candidate(DevelopmentTarget::Values, proposed.clone());
    let event = c.draft().evidence_ids[0];
    let d = trial(
        &c,
        ValidationRoute::SignificantEvent { event_id: event },
        SourceClass::SignificantEvent,
    );
    let checked = validated(
        &d,
        &["significant experience and counterexample"],
        Outcome::Passed,
    );
    let revision = Uuid::new_v4();
    let (accepted, intent) = checked
        .accept(&snapshot, &p, revision, now() + Duration::hours(3))
        .unwrap();
    assert_eq!(accepted.status(), DevelopmentStatus::Accepted);
    assert_eq!(d.status(), DevelopmentStatus::Trial);
    assert_eq!(intent.expected_version, 3);
    assert_eq!(intent.next_version, 4);
    let ApplicationValue::Persona(after) = intent.application else {
        panic!("wrong application layer")
    };
    assert_eq!(after["personality"]["values"], proposed);
    assert_eq!(after["identity"], p["identity"]);
    assert_eq!(after["relationship"], p["relationship"]);
    assert_eq!(
        after["personality"]["initial_interests"],
        p["personality"]["initial_interests"]
    );
    assert_eq!(accepted.history().len(), 3);
    assert_eq!(accepted.validations().len(), 1);
    assert_eq!(
        accepted.validations()[0]
            .assessment
            .reviewed_counterevidence_ids,
        c.draft().counterevidence_ids
    );
}

#[test]
fn stable_habits_need_distinct_contexts_and_preserve_unrelated_current_fields() {
    let (c, snapshot, p) = candidate(DevelopmentTarget::Traits, changed_traits());
    let d = trial(
        &c,
        ValidationRoute::CrossScenarioHabit,
        SourceClass::ObservedOutcome,
    );
    let one = validated(&d, &["same-context"], Outcome::Passed);
    assert!(matches!(
        one.accept(&snapshot, &p, Uuid::new_v4(), now() + Duration::hours(3)),
        Err(DevelopmentError::Unverified)
    ));
    let checked = validated(&d, &["work task", "unrelated social task"], Outcome::Passed);
    let mut current = p.clone();
    current["expression"]["tone"] = json!("另一个合法后续表达变化");
    let (_, intent) = checked
        .accept(
            &snapshot,
            &current,
            Uuid::new_v4(),
            now() + Duration::hours(3),
        )
        .unwrap();
    let ApplicationValue::Persona(after) = intent.application else {
        panic!("wrong layer")
    };
    assert_eq!(after["expression"], current["expression"]);
    assert_eq!(after["personality"]["traits"], changed_traits());
}

#[test]
fn short_affect_self_report_and_initial_setting_alone_cannot_authorize_drift() {
    let (c, _, _) = candidate(DevelopmentTarget::Traits, changed_traits());
    for kind in [
        SourceClass::TransientAffect,
        SourceClass::ModelSelfReport,
        SourceClass::InitialSetting,
    ] {
        assert!(matches!(
            TrustedValidator::authorize_trial(
                &c,
                evidence(&c, kind),
                ValidationRoute::CrossScenarioHabit,
                now()
            ),
            Err(DevelopmentError::Evidence)
        ));
    }
    assert!(matches!(
        TrustedValidator::authorize_trial(
            &c,
            evidence(&c, SourceClass::ExternalReport),
            ValidationRoute::SignificantEvent {
                event_id: c.draft().evidence_ids[0]
            },
            now()
        ),
        Err(DevelopmentError::Evidence)
    ));
}

#[test]
fn schema_target_noop_deadline_source_and_extra_fields_are_strict() {
    let p = persona();
    let target = DevelopmentTarget::Traits;
    let snapshot = TargetSnapshot {
        target,
        version: 3,
        head_revision_id: None,
        value: p["personality"]["traits"].clone(),
        scope: None,
    };
    let owner = Uuid::new_v4();
    let author = Uuid::new_v4();
    for value in [
        json!({"permissions":"expanded"}),
        json!("自然语言不能直接覆盖traits"),
        json!({"curiosity":2.0}),
        json!({"personality":{"traits":changed_traits()}}),
    ] {
        assert!(matches!(
            DevelopmentCandidate::prepare(
                Uuid::new_v4(),
                owner,
                author,
                draft(target, value),
                &snapshot,
                &p,
                now()
            ),
            Err(DevelopmentError::Persona)
        ));
    }
    assert!(matches!(
        DevelopmentCandidate::prepare(
            Uuid::new_v4(),
            owner,
            author,
            draft(target, snapshot.value.clone()),
            &snapshot,
            &p,
            now()
        ),
        Err(DevelopmentError::NoChange)
    ));
    let mut expired = draft(target, changed_traits());
    expired.deadline = now();
    assert!(matches!(
        DevelopmentCandidate::prepare(Uuid::new_v4(), owner, author, expired, &snapshot, &p, now()),
        Err(DevelopmentError::Deadline)
    ));
    let mut overlap = draft(target, changed_traits());
    overlap.counterevidence_ids = overlap.evidence_ids.clone();
    assert!(matches!(
        DevelopmentCandidate::prepare(Uuid::new_v4(), owner, author, overlap, &snapshot, &p, now()),
        Err(DevelopmentError::Evidence)
    ));
    for target in [
        "identity",
        "persona.initial_interests",
        "runtime",
        "permissions",
        "relationship",
    ] {
        let mut raw =
            serde_json::to_value(draft(DevelopmentTarget::Traits, changed_traits())).unwrap();
        raw["target"] = json!(target);
        assert!(serde_json::from_value::<DevelopmentDraft>(raw).is_err());
    }
    let mut raw = serde_json::to_value(draft(DevelopmentTarget::Traits, changed_traits())).unwrap();
    raw["verified"] = json!(true);
    assert!(serde_json::from_value::<DevelopmentDraft>(raw).is_err());
}

#[test]
fn every_persona_target_uses_the_existing_complete_schema_without_cross_path_writes() {
    let p = persona();
    for (target, value) in [
        (DevelopmentTarget::Expression, {
            let mut v = p["expression"].clone();
            v["tone"] = json!("清晰直接且尊重分歧");
            v
        }),
        (
            DevelopmentTarget::CognitivePreferences,
            json!(["遇到反例时复查原推论", "当前明确要求优先于一般偏好"]),
        ),
    ] {
        let (c, snapshot, _) = candidate(target, value.clone());
        let d = validated(
            &trial(
                &c,
                ValidationRoute::CrossScenarioHabit,
                SourceClass::ObservedOutcome,
            ),
            &["first", "second"],
            Outcome::Passed,
        );
        let (_, intent) = d
            .accept(&snapshot, &p, Uuid::new_v4(), now() + Duration::hours(3))
            .unwrap();
        let ApplicationValue::Persona(after) = intent.application else {
            panic!("wrong layer")
        };
        assert_eq!(
            after.pointer(target.persona_pointer().unwrap()),
            Some(&value)
        );
        assert_eq!(after["identity"], p["identity"]);
        assert_eq!(after["profile_version"], p["profile_version"]);
        assert_eq!(
            after["personality"]["initial_interests"],
            p["personality"]["initial_interests"]
        );
    }
}

#[test]
fn no_independent_review_or_missing_counter_audit_cannot_become_accepted() {
    let (c, snapshot, p) = candidate(DevelopmentTarget::Traits, changed_traits());
    let d = trial(
        &c,
        ValidationRoute::CrossScenarioHabit,
        SourceClass::ObservedOutcome,
    );
    assert!(matches!(
        d.accept(&snapshot, &p, Uuid::new_v4(), now() + Duration::hours(1)),
        Err(DevelopmentError::Unverified)
    ));
    for n in 0..5 {
        let (mut a, mut sources) = assessment(&d, &["a", "b"], Outcome::Passed);
        match n {
            0 => a.origin = ReviewOrigin::AuthorSelfAssessment,
            1 => a.reviewed_counterevidence_ids.clear(),
            2 => a.counter_search_event_ids.clear(),
            3 => sources.retain(|s| !c.draft().counterevidence_ids.contains(&s.id)),
            _ => {
                let result = a.observations[0].result_event_id;
                for s in &mut sources {
                    if s.id == result {
                        s.class = SourceClass::ModelSelfReport;
                    }
                }
            }
        };
        assert!(matches!(
            TrustedValidator::review(&d, a, sources, now() + Duration::hours(2)),
            Err(DevelopmentError::Review)
        ));
    }
}

#[test]
fn failed_or_unverified_results_stay_recorded_and_block_acceptance() {
    let (c, snapshot, p) = candidate(DevelopmentTarget::Traits, changed_traits());
    let d = trial(
        &c,
        ValidationRoute::CrossScenarioHabit,
        SourceClass::ObservedOutcome,
    );
    for outcome in [Outcome::Failed, Outcome::Unverified] {
        let checked = validated(&d, &["a", "b"], outcome);
        assert_eq!(
            checked.validations()[0].assessment.observations[0].outcome,
            outcome
        );
        assert!(matches!(
            checked.accept(&snapshot, &p, Uuid::new_v4(), now() + Duration::hours(3)),
            Err(DevelopmentError::Unverified)
        ));
        let rejected = checked
            .reject("保留失败，不以再自评放行", now() + Duration::hours(3))
            .unwrap();
        assert_eq!(rejected.status(), DevelopmentStatus::Rejected);
        assert!(matches!(
            rejected.reject("重复拒绝", now() + Duration::hours(4)),
            Err(DevelopmentError::Transition)
        ));
        assert_eq!(rejected.validations().len(), 1);
    }
    let failed = validated(&d, &["a", "b"], Outcome::Failed);
    let later = validated(&failed, &["c", "d"], Outcome::Passed);
    assert!(
        matches!(
            later.accept(&snapshot, &p, Uuid::new_v4(), now() + Duration::hours(3)),
            Err(DevelopmentError::Unverified)
        ),
        "later success must not conceal a prior failed trial"
    );
}

#[test]
fn expiry_and_repeated_or_out_of_order_transitions_do_not_mutate_old_state() {
    let (c, _, _) = candidate(DevelopmentTarget::Traits, changed_traits());
    let d = trial(
        &c,
        ValidationRoute::CrossScenarioHabit,
        SourceClass::ObservedOutcome,
    );
    let expired = d.expire(c.draft().deadline).unwrap();
    assert_eq!(expired.status(), DevelopmentStatus::Expired);
    assert_eq!(d.status(), DevelopmentStatus::Trial);
    assert!(matches!(
        expired.expire(c.draft().deadline + Duration::days(1)),
        Err(DevelopmentError::Transition)
    ));
    assert!(matches!(
        d.expire(now() + Duration::hours(1)),
        Err(DevelopmentError::Deadline)
    ));
    let (a, s) = assessment(&d, &["a", "b"], Outcome::Passed);
    let token = TrustedValidator::review(&d, a, s, now() + Duration::hours(2)).unwrap();
    assert!(matches!(
        d.record_validation(&token, now() + Duration::hours(1)),
        Err(DevelopmentError::Review)
    ));
    assert!(matches!(
        d.record_validation(&token, now() - Duration::seconds(1)),
        Err(DevelopmentError::Invalid)
    ));
}

#[test]
fn review_and_trial_authorizations_are_bound_to_exact_candidate_and_trial() {
    let (c, snapshot, p) = candidate(DevelopmentTarget::Traits, changed_traits());
    let authorization = TrustedValidator::authorize_trial(
        &c,
        evidence(&c, SourceClass::SignificantEvent),
        ValidationRoute::CrossScenarioHabit,
        now(),
    )
    .unwrap();
    let mut different = c.draft().clone();
    different.proposed_value["curiosity"] = json!(0.4);
    let other = DevelopmentCandidate::prepare(
        c.id(),
        c.individual_id(),
        Uuid::new_v4(),
        different,
        &snapshot,
        &p,
        now(),
    )
    .unwrap();
    assert!(matches!(
        Development::propose(other).start_trial(&authorization, now()),
        Err(DevelopmentError::Evidence)
    ));
    let d = Development::propose(c.clone())
        .start_trial(&authorization, now())
        .unwrap();
    let (a, s) = assessment(&d, &["a", "b"], Outcome::Passed);
    let proof = TrustedValidator::review(&d, a, s, now() + Duration::hours(2)).unwrap();
    let other_route = TrustedValidator::authorize_trial(
        &c,
        evidence(&c, SourceClass::SignificantEvent),
        ValidationRoute::SignificantEvent {
            event_id: c.draft().evidence_ids[0],
        },
        now(),
    )
    .unwrap();
    let same_time_other_trial = Development::propose(c.clone())
        .start_trial(&other_route, now())
        .unwrap();
    assert!(matches!(
        same_time_other_trial.record_validation(&proof, now() + Duration::hours(2)),
        Err(DevelopmentError::Review)
    ));
    let later = Development::propose(c)
        .start_trial(&authorization, now() + Duration::hours(3))
        .unwrap();
    assert!(matches!(
        later.record_validation(&proof, now() + Duration::hours(4)),
        Err(DevelopmentError::Review)
    ));
    let once = d
        .record_validation(&proof, now() + Duration::hours(2))
        .unwrap();
    assert!(matches!(
        once.record_validation(&proof, now() + Duration::hours(3)),
        Err(DevelopmentError::Review)
    ));
}

#[test]
fn revert_is_a_new_intent_preserves_history_and_rejects_successor_or_aba_overwrite() {
    let (c, snapshot, p) = candidate(DevelopmentTarget::Traits, changed_traits());
    let checked = validated(
        &trial(
            &c,
            ValidationRoute::CrossScenarioHabit,
            SourceClass::ObservedOutcome,
        ),
        &["a", "b"],
        Outcome::Passed,
    );
    let revision = Uuid::new_v4();
    let (accepted, apply) = checked
        .accept(&snapshot, &p, revision, now() + Duration::hours(3))
        .unwrap();
    let ApplicationValue::Persona(after) = apply.application else {
        panic!("wrong layer")
    };
    let current = TargetSnapshot {
        target: snapshot.target,
        version: 4,
        head_revision_id: Some(revision),
        value: c.draft().proposed_value.clone(),
        scope: None,
    };
    let mut unrelated = after.clone();
    unrelated["expression"]["tone"] = json!("独立更新的表达");
    let (reverted, intent) = accepted
        .revert(
            &current,
            &unrelated,
            Uuid::new_v4(),
            "这项试用后来不再适合",
            now() + Duration::days(30),
        )
        .unwrap();
    let ApplicationValue::Persona(restored) = intent.application else {
        panic!("wrong layer")
    };
    assert_eq!(
        restored["personality"]["traits"],
        p["personality"]["traits"]
    );
    assert_eq!(restored["expression"], unrelated["expression"]);
    assert_eq!(accepted.status(), DevelopmentStatus::Accepted);
    assert_eq!(reverted.status(), DevelopmentStatus::Reverted);
    assert_eq!(reverted.history()[2].to, DevelopmentStatus::Accepted);
    assert_eq!(reverted.history()[3].before_value, c.draft().proposed_value);
    assert_eq!(reverted.history()[3].after_value, snapshot.value);
    let mut successor = current.clone();
    successor.version = 5;
    successor.head_revision_id = Some(Uuid::new_v4());
    assert!(matches!(
        accepted.revert(
            &successor,
            &after,
            Uuid::new_v4(),
            "不能覆盖后继",
            now() + Duration::days(1)
        ),
        Err(DevelopmentError::Conflict)
    ));
    assert!(matches!(
        reverted.revert(
            &current,
            &after,
            Uuid::new_v4(),
            "重复撤回",
            now() + Duration::days(31)
        ),
        Err(DevelopmentError::Transition)
    ));
}

#[test]
fn scoped_task_strategy_uses_separate_type_and_can_revert_to_no_strategy() {
    let scope = StrategyScope::Goal(Uuid::new_v4());
    let snapshot = TargetSnapshot {
        target: DevelopmentTarget::TaskStrategy,
        version: 3,
        head_revision_id: None,
        value: Value::Null,
        scope: Some(scope.clone()),
    };
    let value =
        json!({"scope":scope,"description":"先查实际资料与反例，再决定行动；不改变任何权限"});
    let c = DevelopmentCandidate::prepare(
        Uuid::new_v4(),
        Uuid::new_v4(),
        Uuid::new_v4(),
        draft(DevelopmentTarget::TaskStrategy, value.clone()),
        &snapshot,
        &persona(),
        now(),
    )
    .unwrap();
    let checked = validated(
        &trial(
            &c,
            ValidationRoute::CrossScenarioHabit,
            SourceClass::ObservedOutcome,
        ),
        &["goal step a", "goal step b"],
        Outcome::Passed,
    );
    let revision = Uuid::new_v4();
    let (accepted, intent) = checked
        .accept(&snapshot, &persona(), revision, now() + Duration::hours(3))
        .unwrap();
    assert!(matches!(
        intent.application,
        ApplicationValue::TaskStrategy {
            strategy: Some(_),
            ..
        }
    ));
    let current = TargetSnapshot {
        version: 4,
        head_revision_id: Some(revision),
        value,
        ..snapshot.clone()
    };
    let (_, undo) = accepted
        .revert(
            &current,
            &persona(),
            Uuid::new_v4(),
            "撤回此作用域策略",
            now() + Duration::hours(4),
        )
        .unwrap();
    assert!(matches!(
        undo.application,
        ApplicationValue::TaskStrategy { strategy: None, .. }
    ));
    let wrong = json!({"scope":{"kind":"task","id":Uuid::new_v4()},"description":"不能偷换scope"});
    assert!(matches!(
        DevelopmentCandidate::prepare(
            Uuid::new_v4(),
            Uuid::new_v4(),
            Uuid::new_v4(),
            draft(DevelopmentTarget::TaskStrategy, wrong),
            &snapshot,
            &persona(),
            now()
        ),
        Err(DevelopmentError::Conflict)
    ));
}

#[test]
fn counter_review_failures_remain_evidence_and_prevent_acceptance() {
    let (c, snapshot, p) = candidate(DevelopmentTarget::Traits, changed_traits());
    let d = trial(
        &c,
        ValidationRoute::CrossScenarioHabit,
        SourceClass::ObservedOutcome,
    );
    for outcome in [Outcome::Failed, Outcome::Unverified] {
        let (mut a, s) = assessment(&d, &["work", "social"], Outcome::Passed);
        a.counter_review = outcome;
        let token = TrustedValidator::review(&d, a, s, now() + Duration::hours(2)).unwrap();
        let checked = d
            .record_validation(&token, now() + Duration::hours(2))
            .unwrap();
        assert_eq!(checked.validations()[0].assessment.counter_review, outcome);
        assert!(matches!(
            checked.accept(&snapshot, &p, Uuid::new_v4(), now() + Duration::hours(3)),
            Err(DevelopmentError::Unverified)
        ));
    }
}

#[test]
fn owner_timestamps_and_immutable_source_metadata_are_checked_before_review() {
    let (c, _, _) = candidate(DevelopmentTarget::Traits, changed_traits());
    for kind in 0..4 {
        let mut s = evidence(&c, SourceClass::ObservedOutcome);
        match kind {
            0 => s[0].individual_id = Uuid::new_v4(),
            1 => s[0].recorded_at = now() + Duration::seconds(1),
            2 => s.push(s[0].clone()),
            _ => s[0].id = Uuid::nil(),
        }
        assert!(matches!(
            TrustedValidator::authorize_trial(&c, s, ValidationRoute::CrossScenarioHabit, now()),
            Err(DevelopmentError::Evidence)
        ));
    }
    let d = trial(
        &c,
        ValidationRoute::CrossScenarioHabit,
        SourceClass::ObservedOutcome,
    );
    for kind in 0..4 {
        let (mut a, mut s) = assessment(&d, &["work", "social"], Outcome::Passed);
        match kind {
            0 => s[0].class = SourceClass::ExternalReport,
            1 => {
                let result = a.observations[0].result_event_id;
                s.iter_mut().find(|s| s.id == result).unwrap().recorded_at =
                    now() - Duration::seconds(1);
            }
            2 => a.reviewer_id = Uuid::nil(),
            _ => a.observations[1].result_event_id = a.observations[0].result_event_id,
        }
        assert!(matches!(
            TrustedValidator::review(&d, a, s, now() + Duration::hours(2)),
            Err(DevelopmentError::Review)
        ));
    }
}

#[test]
fn duplicate_contexts_and_reused_result_ids_across_reviews_cannot_pass() {
    let (c, snapshot, p) = candidate(DevelopmentTarget::Traits, changed_traits());
    let d = trial(
        &c,
        ValidationRoute::CrossScenarioHabit,
        SourceClass::ObservedOutcome,
    );
    let repeated = validated(&d, &["same", "same"], Outcome::Passed);
    assert!(matches!(
        repeated.accept(&snapshot, &p, Uuid::new_v4(), now() + Duration::hours(3)),
        Err(DevelopmentError::Unverified)
    ));
    let (a, s) = assessment(&d, &["a", "b"], Outcome::Passed);
    let proof =
        TrustedValidator::review(&d, a.clone(), s.clone(), now() + Duration::hours(2)).unwrap();
    let once = d
        .record_validation(&proof, now() + Duration::hours(2))
        .unwrap();
    let mut another = a;
    another.review_id = Uuid::new_v4();
    for o in &mut another.observations {
        o.case_id = Uuid::new_v4();
    }
    let other = TrustedValidator::review(&once, another, s, now() + Duration::hours(2)).unwrap();
    let twice = once
        .record_validation(&other, now() + Duration::hours(2))
        .unwrap();
    assert!(matches!(
        twice.accept(&snapshot, &p, Uuid::new_v4(), now() + Duration::hours(3)),
        Err(DevelopmentError::Review)
    ));
}

#[test]
fn draft_bounds_and_target_versions_reject_malformed_candidates() {
    let (c, snapshot, p) = candidate(DevelopmentTarget::Traits, changed_traits());
    for n in 0..9 {
        let mut raw = c.draft().clone();
        match n {
            0 => raw.before_version = 0,
            1 => raw.proposed_description = "x".repeat(2001),
            2 => raw.validation_plan = " ".into(),
            3 => raw.evidence_ids.clear(),
            4 => raw.evidence_ids = vec![Uuid::nil()],
            5 => raw.evidence_ids.push(raw.evidence_ids[0]),
            6 => raw.evidence_ids = (0..33).map(|_| Uuid::new_v4()).collect(),
            7 => raw.counterevidence_ids = vec![Uuid::nil()],
            _ => raw.before_version += 1,
        }
        assert!(
            DevelopmentCandidate::prepare(
                Uuid::new_v4(),
                c.individual_id(),
                Uuid::new_v4(),
                raw,
                &snapshot,
                &p,
                now()
            )
            .is_err()
        );
    }
    let mut invalid_whole = p.clone();
    invalid_whole["relationship"]["assume_intimacy"] = json!("invalid schema outside target");
    assert!(matches!(
        DevelopmentCandidate::prepare(
            Uuid::new_v4(),
            c.individual_id(),
            Uuid::new_v4(),
            c.draft().clone(),
            &snapshot,
            &invalid_whole,
            now()
        ),
        Err(DevelopmentError::Persona)
    ));
    let scope = StrategyScope::Task(Uuid::new_v4());
    let raw = json!({"scope":scope,"description":"strategy","permissions":{"network":true}});
    assert!(serde_json::from_value::<TaskStrategy>(raw).is_err());
}
