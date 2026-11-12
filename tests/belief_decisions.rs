//! Synthetic providers verify execution gates; none of these are live-model evals.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        beliefs::{BeliefDraft, BeliefPosition, CurrentBelief, SelectedBeliefs},
        cognition::{DecisionProposal, ModelContext, ProposalError},
        config::{PersonaSeed, RuntimeRules},
        grounding::{self, ContentCheck, DecisionAssessment, ReviewTarget, Support},
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        execution::{PreparedDecision, StepError, model_step},
        model::*,
        scheduling::{RunLease, TokenAllowance},
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;

fn proposal(source: Uuid, previous: Option<Uuid>) -> Value {
    json!({"schema_version":1,"reply":"我会保留并修正这项理解。","reply_evidence_ids":[source],"memory_changes":[],"belief_changes":[{"statement":"原先的一般理解需要根据新陈述重新审视","scope":"general","position":"questioned","support_event_ids":[source],"counter_event_ids":[],"supersedes":previous,"brief_basis":"根据这次实际陈述形成有范围的解释"}],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"解释不是事实认证"})
}
fn sample() -> ModelContext {
    serde_json::from_str(include_str!("../evals/model_context.example.json")).unwrap()
}
fn selected(c: &mut ModelContext) -> Uuid {
    let id = Uuid::new_v4();
    c.input_manifest.push(id);
    c.selected_beliefs = Some(SelectedBeliefs {
        items: vec![CurrentBelief {
            id,
            version: 1,
            statement: "原先的暂时理解".into(),
            scope: "general".into(),
            position: BeliefPosition::Tentative,
            brief_basis: "保留原先两类来源".into(),
            support_event_ids: vec![c.events[0].id],
            counter_event_ids: vec![c.events[1].id],
        }],
        has_more: false,
    });
    id
}
fn parse(p: &Value) -> DecisionProposal {
    DecisionProposal::parse(p.to_string().as_bytes()).unwrap()
}

#[test]
fn belief_proposals_are_closed_scoped_and_require_current_predecessors() {
    let mut c = sample();
    let id = selected(&mut c);
    let p = proposal(c.current_event_id, Some(id));
    assert!(parse(&p).validate_context(&c).is_ok());
    for replacement in [json!(Uuid::new_v4()), Value::Null] {
        let mut bad = p.clone();
        bad["belief_changes"][0]["supersedes"] = replacement;
        if !bad["belief_changes"][0]["supersedes"].is_null() {
            assert_eq!(parse(&bad).validate_context(&c), Err(ProposalError::Belief));
        } else {
            assert!(parse(&bad).validate_context(&c).is_ok());
        }
    }
    let mut local = p.clone();
    local["belief_changes"][0]["scope"] = json!(c.current_scope);
    assert_eq!(
        parse(&local).validate_context(&c),
        Err(ProposalError::Belief)
    );
    let mut duplicate = parse(&p);
    duplicate
        .belief_changes
        .push(duplicate.belief_changes[0].clone());
    duplicate.belief_changes[1]
        .statement
        .push_str("（另一项提案）");
    assert_eq!(duplicate.validate_context(&c), Err(ProposalError::Belief));
    let mut not_event = p.clone();
    not_event["belief_changes"][0]["support_event_ids"] = json!([id]);
    assert_eq!(
        parse(&not_event).validate_context(&c),
        Err(ProposalError::Belief)
    );
    for (field, value) in [
        ("permissions", json!(["all"])),
        ("position", json!("verified_truth")),
    ] {
        let mut bad = p.clone();
        bad["belief_changes"][0][field] = value;
        assert!(DecisionProposal::parse(bad.to_string().as_bytes()).is_err());
    }
    let old = include_bytes!("../evals/decision.example.json");
    assert!(
        DecisionProposal::parse(old)
            .unwrap()
            .belief_changes
            .is_empty()
    );
    assert!(
        DecisionProposal::parse(include_bytes!("../evals/belief-decision.example.json"))
            .unwrap()
            .validate_context(&c)
            .is_ok()
    );
}

#[test]
fn every_belief_target_requires_both_roles_and_retains_previous_comparison() {
    let mut c = sample();
    let id = selected(&mut c);
    let mut raw = proposal(c.current_event_id, Some(id));
    raw["reply"] = json!("");
    raw["reply_evidence_ids"] = json!([]);
    raw["belief_changes"][0]["counter_event_ids"] = json!([c.events[0].id]);
    let p = parse(&raw);
    let review = grounding::review_context(&c, Uuid::new_v4(), &p).unwrap();
    let packet = grounding::evidence_packet(&review).unwrap();
    assert_eq!(packet["packet_version"], "target-citations/11");
    assert_eq!(
        packet["runtime_atomic_commit_contract"]["belief_change_indices"],
        json!([0])
    );
    let t = &packet["evidence_by_target"][0];
    assert_eq!(t["target"], json!({"kind":"belief","index":0}));
    assert_eq!(
        t["belief_under_review"]["origin"],
        "proposed_interpretation_not_evidence"
    );
    let previous = &t["belief_under_review"]["previous_comparison_not_additional_citations"];
    assert_eq!(previous["record"]["id"], json!(id));
    assert_eq!(previous["basis_events"].as_array().unwrap().len(), 2);
    let mut a = DecisionAssessment {
        schema_version: 1,
        checks: vec![ContentCheck {
            target: ReviewTarget::Belief { index: 0 },
            verdict: Support::Supported,
            reason: "合成完整性意见，不是实际语义评测".into(),
            evidence_ids: vec![c.current_event_id, c.events[0].id],
        }],
    };
    assert!(a.validate(&review).is_ok());
    a.checks[0].evidence_ids = vec![c.current_event_id];
    assert!(
        a.validate(&review).is_err(),
        "support-only acknowledgement cannot approve counterevidence"
    );
    a.checks.clear();
    assert!(
        a.validate(&review).is_err(),
        "belief-only writes cannot skip independent review"
    );
}

struct SyntheticReviewer {
    proposal: Value,
    verdict: Support,
}
fn usage() -> Option<TokenUsage> {
    Some(TokenUsage {
        input: 1,
        output: 1,
        total: 2,
    })
}
#[async_trait::async_trait]
impl ModelProvider for SyntheticReviewer {
    // Intentionally exercise live-mode gates without a network/model call.
    fn mode(&self) -> ProviderMode {
        ProviderMode::Live
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
    fn supports_decision_review(&self) -> bool {
        true
    }
    fn decision_review_allowance(
        &self,
        c: &ModelContext,
        l: CallLimits,
    ) -> Result<TokenAllowance, ModelFailure> {
        assert!(c.decision_review.is_some());
        self.token_allowance(c, l)
    }
    async fn decide(
        &self,
        _: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        Ok(ModelResult {
            proposal: parse(&self.proposal),
            usage: usage(),
            provider_model: "synthetic-belief-gate".into(),
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
        let p = &c.decision_review.as_ref().unwrap().proposal;
        // Verify the exact packet is constructible, never decide semantic truth.
        grounding::evidence_packet(c).unwrap();
        let checks = grounding::targets(p)
            .into_iter()
            .map(|target| {
                let evidence_ids = match target {
                    ReviewTarget::Appraisal { index } => {
                        p.appraisal_changes[index].event_ids.clone()
                    }
                    ReviewTarget::Attitude { index } => p.attitude_changes[index].event_ids.clone(),
                    ReviewTarget::Relationship { index } => {
                        p.relationship_changes[index].event_ids.clone()
                    }
                    ReviewTarget::Reply => p.reply_evidence_ids.clone(),
                    ReviewTarget::Memory { index } => {
                        p.memory_changes[index].source_event_ids.clone()
                    }
                    ReviewTarget::Autobiography { index } => {
                        p.autobiography_changes[index].review_references()
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
                    ReviewTarget::PersonAssertion { index } => {
                        vec![p.entity_changes[index].source.event_id]
                    }
                    ReviewTarget::Development => {
                        let d = p.development.as_ref().unwrap();
                        d.evidence_ids
                            .iter()
                            .chain(&d.counterevidence_ids)
                            .copied()
                            .collect()
                    }
                };
                ContentCheck {
                    target,
                    verdict: self.verdict,
                    evidence_ids,
                    reason: "合成意见只检验协议与事务".into(),
                }
            })
            .collect();
        Ok(DecisionReviewResult {
            assessment: DecisionAssessment {
                schema_version: 1,
                checks,
            },
            usage: usage(),
            provider_model: "synthetic-belief-gate".into(),
            provider_mode: ProviderMode::Live,
        })
    }
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
async fn start(s: &Store, pool: &PgPool, text: &str) -> (RunLease, Uuid) {
    let msg = s
        .submit_message(&Uuid::new_v4().to_string(), text, None)
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET status='cancelled' WHERE status='queued' AND id<>$1")
        .bind(msg.operation_id)
        .execute(pool)
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(lease.run_id, msg.operation_id);
    (lease, msg.event_id)
}
async fn prepare(s: &Store, l: &RunLease, p: Value) -> PreparedDecision {
    model_step(
        s,
        &SyntheticReviewer {
            proposal: p,
            verdict: Support::Supported,
        },
        l,
        vec![],
        CallLimits::default(),
    )
    .await
    .unwrap()
}
async fn snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('beliefs',(SELECT jsonb_agg(to_jsonb(b) ORDER BY b.id) FROM beliefs b),'events',(SELECT count(*) FROM events),'memories',(SELECT count(*) FROM memories),'outbox',(SELECT count(*) FROM outbox),'commits',(SELECT count(*) FROM decision_commits),'outgoing',(SELECT count(*) FROM outgoing_messages),'version',(SELECT state_version FROM individuals WHERE singleton),'charged',(SELECT sum(charged_tokens) FROM usage_reservations))")
        .fetch_one(pool).await.unwrap()
}
async fn seed(s: &Store) -> (Uuid, Uuid) {
    let source = s
        .submit_message("original", "我一般先研究", None)
        .await
        .unwrap()
        .event_id;
    let receipt = s
        .record_belief(
            "initial-belief",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[source],
            &BeliefDraft {
                statement: "对方一般先研究".into(),
                scope: "general".into(),
                position: BeliefPosition::Tentative,
                support_event_ids: vec![source],
                counter_event_ids: vec![],
                supersedes: None,
                brief_basis: "原先陈述".into(),
            },
        )
        .await
        .unwrap();
    (receipt.belief_id, source)
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn reviewed_beliefs_commit_once_with_reply_memory_and_restore_after_reconnection(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (old, original) = seed(&s).await;
    let original_payload: Value =
        sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=$1")
            .bind(original)
            .fetch_one(&pool)
            .await
            .unwrap();
    let (lease, event) = start(&s, &pool, "原先概括需要保留更多例外").await;
    let mut p = proposal(event, Some(old));
    p["belief_changes"][0]["counter_event_ids"] = json!([original]);
    p["memory_changes"] = json!([{"content":"用户提出原概括需要保留例外","kind":"semantic","scope":"general","source_event_ids":[event],"supersedes":null}]);
    let prepared = prepare(&s, &lease, p).await;
    assert!(prepared.grounding_review_id().is_some());
    let receipt = s.commit_decision(&lease, &prepared, &[]).await.unwrap();
    assert_eq!(receipt.belief_ids.len(), 1);
    assert_eq!(receipt.memory_ids.len(), 1);
    assert!(receipt.message_id.is_some());
    let before = snapshot(&pool).await;
    let replay = s.commit_decision(&lease, &prepared, &[]).await.unwrap();
    assert_eq!(
        serde_json::to_value(&receipt).unwrap(),
        serde_json::to_value(replay).unwrap()
    );
    assert_eq!(before, snapshot(&pool).await);
    let rows = s.beliefs(true).await.unwrap();
    assert_eq!(rows.len(), 2);
    assert_eq!(
        rows.iter().find(|b| b["id"] == json!(old)).unwrap()["status"],
        "superseded"
    );
    let current = rows
        .iter()
        .find(|b| b["id"] == json!(receipt.belief_ids[0]))
        .unwrap();
    assert_eq!(current["counter_event_ids"], json!([original]));
    let (source, correlation): (String, Uuid) =
        sqlx::query_as("SELECT source_kind,correlation_id FROM events WHERE kind='belief.revised'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(source, "inference");
    assert_eq!(correlation, lease.run_id);
    assert_eq!(
        original_payload,
        sqlx::query_scalar::<_, Value>("SELECT to_jsonb(e) FROM events e WHERE id=$1")
            .bind(original)
            .fetch_one(&pool)
            .await
            .unwrap()
    );
    let (later, _) = start(&Store::new(pool.clone()), &pool, "继续此前认识").await;
    let restored = Store::new(pool.clone())
        .model_context(later.run_id, vec![])
        .await
        .unwrap();
    let selected = restored.selected_beliefs.unwrap();
    assert_eq!(selected.items.len(), 1);
    assert_eq!(selected.items[0].id, receipt.belief_ids[0]);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn rejected_belief_only_candidate_is_charged_but_cannot_skip_review_or_write(pool: PgPool) {
    let s = setup(&pool).await;
    let (lease, event) = start(&s, &pool, "这是一条待理解陈述").await;
    let mut p = proposal(event, None);
    p["reply"] = json!("");
    p["reply_evidence_ids"] = json!([]);
    let result = model_step(
        &s,
        &SyntheticReviewer {
            proposal: p,
            verdict: Support::Unsupported,
        },
        &lease,
        vec![],
        CallLimits::default(),
    )
    .await;
    assert!(matches!(
        result,
        Err(StepError::Storage(StoreError::Proposal(
            ProposalError::Grounding
        )))
    ));
    assert!(s.beliefs(true).await.unwrap().is_empty());
    let (reservations, charged): (i64, i64) = sqlx::query_as("SELECT count(*),sum(charged_tokens)::bigint FROM usage_reservations WHERE status='settled'").fetch_one(&pool).await.unwrap();
    assert_eq!(
        (reservations, charged),
        (2, 4),
        "author and independent checker both recorded despite rejection"
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM decision_commits")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn late_publish_failure_rolls_back_every_change_and_same_prepared_retry_succeeds(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (old, _) = seed(&s).await;
    let (lease, source) = start(&s, &pool, "需要修订认识").await;
    let mut p = proposal(source, Some(old));
    p["memory_changes"] = json!([{"content":"用户提出修订认识","kind":"semantic","scope":"general","source_event_ids":[source],"supersedes":null}]);
    let prepared = prepare(&s, &lease, p).await;
    let before = snapshot(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION reject_belief_decision_publish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'injected final failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER reject_belief_decision_publish BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION reject_belief_decision_publish();").execute(&pool).await.unwrap();
    assert!(matches!(
        s.commit_decision(&lease, &prepared, &[]).await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(
        before,
        snapshot(&pool).await,
        "model costs survive, all proposed state writes roll back"
    );
    sqlx::query("DROP TRIGGER reject_belief_decision_publish ON outbox")
        .execute(&pool)
        .await
        .unwrap();
    let receipt = s.commit_decision(&lease, &prepared, &[]).await.unwrap();
    assert_eq!(receipt.belief_ids.len(), 1);
    assert_eq!(receipt.memory_ids.len(), 1);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn changed_predecessor_version_and_revoked_sources_reject_late_commit_and_replay(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (old, _) = seed(&s).await;
    let (lease, source) = start(&s, &pool, "修订依据").await;
    let prepared = prepare(&s, &lease, proposal(source, Some(old))).await;
    sqlx::query("UPDATE beliefs SET version=version+1 WHERE id=$1")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
    let before = snapshot(&pool).await;
    assert!(matches!(
        s.commit_decision(&lease, &prepared, &[]).await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(before, snapshot(&pool).await);
    sqlx::query("UPDATE beliefs SET version=version-1 WHERE id=$1")
        .bind(old)
        .execute(&pool)
        .await
        .unwrap();
    let receipt = s.commit_decision(&lease, &prepared, &[]).await.unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.commit_decision(&lease, &prepared, &[]).await,
        Err(StoreError::StaleState)
    ));
    assert!(
        !s.beliefs(true)
            .await
            .unwrap()
            .iter()
            .any(|b| b["id"] == json!(receipt.belief_ids[0]))
    );
}
