//! Fixed-social-v2 engineering gates only. Providers and usage are synthetic.
//! No network/model calls and no mutation of a private prepared/context token.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        affect::{AppraisalDraft, AttitudeDraft},
        cognition::{DecisionProposal, ModelContext},
        config::{PersonaSeed, RuntimeRules},
        grounding::{self, ContentCheck, DecisionAssessment, ReviewTarget, Support},
        relationships::{RelationshipDelta, RelationshipRevisionDraft},
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        execution::{StepError, model_step},
        model::*,
        scheduling::{RunLease, TokenAllowance},
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::sync::{
    Mutex,
    atomic::{AtomicUsize, Ordering},
};
use uuid::Uuid;
struct Provider {
    proposal: Value,
    authors: AtomicUsize,
    reviews: AtomicUsize,
    packets: Mutex<Vec<Value>>,
}
impl Provider {
    fn new(proposal: Value) -> Self {
        Self {
            proposal,
            authors: AtomicUsize::new(0),
            reviews: AtomicUsize::new(0),
            packets: Mutex::new(vec![]),
        }
    }
}
fn usage() -> Option<TokenUsage> {
    Some(TokenUsage {
        input: 1,
        output: 1,
        total: 2,
    })
}
#[async_trait::async_trait]
impl ModelProvider for Provider {
    fn mode(&self) -> ProviderMode {
        ProviderMode::Live
    }
    fn supports_decision_review(&self) -> bool {
        true
    }
    fn token_allowance(
        &self,
        _: &ModelContext,
        l: CallLimits,
    ) -> Result<TokenAllowance, ModelFailure> {
        Ok(TokenAllowance {
            input_upper_bound: 1,
            output_cap: l.max_output_tokens,
        })
    }
    fn decision_review_allowance(
        &self,
        c: &ModelContext,
        l: CallLimits,
    ) -> Result<TokenAllowance, ModelFailure> {
        self.token_allowance(c, l)
    }
    async fn decide(
        &self,
        _: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        self.authors.fetch_add(1, Ordering::SeqCst);
        Ok(ModelResult {
            proposal: parse(&self.proposal),
            usage: usage(),
            provider_model: "p01-synthetic-social-gate".into(),
            provider_mode: ProviderMode::Live,
            http: None,
            system_fingerprint: None,
        })
    }
    async fn review_decision(
        &self,
        c: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<DecisionReviewResult, ModelFailure> {
        self.reviews.fetch_add(1, Ordering::SeqCst);
        self.packets
            .lock()
            .unwrap()
            .push(grounding::evidence_packet(c).unwrap());
        let p = &c.decision_review.as_ref().unwrap().proposal;
        let checks = grounding::targets(p)
            .into_iter()
            .map(|target| {
                let evidence_ids = match target {
                    ReviewTarget::Reply => p.reply_evidence_ids.clone(),
                    ReviewTarget::Memory { index } => {
                        p.memory_changes[index].source_event_ids.clone()
                    }
                    ReviewTarget::Belief { index } => {
                        let b = &p.belief_changes[index];
                        let mut ids: Vec<_> = b
                            .support_event_ids
                            .iter()
                            .chain(&b.counter_event_ids)
                            .copied()
                            .collect();
                        ids.sort();
                        ids.dedup();
                        ids
                    }
                    ReviewTarget::Autobiography { index } => {
                        p.autobiography_changes[index].review_references()
                    }
                    ReviewTarget::PersonAssertion { index } => {
                        vec![p.entity_changes[index].source.event_id]
                    }
                };
                ContentCheck {
                    target,
                    verdict: Support::Supported,
                    reason: "合成程序门意见，不认证事实或他人感情".into(),
                    evidence_ids,
                }
            })
            .collect();
        Ok(DecisionReviewResult {
            assessment: DecisionAssessment {
                schema_version: 1,
                checks,
            },
            usage: usage(),
            provider_model: "p01-synthetic-social-gate".into(),
            provider_mode: ProviderMode::Live,
        })
    }
}
fn parse(p: &Value) -> DecisionProposal {
    DecisionProposal::parse(&serde_json::to_vec(p).unwrap()).unwrap()
}
async fn setup(pool: &PgPool) -> Store {
    let s = Store::new(pool.clone());
    s.initialize(
        "init",
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
async fn start(s: &Store, pool: &PgPool) -> (RunLease, ModelContext) {
    let m = s
        .submit_message(
            &Uuid::new_v4().to_string(),
            "沈遥表示：请核对有来源的陈述；自己的态度不是世界事实。",
            None,
        )
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET status='cancelled' WHERE status='queued' AND id<>$1")
        .bind(m.operation_id)
        .execute(pool)
        .await
        .unwrap();
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(l.run_id, m.operation_id);
    let c = s.model_context(l.run_id, vec![]).await.unwrap();
    (l, c)
}
async fn affect(s: &Store, events: &[Uuid], topic: &str) -> (Uuid, Uuid) {
    let r = s
        .record_affect(
            &Uuid::new_v4().to_string(),
            s.snapshot().await.unwrap().unwrap().expected(),
            events,
            Some(&AppraisalDraft {
                event_ids: events.to_vec(),
                goal_id: None,
                subject_id: None,
                valence: -0.2,
                arousal: 0.5,
                goal_relevance: 0.5,
                expectation_gap: 0.5,
                controllability: 0.5,
                uncertainty: 0.5,
                brief_basis: "明确合成来源的自我评价".into(),
            }),
            Some(&AttitudeDraft {
                topic: topic.into(),
                subject_id: None,
                stance: "先核对资料，保留条件".into(),
                event_ids: events.to_vec(),
                supersedes: None,
                brief_basis: "长期态度不是外部事实".into(),
            }),
        )
        .await
        .unwrap();
    (r.appraisal_id.unwrap(), r.attitude_id.unwrap())
}
async fn owner(pool: &PgPool) -> Uuid {
    sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
        .fetch_one(pool)
        .await
        .unwrap()
}
fn reply(ids: &[Uuid]) -> Value {
    json!({"schema_version":1,"reply":"我目前保留谨慎的评价，自己的态度不能证明他人的感情。","reply_evidence_ids":ids,"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"类型化保存状态只支持当前自我立场"})
}
async fn semantic_rows(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('memories',(SELECT count(*) FROM memories),'beliefs',(SELECT count(*) FROM beliefs),'people',(SELECT count(*) FROM person_entities),'history',(SELECT count(*) FROM autobiographies),'commits',(SELECT count(*) FROM decision_commits),'outgoing',(SELECT count(*) FROM outgoing_messages),'persona',(SELECT persona FROM individuals WHERE singleton))").fetch_one(pool).await.unwrap()
}
async fn assert_one_rejected_author(pool: &PgPool, p: &Provider, l: &RunLease) {
    assert_eq!(p.authors.load(Ordering::SeqCst), 1);
    assert_eq!(p.reviews.load(Ordering::SeqCst), 0);
    let actual:(i64,i64,i32)=sqlx::query_as("SELECT (SELECT count(*) FROM usage_reservations WHERE run_id=$1 AND status='settled'),(SELECT sum(charged_tokens)::bigint FROM usage_reservations WHERE run_id=$1),(SELECT model_calls FROM runs WHERE id=$1)").bind(l.run_id).fetch_one(pool).await.unwrap();
    assert_eq!(actual, (1, 2, 1));
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires fixed social-v2, owned disposable PG and global lock"]
async fn actual_saved_social_state_ids_support_reply_but_never_four_new_event_write_paths(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let source = msg(&s, "沈遥提出讨论时保留事实来源").await;
    let (app, long) = affect(&s, &[source], "typed.boundary").await;
    let who = owner(&pool).await;
    let (l, c) = start(&s, &pool).await;
    assert!(c.social_context.as_ref().unwrap().valid_for(&c));
    for id in [app, long, who] {
        assert!(c.social_context.as_ref().unwrap().ids().any(|i| i == id));
    }
    let good = Provider::new(reply(&[app, long, who]));
    let ready = model_step(&s, &good, &l, vec![], CallLimits::default())
        .await
        .unwrap();
    let receipt = s.commit_decision(&l, &ready, &[]).await.unwrap();
    assert!(receipt.message_id.is_some());
    assert_eq!(good.authors.load(Ordering::SeqCst), 1);
    assert_eq!(good.reviews.load(Ordering::SeqCst), 1);
    for packet in good.packets.lock().unwrap().iter() {
        assert_eq!(packet["packet_version"], "target-citations/8");
        let evidence = packet["evidence_by_target"][0]["evidence"]
            .as_array()
            .unwrap();
        assert_eq!(evidence.len(), 3);
        for item in evidence {
            assert_eq!(item["type"], "social_interpretation");
            assert!(
                item["proof_scope"]
                    .as_str()
                    .unwrap()
                    .contains("not_world_fact_or_others_feelings")
            );
        }
    }
    for variant in 0..4 {
        let (l, c) = start(&s, &pool).await;
        let e = c.current_event_id;
        let mut p = reply(&[long]);
        match variant {
            0 => {
                p["memory_changes"] = json!([{"kind":"episodic","content":"沈遥提出了陈述","scope":"general","source_event_ids":[e],"supersedes":null}])
            }
            1 => {
                p["belief_changes"] = json!([{"statement":"这是有范围的陈述","scope":"general","position":"tentative","support_event_ids":[e],"counter_event_ids":[],"supersedes":null,"brief_basis":"保留来源"}])
            }
            2 => {
                p["memory_changes"] = json!([{"kind":"semantic","content":"沈遥提出了陈述","scope":"general","source_event_ids":[e],"supersedes":null}]);
                p["entity_changes"] = json!([{"memory":{"kind":"created","index":0},"person":{"kind":"new","name":"沈遥"},"role":"subject","source":{"event_id":e,"start_char":0,"end_char":2},"memory_start_char":0}]);
            }
            _ => {
                p["autobiography_changes"] = json!([{"topic":"social.current.view","title":"当前解释","interpretation":"仅依据实际交流保留范围","event_ids":[e],"belief_ids":[],"supersedes":null}])
            }
        }
        // Every control draft with a real event is valid before replacing only
        // its purported event reference with an actually selected state UUID.
        parse(&p).validate_context(&c).unwrap();
        match variant {
            0 => p["memory_changes"][0]["source_event_ids"] = json!([app]),
            1 => p["belief_changes"][0]["support_event_ids"] = json!([who]),
            2 => p["entity_changes"][0]["source"]["event_id"] = json!(long),
            _ => p["autobiography_changes"][0]["event_ids"] = json!([long]),
        }
        let before = semantic_rows(&pool).await;
        let bad = Provider::new(p);
        let result = model_step(&s, &bad, &l, vec![], CallLimits::default()).await;
        assert!(matches!(
            result,
            Err(StepError::Model(ModelFailure {
                error: ModelError::Proposal(_),
                ..
            }))
        ));
        assert_one_rejected_author(&pool, &bad, &l).await;
        assert_eq!(semantic_rows(&pool).await, before);
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires fixed social-v2, owned disposable PG and global lock"]
async fn actual_large_source_bundle_omission_cannot_be_bypassed_by_remembered_state_uuid(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let mut sources = vec![];
    // Real bounded owner inputs, rather than rewriting a stored event payload.
    for _ in 0..8 {
        sources.push(msg(&s, &"x".repeat(12000)).await);
    }
    let (app, long) = affect(&s, &sources, "large.actual.bundle").await;
    for n in 0..12 {
        s.import_document(
            &format!("padding-{n}"),
            "独立的小资料",
            "小段公开素材",
            "synthetic",
        )
        .await
        .unwrap();
    }
    let (l, c) = start(&s, &pool).await;
    let selected = c.social_context.as_ref().unwrap();
    assert!(selected.has_more && selected.valid_for(&c));
    for id in [app, long] {
        assert!(!selected.ids().any(|i| i == id));
        assert!(!c.input_manifest.contains(&id));
    }
    assert!(
        sources
            .iter()
            .all(|id| !c.events.iter().any(|e| e.id == *id))
    );
    assert!(serde_json::to_vec(&c).unwrap().len() <= 96 * 1024);
    let before = semantic_rows(&pool).await;
    let bad = Provider::new(reply(&[long]));
    let result = model_step(&s, &bad, &l, vec![], CallLimits::default()).await;
    assert!(matches!(
        result,
        Err(StepError::Model(ModelFailure {
            error: ModelError::Proposal(_),
            ..
        }))
    ));
    assert_one_rejected_author(&pool, &bad, &l).await;
    assert_eq!(semantic_rows(&pool).await, before);
}

async fn revise(
    s: &Store,
    pool: &PgPool,
    who: Uuid,
    event: Uuid,
    previous: Option<Uuid>,
    first: bool,
) -> Uuid {
    let version: i64 = sqlx::query_scalar("SELECT version FROM relationships WHERE subject_id=$1")
        .bind(who)
        .fetch_one(pool)
        .await
        .unwrap();
    let mut refs = vec![who, event];
    refs.extend(previous);
    let changes = if first {
        RelationshipDelta {
            expectations: Some(vec!["讨论时明确区分依据与解释".into()]),
            ..Default::default()
        }
    } else {
        RelationshipDelta {
            boundaries: Some(vec!["不替对方断言感情".into()]),
            ..Default::default()
        }
    };
    s.revise_relationship(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        &refs,
        &RelationshipRevisionDraft {
            subject_id: who,
            expected_relationship_version: version,
            changes,
            event_ids: vec![event],
            brief_basis: "实际合成交流的关系修订".into(),
        },
    )
    .await
    .unwrap()
    .revision_id
}
#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires fixed social-v2, owned disposable PG and global lock"]
async fn revoked_previous_relation_source_without_generic_edges_fences_dispatch_and_preserves_independent_attitude(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let old = msg(&s, "早期关于沟通边界的陈述").await;
    let who = owner(&pool).await;
    let first = revise(&s, &pool, who, old, None, true).await;
    let recent = msg(&s, "新的沟通边界陈述").await;
    let second = revise(&s, &pool, who, recent, Some(first), false).await;
    let independent = msg(&s, "独立公开资料引起的个人评价").await;
    let (app, long) = affect(&s, &[independent], "independent.attitude").await;
    let (l, c) = start(&s, &pool).await;
    for id in [old, first, second, who] {
        assert!(c.input_manifest.contains(&id));
    }
    let before = s.snapshot().await.unwrap().unwrap();
    let r = s
        .reserve_model_call(
            &l,
            Uuid::new_v4(),
            &c,
            TokenAllowance {
                input_upper_bound: 200,
                output_cap: 32,
            },
        )
        .await
        .unwrap();
    // Defined privacy withdrawal plus a missing generic index: actual previous
    // revision/event FKs must remain the authoritative barrier.
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.dispatch_model_call(&l, r.id).await,
        Err(StoreError::LeaseLost)
    ));
    let fresh = s.model_context(l.run_id, vec![]).await.unwrap();
    let sc = fresh.social_context.as_ref().unwrap();
    assert!(sc.valid_for(&fresh) && sc.has_more);
    assert!(!sc.relationships.iter().any(|x| x.subject_id == who));
    assert!(fresh.relationship.as_object().unwrap().is_empty());
    assert_eq!(sc.appraisal.as_ref().unwrap().id, app);
    assert!(sc.attitudes.iter().any(|a| a.id == long));
    assert!(fresh.events.iter().any(|e| e.id == independent));
    for id in [old, first, second, who] {
        assert!(!fresh.input_manifest.contains(&id));
    }
    let(sent,calls):(bool,i32)=sqlx::query_as("SELECT u.dispatched_at IS NOT NULL,r.model_calls FROM usage_reservations u JOIN runs r ON r.id=u.run_id WHERE u.id=$1").bind(r.id).fetch_one(&pool).await.unwrap();
    assert!(!sent);
    assert_eq!(calls, 0);
    let after = s.snapshot().await.unwrap().unwrap();
    assert_eq!(after.runtime_config, before.runtime_config);
    assert_eq!(after.persona, before.persona);
}
