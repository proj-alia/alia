use alia::domain::{
    cognition::*,
    history::{MemoryKind, NewMemory, SourceKind},
};
use serde_json::json;
use uuid::Uuid;
fn context() -> ModelContext {
    serde_json::from_str(include_str!("../evals/model_context.example.json")).unwrap()
}
fn proposal() -> DecisionProposal {
    DecisionProposal::parse(br#"{"schema_version":1,"reply":"ok","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"evidence"}"#).unwrap()
}
#[test]
fn completion_requires_explicit_scope_and_observed_document_identity() {
    let mut c = context();
    let mut p: serde_json::Value =
        serde_json::from_str(include_str!("../evals/completion.example.json")).unwrap();
    let doc = c.current_event_id;
    p["goal_changes"][0]["origin_ids"] = json!([doc]);
    p["goal_changes"][0]["completion"]["document_id"] = json!(doc);
    let current = c.events.iter_mut().find(|e| e.id == doc).unwrap();
    current.kind = "document.imported".into();
    assert!(
        DecisionProposal::parse(p.to_string().as_bytes())
            .unwrap()
            .validate_context(&c)
            .is_ok()
    );
    p["goal_changes"][0]["success_criterion"] = json!("彻底理解整个领域");
    assert!(DecisionProposal::parse(p.to_string().as_bytes()).is_err());
    p["goal_changes"][0]["success_criterion"] = json!(alia::domain::completion::EXCERPT_CRITERION);
    p["goal_changes"][0]["completion"]["document_id"] = json!(Uuid::new_v4());
    assert_eq!(
        DecisionProposal::parse(p.to_string().as_bytes())
            .unwrap()
            .validate_context(&c),
        Err(ProposalError::Goal)
    );
}
#[test]
fn internal_goal_notifications_cannot_bootstrap_new_goals() {
    let mut c = context();
    let mut p = proposal();
    p.goal_changes = vec![GoalChange::Create {
        completion: None,
        origins: vec![GoalOriginRef::Event(c.current_event_id)],
        description: "探索".into(),
        success_criterion: "实际记录".into(),
        next_step: "读取".into(),
        stop_condition: "无资料等待".into(),
        priority: 50,
    }];
    for kind in [
        "goal.due",
        "goal.cycle_evaluated",
        "decision.proposed",
        "message.sent",
        "message.drafted",
        "run.retry_scheduled",
        "run.stopped",
    ] {
        c.events
            .iter_mut()
            .find(|e| e.id == c.current_event_id)
            .unwrap()
            .kind = kind.into();
        assert_eq!(p.validate_context(&c), Err(ProposalError::GoalOrigin));
    }
    c.events
        .iter_mut()
        .find(|e| e.id == c.current_event_id)
        .unwrap()
        .kind = "document.imported".into();
    assert!(p.validate_context(&c).is_ok());
}
#[test]
fn typed_goal_motives_preserve_events_interests_and_legacy_input() {
    let mut c = context();
    let interest = format!("interest:{}:0", c.individual_id);
    c.interests.push(InterestRef {
        id: interest.clone(),
        description: "理解观察中的问题".into(),
    });
    let mut raw = serde_json::to_value(proposal()).unwrap();
    raw["goal_changes"] = json!([{"action":"create","origins":[{"origin_type":"event","origin_id":c.current_event_id},{"origin_type":"interest","origin_id":interest}],"description":"比较实际方案","success_criterion":"有来源的比较与限制","next_step":"检查资料","stop_condition":"有结果或缺少资料时停止","priority":50}]);
    let p = DecisionProposal::parse(raw.to_string().as_bytes()).unwrap();
    assert!(p.validate_context(&c).is_ok());
    let GoalChange::Create { origins, .. } = &p.goal_changes[0] else {
        panic!("create")
    };
    let (kind, ids) = GoalOriginRef::storage_projection(origins);
    assert_eq!(kind, "mixed");
    assert_eq!(GoalOriginRef::from_storage(kind, &ids).unwrap(), *origins);
    let mut unknown = p.clone();
    if let GoalChange::Create { origins, .. } = &mut unknown.goal_changes[0] {
        origins[1] = GoalOriginRef::Interest("unregistered".into());
    }
    assert_eq!(unknown.validate_context(&c), Err(ProposalError::GoalOrigin));
    let mut ambiguous = raw.clone();
    ambiguous["goal_changes"][0]["origin_type"] = json!("event");
    ambiguous["goal_changes"][0]["origin_ids"] = json!([c.current_event_id]);
    assert!(DecisionProposal::parse(ambiguous.to_string().as_bytes()).is_err());
    raw["goal_changes"][0]["origins"] =
        json!([{"origin_type":"event","origin_id":c.current_event_id}]);
    let canonical = DecisionProposal::parse(raw.to_string().as_bytes()).unwrap();
    raw["goal_changes"][0]
        .as_object_mut()
        .unwrap()
        .remove("origins");
    raw["goal_changes"][0]["origin_type"] = json!("event");
    raw["goal_changes"][0]["origin_ids"] = json!([c.current_event_id]);
    let legacy = DecisionProposal::parse(raw.to_string().as_bytes()).unwrap();
    assert_eq!(
        serde_json::to_value(legacy).unwrap(),
        serde_json::to_value(canonical).unwrap()
    );
    raw["goal_changes"][0]["origin_ids"] = json!([interest]);
    assert!(matches!(
        DecisionProposal::parse(raw.to_string().as_bytes()),
        Err(ProposalError::GoalOrigin)
    ));
}
#[test]
fn stored_origin_types_are_closed_and_mixed_history_is_lossless() {
    let event = Uuid::new_v4().to_string();
    assert!(GoalOriginRef::from_storage("invented", &[]).is_err());
    assert!(GoalOriginRef::from_storage("mixed", std::slice::from_ref(&event)).is_err());
    assert!(GoalOriginRef::from_storage("event", &["interest:missing:0".into()]).is_err());
    assert_eq!(
        GoalOriginRef::from_storage("interest", &["legacy-interest".into()]).unwrap(),
        vec![GoalOriginRef::Interest("legacy-interest".into())]
    );
}
#[test]
fn schema_rejects_mutations_duplicate_keys_and_unsupported_development() {
    let p = serde_json::to_value(proposal()).unwrap();
    let mut bad = p.clone();
    bad["identity"] = json!({"id":"new"});
    assert!(DecisionProposal::parse(bad.to_string().as_bytes()).is_err());
    let duplicate = p.to_string().replacen("{", "{\"schema_version\":1,", 1);
    assert!(DecisionProposal::parse(duplicate.as_bytes()).is_err());
    let mut bad = p;
    bad["development"] = json!({"change":"personality"});
    assert!(DecisionProposal::parse(bad.to_string().as_bytes()).is_err());
}
#[test]
fn evidence_and_local_scope_cannot_override_general_memory() {
    let c = context();
    let mut p = proposal();
    p.reply_evidence_ids = vec![Uuid::new_v4()];
    assert_eq!(p.validate_context(&c), Err(ProposalError::UnknownEvidence));
    p.reply_evidence_ids = vec![c.events[0].id];
    p.memory_changes = vec![NewMemory {
        content: "本次先原型".into(),
        kind: MemoryKind::Semantic,
        scope: c.current_scope.clone(),
        source_event_ids: vec![c.current_event_id],
        supersedes: Some(c.memories[0].id),
    }];
    assert_eq!(p.validate_context(&c), Err(ProposalError::Scope));
    p.memory_changes[0].supersedes = None;
    assert_eq!(p.validate_context(&c), Ok(()));
    p.memory_changes[0].source_event_ids = vec![Uuid::new_v4()];
    assert_eq!(p.validate_context(&c), Err(ProposalError::UnknownEvidence));
}
#[test]
fn imported_story_cannot_become_personal_experience() {
    let mut c = context();
    let mut p = proposal();
    c.events[0].source_kind = SourceKind::ExternalDocument;
    p.memory_changes = vec![NewMemory {
        content: "亲身经历".into(),
        kind: MemoryKind::Episodic,
        scope: "general".into(),
        source_event_ids: vec![c.events[0].id],
        supersedes: None,
    }];
    assert_eq!(
        p.validate_context(&c),
        Err(ProposalError::UnobservedExperience)
    );
    p.memory_changes[0].kind = MemoryKind::Semantic;
    assert_eq!(p.validate_context(&c), Ok(()));
}
#[test]
fn tools_require_actual_authorization_and_cannot_claim_finished_in_same_reply() {
    let mut c = context();
    let mut p = proposal();
    p.tool_intents = vec![ToolIntent {
        capability_id: "notes.write".into(),
        input: json!({"title":"test","text":"test"}),
    }];
    assert_eq!(p.validate_context(&c), Err(ProposalError::PrematureReply));
    p.reply.clear();
    assert_eq!(p.validate_context(&c), Err(ProposalError::Capability));
    c.capabilities.push(CapabilityInfo {
        id: "notes.write".into(),
        version: "1".into(),
        available: true,
        authorized: false,
        description: "note".into(),
    });
    assert_eq!(p.validate_context(&c), Err(ProposalError::Capability));
    c.capabilities[0].authorized = true;
    assert_eq!(p.validate_context(&c), Ok(()));
    c.capabilities[0].available = false;
    assert_eq!(p.validate_context(&c), Err(ProposalError::Capability));
}
#[test]
fn model_cannot_self_certify_goal_completion_or_change_it_twice() {
    let mut c = context();
    let mut p = proposal();
    let id = Uuid::new_v4();
    c.goals.push(GoalView {
        priority: 50,
        completion: None,
        stop_condition: String::new(),
        waiting_reason: None,
        no_progress_cycles: 0,
        id,
        description: "笔记".into(),
        status: "active".into(),
        success_criterion: "文件验证".into(),
        next_step: "写入".into(),
    });
    p.goal_changes.push(GoalChange::Complete {
        goal_id: id,
        reason: "有结果".into(),
        evidence_ids: vec![c.current_event_id],
    });
    assert_eq!(
        p.validate_context(&c),
        Err(ProposalError::UnverifiedCompletion)
    );
    c.verified_goal_completions.push(id);
    assert_eq!(p.validate_context(&c), Ok(()));
    p.tool_intents.push(ToolIntent {
        capability_id: "notes.write".into(),
        input: json!({"title":"补充","text":"新产物"}),
    });
    assert_eq!(
        p.validate_context(&c),
        Err(ProposalError::CompletionActionConflict)
    );
    p.tool_intents.clear();
    if let GoalChange::Complete { evidence_ids, .. } = &mut p.goal_changes[0] {
        evidence_ids.clear();
    }
    assert_eq!(
        p.validate_context(&c),
        Err(ProposalError::CompletionEvidenceMissing)
    );
    if let GoalChange::Complete { evidence_ids, .. } = &mut p.goal_changes[0] {
        evidence_ids.push(c.current_event_id);
    }

    p.goal_changes.push(GoalChange::Wait {
        goal_id: id,
        reason: "重复转换".into(),
        evidence_ids: vec![],
    });
    assert_eq!(p.validate_context(&c), Err(ProposalError::Goal));
}

#[tokio::test]
async fn runtime_rejection_keeps_real_call_usage_and_does_not_return_invalid_proposal() {
    use alia::runtime::model::*;
    struct InvalidProvider;
    #[async_trait::async_trait]
    impl ModelProvider for InvalidProvider {
        fn mode(&self) -> ProviderMode {
            ProviderMode::Live
        }
        async fn decide(
            &self,
            _: &ModelContext,
            _: CallLimits,
            _: ModelCallMetadata,
        ) -> Result<ModelResult, ModelFailure> {
            let mut p = proposal();
            p.reply_evidence_ids = vec![Uuid::new_v4()];
            Ok(ModelResult {
                http: None,
                proposal: p,
                usage: Some(TokenUsage {
                    input: 100,
                    output: 50,
                    total: 150,
                }),
                provider_model: "test".into(),
                system_fingerprint: None,
                provider_mode: ProviderMode::Live,
            })
        }
    }
    let failure = evaluate_proposal(
        &InvalidProvider,
        &context(),
        CallLimits::default(),
        ModelCallMetadata::foreground(&context()).unwrap(),
    )
    .await
    .err()
    .unwrap();
    assert_eq!(
        failure.error,
        ModelError::Proposal(ProposalError::UnknownEvidence)
    );
    assert_eq!(failure.usage.unwrap().total, 150);
    assert!(failure.request_sent);
}

#[test]
fn owner_wait_requires_owner_resume_not_an_unrelated_model_decision() {
    let mut c = context();
    let id = Uuid::new_v4();
    c.goals.push(GoalView {
        priority: 50,
        id,
        description: "等待用户补充".into(),
        status: "waiting".into(),
        success_criterion: "可检查结果".into(),
        next_step: "读取".into(),
        stop_condition: "等待".into(),
        waiting_reason: Some("owner_wait".into()),
        no_progress_cycles: 0,
        completion: None,
    });
    let mut p = proposal();
    p.goal_changes = vec![GoalChange::Activate {
        goal_id: id,
        reason: "我想继续".into(),
        evidence_ids: vec![],
    }];
    assert_eq!(p.validate_context(&c), Err(ProposalError::Goal));
    p.goal_changes = vec![GoalChange::Abandon {
        goal_id: id,
        reason: "我想放弃".into(),
        evidence_ids: vec![],
    }];
    assert_eq!(p.validate_context(&c), Err(ProposalError::Goal));
    c.goals.last_mut().unwrap().waiting_reason = Some("model_unavailable".into());
    assert!(p.validate_context(&c).is_ok());
}

#[test]
fn schema_diagnostics_report_only_owned_fields_not_rejected_content() {
    let mut value = serde_json::to_value(proposal()).unwrap();
    value["goal_changes"] =
        json!([{"action":"create","private-injected-key":"private-rejected-text"}]);
    let error = DecisionProposal::parse(value.to_string().as_bytes())
        .err()
        .unwrap();
    assert_eq!(error, ProposalError::SchemaField("goal_changes"));
    assert_eq!(error.feedback().0, "invalid_goal_fields");
    assert!(!format!("{error:?} {error} {:?}", error.feedback()).contains("private-"));
    value["goal_changes"] = json!([]);
    value["private-top-level"] = json!("secret");
    assert_eq!(
        DecisionProposal::parse(value.to_string().as_bytes()).err(),
        Some(ProposalError::Schema)
    );
}

#[test]
fn waiting_for_tools_does_not_pause_or_abandon_a_goal() {
    let mut c = context();
    let id = Uuid::new_v4();
    c.goals.push(GoalView {
        priority: 50,
        id,
        description: "研究".into(),
        status: "active".into(),
        success_criterion: "有来源的笔记".into(),
        next_step: "保存".into(),
        stop_condition: "缺少资料".into(),
        waiting_reason: None,
        no_progress_cycles: 0,
        completion: None,
    });
    c.capabilities.push(CapabilityInfo {
        id: "notes.write".into(),
        version: "1".into(),
        available: true,
        authorized: true,
        description: "fixture".into(),
    });
    for action in ["wait", "abandon"] {
        let mut p = proposal();
        p.reply.clear();
        p.goal_changes = vec![serde_json::from_value(json!({"action":action,"goal_id":id,"reason":"资料不足暂停目标","evidence_ids":[c.current_event_id]})).unwrap()];
        assert!(p.validate_context(&c).is_ok());
        p.tool_intents.push(ToolIntent {
            capability_id: "notes.write".into(),
            input: json!({"title":"笔记","text":"观察"}),
        });
        assert_eq!(
            p.validate_context(&c),
            Err(ProposalError::GoalActionConflict)
        );
        p.goal_changes.clear();
        assert!(p.validate_context(&c).is_ok());
    }
}

#[test]
fn entity_proposals_need_visible_literal_sources_and_valid_references() {
    use alia::domain::history::SourceKind;
    let mut c = context();
    let event = c.current_event_id;
    let e = c.events.iter_mut().find(|e| e.id == event).unwrap();
    e.kind = "message.received".into();
    e.source_kind = SourceKind::ReportedClaim;
    e.payload = json!({"text":"沈遥喜欢茶，也喝咖啡。"});
    let raw = json!({"schema_version":1,"reply":"已记录。","reply_evidence_ids":[],"memory_changes":[
 {"kind":"semantic","content":"沈遥喜欢茶。","scope":"general","source_event_ids":[event],"supersedes":null},
 {"kind":"semantic","content":"沈遥也喝咖啡。","scope":"general","source_event_ids":[event],"supersedes":null}],
 "entity_changes":[
 {"memory":{"kind":"created","index":0},"person":{"kind":"new","name":"沈遥"},"role":"subject","source":{"event_id":event,"start_char":0,"end_char":2},"memory_start_char":0},
 {"memory":{"kind":"created","index":1},"person":{"kind":"prior","index":0},"role":"subject","source":{"event_id":event,"start_char":0,"end_char":2},"memory_start_char":0}],
 "goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"从实际文本推测主体"});
    assert!(
        DecisionProposal::parse(raw.to_string().as_bytes())
            .unwrap()
            .validate_context(&c)
            .is_ok()
    );
    let mut automatic = raw.clone();
    for item in automatic["entity_changes"].as_array_mut().unwrap() {
        item.as_object_mut().unwrap().remove("memory_start_char");
    }
    assert!(
        DecisionProposal::parse(automatic.to_string().as_bytes())
            .unwrap()
            .validate_context(&c)
            .is_ok()
    );
    for (field, value, code) in [
        (
            "source",
            json!({"event_id":Uuid::new_v4(),"start_char":0,"end_char":2}),
            "entity_unobserved_source",
        ),
        (
            "memory",
            json!({"kind":"created","index":5}),
            "entity_memory_reference",
        ),
        (
            "person",
            json!({"kind":"prior","index":0}),
            "entity_prior_reference",
        ),
        (
            "person",
            json!({"kind":"existing","id":Uuid::new_v4()}),
            "entity_unknown_person",
        ),
        ("memory_start_char", json!(4), "entity_memory_quote"),
    ] {
        let mut bad = raw.clone();
        bad["entity_changes"][0][field] = value;
        let error = DecisionProposal::parse(bad.to_string().as_bytes())
            .unwrap()
            .validate_context(&c)
            .unwrap_err();
        assert_eq!(error.feedback().0, code);
    }
    let mut bad = raw.clone();
    bad["entity_changes"][0]["actor"] = json!("owner");
    assert!(DecisionProposal::parse(bad.to_string().as_bytes()).is_err());
    c.events
        .iter_mut()
        .find(|e| e.id == event)
        .unwrap()
        .source_kind = SourceKind::Generated;
    assert!(matches!(
        DecisionProposal::parse(raw.to_string().as_bytes())
            .unwrap()
            .validate_context(&c),
        Err(ProposalError::EntityProposal(_))
    ));
}

#[test]
fn memory_person_offsets_are_literal_unicode_and_unambiguous() {
    use alia::domain::entities::{EntityValidationError, memory_name_offset};
    assert_eq!(
        memory_name_offset("🙂资料：沈遥喜欢茶。", "沈遥", None),
        Ok(4)
    );
    assert_eq!(
        memory_name_offset("沈遥与沈遥", "沈遥", None),
        Err(EntityValidationError::MemoryQuote)
    );
    assert_eq!(memory_name_offset("沈遥与沈遥", "沈遥", Some(3)), Ok(3));
    assert_eq!(
        memory_name_offset("沈遥与沈遥", "沈遥", Some(4)),
        Err(EntityValidationError::MemoryQuote)
    );
    assert_eq!(
        memory_name_offset("哈哈哈", "哈哈", None),
        Err(EntityValidationError::MemoryQuote)
    );
    assert_eq!(
        memory_name_offset("沈瑶", "沈遥", None),
        Err(EntityValidationError::MemoryQuote)
    );
    assert_eq!(
        memory_name_offset("沈遥", "", None),
        Err(EntityValidationError::MemoryQuote)
    );
}
