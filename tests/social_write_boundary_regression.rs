//! P04 non-author review of fixed Root r3. Fixture helpers are derived from its frozen social_decisions source; assertions below are independent. No network/provider calls.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        cognition::{DecisionProposal, ModelContext},
        config::{PersonaSeed, RuntimeRules},
        grounding::{self, ContentCheck, DecisionAssessment, ReviewTarget, Support},
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        execution::{PreparedDecision, model_step},
        model::*,
        scheduling::{RunLease, TokenAllowance},
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
fn parse(p: &Value) -> DecisionProposal {
    DecisionProposal::parse(&serde_json::to_vec(p).unwrap()).unwrap()
}
fn proposal(source: Uuid, who: Uuid, version: i64, previous: Option<Uuid>) -> Value {
    json!({"schema_version":1,"reply":"我会记录这次交流带来的评价，并按我们明确的沟通边界继续讨论。","reply_evidence_ids":[source],"memory_changes":[],"appraisal_changes":[{"event_ids":[source],"goal_id":null,"subject_id":who,"valence":0.1,"arousal":0.4,"goal_relevance":0.5,"expectation_gap":0.2,"controllability":0.7,"uncertainty":0.5,"brief_basis":"当事人明确陈述使我保持谨慎好奇"}],"attitude_changes":[{"topic":"discussion.evidence","subject_id":who,"stance":"讨论分歧时先核对依据，接受有效反证","event_ids":[source],"supersedes":previous,"brief_basis":"对共同讨论方式的长期倾向，不断言对方感情"}],"relationship_changes":[{"subject_id":who,"expected_relationship_version":version,"changes":{"expectations":["讨论分歧时说明具体依据"]},"event_ids":[source],"brief_basis":"保留明确沟通约定"}],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"短期评价、长期态度和关系记录分别保存"})
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
async fn start(s: &Store, pool: &PgPool, text: &str) -> (RunLease, Uuid, Uuid, i64) {
    let m = s
        .submit_message(&Uuid::new_v4().to_string(), text, None)
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET status='cancelled' WHERE status='queued' AND id<>$1")
        .bind(m.operation_id)
        .execute(pool)
        .await
        .unwrap();
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(l.run_id, m.operation_id);
    let (who, v): (Uuid, i64) =
        sqlx::query_as("SELECT subject_id,version FROM relationships WHERE subject_key='owner'")
            .fetch_one(pool)
            .await
            .unwrap();
    (l, m.event_id, who, v)
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
    let mut result = serde_json::Map::new();
    let tables: Vec<String> = sqlx::query_scalar(
        "SELECT tablename FROM pg_tables WHERE schemaname='public' ORDER BY tablename",
    )
    .fetch_all(pool)
    .await
    .unwrap();
    for t in tables {
        assert!(t.chars().all(|c| c.is_ascii_alphanumeric() || c == '_'));
        let hash:String=sqlx::query_scalar(&format!("SELECT md5(COALESCE(string_agg(to_jsonb(t)::text,'' ORDER BY to_jsonb(t)::text),'')) FROM \"{t}\" t")).fetch_one(pool).await.unwrap();
        result.insert(t, json!(hash));
    }
    Value::Object(result)
}

#[sqlx::test]
#[ignore = "P04 non-author fixed schema27 isolated PG review; no live provider"]
async fn cached_receipt_must_not_return_newly_created_objects_that_are_directly_blocked(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (l, e, who, v) = start(&s, &pool, "synthetic source for receipt object boundary").await;
    let ready = prepare(&s, &l, proposal(e, who, v, None)).await;
    let receipt = s.commit_decision(&l, &ready, &[]).await.unwrap();
    let mut resources = vec![
        ("appraisal", receipt.appraisal_ids[0]),
        ("attitude", receipt.attitude_ids[0]),
        (
            "relationship_revision",
            receipt.relationship_revision_ids[0],
        ),
    ];
    resources.extend(receipt.message_id.map(|id| ("message", id)));
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    let mut leaked = vec![];
    for (kind, id) in resources {
        sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        let before = snapshot(&pool).await;
        let result = s.commit_decision(&l, &ready, &[]).await;
        if let Ok(replayed) = result {
            assert_eq!(replayed, receipt);
            leaked.push(kind);
        }
        assert_eq!(
            snapshot(&pool).await,
            before,
            "replay must not mutate stored state"
        );
        sqlx::query("UPDATE objects SET blocked=false WHERE id=$1")
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
    }
    assert!(
        leaked.is_empty(),
        "cached receipt returned directly blocked output identifiers: {leaked:?}"
    );
}

#[sqlx::test]
#[ignore = "P04 non-author fixed schema27 isolated PG review; no live provider"]
async fn cached_receipt_must_recheck_post_author_grounding_proof_when_edges_are_missing(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (l, e, who, v) = start(&s, &pool, "synthetic source for review proof boundary").await;
    let ready = prepare(&s, &l, proposal(e, who, v, None)).await;
    let review = ready.grounding_review_id().unwrap();
    let receipt = s.commit_decision(&l, &ready, &[]).await.unwrap();
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(review)
        .execute(&pool)
        .await
        .unwrap();
    let before = snapshot(&pool).await;
    let result = s.commit_decision(&l, &ready, &[]).await;
    assert_eq!(snapshot(&pool).await, before);
    assert!(
        matches!(
            result,
            Err(StoreError::StaleState) | Err(StoreError::NotFound)
        ),
        "cached decision with a blocked actual grounding proof returned {result:?}; original receipt was {receipt:?}"
    );
}
