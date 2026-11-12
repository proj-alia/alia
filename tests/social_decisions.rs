//! Synthetic opinions exercise real review/SQL/accounting gates; no live semantic verdict.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        cognition::{DecisionProposal, ModelContext},
        config::{PersonaSeed, RuntimeRules},
        grounding::{self, ContentCheck, DecisionAssessment, ReviewTarget, Support},
        relationships::RelationshipState,
        social::{CurrentRelation, SocialContext},
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
fn parse(p: &Value) -> DecisionProposal {
    DecisionProposal::parse(&serde_json::to_vec(p).unwrap()).unwrap()
}
fn sample() -> ModelContext {
    serde_json::from_str(include_str!("../evals/model_context.example.json")).unwrap()
}
fn proposal(source: Uuid, who: Uuid, version: i64, previous: Option<Uuid>) -> Value {
    json!({"schema_version":1,"reply":"我会记录这次交流带来的评价，并按我们明确的沟通边界继续讨论。","reply_evidence_ids":[source],"memory_changes":[],"appraisal_changes":[{"event_ids":[source],"goal_id":null,"subject_id":who,"valence":0.1,"arousal":0.4,"goal_relevance":0.5,"expectation_gap":0.2,"controllability":0.7,"uncertainty":0.5,"brief_basis":"当事人明确陈述使我保持谨慎好奇"}],"attitude_changes":[{"topic":"discussion.evidence","subject_id":who,"stance":"讨论分歧时先核对依据，接受有效反证","event_ids":[source],"supersedes":previous,"brief_basis":"对共同讨论方式的长期倾向，不断言对方感情"}],"relationship_changes":[{"subject_id":who,"expected_relationship_version":version,"changes":{"expectations":["讨论分歧时说明具体依据"]},"event_ids":[source],"brief_basis":"保留明确沟通约定"}],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"短期评价、长期态度和关系记录分别保存"})
}
fn typed() -> (ModelContext, Uuid) {
    let mut c = sample();
    let who = Uuid::new_v4();
    let source = c.events[0].id;
    c.input_manifest.extend([who, source]);
    c.input_manifest.sort();
    c.input_manifest.dedup();
    c.social_context = Some(SocialContext {
        relationships: vec![CurrentRelation {
            subject_id: who,
            version: 1,
            state: RelationshipState {
                stage: "初识".into(),
                expectations: vec![],
                boundaries: vec!["尊重独立空间".into()],
                commitments: vec![],
                unresolved: vec![],
            },
            latest_revision_id: None,
            latest_revision_version: None,
            provenance_ids: vec![who, source],
            basis_event_ids: vec![source],
        }],
        ..Default::default()
    });
    (c, who)
}
fn assessment(c: &ModelContext) -> DecisionAssessment {
    let p = &c.decision_review.as_ref().unwrap().proposal;
    DecisionAssessment {
        schema_version: 1,
        checks: grounding::targets(p)
            .into_iter()
            .map(|target| {
                let evidence_ids = match target {
                    ReviewTarget::Reply => p.reply_evidence_ids.clone(),
                    ReviewTarget::Appraisal { index } => {
                        p.appraisal_changes[index].event_ids.clone()
                    }
                    ReviewTarget::Attitude { index } => p.attitude_changes[index].event_ids.clone(),
                    ReviewTarget::Relationship { index } => {
                        p.relationship_changes[index].event_ids.clone()
                    }
                    _ => unreachable!(),
                };
                ContentCheck {
                    target,
                    verdict: Support::Supported,
                    reason: "仅合成结构意见".into(),
                    evidence_ids,
                }
            })
            .collect(),
    }
}
#[test]
fn every_social_target_and_all_its_declared_events_require_independent_review() {
    let (mut c, who) = typed();
    let mut e = c.events[0].clone();
    e.id = Uuid::new_v4();
    e.sequence += 1;
    c.input_manifest.push(e.id);
    c.events.push(e);
    let mut p = proposal(c.events[0].id, who, 1, None);
    for field in [
        "appraisal_changes",
        "attitude_changes",
        "relationship_changes",
    ] {
        p[field][0]["event_ids"] = json!([c.events[0].id, c.events[1].id]);
    }
    let p = parse(&p);
    p.validate_context(&c).unwrap();
    let review = grounding::review_context(&c, Uuid::new_v4(), &p).unwrap();
    let packet = grounding::evidence_packet(&review).unwrap();
    assert_eq!(packet["packet_version"], "target-citations/11");
    assert_eq!(packet["evidence_by_target"].as_array().unwrap().len(), 4);
    let mut opinion = assessment(&review);
    opinion.validate(&review).unwrap();
    opinion.checks.pop();
    assert!(opinion.validate(&review).is_err());
    let mut opinion = assessment(&review);
    opinion.checks[1].evidence_ids.pop();
    assert!(opinion.validate(&review).is_err());
    for field in [
        "appraisal_changes",
        "attitude_changes",
        "relationship_changes",
    ] {
        let mut bad = serde_json::to_value(&p).unwrap();
        bad[field][0]["event_ids"] = json!([who]);
        assert!(parse(&bad).validate_context(&c).is_err());
    }
}
#[test]
fn old_state_is_full_comparison_not_an_undeclared_source_and_duplicates_versions_are_rejected() {
    let (mut c, who) = typed();
    let old = c.events[0].id;
    let mut e = c.events[0].clone();
    e.id = Uuid::new_v4();
    e.sequence += 1;
    let new = e.id;
    c.input_manifest.push(new);
    c.events.push(e);
    let p = proposal(new, who, 1, None);
    let parsed = parse(&p);
    parsed.validate_context(&c).unwrap();
    let review = grounding::review_context(&c, Uuid::new_v4(), &parsed).unwrap();
    let packet = grounding::evidence_packet(&review).unwrap();
    let relationship = packet["evidence_by_target"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["target"]["kind"] == "relationship")
        .unwrap();
    assert_eq!(relationship["allowed_evidence_ids"], json!([new]));
    assert!(
        relationship["social_under_review"]["previous_comparison_not_additional_citations"]
            .to_string()
            .contains(&old.to_string())
    );
    let mut opinion = assessment(&review);
    opinion.checks.last_mut().unwrap().evidence_ids.push(old);
    assert!(opinion.validate(&review).is_err());
    for field in ["attitude_changes", "relationship_changes"] {
        let mut bad = p.clone();
        let duplicate = bad[field][0].clone();
        bad[field].as_array_mut().unwrap().push(duplicate);
        assert!(
            DecisionProposal::parse(&serde_json::to_vec(&bad).unwrap()).is_err()
                || parse(&bad).validate_context(&c).is_err()
        );
    }
    let mut bad = p.clone();
    bad["relationship_changes"][0]["expected_relationship_version"] = json!(2);
    assert!(parse(&bad).validate_context(&c).is_err());
    let mut bad = p;
    bad["appraisal_changes"][0]["grant_permission"] = json!(true);
    assert!(DecisionProposal::parse(&serde_json::to_vec(&bad).unwrap()).is_err());
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
#[ignore = "requires isolated PostgreSQL"]
async fn social_memory_and_history_commit_once_preserve_persona_and_actual_source(pool: PgPool) {
    let s = setup(&pool).await;
    let (l, e, who, v) = start(&s, &pool, "希望讨论分歧时说明具体依据").await;
    let persona = s.snapshot().await.unwrap().unwrap().persona;
    let original: Value = sqlx::query_scalar("SELECT to_jsonb(e) FROM events e WHERE id=$1")
        .bind(e)
        .fetch_one(&pool)
        .await
        .unwrap();
    let mut p = proposal(e, who, v, None);
    p["memory_changes"] = json!([{"content":"对方提出讨论分歧时说明依据","kind":"semantic","scope":"general","source_event_ids":[e],"supersedes":null}]);
    p["autobiography_changes"] = json!([{"topic":"discussion.shared","title":"一次有依据的沟通约定","interpretation":"我将这次明确陈述理解为讨论方式的约定，不推断对方情感","event_ids":[e],"belief_ids":[],"supersedes":null}]);
    let ready = prepare(&s, &l, p).await;
    assert!(ready.grounding_review_id().is_some());
    let r = s.commit_decision(&l, &ready, &[]).await.unwrap();
    assert_eq!(r.appraisal_ids.len(), 1);
    assert_eq!(r.attitude_ids.len(), 1);
    assert_eq!(r.relationship_revision_ids.len(), 1);
    assert_eq!(r.memory_ids.len(), 1);
    assert_eq!(r.autobiography_ids.len(), 1);
    assert!(r.message_id.is_some());
    let before = snapshot(&pool).await;
    assert_eq!(s.commit_decision(&l, &ready, &[]).await.unwrap(), r);
    assert_eq!(snapshot(&pool).await, before);
    assert_eq!(s.snapshot().await.unwrap().unwrap().persona, persona);
    assert_eq!(
        sqlx::query_scalar::<_, Value>("SELECT to_jsonb(e) FROM events e WHERE id=$1")
            .bind(e)
            .fetch_one(&pool)
            .await
            .unwrap(),
        original
    );
    let (after,current):(i64,i64)=sqlx::query_as("SELECT r.after_version,s.version FROM relationship_revisions r JOIN relationships s ON s.subject_id=r.subject_id AND s.individual_id=r.individual_id WHERE r.id=$1").bind(r.relationship_revision_ids[0]).fetch_one(&pool).await.unwrap();
    assert_eq!(after, v + 1);
    assert_eq!(
        current,
        v + 2,
        "outgoing reply increments the same relationship after the explicit revision"
    );
    let rows:Vec<(String,Uuid)>=sqlx::query_as("SELECT source_kind,correlation_id FROM events WHERE kind IN ('affect.updated','relationship.revised')").fetch_all(&pool).await.unwrap();
    assert_eq!(rows.len(), 3);
    assert!(
        rows.iter()
            .all(|(kind, run)| kind == "inference" && *run == l.run_id)
    );
    sqlx::query("DELETE FROM derivation_edges WHERE source_id=$1")
        .bind(e)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(e)
        .execute(&pool)
        .await
        .unwrap();
    let before = snapshot(&pool).await;
    assert!(
        matches!(
            s.commit_decision(&l, &ready, &[]).await,
            Err(StoreError::StaleState)
        ),
        "cached receipt must still fence every original context source when general edges are absent"
    );
    assert_eq!(snapshot(&pool).await, before);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn restart_revises_observed_predecessor_and_keeps_old_attitude_and_relationship_history(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (l, e, who, v) = start(&s, &pool, "讨论时希望给出依据").await;
    let first = prepare(&s, &l, proposal(e, who, v, None)).await;
    let r = s.commit_decision(&l, &first, &[]).await.unwrap();
    let s = Store::new(pool.clone());
    let (l, e, who, v) = start(&s, &pool, "不确定时先确认含义，再提供依据").await;
    let c = s.model_context(l.run_id, vec![]).await.unwrap();
    assert!(
        c.social_context
            .as_ref()
            .unwrap()
            .attitudes
            .iter()
            .any(|a| a.id == r.attitude_ids[0])
    );
    let mut p = proposal(e, who, v, Some(r.attitude_ids[0]));
    p["attitude_changes"][0]["stance"] = json!("不确定时先确认含义，再讨论证据");
    p["relationship_changes"][0]["changes"] = json!({"expectations":["先确认含义，再提供依据"]});
    let second = prepare(&s, &l, p).await;
    let next = s.commit_decision(&l, &second, &[]).await.unwrap();
    let state = s.affect_snapshot(chrono::Utc::now(), 20).await.unwrap();
    assert_eq!(state["attitudes"].as_array().unwrap().len(), 1);
    assert_eq!(state["attitudes"][0]["id"], json!(next.attitude_ids[0]));
    assert_eq!(
        state["attitudes"][0]["supersedes"],
        json!(r.attitude_ids[0])
    );
    let old: String = sqlx::query_scalar("SELECT status FROM affective_attitudes WHERE id=$1")
        .bind(r.attitude_ids[0])
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(old, "superseded");
    let history = s.relationship_history(who, 20).await.unwrap();
    assert_eq!(history["items"].as_array().unwrap().len(), 2);
    assert_eq!(
        history["items"][0]["previous_revision_id"],
        json!(r.relationship_revision_ids[0])
    );
    assert_eq!(history["items"][0]["before_version"], json!(v));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn social_only_rejection_has_no_state_writes_but_both_actual_synthetic_calls_remain_charged(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (l, e, who, v) = start(&s, &pool, "一次不顺利的讨论").await;
    let mut p = proposal(e, who, v, None);
    p["reply"] = json!("");
    p["reply_evidence_ids"] = json!([]);
    p["appraisal_changes"] = json!([]);
    p["relationship_changes"] = json!([]);
    let result = model_step(
        &s,
        &SyntheticReviewer {
            proposal: p,
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
            alia::domain::cognition::ProposalError::Grounding
        )))
    ));
    let (calls,charged):(i64,i64)=sqlx::query_as("SELECT count(*),COALESCE(sum(charged_tokens),0)::bigint FROM usage_reservations WHERE run_id=$1 AND status='settled'").bind(l.run_id).fetch_one(&pool).await.unwrap();
    assert_eq!((calls, charged), (2, 4));
    let count:i64=sqlx::query_scalar("SELECT (SELECT count(*) FROM affective_appraisals)+(SELECT count(*) FROM affective_attitudes)+(SELECT count(*) FROM relationship_revisions)+(SELECT count(*) FROM decision_commits)+(SELECT count(*) FROM outgoing_messages)").fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn late_failure_rolls_back_all_social_and_reply_rows_and_retry_uses_original_prepared_review(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (l, e, who, v) = start(&s, &pool, "请保留明确边界和有依据的理解").await;
    let ready = prepare(&s, &l, proposal(e, who, v, None)).await;
    let before = snapshot(&pool).await;
    sqlx::raw_sql("CREATE FUNCTION fail_social_publish() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.event_type='operation.updated' THEN RAISE EXCEPTION 'qa late failure'; END IF; RETURN NEW; END $$; CREATE TRIGGER qa_social_publish BEFORE INSERT ON outbox FOR EACH ROW EXECUTE FUNCTION fail_social_publish();").execute(&pool).await.unwrap();
    assert!(s.commit_decision(&l, &ready, &[]).await.is_err());
    assert_eq!(snapshot(&pool).await, before);
    sqlx::raw_sql("DROP TRIGGER qa_social_publish ON outbox;DROP FUNCTION fail_social_publish();")
        .execute(&pool)
        .await
        .unwrap();
    let r = s.commit_decision(&l, &ready, &[]).await.unwrap();
    assert_eq!(r.attitude_ids.len(), 1);
    assert_eq!(r.appraisal_ids.len(), 1);
    assert_eq!(r.relationship_revision_ids.len(), 1);
    assert!(r.message_id.is_some());
    let calls: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations WHERE run_id=$1")
        .bind(l.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(calls, 2);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn revocation_after_review_never_commits_social_state_or_reply_even_without_general_edges(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (l, e, who, v) = start(&s, &pool, "后来撤销的明确陈述").await;
    let ready = prepare(&s, &l, proposal(e, who, v, None)).await;
    sqlx::query("DELETE FROM derivation_edges WHERE source_id=$1")
        .bind(e)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(e)
        .execute(&pool)
        .await
        .unwrap();
    let before = snapshot(&pool).await;
    assert!(matches!(
        s.commit_decision(&l, &ready, &[]).await,
        Err(StoreError::LeaseLost) | Err(StoreError::StaleState)
    ));
    assert_eq!(snapshot(&pool).await, before);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn expiry_prevents_a_new_commit_but_does_not_revoke_an_already_committed_receipt(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let e = s
        .submit_message("earlier", "一项短期评价的真实合成依据", None)
        .await
        .unwrap()
        .event_id;
    let who: Uuid =
        sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
            .fetch_one(&pool)
            .await
            .unwrap();
    let app =
        serde_json::from_value(proposal(e, who, 1, None)["appraisal_changes"][0].clone()).unwrap();
    s.record_affect(
        "seed-short",
        s.snapshot().await.unwrap().unwrap().expected(),
        &[who, e],
        Some(&app),
        None,
    )
    .await
    .unwrap();
    sqlx::query("UPDATE affective_appraisals SET expires_at=clock_timestamp()+interval '3 seconds' WHERE status='current'").execute(&pool).await.unwrap();
    let (l, e, who, v) = start(&s, &pool, "明确说明现在的沟通约定").await;
    let ready = prepare(&s, &l, proposal(e, who, v, None)).await;
    let expires = ready
        .context()
        .social_context
        .as_ref()
        .unwrap()
        .appraisal
        .as_ref()
        .unwrap()
        .expires_at;
    let receipt = s.commit_decision(&l, &ready, &[]).await.unwrap();
    let remaining = (expires - chrono::Utc::now()).num_milliseconds().max(0) as u64;
    tokio::time::sleep(std::time::Duration::from_millis(remaining + 50)).await;
    let before = snapshot(&pool).await;
    assert_eq!(s.commit_decision(&l, &ready, &[]).await.unwrap(), receipt);
    assert_eq!(snapshot(&pool).await, before);

    sqlx::query("UPDATE affective_appraisals SET expires_at=clock_timestamp()+interval '3 seconds' WHERE status='current'").execute(&pool).await.unwrap();
    let (l, e, who, v) = start(&s, &pool, "这次希望先倾听再讨论依据").await;
    let mut p = proposal(e, who, v, Some(receipt.attitude_ids[0]));
    p["relationship_changes"][0]["changes"] = json!({"expectations":["先倾听，再说明依据"]});
    let ready = prepare(&s, &l, p).await;
    let expires = ready
        .context()
        .social_context
        .as_ref()
        .unwrap()
        .appraisal
        .as_ref()
        .unwrap()
        .expires_at;
    let remaining = (expires - chrono::Utc::now()).num_milliseconds().max(0) as u64;
    tokio::time::sleep(std::time::Duration::from_millis(remaining + 50)).await;
    let before = snapshot(&pool).await;
    assert!(matches!(
        s.commit_decision(&l, &ready, &[]).await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(snapshot(&pool).await, before);
    let charged: i64 = sqlx::query_scalar(
        "SELECT sum(charged_tokens)::bigint FROM usage_reservations WHERE run_id=$1",
    )
    .bind(l.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(charged, 4);
}
