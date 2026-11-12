//! Real atomic proposal writes; synthetic model/reviewer, no claim of growth.
#![allow(dead_code)] // This target uses only a subset of the shared fixture.
include!("support/development_context_fixture.rs");
use alia::{
    adapters::store::StoreError,
    domain::{
        cognition::{DecisionProposal, ModelContext, ProposalError},
        grounding::{self, ContentCheck, DecisionAssessment, ReviewTarget, Support},
    },
    runtime::{
        execution::{StepError, model_step},
        model::*,
        scheduling::{RunLease, TokenAllowance},
    },
};

fn candidate(c: &ModelContext) -> DecisionProposal {
    let target = c
        .development_targets
        .iter()
        .find(|t| t.target == DevelopmentTarget::Expression)
        .unwrap();
    let mut value = c.persona["expression"].clone();
    value["tone"] = json!("温和直接，在不确定时先说明依据");
    serde_json::from_value(json!({"schema_version":1,
        "reply":"我会保存一个表达方式调整的候选，等待后续验证。",
        "reply_evidence_ids":[c.current_event_id],"memory_changes":[],"goal_changes":[],"tool_intents":[],
        "development":{"target":"persona.expression","before_version":target.before_version,
        "proposed_value":value,"proposed_description":"在信息不足时更清楚地说明依据",
        "evidence_ids":[c.current_event_id],"counterevidence_ids":[],
        "validation_plan":"在相关和无关场景比较；未观察不当作改善", "deadline":c.now+Duration::hours(1)},
        "brief_reason":"真实工程接线，不是已验证的成长"})).unwrap()
}
fn sample() -> ModelContext {
    let mut c: ModelContext =
        serde_json::from_str(include_str!("../evals/model_context.example.json")).unwrap();
    c.development_targets = vec![DevelopmentTargetView {
        target: DevelopmentTarget::Expression,
        scope: None,
        before_version: 7,
        target_id: None,
        head_revision_id: None,
    }];
    c
}
#[test]
fn typed_value_version_scope_and_complete_sources_are_required() {
    let c = sample();
    let p = candidate(&c);
    assert!(p.validate_context(&c).is_ok());
    let mut bad = p.clone();
    bad.development.as_mut().unwrap().before_version += 1;
    assert_eq!(bad.validate_context(&c), Err(ProposalError::Development));
    let mut bad = p.clone();
    bad.development.as_mut().unwrap().evidence_ids = vec![Uuid::new_v4()];
    assert_eq!(bad.validate_context(&c), Err(ProposalError::Development));
    let mut bad = p.clone();
    bad.development.as_mut().unwrap().counterevidence_ids =
        bad.development.as_ref().unwrap().evidence_ids.clone();
    assert!(bad.validate_context(&c).is_err());
    let mut bad = p.clone();
    bad.development.as_mut().unwrap().proposed_value = c.persona["expression"].clone();
    assert_eq!(bad.validate_context(&c), Err(ProposalError::Development));
    let mut raw = serde_json::to_value(&p).unwrap();
    raw["development"]["proposed_value"]["identity"] = json!({"id":"forged"});
    assert!(DecisionProposal::parse(raw.to_string().as_bytes()).is_err());
    let mut raw = serde_json::to_value(&p).unwrap();
    raw["development"]["target"] = json!("permissions");
    assert!(DecisionProposal::parse(raw.to_string().as_bytes()).is_err());
    let mut c = c.clone();
    c.events
        .iter_mut()
        .find(|e| e.id == c.current_event_id)
        .unwrap()
        .source_kind = alia::domain::history::SourceKind::FictionalBackground;
    assert_eq!(p.validate_context(&c), Err(ProposalError::Development));
}
#[test]
fn proposal_review_requires_every_support_and_counter_event_and_proves_no_acceptance() {
    let mut c = sample();
    let mut e = c
        .events
        .iter()
        .find(|e| e.id == c.current_event_id)
        .unwrap()
        .clone();
    e.id = Uuid::new_v4();
    c.input_manifest.push(e.id);
    c.events.push(e.clone());
    let mut p = candidate(&c);
    p.development.as_mut().unwrap().counterevidence_ids = vec![e.id];
    p.validate_context(&c).unwrap();
    let review = grounding::review_context(&c, Uuid::new_v4(), &p).unwrap();
    let packet = grounding::evidence_packet(&review).unwrap();
    let target = packet["evidence_by_target"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["target"]["kind"] == "development")
        .unwrap();
    assert_eq!(target["allowed_evidence_ids"].as_array().unwrap().len(), 2);
    assert_eq!(
        target["development_under_review"]["status"],
        "proposed_only_not_current_persona_or_trial_result"
    );
    let mut assessment = opinion(&review, Support::Supported);
    assessment.validate(&review).unwrap();
    assessment
        .checks
        .iter_mut()
        .find(|x| x.target == ReviewTarget::Development)
        .unwrap()
        .evidence_ids
        .pop();
    assert!(assessment.validate(&review).is_err());
}
fn opinion(c: &ModelContext, verdict: Support) -> DecisionAssessment {
    let p = &c.decision_review.as_ref().unwrap().proposal;
    DecisionAssessment {
        schema_version: 1,
        checks: grounding::targets(p)
            .into_iter()
            .map(|target| {
                let mut evidence_ids = match target {
                    ReviewTarget::Reply => p.reply_evidence_ids.clone(),
                    ReviewTarget::Development => {
                        let d = p.development.as_ref().unwrap();
                        d.evidence_ids
                            .iter()
                            .chain(&d.counterevidence_ids)
                            .copied()
                            .collect()
                    }
                    _ => panic!("unexpected synthetic target"),
                };
                evidence_ids.sort();
                evidence_ids.dedup();
                ContentCheck {
                    verdict: if target == ReviewTarget::Development {
                        verdict
                    } else {
                        Support::Supported
                    },
                    target,
                    reason: "明确合成审查，不代替真实语义评测".into(),
                    evidence_ids,
                }
            })
            .collect(),
    }
}
struct Developing {
    verdict: Support,
}
#[async_trait::async_trait]
impl ModelProvider for Developing {
    fn mode(&self) -> ProviderMode {
        ProviderMode::Fixture
    }
    fn token_allowance(
        &self,
        _: &ModelContext,
        l: CallLimits,
    ) -> Result<TokenAllowance, ModelFailure> {
        Ok(TokenAllowance {
            input_upper_bound: 100,
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
        self.token_allowance(c, l)
    }
    async fn decide(
        &self,
        c: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        let proposal = candidate(c);
        proposal.validate_context(c).unwrap();
        Ok(ModelResult {
            proposal,
            usage: Some(TokenUsage {
                input: 1,
                output: 1,
                total: 2,
            }),
            provider_model: "synthetic-development-author".into(),
            http: None,
            system_fingerprint: None,
            provider_mode: ProviderMode::Fixture,
        })
    }
    async fn review_decision(
        &self,
        c: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<DecisionReviewResult, ModelFailure> {
        assert!(
            c.decision_review
                .as_ref()
                .unwrap()
                .expected_targets
                .contains(&ReviewTarget::Development)
        );
        Ok(DecisionReviewResult {
            assessment: opinion(c, self.verdict),
            usage: Some(TokenUsage {
                input: 1,
                output: 1,
                total: 2,
            }),
            provider_model: "synthetic-development-reviewer".into(),
            provider_mode: ProviderMode::Fixture,
        })
    }
}
async fn start(pool: &PgPool) -> (Store, RunLease) {
    let s = setup(pool).await;
    message(&s).await;
    let l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    (s, l)
}
async fn ready(s: &Store, l: &RunLease) -> alia::runtime::execution::PreparedDecision {
    model_step(
        s,
        &Developing {
            verdict: Support::Supported,
        },
        l,
        vec![],
        CallLimits::default(),
    )
    .await
    .unwrap()
}

#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; synthetic provider and source-bound reviewer"]
async fn actual_model_proposal_reply_and_receipt_commit_once_and_survive_restart(pool: PgPool) {
    let (s, l) = start(&pool).await;
    let before = s.snapshot().await.unwrap().unwrap();
    let prepared = ready(&s, &l).await;
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM developments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(s.snapshot().await.unwrap().unwrap().persona, before.persona);
    let receipt = s.commit_decision(&l, &prepared, &[]).await.unwrap();
    assert_eq!(receipt.development_ids.len(), 1);
    assert!(receipt.message_id.is_some());
    let after = s.snapshot().await.unwrap().unwrap();
    assert_eq!(after.state_version, before.state_version + 1);
    assert_eq!(after.persona, before.persona);
    assert_eq!(after.seed, before.seed);
    assert_eq!(after.runtime_config, before.runtime_config);
    let restored = Store::new(pool.clone());
    let detail = restored
        .development(receipt.development_ids[0])
        .await
        .unwrap();
    assert_eq!(detail["development"]["status"], "proposed");
    assert_eq!(detail["development"]["target"], "persona.expression");
    let stable = fingerprint(&pool).await;
    assert_eq!(
        restored.commit_decision(&l, &prepared, &[]).await.unwrap(),
        receipt
    );
    assert_eq!(fingerprint(&pool).await, stable);
}
#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; late outbox fault must roll back all proposal writes"]
async fn late_failure_rolls_back_candidate_reply_and_outbox_then_original_intent_recovers(
    pool: PgPool,
) {
    let (s, l) = start(&pool).await;
    let prepared = ready(&s, &l).await;
    sqlx::raw_sql("CREATE FUNCTION root_fail_development_state() RETURNS trigger AS $$ BEGIN IF NEW.event_type='state.changed' THEN RAISE EXCEPTION 'synthetic late development fault'; END IF; RETURN NEW; END $$ LANGUAGE plpgsql; CREATE TRIGGER root_fail_development_state BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION root_fail_development_state();").execute(&pool).await.unwrap();
    let stable = fingerprint(&pool).await;
    assert!(s.commit_decision(&l, &prepared, &[]).await.is_err());
    assert_eq!(fingerprint(&pool).await, stable);
    sqlx::raw_sql("DROP TRIGGER root_fail_development_state ON outbox; DROP FUNCTION root_fail_development_state();").execute(&pool).await.unwrap();
    let (a, b) = tokio::join!(
        s.commit_decision(&l, &prepared, &[]),
        s.commit_decision(&l, &prepared, &[])
    );
    assert_eq!(a.unwrap(), b.unwrap());
    let (drafts, calls): (i64, i32) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM developments),model_calls FROM runs WHERE id=$1",
    )
    .bind(l.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((drafts, calls), (1, 2));
}
#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; revoked generated output stays private on receipt replay"]
async fn blocked_proposal_created_event_without_general_edges_stops_cached_receipt(pool: PgPool) {
    let (s, l) = start(&pool).await;
    let prepared = ready(&s, &l).await;
    let receipt = s.commit_decision(&l, &prepared, &[]).await.unwrap();
    let event: Uuid = sqlx::query_scalar("SELECT created_event_id FROM developments WHERE id=$1")
        .bind(receipt.development_ids[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(event)
        .execute(&pool)
        .await
        .unwrap();
    let stable = fingerprint(&pool).await;
    assert!(s.commit_decision(&l, &prepared, &[]).await.is_err());
    assert_eq!(fingerprint(&pool).await, stable);
}
#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; untrusted context cannot reserve and unsupported draft cannot commit"]
async fn forged_catalog_or_failed_review_never_creates_candidate(pool: PgPool) {
    let (s, l) = start(&pool).await;
    let mut c = s.model_context(l.run_id, vec![]).await.unwrap();
    c.development_targets[0].before_version += 1;
    let stable = fingerprint(&pool).await;
    assert!(matches!(
        s.reserve_model_call(
            &l,
            Uuid::new_v4(),
            &c,
            TokenAllowance {
                input_upper_bound: 100,
                output_cap: 50
            }
        )
        .await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(fingerprint(&pool).await, stable);
    let before = s.snapshot().await.unwrap().unwrap();
    let result = model_step(
        &s,
        &Developing {
            verdict: Support::Unsupported,
        },
        &l,
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
    assert_eq!(
        sqlx::query_scalar::<_, i64>("SELECT count(*) FROM developments")
            .fetch_one(&pool)
            .await
            .unwrap(),
        0
    );
    assert_eq!(s.snapshot().await.unwrap().unwrap().persona, before.persona);
    let (calls, charge): (i64, i64) = sqlx::query_as(
        "SELECT count(*),sum(charged_tokens)::bigint FROM usage_reservations WHERE run_id=$1",
    )
    .bind(l.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((calls, charge), (2, 4));
}
