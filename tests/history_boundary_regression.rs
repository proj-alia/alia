//! Independent fixed-v3 engineering gates. All provider outputs/usage are synthetic.
//! Mode::Live exercises the production review gate; no network or semantic grading.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        autobiography::AutobiographyDraft,
        beliefs::{BeliefDraft, BeliefPosition},
        cognition::{DecisionProposal, ModelContext, ProposalError},
        config::{PersonaSeed, RuntimeRules},
        grounding::{self, ContentCheck, DecisionAssessment, ReviewTarget, Support},
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

#[derive(Clone, Copy)]
enum Opinion {
    Complete,
    MissingSecondTarget,
    MissingPredecessor,
}
struct Provider {
    proposal: Value,
    opinion: Opinion,
    authors: AtomicUsize,
    reviews: AtomicUsize,
    packets: Mutex<Vec<Value>>,
}
impl Provider {
    fn new(proposal: Value, opinion: Opinion) -> Self {
        Self {
            proposal,
            opinion,
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
        self.authors.fetch_add(1, Ordering::SeqCst);
        Ok(ModelResult {
            proposal: DecisionProposal::parse(&serde_json::to_vec(&self.proposal).unwrap())
                .unwrap(),
            usage: usage(),
            provider_model: "p01-synthetic-history-gate".into(),
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
        let mut checks = vec![];
        for target in grounding::targets(p) {
            if matches!(self.opinion, Opinion::MissingSecondTarget)
                && target == (ReviewTarget::Autobiography { index: 1 })
            {
                continue;
            }
            let mut ids = match target {
                ReviewTarget::Appraisal { index } => p.appraisal_changes[index].event_ids.clone(),
                ReviewTarget::Attitude { index } => p.attitude_changes[index].event_ids.clone(),
                ReviewTarget::Relationship { index } => {
                    p.relationship_changes[index].event_ids.clone()
                }
                ReviewTarget::Reply => p.reply_evidence_ids.clone(),
                ReviewTarget::Memory { index } => p.memory_changes[index].source_event_ids.clone(),
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
                ReviewTarget::Development => {
                    let d = p.development.as_ref().unwrap();
                    d.evidence_ids
                        .iter()
                        .chain(&d.counterevidence_ids)
                        .copied()
                        .collect()
                }
            };
            if matches!(self.opinion, Opinion::MissingPredecessor)
                && target == (ReviewTarget::Autobiography { index: 1 })
            {
                let old = p.autobiography_changes[1].supersedes.unwrap();
                ids.retain(|id| *id != old);
            }
            checks.push(ContentCheck {
                target,
                verdict: Support::Supported,
                reason: "合成意见不认证事实，只检验拒绝/提交门".into(),
                evidence_ids: ids,
            });
        }
        Ok(DecisionReviewResult {
            assessment: DecisionAssessment {
                schema_version: 1,
                checks,
            },
            usage: usage(),
            provider_model: "p01-synthetic-history-gate".into(),
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
async fn event(s: &Store, text: &str) -> Uuid {
    s.submit_message(&Uuid::new_v4().to_string(), text, None)
        .await
        .unwrap()
        .event_id
}
async fn start(s: &Store, pool: &PgPool) -> (RunLease, Uuid) {
    let m = s
        .submit_message(
            &Uuid::new_v4().to_string(),
            "请重新理解这些有来源的交流，不将解释认证为亲历事实。",
            None,
        )
        .await
        .unwrap();
    // Fixture scheduling only: select the actual new foreground request.
    sqlx::query("UPDATE runs SET status='cancelled' WHERE status='queued' AND id<>$1")
        .bind(m.operation_id)
        .execute(pool)
        .await
        .unwrap();
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(l.run_id, m.operation_id);
    (l, m.event_id)
}
async fn belief(s: &Store, support: Uuid, counter: Uuid, previous: Option<Uuid>) -> Uuid {
    let mut ids = vec![support, counter];
    ids.extend(previous);
    s.record_belief(
        &Uuid::new_v4().to_string(),
        s.snapshot().await.unwrap().unwrap().expected(),
        &ids,
        &BeliefDraft {
            statement: "保留条件和反例的当前认识".into(),
            scope: "general".into(),
            position: BeliefPosition::Questioned,
            support_event_ids: vec![support],
            counter_event_ids: vec![counter],
            supersedes: previous,
            brief_basis: "合成来源边界".into(),
        },
    )
    .await
    .unwrap()
    .belief_id
}
async fn bases(s: &Store) -> (Uuid, Uuid, Uuid, Uuid) {
    let support = event(s, "早期陈述：一般先研究").await;
    let counter = event(s, "反例陈述：这次一小时内先原型").await;
    let old = belief(s, support, counter, None).await;
    let current = belief(s, support, counter, Some(old)).await;
    let mut histories = vec![];
    for topic in ["review.first", "review.second"] {
        histories.push(
            s.record_autobiography(
                &Uuid::new_v4().to_string(),
                s.snapshot().await.unwrap().unwrap().expected(),
                &[support, counter, old, current],
                &AutobiographyDraft {
                    topic: topic.into(),
                    title: "先前解释".into(),
                    interpretation: "保留旧认识及其反证；这是当前解释而非世界事实。".into(),
                    event_ids: vec![support, counter],
                    belief_ids: vec![current],
                    supersedes: None,
                },
            )
            .await
            .unwrap()
            .autobiography_id,
        );
    }
    (current, histories[0], histories[1], counter)
}
fn draft(source: Uuid, previous: Option<(Uuid, Uuid, Uuid)>) -> Value {
    let mut histories = vec![];
    for (i, topic) in ["review.first", "review.second"].iter().enumerate() {
        histories.push(json!({"topic":topic,"title":"新解释","interpretation":"这次陈述使我修正理解，保留条件和来源；不把旧立场当亲历。","event_ids":[source],"belief_ids":previous.map(|(b,_,_)|vec![b]).unwrap_or_default(),"supersedes":previous.map(|(_,a,b)|if i==0{a}else{b})}));
    }
    json!({"schema_version":1,"reply":"我会保存对交流历史的当前理解。","reply_evidence_ids":[source],"memory_changes":[],"autobiography_changes":histories,"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"解释不能替代原事件"})
}
async fn history_snapshot(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('histories',(SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM autobiographies t),'beliefs',(SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM beliefs t),'memories',(SELECT jsonb_agg(to_jsonb(t) ORDER BY id) FROM memories t),'events',(SELECT count(*) FROM events),'outbox',(SELECT count(*) FROM outbox),'commits',(SELECT count(*) FROM decision_commits),'outgoing',(SELECT count(*) FROM outgoing_messages),'version',(SELECT state_version FROM individuals WHERE singleton),'charged',(SELECT sum(charged_tokens) FROM usage_reservations))").fetch_one(pool).await.unwrap()
}
async fn usage_for(pool: &PgPool, run: Uuid) -> (i64, i64) {
    sqlx::query_as("SELECT count(*),COALESCE(sum(charged_tokens),0)::bigint FROM usage_reservations WHERE run_id=$1 AND status='settled'").bind(run).fetch_one(pool).await.unwrap()
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated owned PostgreSQL, fixed v3 source and global heavy lock"]
async fn two_history_target_and_predecessor_omissions_never_become_live_approval(pool: PgPool) {
    let s = setup(&pool).await;
    let (b, a1, a2, counter) = bases(&s).await;
    for opinion in [Opinion::MissingSecondTarget, Opinion::MissingPredecessor] {
        let (l, source) = start(&s, &pool).await;
        let provider = Provider::new(draft(source, Some((b, a1, a2))), opinion);
        let result = model_step(&s, &provider, &l, vec![], CallLimits::default()).await;
        assert!(matches!(
            result,
            Err(StepError::Model(ModelFailure {
                error: ModelError::Assessment(_),
                ..
            }))
        ));
        assert_eq!(provider.authors.load(Ordering::SeqCst), 1);
        assert_eq!(provider.reviews.load(Ordering::SeqCst), 2);
        assert_eq!(usage_for(&pool, l.run_id).await, (3, 6));
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM decision_commits")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM autobiographies")
                .fetch_one(&pool)
                .await
                .unwrap(),
            2
        );
        assert_eq!(
            sqlx::query_scalar::<_, i64>("SELECT count(*) FROM outgoing_messages")
                .fetch_one(&pool)
                .await
                .unwrap(),
            0
        );
        for packet in provider.packets.lock().unwrap().iter() {
            assert_eq!(packet["packet_version"], "target-citations/11");
            for target in packet["evidence_by_target"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|t| t["target"]["kind"] == "autobiography")
            {
                let evidence = target["evidence"].as_array().unwrap();
                let stance = evidence
                    .iter()
                    .find(|e| e["type"] == "historical_belief")
                    .unwrap();
                let lineage = stance["complete_lineage"].as_array().unwrap();
                assert_eq!(lineage.len(), 2);
                assert!(
                    lineage
                        .iter()
                        .any(|r| r["record"]["status"] == "superseded")
                );
                assert!(lineage.iter().any(|r| {
                    r["basis_events"]
                        .as_array()
                        .unwrap()
                        .iter()
                        .any(|e| e["record"]["id"] == json!(counter) && e["counter"] == true)
                }));
                assert!(evidence.iter().any(|e| e["type"] == "autobiography"
                    && e["proof_scope"]
                        == "current_historical_interpretation_not_event_or_world_fact"));
            }
        }
    }
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated owned PostgreSQL, fixed v3 source and global heavy lock"]
async fn omitted_65_stance_lineage_cannot_be_reintroduced_as_complete_history(pool: PgPool) {
    let s = setup(&pool).await;
    let support = event(&s, "有范围的早期陈述").await;
    let counter = event(&s, "保留这条反例").await;
    let mut current = None;
    for _ in 0..65 {
        current = Some(belief(&s, support, counter, current).await);
    }
    let current = current.unwrap();
    let (l, source) = start(&s, &pool).await;
    let c = s.model_context(l.run_id, vec![]).await.unwrap();
    let history = c.autobiographical_context.as_ref().unwrap();
    assert!(history.has_more && history.belief_basis.is_empty() && history.entries.is_empty());
    assert!(
        c.selected_beliefs
            .as_ref()
            .unwrap()
            .items
            .iter()
            .any(|b| b.id == current)
    );
    assert!(serde_json::to_vec(&c).unwrap().len() <= 96 * 1024);
    let mut p = draft(source, None);
    p["autobiography_changes"][0]["belief_ids"] = json!([current]);
    let provider = Provider::new(p, Opinion::Complete);
    let result = model_step(&s, &provider, &l, vec![], CallLimits::default()).await;
    assert!(matches!(
        result,
        Err(StepError::Model(ModelFailure {
            error: ModelError::Proposal(ProposalError::Autobiography),
            ..
        }))
    ));
    assert_eq!(provider.authors.load(Ordering::SeqCst), 1);
    assert_eq!(provider.reviews.load(Ordering::SeqCst), 0);
    assert_eq!(usage_for(&pool, l.run_id).await, (1, 2));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM autobiographies")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM decision_commits")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test(migrations = "./migrations")]
#[ignore = "requires isolated owned PostgreSQL, fixed v3 source and global heavy lock"]
async fn second_history_insert_fault_rolls_back_both_revisions_and_siblings_then_replays_once(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (b, a1, a2, counter) = bases(&s).await;
    let (l, source) = start(&s, &pool).await;
    let mut p = draft(source, Some((b, a1, a2)));
    p["memory_changes"] = json!([{"content":"用户明确要求保留来源和范围","kind":"semantic","scope":"general","source_event_ids":[source],"supersedes":null}]);
    p["belief_changes"] = json!([{"statement":"本次沟通要求有范围的解释","scope":"general","position":"tentative","support_event_ids":[source],"counter_event_ids":[],"supersedes":null,"brief_basis":"实际陈述"}]);
    let provider = Provider::new(p, Opinion::Complete);
    let ready = model_step(&s, &provider, &l, vec![], CallLimits::default())
        .await
        .unwrap();
    assert!(ready.grounding_review_id().is_some());
    assert_eq!(usage_for(&pool, l.run_id).await, (2, 4));
    let before = history_snapshot(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION p01_fail_second_history() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.topic='review.second' AND NEW.supersedes IS NOT NULL THEN RAISE EXCEPTION 'p01 synthetic second-history insert fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER p01_fail_second_history BEFORE INSERT ON autobiographies FOR EACH ROW EXECUTE FUNCTION p01_fail_second_history();").execute(&pool).await.unwrap();
    assert!(matches!(
        s.commit_decision(&l, &ready, &[]).await,
        Err(StoreError::Database(_))
    ));
    assert_eq!(history_snapshot(&pool).await, before);
    sqlx::raw_sql("DROP TRIGGER p01_fail_second_history ON autobiographies; DROP FUNCTION p01_fail_second_history();").execute(&pool).await.unwrap();
    let receipt = s.commit_decision(&l, &ready, &[]).await.unwrap();
    assert_eq!(receipt.autobiography_ids.len(), 2);
    assert_eq!(receipt.memory_ids.len(), 1);
    assert_eq!(receipt.belief_ids.len(), 1);
    assert!(receipt.message_id.is_some());
    let committed = history_snapshot(&pool).await;
    let (one, two) = tokio::join!(
        s.commit_decision(&l, &ready, &[]),
        s.commit_decision(&l, &ready, &[])
    );
    assert_eq!(one.unwrap(), receipt);
    assert_eq!(two.unwrap(), receipt);
    assert_eq!(history_snapshot(&pool).await, committed);
    let current: i64 =
        sqlx::query_scalar("SELECT count(*) FROM autobiographies WHERE status='current'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(current, 2);
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM autobiographies WHERE id=ANY($1) AND status='superseded'"
        )
        .bind([a1, a2].as_slice())
        .fetch_one(&pool)
        .await
        .unwrap(),
        2
    );
    // Defined source revocation, not arbitrary mutation of PreparedDecision.
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(counter)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.commit_decision(&l, &ready, &[]).await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(usage_for(&pool, l.run_id).await, (2, 4));
    assert_eq!(provider.authors.load(Ordering::SeqCst), 1);
    assert_eq!(provider.reviews.load(Ordering::SeqCst), 1);
}
