//! Synthetic semantic opinions test actual review/atomic SQL gates, never live-model quality.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        beliefs::{BeliefDraft, BeliefPosition},
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
    json!({"schema_version":1,"reply":"我会记录现在对这次交流的理解。","reply_evidence_ids":[source],"memory_changes":[],"autobiography_changes":[{"topic":"communication.preferences","title":"对交流方式的理解","interpretation":"这次陈述使我重新理解对方的沟通偏好；这仍是有范围的认识。","event_ids":[source],"belief_ids":[],"supersedes":previous}],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"保留事实与解释的区别"})
}
fn parse(p: &Value) -> DecisionProposal {
    DecisionProposal::parse(&serde_json::to_vec(p).unwrap()).unwrap()
}
fn sample() -> ModelContext {
    serde_json::from_str(include_str!("../evals/model_context.example.json")).unwrap()
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
    sqlx::query_scalar("SELECT jsonb_build_object('autobiographies',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM autobiographies a),'beliefs',(SELECT jsonb_agg(to_jsonb(b) ORDER BY b.id) FROM beliefs b),'events',(SELECT count(*) FROM events),'memories',(SELECT count(*) FROM memories),'outbox',(SELECT count(*) FROM outbox),'commits',(SELECT count(*) FROM decision_commits),'outgoing',(SELECT count(*) FROM outgoing_messages),'version',(SELECT state_version FROM individuals WHERE singleton),'charged',(SELECT sum(charged_tokens) FROM usage_reservations))")
        .fetch_one(pool).await.unwrap()
}

#[test]
fn proposed_interpretations_require_actual_visible_sources_and_distinct_topics() {
    let mut c = sample();
    let id = c.events[0].id;
    let p = proposal(id, None);
    assert!(parse(&p).validate_context(&c).is_ok());
    c.events[0].source_kind = alia::domain::history::SourceKind::FictionalBackground;
    assert!(parse(&p).validate_context(&c).is_err());
    c.events[0].source_kind = alia::domain::history::SourceKind::ReportedClaim;
    c.events[0].kind = "decision.reviewed".into();
    assert!(parse(&p).validate_context(&c).is_err());
    let mut repeated = p.clone();
    repeated["autobiography_changes"]
        .as_array_mut()
        .unwrap()
        .push(p["autobiography_changes"][0].clone());
    assert!(parse(&repeated).validate_context(&sample()).is_err());
    for (field, value) in [
        ("permissions", json!(["all"])),
        ("event_ids", json!([])),
        ("topic", json!("../identity")),
    ] {
        let mut bad = p.clone();
        bad["autobiography_changes"][0][field] = value;
        assert!(DecisionProposal::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
    }
}

#[test]
fn reviewer_receives_full_stance_lineage_counterevidence_and_requires_every_reference() {
    use alia::domain::{
        autobiography::{AutobiographyContext, HistoricalBelief},
        beliefs::BeliefPosition,
    };
    let mut c = sample();
    let support = c.events[0].id;
    let mut counter = c.events[0].clone();
    counter.id = Uuid::new_v4();
    counter.sequence += 1;
    counter.payload = json!({"text":"此前认识存在反例"});
    let counter_id = counter.id;
    c.events.push(counter);
    let old = Uuid::new_v4();
    let current = Uuid::new_v4();
    c.input_manifest.extend([old, current, counter_id]);
    c.autobiographical_context = Some(AutobiographyContext {
        entries: vec![],
        has_more: false,
        belief_basis: vec![
            HistoricalBelief {
                id: old,
                version: 2,
                statement: "旧认识".into(),
                scope: "general".into(),
                position: BeliefPosition::Held,
                brief_basis: "此前陈述".into(),
                status: "superseded".into(),
                supersedes: None,
                support_event_ids: vec![support],
                counter_event_ids: vec![],
            },
            HistoricalBelief {
                id: current,
                version: 1,
                statement: "保留反例的新认识".into(),
                scope: "general".into(),
                position: BeliefPosition::Questioned,
                brief_basis: "新反证".into(),
                status: "current".into(),
                supersedes: Some(old),
                support_event_ids: vec![support],
                counter_event_ids: vec![counter_id],
            },
        ],
    });
    let mut p = proposal(support, None);
    p["autobiography_changes"][0]["belief_ids"] = json!([current]);
    let p = parse(&p);
    p.validate_context(&c).unwrap();
    let review = grounding::review_context(&c, Uuid::new_v4(), &p).unwrap();
    let packet = grounding::evidence_packet(&review).unwrap();
    let text = packet.to_string();
    assert_eq!(packet["packet_version"], "target-citations/11");
    assert!(text.contains("historical_belief"));
    assert!(text.contains("complete_lineage"));
    assert!(text.contains("superseded"));
    assert!(text.contains(&old.to_string()));
    assert!(text.contains(&counter_id.to_string()));
    let checks = grounding::targets(&p)
        .into_iter()
        .map(|target| ContentCheck {
            evidence_ids: match target {
                ReviewTarget::Appraisal { index } => p.appraisal_changes[index].event_ids.clone(),
                ReviewTarget::Attitude { index } => p.attitude_changes[index].event_ids.clone(),
                ReviewTarget::Relationship { index } => {
                    p.relationship_changes[index].event_ids.clone()
                }
                ReviewTarget::Reply => p.reply_evidence_ids.clone(),
                ReviewTarget::Autobiography { index } => {
                    p.autobiography_changes[index].review_references()
                }
                _ => unreachable!(),
            },
            target,
            verdict: Support::Supported,
            reason: "合成协议检查".into(),
        })
        .collect();
    let mut opinion = DecisionAssessment {
        schema_version: 1,
        checks,
    };
    opinion.validate(&review).unwrap();
    opinion.checks[1].evidence_ids.retain(|id| *id != current);
    assert!(opinion.validate(&review).is_err());
    opinion.checks.remove(1);
    assert!(opinion.validate(&review).is_err());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn actual_model_history_write_revision_restart_and_idempotency_are_atomic(pool: PgPool) {
    let s = setup(&pool).await;
    let (lease, source) = start(&s, &pool, "我通常喜欢先研究").await;
    let original: Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=$1")
        .bind(source)
        .fetch_one(&pool)
        .await
        .unwrap();
    let ready = prepare(&s, &lease, proposal(source, None)).await;
    assert!(ready.grounding_review_id().is_some());
    let first = s.commit_decision(&lease, &ready, &[]).await.unwrap();
    assert_eq!(first.autobiography_ids.len(), 1);
    let before = snapshot(&pool).await;
    assert_eq!(s.commit_decision(&lease, &ready, &[]).await.unwrap(), first);
    assert_eq!(snapshot(&pool).await, before);
    let restarted = Store::new(pool.clone());
    let (next, event) = start(&restarted, &pool, "这次情况不同，先做原型").await;
    let c = restarted.model_context(next.run_id, vec![]).await.unwrap();
    assert_eq!(
        c.autobiographical_context.as_ref().unwrap().entries[0].id,
        first.autobiography_ids[0]
    );
    let ready = prepare(
        &restarted,
        &next,
        proposal(event, Some(first.autobiography_ids[0])),
    )
    .await;
    let second = restarted.commit_decision(&next, &ready, &[]).await.unwrap();
    assert_eq!(second.autobiography_ids.len(), 1);
    let history = restarted.autobiography(true, 100).await.unwrap();
    assert_eq!(history["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        history["items"][0]["supersedes"],
        json!(first.autobiography_ids[0])
    );
    assert_eq!(history["items"][1]["status"], "superseded");
    assert_eq!(
        sqlx::query_scalar::<_, Value>("SELECT to_jsonb(e) FROM events e WHERE id=$1")
            .bind(source)
            .fetch_one(&pool)
            .await
            .unwrap(),
        original
    );
    let (kind, correlation): (String, Uuid) = sqlx::query_as(
        "SELECT source_kind,correlation_id FROM events WHERE kind='autobiography.revised'",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(kind, "inference");
    assert_eq!(correlation, next.run_id);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn rejected_history_only_candidate_is_charged_and_never_persisted(pool: PgPool) {
    let s = setup(&pool).await;
    let (lease, source) = start(&s, &pool, "一次真实交流").await;
    let mut p = proposal(source, None);
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
    assert!(
        s.autobiography(true, 100).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let charged:(i64,i64)=sqlx::query_as("SELECT count(*),sum(charged_tokens)::bigint FROM usage_reservations WHERE status='settled'").fetch_one(&pool).await.unwrap();
    assert_eq!(charged, (2, 4));
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM decision_commits")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn late_publication_fault_rolls_back_history_reply_and_all_other_changes(pool: PgPool) {
    let s = setup(&pool).await;
    let (lease, source) = start(&s, &pool, "一起研究之后的新理解").await;
    let mut p = proposal(source, None);
    p["belief_changes"] = json!([{"statement":"这次陈述需要保留范围","scope":"general","position":"tentative","support_event_ids":[source],"counter_event_ids":[],"supersedes":null,"brief_basis":"来自实际陈述"}]);
    p["memory_changes"] = json!([{"content":"对方提出了一次有范围的陈述","kind":"semantic","scope":"general","source_event_ids":[source],"supersedes":null}]);
    let ready = prepare(&s, &lease, p).await;
    let before = snapshot(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION fail_publish_history() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='operation.updated' THEN RAISE EXCEPTION 'qa fault'; END IF; RETURN NEW; END $$; CREATE TRIGGER qa_publish_history BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION fail_publish_history();").execute(&pool).await.unwrap();
    assert!(s.commit_decision(&lease, &ready, &[]).await.is_err());
    assert_eq!(snapshot(&pool).await, before);
    sqlx::raw_sql(
        "DROP TRIGGER qa_publish_history ON outbox; DROP FUNCTION fail_publish_history();",
    )
    .execute(&pool)
    .await
    .unwrap();
    let receipt = s.commit_decision(&lease, &ready, &[]).await.unwrap();
    assert_eq!(receipt.autobiography_ids.len(), 1);
    assert_eq!(receipt.belief_ids.len(), 1);
    assert_eq!(receipt.memory_ids.len(), 1);
    assert!(receipt.message_id.is_some());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn first_history_can_use_complete_current_and_predecessor_stance_basis(pool: PgPool) {
    let s = setup(&pool).await;
    let original = s
        .submit_message("early", "我一般先研究", None)
        .await
        .unwrap()
        .event_id;
    let one = s
        .record_belief(
            "one",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[original],
            &BeliefDraft {
                statement: "通常先研究".into(),
                scope: "general".into(),
                position: BeliefPosition::Held,
                support_event_ids: vec![original],
                counter_event_ids: vec![],
                supersedes: None,
                brief_basis: "早期陈述".into(),
            },
        )
        .await
        .unwrap();
    let later = s
        .submit_message("later", "但这次先做原型", None)
        .await
        .unwrap()
        .event_id;
    let two = s
        .record_belief(
            "two",
            s.snapshot().await.unwrap().unwrap().expected(),
            &[original, later, one.belief_id],
            &BeliefDraft {
                statement: "通常偏好不能覆盖本次例外".into(),
                scope: "general".into(),
                position: BeliefPosition::Questioned,
                support_event_ids: vec![original],
                counter_event_ids: vec![later],
                supersedes: Some(one.belief_id),
                brief_basis: "新例外".into(),
            },
        )
        .await
        .unwrap();
    let (lease, source) = start(&s, &pool, "回顾我们的沟通方式").await;
    let c = s.model_context(lease.run_id, vec![]).await.unwrap();
    let history = c.autobiographical_context.as_ref().unwrap();
    assert!(history.entries.is_empty());
    assert_eq!(history.belief_basis.len(), 2);
    assert!(c.events.iter().any(|e| e.id == later));
    assert!(c.input_manifest.contains(&one.belief_id));
    let mut p = proposal(source, None);
    p["autobiography_changes"][0]["belief_ids"] = json!([two.belief_id]);
    let ready = prepare(&s, &lease, p).await;
    let receipt = s.commit_decision(&lease, &ready, &[]).await.unwrap();
    assert_eq!(receipt.autobiography_ids.len(), 1);
    let saved = s.autobiography(false, 100).await.unwrap();
    assert_eq!(saved["items"][0]["belief_ids"], json!([two.belief_id]));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn revoked_source_blocks_prepared_history_and_preserves_the_usage_ledger(pool: PgPool) {
    let s = setup(&pool).await;
    let (lease, source) = start(&s, &pool, "这是之后撤销的来源").await;
    let ready = prepare(&s, &lease, proposal(source, None)).await;
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    let before = snapshot(&pool).await;
    assert!(s.commit_decision(&lease, &ready, &[]).await.is_err());
    assert_eq!(snapshot(&pool).await, before);
    assert!(
        s.autobiography(true, 100).await.unwrap()["items"]
            .as_array()
            .unwrap()
            .is_empty()
    );
}
