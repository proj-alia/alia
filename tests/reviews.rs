//! Deterministic review fixtures exercise storage/runtime; not behavioral evidence.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        cognition::{ModelContext, RunMode},
        config::{PersonaSeed, RuntimeRules},
        review::*,
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        execution::{StepError, model_step},
        model::*,
        review::{PreparedReview, review_step},
        scheduling::{RunLease, TokenAllowance},
        worker::StopReason,
    },
};
use async_trait::async_trait;
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
const BODY: &str =
    "直接覆盖可能留下半份文件。临时文件替换能避免读到半份内容，但持久化仍需同步文件与目录。";
const CRITERION: &str = "比较两种写入方法，说明原子性与持久性的差别并保存有来源的建议。";
fn proposal() -> Value {
    json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"检查实际产物"})
}
async fn commit(
    s: &Store,
    l: &mut RunLease,
    p: Value,
) -> alia::runtime::scheduling::DecisionReceipt {
    let reg = s.capabilities().await.unwrap();
    let ready = model_step(
        s,
        &FixtureProvider {
            response: p.to_string(),
        },
        l,
        reg.clone(),
        CallLimits::default(),
    )
    .await
    .unwrap();
    let r = s.commit_decision(l, &ready, &reg).await.unwrap();
    l.state_version = r.state_version;
    r
}
async fn setup(pool: &PgPool, cited: bool) -> (Store, RunLease, Uuid, Uuid) {
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
    let doc = s
        .import_document("doc", "测试资料", BODY, "owner")
        .await
        .unwrap()
        .event_id;
    s.submit_message("msg", "研究两种写入方式", None)
        .await
        .unwrap();
    let mut l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"create","origin_type":"event","origin_ids":[doc],"description":"写入方法研究","success_criterion":CRITERION,"next_step":"读取资料并保存比较","stop_condition":"资料不足时等待","priority":50}]);
    let g = commit(&s, &mut l, p).await.goal_ids[0];
    s.tick_goals().await.unwrap();
    let mut l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(l.mode, RunMode::Background);
    let mut p = proposal();
    p["goal_changes"] =
        json!([{"action":"activate","goal_id":g,"reason":"开始研究","evidence_ids":[doc]}]);
    p["tool_intents"] = json!([{"capability_id":"documents.read","input":{"document_id":doc,"offset":0,"max_chars":1024}}]);
    for id in commit(&s, &mut l, p).await.action_ids {
        let r = s.execute_local_action(&l, id).await.unwrap();
        assert_eq!(r.status, "succeeded");
        l.state_version = r.state_version;
    }
    let mut p = proposal();
    let mut input = json!({"title":"研究成果","text":format!("引用：{BODY}\n结论：替换解决读者可见的完整性；目录同步影响断电后的持久性。采用临时写入、同步及替换方案。")});
    if cited {
        input["citations"] =
            json!([{"document_id":doc,"start_char":0,"end_char":BODY.chars().count()}]);
    }
    p["tool_intents"] = json!([{"capability_id":"notes.write","input":input}]);
    for id in commit(&s, &mut l, p).await.action_ids {
        let r = s.execute_local_action(&l, id).await.unwrap();
        assert_eq!(r.status, "succeeded");
        l.state_version = r.state_version;
    }
    let revision: i64 = sqlx::query_scalar("SELECT review_revision FROM goals WHERE id=$1")
        .bind(g)
        .fetch_one(pool)
        .await
        .unwrap();
    assert_eq!(revision, 2, "model activation advances review revision");
    (s, l, g, doc)
}
struct Reviewer(ReviewVerdict);
#[async_trait]
impl ModelProvider for Reviewer {
    fn mode(&self) -> ProviderMode {
        ProviderMode::Fixture
    }
    fn supports_goal_review(&self) -> bool {
        true
    }
    fn review_allowance(
        &self,
        _: &GoalReviewInput,
        l: CallLimits,
    ) -> Result<TokenAllowance, ModelFailure> {
        Ok(TokenAllowance {
            input_upper_bound: 100,
            output_cap: l.max_output_tokens,
        })
    }
    async fn decide(
        &self,
        _: &ModelContext,
        _: CallLimits,
        _: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        Err(ModelFailure::before_request(ModelError::Unavailable))
    }
    async fn review(
        &self,
        input: &GoalReviewInput,
        _: CallLimits,
        _: ModelCallMetadata,
        _: &ModelContext,
    ) -> Result<GoalReviewResult, ModelFailure> {
        Ok(GoalReviewResult {
            assessment: GoalAssessment {
                goal_id: input.goal_id,
                goal_revision: input.goal_revision,
                criterion: input.success_criterion.clone(),
                verdict: self.0,
                reason: "显式测试审查".into(),
                evidence_ids: input.evidence_ids.clone(),
                unmet_conditions: if self.0 == ReviewVerdict::NotMet {
                    vec!["缺少比较".into()]
                } else {
                    vec![]
                },
                uncertainties: if self.0 == ReviewVerdict::Uncertain {
                    vec!["无法核实".into()]
                } else {
                    vec![]
                },
            },
            usage: Some(TokenUsage {
                input: 50,
                output: 20,
                total: 70,
            }),
            provider_model: "fixture-review".into(),
            provider_mode: ProviderMode::Fixture,
        })
    }
}
async fn prepare(s: &Store, l: &RunLease, v: ReviewVerdict) -> PreparedReview {
    let ctx = s
        .prepare_goal_review(l, s.capabilities().await.unwrap())
        .await
        .unwrap()
        .unwrap();
    review_step(s, &Reviewer(v), l, ctx, CallLimits::default())
        .await
        .unwrap()
}
async fn next_cycle(s: &Store, pool: &PgPool, l: &RunLease, g: Uuid) -> RunLease {
    s.stop_cycle(l, StopReason::CallLimit).await.unwrap();
    s.tick_goals().await.unwrap();
    // Virtual clock only for deterministic scheduling tests, not live acceptance.
    sqlx::query("UPDATE goals SET due_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(g)
        .execute(pool)
        .await
        .unwrap();
    s.tick_goals().await.unwrap();
    s.claim_run(Uuid::new_v4()).await.unwrap().unwrap()
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn accepted_review_is_charged_replayable_and_survives_cooldown_not_auto_completion(
    pool: PgPool,
) {
    let (s, mut l, g, _) = setup(&pool, true).await;
    assert!(
        s.model_context(l.run_id, vec![])
            .await
            .unwrap()
            .verified_goal_completions
            .is_empty()
    );
    let prepared = prepare(&s, &l, ReviewVerdict::Met).await;
    let receipt = s.commit_goal_review(&l, &prepared).await.unwrap();
    assert_eq!(s.commit_goal_review(&l, &prepared).await.unwrap(), receipt);
    l.state_version = receipt.state_version;
    assert!(s.prepare_goal_review(&l, vec![]).await.unwrap().is_none());
    let status: String = sqlx::query_scalar("SELECT status FROM goals WHERE id=$1")
        .bind(g)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "active");
    let calls: i32 = sqlx::query_scalar("SELECT model_calls FROM runs WHERE id=$1")
        .bind(l.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(calls, 3);
    let ctx = s.model_context(l.run_id, vec![]).await.unwrap();
    assert!(ctx.verified_goal_completions.contains(&g));
    let fail = model_step(
        &s,
        &FixtureProvider {
            response: proposal().to_string(),
        },
        &l,
        vec![],
        CallLimits::default(),
    )
    .await;
    assert!(matches!(
        fail,
        Err(StepError::Storage(StoreError::CallLimit))
    ));
    let mut l = next_cycle(&s, &pool, &l, g).await;
    assert!(s.prepare_goal_review(&l, vec![]).await.unwrap().is_none());
    let ctx = s.model_context(l.run_id, vec![]).await.unwrap();
    assert!(ctx.verified_goal_completions.contains(&g));
    assert!(ctx.events.iter().any(|e| e.id == receipt.event_id));
    assert_eq!(ctx.current_goal_id, Some(g));
    assert!(
        ctx.completion_evidence
            .iter()
            .any(|r| r.goal_id == g && r.result_event_id == receipt.event_id)
    );
    let mut wrong = proposal();
    wrong["goal_changes"] = json!([{"action":"complete","goal_id":g,"reason":"测试错误结果引用","evidence_ids":[ctx.current_event_id]}]);
    let ready = model_step(
        &s,
        &FixtureProvider {
            response: wrong.to_string(),
        },
        &l,
        vec![],
        CallLimits::default(),
    )
    .await
    .unwrap();
    assert!(matches!(
        s.commit_decision(&l, &ready, &[]).await,
        Err(StoreError::Proposal(
            alia::domain::cognition::ProposalError::CompletionEvidenceMissing
        ))
    ));
    assert_eq!(
        sqlx::query_scalar::<_, String>("SELECT status FROM goals WHERE id=$1")
            .bind(g)
            .fetch_one(&pool)
            .await
            .unwrap(),
        "active"
    );
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"complete","goal_id":g,"reason":"经独立审查接受","evidence_ids":[receipt.event_id]}]);
    commit(&s, &mut l, p).await;
    let (status,kind):(String,String)=sqlx::query_as("SELECT g.status,e.source_kind FROM goals g CROSS JOIN events e WHERE g.id=$1 AND e.kind='goal.completion_verified'").bind(g).fetch_one(&pool).await.unwrap();
    assert_eq!((status.as_str(), kind.as_str()), ("completed", "inference"));
    let charged: i64 =
        sqlx::query_scalar("SELECT charged_tokens FROM usage_reservations WHERE id=$1")
            .bind(prepared.reservation_id())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(charged, 70);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn rejected_review_is_feedback_and_is_not_repeated_on_unchanged_artifacts(pool: PgPool) {
    let (s, mut l, g, _) = setup(&pool, true).await;
    let p = prepare(&s, &l, ReviewVerdict::NotMet).await;
    let r = s.commit_goal_review(&l, &p).await.unwrap();
    l.state_version = r.state_version;
    let l = next_cycle(&s, &pool, &l, g).await;
    assert!(s.prepare_goal_review(&l, vec![]).await.unwrap().is_none());
    let c = s.model_context(l.run_id, vec![]).await.unwrap();
    assert!(c.verified_goal_completions.is_empty());
    assert!(c.events.iter().any(
        |e| e.id == r.event_id && e.payload["assessment"]["unmet_conditions"][0] == "缺少比较"
    ));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn uncited_notes_and_forged_review_inputs_cannot_obtain_a_review(pool: PgPool) {
    let (s, l, _, _) = setup(&pool, false).await;
    assert!(s.prepare_goal_review(&l, vec![]).await.unwrap().is_none());
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn changed_source_and_tampered_snapshot_are_fenced_before_dispatch_or_commit(pool: PgPool) {
    let (s, l, _, doc) = setup(&pool, true).await;
    let mut ctx = s.prepare_goal_review(&l, vec![]).await.unwrap().unwrap();
    ctx.goal_review.as_mut().unwrap().artifacts[0].body = "伪造产物".into();
    assert!(matches!(
        review_step(
            &s,
            &Reviewer(ReviewVerdict::Met),
            &l,
            ctx,
            CallLimits::default()
        )
        .await,
        Err(StepError::Storage(StoreError::StaleState))
    ));
    let p = prepare(&s, &l, ReviewVerdict::Met).await;
    sqlx::query("UPDATE documents SET body='changed' WHERE event_id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.commit_goal_review(&l, &p).await,
        Err(StoreError::StaleState)
    ));
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM goal_reviews")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn review_insert_failure_rolls_back_and_retry_uses_same_charge(pool: PgPool) {
    let (s, l, _, _) = setup(&pool, true).await;
    let p = prepare(&s, &l, ReviewVerdict::Uncertain).await;
    sqlx::query("CREATE FUNCTION reject_review() RETURNS trigger LANGUAGE plpgsql AS $$BEGIN RAISE EXCEPTION 'test rollback'; END$$").execute(&pool).await.unwrap();
    sqlx::query("CREATE TRIGGER reject_review BEFORE INSERT ON goal_reviews FOR EACH ROW EXECUTE FUNCTION reject_review()").execute(&pool).await.unwrap();
    assert!(matches!(
        s.commit_goal_review(&l, &p).await,
        Err(StoreError::Database(_))
    ));
    let n: i64 = sqlx::query_scalar("SELECT count(*) FROM events WHERE kind='goal.reviewed'")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(n, 0);
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().state_version,
        l.state_version
    );
    sqlx::query("DROP TRIGGER reject_review ON goal_reviews")
        .execute(&pool)
        .await
        .unwrap();
    let r = s.commit_goal_review(&l, &p).await.unwrap();
    assert_eq!(r.verdict, ReviewVerdict::Uncertain);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn new_foreground_input_fences_review_and_preserves_actual_cost(pool: PgPool) {
    let (s, l, _, _) = setup(&pool, true).await;
    let p = prepare(&s, &l, ReviewVerdict::Met).await;
    s.submit_message("interrupt", "暂时先处理新消息", None)
        .await
        .unwrap();
    assert!(matches!(
        s.commit_goal_review(&l, &p).await,
        Err(StoreError::LeaseLost | StoreError::StaleState)
    ));
    let charged: i64 =
        sqlx::query_scalar("SELECT charged_tokens FROM usage_reservations WHERE id=$1")
            .bind(p.reservation_id())
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(charged, 70);
}

struct CompletingReviewer;
#[async_trait]
impl ModelProvider for CompletingReviewer {
    fn mode(&self) -> ProviderMode {
        ProviderMode::Fixture
    }
    fn supports_goal_review(&self) -> bool {
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
    fn review_allowance(
        &self,
        i: &GoalReviewInput,
        l: CallLimits,
    ) -> Result<TokenAllowance, ModelFailure> {
        Reviewer(ReviewVerdict::Met).review_allowance(i, l)
    }
    async fn review(
        &self,
        i: &GoalReviewInput,
        l: CallLimits,
        m: ModelCallMetadata,
        c: &ModelContext,
    ) -> Result<GoalReviewResult, ModelFailure> {
        Reviewer(ReviewVerdict::Met).review(i, l, m, c).await
    }
    async fn decide(
        &self,
        c: &ModelContext,
        l: CallLimits,
        m: ModelCallMetadata,
    ) -> Result<ModelResult, ModelFailure> {
        let g = *c
            .verified_goal_completions
            .first()
            .expect("worker must review before acting");
        let e = c
            .events
            .iter()
            .find(|e| e.kind == "goal.reviewed")
            .expect("review evidence in context");
        let mut p = proposal();
        p["goal_changes"] = json!([{"action":"complete","goal_id":g,"reason":"已检查完整产物","evidence_ids":[e.id]}]);
        FixtureProvider {
            response: p.to_string(),
        }
        .decide(c, l, m)
        .await
    }
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn worker_runs_review_and_decision_through_same_budget_and_identity(pool: PgPool) {
    use alia::{
        adapters::web::HttpWebReader,
        runtime::worker::{Worker, WorkerOutcome},
    };
    use std::sync::Arc;
    let (s, l, g, _) = setup(&pool, true).await;
    s.stop_cycle(&l, StopReason::CallLimit).await.unwrap();
    s.tick_goals().await.unwrap();
    sqlx::query("UPDATE goals SET due_at=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    let worker = Worker::new(
        s,
        Arc::new(CompletingReviewer),
        Arc::new(HttpWebReader),
        Arc::new(tokio::sync::Notify::new()),
    );
    assert!(matches!(
        worker.run_once().await.unwrap(),
        WorkerOutcome::Committed(_)
    ));
    let status: String = sqlx::query_scalar("SELECT status FROM goals WHERE id=$1")
        .bind(g)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "completed");
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM goal_reviews")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 1);
    let calls:i32=sqlx::query_scalar("SELECT r.model_calls FROM runs r JOIN goal_reviews v ON r.id=(SELECT run_id FROM usage_reservations WHERE id=v.reservation_id)").fetch_one(&pool).await.unwrap();
    assert_eq!(calls, 2);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn blocked_ancestor_prevents_review_replay_and_completion(pool: PgPool) {
    let (s, l, _, doc) = setup(&pool, true).await;
    let p = prepare(&s, &l, ReviewVerdict::Met).await;
    s.commit_goal_review(&l, &p).await.unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(doc)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.commit_goal_review(&l, &p).await,
        Err(StoreError::StaleState)
    ));
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn changed_goal_revision_cannot_reuse_review_even_if_artifact_is_same(pool: PgPool) {
    let (s, mut l, g, _) = setup(&pool, true).await;
    let p = prepare(&s, &l, ReviewVerdict::Met).await;
    let r = s.commit_goal_review(&l, &p).await.unwrap();
    l.state_version = r.state_version;
    let l = next_cycle(&s, &pool, &l, g).await;
    sqlx::query("UPDATE goals SET review_revision=review_revision+1 WHERE id=$1")
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    assert!(
        s.model_context(l.run_id, vec![])
            .await
            .unwrap()
            .verified_goal_completions
            .is_empty()
    );
    assert!(s.prepare_goal_review(&l, vec![]).await.unwrap().is_some());
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn reviewer_receives_unquoted_read_context_including_possible_counterevidence(pool: PgPool) {
    let (s, l, _, doc) = setup(&pool, true).await;
    sqlx::query("UPDATE notes SET citations=$1")
        .bind(json!([{"document_id":doc,"start_char":0,"end_char":10}]))
        .execute(&pool)
        .await
        .unwrap();
    let c = s.prepare_goal_review(&l, vec![]).await.unwrap().unwrap();
    let input = c.goal_review.unwrap();
    assert_eq!(input.sources[0].text.chars().count(), 10);
    assert_eq!(input.observations[0].text, BODY);
    assert!(input.evidence_ids.contains(&input.observations[0].event_id));
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn uncited_draft_is_preserved_without_permanently_blocking_a_cited_revision(pool: PgPool) {
    let (s, l, g, doc) = setup(&pool, false).await;
    let mut l = next_cycle(&s, &pool, &l, g).await;
    let mut p = proposal();
    p["tool_intents"] = json!([{"capability_id":"notes.write","input":{"title":"修正稿","text":format!("前稿尚未验证。依据：{BODY}\n更正：原子替换不保证断电持久性，需要同步。"),"citations":[{"document_id":doc,"start_char":0,"end_char":BODY.chars().count()}]}}]);
    let r = commit(&s, &mut l, p).await;
    let result = s.execute_local_action(&l, r.action_ids[0]).await.unwrap();
    assert_eq!(result.status, "succeeded");
    l.state_version = result.state_version;
    let input = s
        .prepare_goal_review(&l, vec![])
        .await
        .unwrap()
        .unwrap()
        .goal_review
        .unwrap();
    assert_eq!(input.artifacts.len(), 2);
    assert!(input.artifacts[0].citations.is_empty());
    assert_eq!(input.artifacts[1].citations.len(), 1);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn review_includes_formation_artifacts_and_counterevidence_but_not_unrelated_runs(
    pool: PgPool,
) {
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
    let doc = s
        .import_document("doc", "资料", BODY, "owner")
        .await
        .unwrap()
        .event_id;
    s.submit_message("unrelated", "无关事务", None)
        .await
        .unwrap();
    let mut unrelated = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let mut p = proposal();
    p["tool_intents"] = json!([{"capability_id":"notes.write","input":{"title":"无关","text":"UNRELATED_ARTIFACT"}}]);
    for id in commit(&s, &mut unrelated, p).await.action_ids {
        unrelated.state_version = s
            .execute_local_action(&unrelated, id)
            .await
            .unwrap()
            .state_version;
    }
    commit(&s, &mut unrelated, proposal()).await;
    let birth = s.tick_discovery().await.unwrap().unwrap();
    let mut l = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(l.run_id, birth);
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"create","origin_type":"event","origin_ids":[doc],"description":"自行研究","success_criterion":CRITERION,"next_step":"读取并比较","stop_condition":"资料不足","priority":50}]);
    p["tool_intents"] = json!([{"capability_id":"documents.read","input":{"document_id":doc}}]);
    let receipt = commit(&s, &mut l, p).await;
    let g = receipt.goal_ids[0];
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT formed_in_run FROM goals WHERE id=$1")
            .bind(g)
            .fetch_one(&pool)
            .await
            .unwrap(),
        birth
    );
    sqlx::query("UPDATE goals SET formed_in_run=NULL WHERE id=$1")
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    let migration = include_str!("../migrations/0014_goal_formation.sql");
    let backfill = migration
        .split_once("UPDATE goals")
        .unwrap()
        .1
        .split_once("CREATE INDEX")
        .unwrap()
        .0;
    sqlx::query(&format!("UPDATE goals{backfill}"))
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(
        sqlx::query_scalar::<_, Uuid>("SELECT formed_in_run FROM goals WHERE id=$1")
            .bind(g)
            .fetch_one(&pool)
            .await
            .unwrap(),
        birth
    );
    for id in receipt.action_ids {
        l.state_version = s.execute_local_action(&l, id).await.unwrap().state_version;
    }
    let mut p = proposal();
    p["tool_intents"] = json!([
        {"capability_id":"notes.write","input":{"title":"比较","text":format!("{BODY}\n分析：不能把原子性当成持久性。"),"citations":[{"document_id":doc,"start_char":0,"end_char":BODY.chars().count()}]}},
        {"capability_id":"notes.write","input":{"title":"未引用的限制","text":"COUNTEREVIDENCE：尚未做实际断电实验。"}}
    ]);
    for id in commit(&s, &mut l, p).await.action_ids {
        l.state_version = s.execute_local_action(&l, id).await.unwrap().state_version;
    }
    let mut p = proposal();
    p["goal_changes"] = json!([{"action":"activate","goal_id":g,"reason":"保留活动状态等待审查","evidence_ids":[doc]}]);
    commit(&s, &mut l, p).await;
    s.tick_goals().await.unwrap().unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    let context = s
        .prepare_goal_review(&lease, s.capabilities().await.unwrap())
        .await
        .unwrap()
        .unwrap();
    let input = context.goal_review.unwrap();
    assert_eq!(input.artifacts.len(), 2);
    assert_eq!(input.observations.len(), 1);
    assert_eq!(input.sources.len(), 1);
    assert!(
        input
            .artifacts
            .iter()
            .any(|a| a.body.contains("COUNTEREVIDENCE"))
    );
    assert!(
        !serde_json::to_string(&input)
            .unwrap()
            .contains("UNRELATED_ARTIFACT")
    );
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(birth)
        .execute(&pool)
        .await
        .unwrap();
    let result = s
        .prepare_goal_review(&lease, s.capabilities().await.unwrap())
        .await;
    assert!(
        !matches!(result, Ok(Some(_))),
        "invalid formation source must never be reviewed"
    );
}
