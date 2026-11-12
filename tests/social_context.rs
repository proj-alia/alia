//! Synthetic engineering observations; no claim of experienced emotion or model quality.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        affect::{AppraisalDraft, AttitudeDraft},
        cognition::{DecisionProposal, GoalView, ModelContext},
        config::{PersonaSeed, RuntimeRules},
        grounding::{self, ContentCheck, DecisionAssessment, ReviewTarget, Support},
        interests::{InterestDraft, InterestStatus},
        relationships::{RelationshipDelta, RelationshipRevisionDraft},
        social::{CurrentAppraisal, SocialContext},
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        scheduling::{RunLease, TokenAllowance},
    },
};
use chrono::Duration;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
fn sample() -> ModelContext {
    serde_json::from_str(include_str!("../evals/model_context.example.json")).unwrap()
}
fn appraisal(event: Uuid, goal: Option<Uuid>) -> AppraisalDraft {
    AppraisalDraft {
        event_ids: vec![event],
        goal_id: goal,
        subject_id: None,
        valence: -0.2,
        arousal: 0.6,
        goal_relevance: 1.,
        expectation_gap: 1.,
        controllability: 0.5,
        uncertainty: 1.,
        brief_basis: "实际测试事件引起的有限评价".into(),
    }
}
fn with_appraisal() -> ModelContext {
    let mut c = sample();
    let id = Uuid::new_v4();
    let event = c.events[0].id;
    c.input_manifest.extend([id, event]);
    c.input_manifest.sort();
    c.input_manifest.dedup();
    c.social_context = Some(SocialContext {
        appraisal: Some(CurrentAppraisal {
            id,
            draft: appraisal(event, None),
            created_at: c.now - Duration::minutes(1),
            expires_at: c.now + Duration::minutes(59),
            attention_weight: 1.,
            provenance_ids: vec![id, event],
            basis_event_ids: vec![event],
        }),
        ..Default::default()
    });
    c
}
fn proposal(id: Uuid) -> DecisionProposal {
    DecisionProposal::parse(&serde_json::to_vec(&json!({"schema_version":1,"reply":"我目前对那件事保留谨慎的理解。","reply_evidence_ids":[id],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"自己的评价不是他人的感情"})).unwrap()).unwrap()
}
#[test]
fn interpretation_can_support_self_description_but_not_new_experience_or_other_peoples_feelings() {
    let c = with_appraisal();
    let a = c
        .social_context
        .as_ref()
        .unwrap()
        .appraisal
        .as_ref()
        .unwrap();
    let p = proposal(a.id);
    assert!(p.validate_context(&c).is_ok());
    let mut invalid = p.clone();
    invalid.memory_changes=serde_json::from_value(json!([{"content":"共同亲历","kind":"episodic","scope":"general","source_event_ids":[a.id],"supersedes":null}])).unwrap();
    assert!(invalid.validate_context(&c).is_err());
    let review = grounding::review_context(&c, Uuid::new_v4(), &p).unwrap();
    let packet = grounding::evidence_packet(&review).unwrap();
    assert_eq!(packet["packet_version"], "target-citations/11");
    assert_eq!(
        packet["evidence_by_target"][0]["evidence"][0]["type"],
        "social_interpretation"
    );
    assert!(
        packet
            .to_string()
            .contains("not_world_fact_or_others_feelings")
    );
    assert_eq!(
        packet["evidence_by_target"][0]["evidence"][0]["basis_events"][0]["id"],
        json!(c.events[0].id)
    );
    let assessment = DecisionAssessment {
        schema_version: 1,
        checks: vec![ContentCheck {
            target: ReviewTarget::Reply,
            verdict: Support::Supported,
            reason: "仅审核合成自我表述".into(),
            evidence_ids: vec![a.id],
        }],
    };
    assert!(assessment.validate(&review).is_ok());
}
#[test]
fn missing_basis_forged_weight_or_expired_appraisal_cannot_be_sent_as_valid_state() {
    let c = with_appraisal();
    assert!(c.social_context.as_ref().unwrap().valid_for(&c));
    let mut missing = c.clone();
    missing.events.clear();
    assert!(!missing.social_context.as_ref().unwrap().valid_for(&missing));
    assert!(
        proposal(
            c.social_context
                .as_ref()
                .unwrap()
                .appraisal
                .as_ref()
                .unwrap()
                .id
        )
        .validate_context(&missing)
        .is_err()
    );
    let mut forged = c.clone();
    forged
        .social_context
        .as_mut()
        .unwrap()
        .appraisal
        .as_mut()
        .unwrap()
        .attention_weight = 100.;
    assert!(!forged.social_context.as_ref().unwrap().valid_for(&forged));
    assert!(
        !c.social_context
            .as_ref()
            .unwrap()
            .current_at(c.now + Duration::hours(2))
    );
}
fn goal(id: Uuid, priority: u8) -> GoalView {
    serde_json::from_value(json!({"id":id,"priority":priority,"description":"可检查目标","status":"active","success_criterion":"明确产物","next_step":"读取","stop_condition":"无资料等待"})).unwrap()
}
#[test]
fn attention_is_bounded_and_never_displaces_current_run_goal_or_mutates_priorities() {
    let mut c = with_appraisal();
    let current = Uuid::new_v4();
    let focused = Uuid::new_v4();
    let other = Uuid::new_v4();
    c.current_goal_id = Some(current);
    c.goals = vec![goal(other, 55), goal(focused, 50), goal(current, 1)];
    c.social_context
        .as_mut()
        .unwrap()
        .appraisal
        .as_mut()
        .unwrap()
        .draft
        .goal_id = Some(focused);
    alia::domain::social::order_attention(&mut c);
    assert_eq!(
        c.goals.iter().map(|g| g.id).collect::<Vec<_>>(),
        vec![current, focused, other]
    );
    assert_eq!(
        c.goals.iter().map(|g| g.priority).collect::<Vec<_>>(),
        vec![1, 50, 55]
    );
    c.goals[2].priority = 61;
    alia::domain::social::order_attention(&mut c);
    assert_eq!(c.goals[1].id, other);
}
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "social-init",
        &PersonaSeed::parse(include_bytes!("../config/persona.example.json")).unwrap(),
        &RuntimeRules::parse(
            include_bytes!("../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )
        .unwrap(),
    )
    .await
    .unwrap();
    s
}
async fn msg(s: &Store, text: &str) -> Uuid {
    s.submit_message(&Uuid::new_v4().to_string(), text, None)
        .await
        .unwrap()
        .event_id
}
async fn context(s: &Store, pool: &PgPool) -> (ModelContext, RunLease) {
    let m = s
        .submit_message(&Uuid::new_v4().to_string(), "本轮独立输入", None)
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET status='cancelled' WHERE status='queued' AND id<>$1")
        .bind(m.operation_id)
        .execute(pool)
        .await
        .unwrap();
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(l.run_id, m.operation_id);
    (s.model_context(l.run_id, vec![]).await.unwrap(), l)
}
async fn affect(s: &Store, event: Uuid, goal: Option<Uuid>, topic: &str) -> (Uuid, Uuid) {
    let mut refs = vec![event];
    refs.extend(goal);
    let receipt = s
        .record_affect(
            &Uuid::new_v4().to_string(),
            s.snapshot().await.unwrap().unwrap().expected(),
            &refs,
            Some(&appraisal(event, goal)),
            Some(&AttitudeDraft {
                topic: topic.into(),
                subject_id: None,
                stance: "愿意继续探索，先检查证据".into(),
                event_ids: vec![event],
                supersedes: None,
                brief_basis: "有限长期态度".into(),
            }),
        )
        .await
        .unwrap();
    (receipt.appraisal_id.unwrap(), receipt.attitude_id.unwrap())
}
async fn owner(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
        .fetch_one(pool)
        .await
        .unwrap()
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn restart_restores_three_distinct_layers_with_complete_sources_and_read_does_not_rewrite_identity(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = msg(&s, "约定讨论时给出依据").await;
    let who = owner(&pool).await;
    let version: i64 = sqlx::query_scalar("SELECT version FROM relationships WHERE subject_id=$1")
        .bind(who)
        .fetch_one(&pool)
        .await
        .unwrap();
    let revision = s
        .revise_relationship(
            "revision",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[who, source],
            &RelationshipRevisionDraft {
                subject_id: who,
                expected_relationship_version: version,
                changes: RelationshipDelta {
                    expectations: Some(vec!["讨论分歧时说明依据".into()]),
                    ..Default::default()
                },
                event_ids: vec![source],
                brief_basis: "记录明确沟通约定".into(),
            },
        )
        .await
        .unwrap();
    let (app, long) = affect(&s, source, None, "discussion.evidence").await;
    let restored = Store::new(pool.clone());
    let (c, l) = context(&restored, &pool).await;
    let before = s.snapshot().await.unwrap().unwrap();
    let selection = c.social_context.as_ref().unwrap();
    assert!(selection.valid_for(&c));
    assert_eq!(selection.appraisal.as_ref().unwrap().id, app);
    assert!(selection.attitudes.iter().any(|a| a.id == long));
    let relation = selection
        .relationships
        .iter()
        .find(|r| r.subject_id == who)
        .unwrap();
    assert_eq!(relation.latest_revision_id, Some(revision.revision_id));
    assert_eq!(relation.state.expectations, vec!["讨论分歧时说明依据"]);
    assert!(
        relation.version > revision.relationship_version,
        "later message legitimately advances relationship version"
    );
    for id in [source, revision.revision_id, app, long, who] {
        assert!(c.input_manifest.contains(&id));
    }
    let again = restored.model_context(l.run_id, vec![]).await.unwrap();
    assert!(again.social_context.as_ref().unwrap().valid_for(&again));
    let after = restored.snapshot().await.unwrap().unwrap();
    assert_eq!(before.state_version, after.state_version);
    assert_eq!(before.persona, after.persona);
    assert_eq!(before.seed_digest, after.seed_digest);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn ttl_expiry_removes_only_short_state_and_blocks_stale_paid_reservation(pool: PgPool) {
    let s = setup(&pool).await;
    let source = msg(&s, "一次失败后仍保留合作约定").await;
    let (_, long) = affect(&s, source, None, "learning.failure").await;
    let (mut c, l) = context(&s, &pool).await;
    let relation = c.social_context.as_ref().unwrap().relationships[0].clone();
    c.now -= Duration::hours(2);
    let app = c
        .social_context
        .as_mut()
        .unwrap()
        .appraisal
        .as_mut()
        .unwrap();
    app.created_at -= Duration::hours(2);
    app.expires_at -= Duration::hours(2);
    assert!(c.social_context.as_ref().unwrap().valid_for(&c));
    assert!(matches!(
        s.reserve_model_call(
            &l,
            Uuid::new_v4(),
            &c,
            TokenAllowance {
                input_upper_bound: 100,
                output_cap: 32
            }
        )
        .await,
        Err(StoreError::StaleState)
    ));
    let used: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(used, 0);
    sqlx::query("UPDATE affective_appraisals SET created_at=clock_timestamp()-interval '2 hours',expires_at=clock_timestamp()-interval '1 hour'").execute(&pool).await.unwrap();
    let c = s.model_context(l.run_id, vec![]).await.unwrap();
    let sc = c.social_context.unwrap();
    assert!(sc.appraisal.is_none());
    assert!(sc.attitudes.iter().any(|a| a.id == long));
    assert_eq!(sc.relationships[0].state, relation.state);
    assert_eq!(sc.relationships[0].version, relation.version);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn missing_derivation_edges_do_not_hide_revoked_actual_sources_and_dispatch_is_fenced(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = msg(&s, "待撤销的实际陈述").await;
    let (app, long) = affect(&s, source, None, "source.revocation").await;
    let (c, l) = context(&s, &pool).await;
    assert!(c.input_manifest.contains(&source));
    let r = s
        .reserve_model_call(
            &l,
            Uuid::new_v4(),
            &c,
            TokenAllowance {
                input_upper_bound: 100,
                output_cap: 32,
            },
        )
        .await
        .unwrap();
    sqlx::query("DELETE FROM derivation_edges WHERE source_id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.dispatch_model_call(&l, r.id).await,
        Err(StoreError::LeaseLost)
    ));
    let c = s.model_context(l.run_id, vec![]).await.unwrap();
    let sc = c.social_context.as_ref().unwrap();
    assert!(sc.appraisal.is_none());
    assert!(!sc.attitudes.iter().any(|a| a.id == long));
    assert!(!c.input_manifest.contains(&app));
    assert!(c.relationship.as_object().unwrap().is_empty());
    assert!(sc.has_more);
    let sent: bool =
        sqlx::query_scalar("SELECT dispatched_at IS NOT NULL FROM usage_reservations WHERE id=$1")
            .bind(r.id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert!(!sent);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn oversized_complete_source_bundle_is_omitted_not_truncated_or_turned_into_missing_history(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = msg(&s, "原始陈述").await;
    let (_, long) = affect(&s, source, None, "large.source").await;
    sqlx::query(
        "UPDATE events SET payload=jsonb_build_object('text',repeat('界',40000)) WHERE id=$1",
    )
    .bind(source)
    .execute(&pool)
    .await
    .unwrap();
    for i in 0..12 {
        s.import_document(
            &format!("padding-{i}"),
            "非相关资料",
            "小型公开素材",
            "synthetic",
        )
        .await
        .unwrap();
    }
    let (c, _) = context(&s, &pool).await;
    let sc = c.social_context.as_ref().unwrap();
    assert!(sc.has_more);
    assert!(sc.appraisal.is_none());
    assert!(!sc.attitudes.iter().any(|a| a.id == long));
    assert!(!c.events.iter().any(|e| e.id == source));
    assert!(serde_json::to_vec(&c).unwrap().len() <= 96 * 1024);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn appraisal_orders_real_goal_attention_with_original_persisted_priority(pool: PgPool) {
    let s = setup(&pool).await;
    let source = msg(&s, "关注一个未解决的问题").await;
    let mut goals = vec![];
    for name in ["high", "focus", "low"] {
        goals.push(
            s.create_owner_goal(
                name,
                &serde_json::from_value(json!({
                    "description":name,"success_criterion":"有产物","next_step":"读取资料","stop_condition":"无资料时等待"})).unwrap(),
            )
            .await
            .unwrap()
            .goal_id,
        );
    }
    for (g, p) in goals.iter().zip([55, 50, 1]) {
        sqlx::query("UPDATE goals SET priority=$2 WHERE id=$1")
            .bind(g)
            .bind(p)
            .execute(&pool)
            .await
            .unwrap();
    }
    affect(&s, source, Some(goals[1]), "goal.attention").await;
    let (c, _) = context(&s, &pool).await;
    assert_eq!(
        c.goals.iter().map(|g| g.id).collect::<Vec<_>>(),
        vec![goals[1], goals[0], goals[2]]
    );
    let persisted: Vec<i32> =
        sqlx::query_scalar("SELECT priority FROM goals ORDER BY priority DESC")
            .fetch_all(&pool)
            .await
            .unwrap();
    assert_eq!(persisted, vec![55, 50, 1]);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn interest_relabel_and_retirement_keep_identity_and_do_not_reinitialize_from_persona_array(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = msg(&s, "兴趣有了新的具体方向").await;
    let (id, object, version): (String, Uuid, i64) =
        sqlx::query_as("SELECT id,object_id,version FROM interests ORDER BY id LIMIT 1")
            .fetch_one(&pool)
            .await
            .unwrap();
    let seed: Value = sqlx::query_scalar("SELECT seed FROM individuals")
        .fetch_one(&pool)
        .await
        .unwrap();
    let r = s
        .record_interest(
            "relabel",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[source, object],
            &InterestDraft {
                id: Some(id.clone()),
                expected_version: Some(version),
                description: "观察可核查的天文资料".into(),
                status: InterestStatus::Active,
                event_ids: vec![source],
                brief_basis: "新经历修正兴趣表达".into(),
            },
        )
        .await
        .unwrap();
    let (c, _) = context(&s, &pool).await;
    let selected = c.interests.iter().find(|i| i.id == id).unwrap();
    assert_eq!(selected.description, "观察可核查的天文资料");
    assert!(c.input_manifest.contains(&object));
    s.record_interest(
        "retire",
        s.snapshot().await.unwrap().unwrap().expected(),
        &[source, object, r.revision_id],
        &InterestDraft {
            id: Some(id.clone()),
            expected_version: Some(version + 1),
            description: selected.description.clone(),
            status: InterestStatus::Retired,
            event_ids: vec![source],
            brief_basis: "暂时结束此兴趣".into(),
        },
    )
    .await
    .unwrap();
    let restored = Store::new(pool.clone());
    let (c, _) = context(&restored, &pool).await;
    assert!(!c.interests.iter().any(|i| i.id == id));
    let now: Value = sqlx::query_scalar("SELECT seed FROM individuals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(seed, now);
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM interests WHERE id=$1 AND status='retired'")
            .bind(id)
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 1);
}
