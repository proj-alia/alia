//! Actual DB/HTTP provenance gates with explicitly synthetic development reviews.
include!("support/development_context_fixture.rs");
use alia::{
    adapters::store::StoreError,
    domain::cognition::ModelContext,
    runtime::{
        execution::model_step,
        model::{CallLimits, FixtureProvider},
        scheduling::{RunLease, TokenAllowance},
    },
};
fn allowance() -> TokenAllowance {
    TokenAllowance {
        input_upper_bound: 100,
        output_cap: 50,
    }
}
fn empty_proposal() -> String {
    json!({"schema_version":1,"reply":"","reply_evidence_ids":[],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"synthetic no-op decision; no claim of growth"}).to_string()
}
async fn active(s: &Store) -> RunLease {
    s.claim_run(Uuid::new_v4()).await.unwrap().unwrap()
}
async fn prepared(s: &Store, l: &RunLease) -> alia::runtime::execution::PreparedDecision {
    model_step(
        s,
        &FixtureProvider {
            response: empty_proposal(),
        },
        l,
        vec![],
        CallLimits::default(),
    )
    .await
    .unwrap()
}
async fn accepted_traits(s: &Store, pool: &PgPool) -> (Uuid, DevelopmentReceipt) {
    let e = message(s).await;
    let v = registered(pool).await;
    let r = accepted(
        s,
        pool,
        &draft(e, DevelopmentTarget::Traits, traits(0.61)),
        v,
    )
    .await;
    (e, r)
}
async fn sever(pool: &PgPool, id: Uuid) {
    sqlx::query("DELETE FROM derivation_edges")
        .execute(pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(id)
        .execute(pool)
        .await
        .unwrap();
}

#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; synthetic verifier records only"]
async fn actual_current_heads_reach_author_review_and_restart_without_rewriting_history(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (e, a) = accepted_traits(&s, &pool).await;
    let l = active(&s).await;
    let before = fingerprint(&pool).await;
    let c = s.model_context(l.run_id, vec![]).await.unwrap();
    let d = c.development_context.as_ref().unwrap();
    assert_eq!(d.heads.len(), 1);
    assert_eq!(d.heads[0].revision_id, a.revision_id);
    assert_eq!(d.heads[0].target, DevelopmentTarget::Traits);
    assert_eq!(c.persona["personality"]["traits"], traits(0.61));
    assert!(d.source_ids.contains(&e));
    assert!(d.source_ids.contains(&a.development_id));
    assert!(d.source_ids.contains(&a.event_id));
    assert!(d.source_ids.iter().all(|id| c.input_manifest.contains(id)));
    assert_eq!(fingerprint(&pool).await, before);
    let restored = Store::new(pool.clone());
    let again = restored.model_context(l.run_id, vec![]).await.unwrap();
    assert_eq!(again.development_context, c.development_context);
    let mut proposal =
        serde_json::from_str::<alia::domain::cognition::DecisionProposal>(&empty_proposal())
            .unwrap();
    proposal.reply = "我会按当前真实人格继续交流".into();
    proposal.reply_evidence_ids = vec![e];
    let reviewer = alia::domain::grounding::review_context(&c, Uuid::new_v4(), &proposal).unwrap();
    let packet = alia::domain::grounding::evidence_packet(&reviewer).unwrap();
    assert_eq!(packet["packet_version"], "target-citations/11");
    assert_eq!(
        packet["development_context_not_experience"],
        serde_json::to_value(d).unwrap()
    );
    let ready = prepared(&s, &l).await;
    let receipt = s.commit_decision(&l, &ready, &[]).await.unwrap();
    let server = Server::start(&pool).await;
    let (status, _, view) = server.get("/persona").await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(view["initial"], seed());
    assert_eq!(view["current"]["personality"]["traits"], traits(0.61));
    assert_eq!(
        server
            .revert(
                a.development_id,
                a.development_version,
                "legitimate-later-revert",
                "合法后续撤回"
            )
            .await
            .0,
        StatusCode::OK
    );
    let before = fingerprint(&pool).await;
    assert_eq!(s.commit_decision(&l, &ready, &[]).await.unwrap(), receipt);
    assert_eq!(fingerprint(&pool).await, before);
    assert_eq!(s.snapshot().await.unwrap().unwrap().persona, seed());
}

#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; synthetic verifier records only"]
async fn forged_persona_metadata_missing_sources_and_goal_substitution_cannot_reserve(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let (_e, _a) = accepted_traits(&s, &pool).await;
    let l = active(&s).await;
    let c = s.model_context(l.run_id, vec![]).await.unwrap();
    let before = fingerprint(&pool).await;
    let mut cases: Vec<ModelContext> = vec![];
    let mut x = c.clone();
    x.persona["personality"]["traits"]["curiosity"] = json!(0.99);
    cases.push(x);
    let mut x = c.clone();
    x.development_context = None;
    cases.push(x);
    let mut x = c.clone();
    x.development_context.as_mut().unwrap().heads[0].version += 1;
    cases.push(x);
    let mut x = c.clone();
    x.input_manifest.retain(|id| {
        !c.development_context
            .as_ref()
            .unwrap()
            .source_ids
            .contains(id)
    });
    cases.push(x);
    let mut x = c.clone();
    x.current_goal_id = Some(Uuid::new_v4());
    cases.push(x);
    for x in cases {
        assert!(
            s.reserve_model_call(&l, Uuid::new_v4(), &x, allowance())
                .await
                .is_err()
        );
        assert_eq!(fingerprint(&pool).await, before);
    }
}

#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; synthetic verifier records only"]
async fn ancestor_revocation_before_reservation_or_dispatch_has_no_new_charge(pool: PgPool) {
    let s = setup(&pool).await;
    let (e, _a) = accepted_traits(&s, &pool).await;
    let l = active(&s).await;
    let c = s.model_context(l.run_id, vec![]).await.unwrap();
    sever(&pool, e).await;
    let before = fingerprint(&pool).await;
    assert!(
        s.reserve_model_call(&l, Uuid::new_v4(), &c, allowance())
            .await
            .is_err()
    );
    assert!(s.model_context(l.run_id, vec![]).await.is_err());
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("UPDATE objects SET blocked=false WHERE id=$1")
        .bind(e)
        .execute(&pool)
        .await
        .unwrap();
    let r = s
        .reserve_model_call(&l, Uuid::new_v4(), &c, allowance())
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(e)
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(s.dispatch_model_call(&l, r.id).await.is_err());
    assert_eq!(fingerprint(&pool).await, before);
    let (status,calls):(String,i32)=sqlx::query_as("SELECT u.status,r.model_calls FROM usage_reservations u JOIN runs r ON r.id=u.run_id WHERE u.id=$1").bind(r.id).fetch_one(&pool).await.unwrap();
    assert_eq!(status, "reserved");
    assert_eq!(calls, 0);
    sqlx::query("UPDATE objects SET blocked=false WHERE id=$1")
        .bind(e)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE development_targets SET provenance_removed=true")
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(s.dispatch_model_call(&l, r.id).await.is_err());
    assert_eq!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; synthetic verifier records only"]
async fn removed_flags_without_blocked_objects_stop_commit_and_cached_receipts(pool: PgPool) {
    let s = setup(&pool).await;
    let (_e, a) = accepted_traits(&s, &pool).await;
    let l = active(&s).await;
    let ready = prepared(&s, &l).await;
    sqlx::query("DELETE FROM derivation_edges")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE developments SET provenance_removed=true WHERE id=$1")
        .bind(a.development_id)
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(s.commit_decision(&l, &ready, &[]).await.is_err());
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("UPDATE developments SET provenance_removed=false WHERE id=$1")
        .bind(a.development_id)
        .execute(&pool)
        .await
        .unwrap();
    let receipt = s.commit_decision(&l, &ready, &[]).await.unwrap();
    sqlx::query("UPDATE development_revisions SET provenance_removed=true WHERE id=$1")
        .bind(a.revision_id.unwrap())
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(s.commit_decision(&l, &ready, &[]).await.is_err());
    assert_eq!(fingerprint(&pool).await, before);
    assert!(receipt.memory_ids.is_empty());
    let charged: i64 = sqlx::query_scalar(
        "SELECT coalesce(sum(charged_tokens),0)::bigint FROM usage_reservations",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!(
        charged > 0,
        "already settled synthetic invocation remains in ledger"
    );
}

#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; synthetic verifier records only"]
async fn public_state_and_persona_do_not_return_unprovable_cached_values(pool: PgPool) {
    let s = setup(&pool).await;
    let (e, _a) = accepted_traits(&s, &pool).await;
    let server = Server::start(&pool).await;
    assert_eq!(server.get("/state").await.0, StatusCode::OK);
    assert_eq!(server.get("/persona").await.0, StatusCode::OK);
    sever(&pool, e).await;
    let before = fingerprint(&pool).await;
    for path in ["/state", "/persona", "/management"] {
        let (status, _, body) = server.get(path).await;
        assert!(!status.is_success());
        assert!(body.get("persona").is_none());
        assert!(body.get("current").is_none());
    }
    assert_eq!(fingerprint(&pool).await, before);
    server.rt.store.fence_privacy();
    assert_eq!(
        server.get("/persona").await.0,
        StatusCode::SERVICE_UNAVAILABLE
    );
}

#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; synthetic verifier records only"]
async fn matching_task_strategy_overrides_goal_and_other_scopes_stay_out(pool: PgPool) {
    let s = setup(&pool).await;
    let server = Server::start(&pool).await;
    async fn goal(server: &Server, key: &str) -> Uuid {
        let (status,_,r)=server.call("POST","/goals",&[("idempotency-key",key)],Some(&json!({"description":key,"success_criterion":"保存有来源结果","next_step":"检查资料","stop_condition":"无资料等待"}).to_string()),true).await;
        assert_eq!(status, StatusCode::CREATED, "{r}");
        Uuid::parse_str(r["goal_id"].as_str().unwrap()).unwrap()
    }
    let g = goal(&server, "goal-one").await;
    let other = goal(&server, "goal-two").await;
    let input = s
        .submit_message(
            "task-source",
            "synthetic controlled scope association",
            None,
        )
        .await
        .unwrap();
    let e = input.event_id;
    let task = input.operation_id;
    sqlx::query("UPDATE runs SET goal_id=$2 WHERE id=$1")
        .bind(task)
        .bind(g)
        .execute(&pool)
        .await
        .unwrap();
    let verifier = registered(&pool).await;
    for (scope, description) in [
        (StrategyScope::Goal(g), "goal strategy"),
        (StrategyScope::Goal(other), "unrelated private strategy"),
        (StrategyScope::Task(task), "precise task strategy"),
    ] {
        accepted(
            &s,
            &pool,
            &draft(
                e,
                DevelopmentTarget::TaskStrategy,
                json!({"scope":scope,"description":description}),
            ),
            verifier,
        )
        .await;
    }
    let l = active(&s).await;
    assert_eq!(l.run_id, task);
    let c = s.model_context(task, vec![]).await.unwrap();
    let d = c.development_context.as_ref().unwrap();
    assert_eq!(d.heads.len(), 2);
    assert_eq!(
        d.effective_task_strategy.as_ref().unwrap().description,
        "precise task strategy"
    );
    assert!(
        !serde_json::to_string(d)
            .unwrap()
            .contains(&other.to_string())
    );
    assert_eq!(c.persona, seed());
    let before = fingerprint(&pool).await;
    let ready = prepared(&s, &l).await;
    let receipt = s.commit_decision(&l, &ready, &[]).await.unwrap();
    assert_eq!(receipt.run_status, "committed");
    assert_ne!(fingerprint(&pool).await, before);
}

#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; synthetic verifier records only"]
async fn oversized_complete_provenance_fails_instead_of_truncating_a_persona_source(pool: PgPool) {
    let s = setup(&pool).await;
    let (_e, a) = accepted_traits(&s, &pool).await;
    let l = active(&s).await;
    let target: Uuid = sqlx::query_scalar("SELECT target_id FROM developments WHERE id=$1")
        .bind(a.development_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    let ids: Vec<Uuid> = (0..4097).map(|_| Uuid::new_v4()).collect();
    sqlx::query("INSERT INTO objects(individual_id,id,kind) SELECT i.id,x,'event' FROM individuals i CROSS JOIN unnest($1::uuid[]) x").bind(&ids).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO development_dependencies(individual_id,object_id,source_id) SELECT i.id,$1,x FROM individuals i CROSS JOIN unnest($2::uuid[]) x ON CONFLICT DO NOTHING").bind(target).bind(&ids).execute(&pool).await.unwrap();
    let before = fingerprint(&pool).await;
    assert_eq!(
        s.public_state().await.unwrap()["persona"]["personality"]["traits"],
        traits(0.61)
    );
    assert!(matches!(
        s.model_context(l.run_id, vec![]).await,
        Err(StoreError::Invalid(
            "development context provenance exceeds limit"
        ))
    ));
    assert_eq!(fingerprint(&pool).await, before);
}

struct ScopedFixtureReview {
    source: Uuid,
}
#[async_trait::async_trait]
impl alia::runtime::model::ModelProvider for ScopedFixtureReview {
    fn mode(&self) -> alia::runtime::model::ProviderMode {
        alia::runtime::model::ProviderMode::Fixture
    }
    fn token_allowance(
        &self,
        c: &ModelContext,
        l: CallLimits,
    ) -> Result<TokenAllowance, alia::runtime::model::ModelFailure> {
        FixtureProvider {
            response: empty_proposal(),
        }
        .token_allowance(c, l)
    }
    fn supports_decision_review(&self) -> bool {
        true
    }
    fn decision_review_allowance(
        &self,
        c: &ModelContext,
        l: CallLimits,
    ) -> Result<TokenAllowance, alia::runtime::model::ModelFailure> {
        self.token_allowance(c, l)
    }
    async fn decide(
        &self,
        c: &ModelContext,
        l: CallLimits,
        m: alia::runtime::model::ModelCallMetadata,
    ) -> Result<alia::runtime::model::ModelResult, alia::runtime::model::ModelFailure> {
        assert!(
            c.development_context
                .as_ref()
                .unwrap()
                .effective_task_strategy
                .is_some()
        );
        let mut p: Value = serde_json::from_str(&empty_proposal()).unwrap();
        p["reply"] = json!("本任务先检查实际来源，再决定下一步");
        p["reply_evidence_ids"] = json!([self.source]);
        FixtureProvider {
            response: p.to_string(),
        }
        .decide(c, l, m)
        .await
    }
    async fn review_decision(
        &self,
        c: &ModelContext,
        _: CallLimits,
        _: alia::runtime::model::ModelCallMetadata,
    ) -> Result<alia::runtime::model::DecisionReviewResult, alia::runtime::model::ModelFailure>
    {
        use alia::domain::grounding::{ContentCheck, DecisionAssessment, ReviewTarget, Support};
        let draft = c.decision_review.as_ref().unwrap();
        assert_eq!(draft.expected_targets, vec![ReviewTarget::Reply]);
        assert!(
            c.development_context
                .as_ref()
                .unwrap()
                .effective_task_strategy
                .is_some()
        );
        Ok(alia::runtime::model::DecisionReviewResult {
            assessment: DecisionAssessment {
                schema_version: 1,
                checks: vec![ContentCheck {
                    target: ReviewTarget::Reply,
                    verdict: Support::Supported,
                    reason: "synthetic exact request binding, not semantic validation".into(),
                    evidence_ids: vec![self.source],
                }],
            },
            usage: None,
            provider_model: "synthetic-scoped-review".into(),
            provider_mode: alia::runtime::model::ProviderMode::Fixture,
        })
    }
}

#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; synthetic author/reviewer, no network"]
async fn scoped_author_review_and_commit_keep_original_source_snapshot_bound(pool: PgPool) {
    let s = setup(&pool).await;
    let e = message(&s).await;
    let task: Uuid = sqlx::query_scalar("SELECT id FROM runs WHERE trigger_event_id=$1")
        .bind(e)
        .fetch_one(&pool)
        .await
        .unwrap();
    let verifier = registered(&pool).await;
    accepted(
        &s,
        &pool,
        &draft(
            e,
            DevelopmentTarget::TaskStrategy,
            json!({"scope":{"kind":"task","id":task},"description":"先核查本任务来源"}),
        ),
        verifier,
    )
    .await;
    let l = active(&s).await;
    assert_eq!(l.run_id, task);
    let ready = model_step(
        &s,
        &ScopedFixtureReview { source: e },
        &l,
        vec![],
        CallLimits::default(),
    )
    .await
    .unwrap();
    let review = ready.grounding_review_id().unwrap();
    let author: Uuid =
        sqlx::query_scalar("SELECT author_reservation_id FROM decision_reviews WHERE event_id=$1")
            .bind(review)
            .fetch_one(&pool)
            .await
            .unwrap();
    let mut forged =
        alia::domain::grounding::review_context(ready.context(), author, &ready.result().proposal)
            .unwrap();
    forged
        .development_context
        .as_mut()
        .unwrap()
        .source_ids
        .retain(|id| *id != e);
    // e remains in the manifest and every target/root is unchanged. Only the
    // settled original author request digest prevents altering this observed list.
    assert!(forged.input_manifest.contains(&e));
    let before = fingerprint(&pool).await;
    assert!(matches!(
        s.reserve_model_call(&l, Uuid::new_v4(), &forged, allowance())
            .await,
        Err(StoreError::StaleState)
    ));
    assert_eq!(fingerprint(&pool).await, before);
    let receipt = s.commit_decision(&l, &ready, &[]).await.unwrap();
    assert!(receipt.message_id.is_some());
    let before = fingerprint(&pool).await;
    assert_eq!(s.commit_decision(&l, &ready, &[]).await.unwrap(), receipt);
    assert_eq!(fingerprint(&pool).await, before);
}

async fn scoped_reservation_subject(
    pool: &PgPool,
) -> (Store, Uuid, DevelopmentReceipt, RunLease, ModelContext) {
    let s = setup(pool).await;
    let e = message(&s).await;
    let task: Uuid = sqlx::query_scalar("SELECT id FROM runs WHERE trigger_event_id=$1")
        .bind(e)
        .fetch_one(pool)
        .await
        .unwrap();
    let verifier = registered(pool).await;
    let accepted = accepted(
        &s,
        pool,
        &draft(
            e,
            DevelopmentTarget::TaskStrategy,
            json!({"scope":{"kind":"task","id":task},"description":"先核对任务的真实来源"}),
        ),
        verifier,
    )
    .await;
    let lease = active(&s).await;
    assert_eq!(lease.run_id, task);
    let context = s.model_context(task, vec![]).await.unwrap();
    (s, e, accepted, lease, context)
}

#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; synthetic dispatch accounting, no provider call"]
async fn scoped_reservation_recovery_preserves_original_id_and_all_accounting_phases(pool: PgPool) {
    let (s, _e, _a, lease, context) = scoped_reservation_subject(&pool).await;
    let key = Uuid::new_v4();
    let first = s
        .reserve_model_call(&lease, key, &context, allowance())
        .await
        .unwrap();
    let fresh = s.model_context(lease.run_id, vec![]).await.unwrap();
    assert_ne!(fresh.development_context, context.development_context);
    for expected_status in ["reserved", "dispatched", "settled"] {
        let before = fingerprint(&pool).await;
        let (one, two) = tokio::join!(
            s.reserve_model_call(&lease, key, &context, allowance()),
            s.reserve_model_call(&lease, key, &context, allowance())
        );
        for r in [one.unwrap(), two.unwrap()] {
            assert_eq!(r.id, first.id);
            assert_eq!(r.status, expected_status);
            assert_eq!(r.reserved_tokens, 150);
            assert_eq!(
                r.charged_tokens,
                if expected_status == "settled" { 50 } else { 0 }
            );
        }
        assert_eq!(fingerprint(&pool).await, before);
        match expected_status {
            "reserved" => {
                assert!(s.dispatch_model_call(&lease, first.id).await.unwrap());
                assert!(!s.dispatch_model_call(&lease, first.id).await.unwrap());
            }
            "dispatched" => {
                s.settle_model_call(
                    first.id,
                    Some(alia::runtime::model::TokenUsage {
                        input: 40,
                        output: 10,
                        total: 50,
                    }),
                )
                .await
                .unwrap();
            }
            _ => {}
        }
    }
    let (reservations, calls): (i64, i32) = sqlx::query_as(
        "SELECT (SELECT count(*) FROM usage_reservations WHERE run_id=r.id),model_calls FROM runs r WHERE id=$1",
    )
    .bind(lease.run_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!((reservations, calls), (1, 1));
}

#[sqlx::test]
#[ignore = "actual schema29 PostgreSQL; synthetic verifier records only"]
async fn scoped_reservation_replay_keeps_digest_generation_and_revocation_gates(pool: PgPool) {
    let (s, event, accepted, lease, context) = scoped_reservation_subject(&pool).await;
    let key = Uuid::new_v4();
    s.reserve_model_call(&lease, key, &context, allowance())
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    let mut altered = context.clone();
    altered.persona["expression"]["tone"] = json!("伪造的原请求");
    let mut omitted = context.clone();
    omitted
        .development_context
        .as_mut()
        .unwrap()
        .source_ids
        .retain(|id| *id != event);
    for changed in [altered, omitted] {
        assert!(matches!(
            s.reserve_model_call(&lease, key, &changed, allowance())
                .await,
            Err(StoreError::IdempotencyConflict)
        ));
    }
    assert!(matches!(
        s.reserve_model_call(
            &lease,
            key,
            &context,
            TokenAllowance {
                input_upper_bound: 101,
                output_cap: 50
            }
        )
        .await,
        Err(StoreError::IdempotencyConflict)
    ));
    assert!(matches!(
        s.reserve_model_call(&lease, Uuid::new_v4(), &context, allowance())
            .await,
        Err(StoreError::StaleState)
    ));
    let mut old_lease = lease.clone();
    old_lease.generation += 1;
    assert!(matches!(
        s.reserve_model_call(&old_lease, key, &context, allowance())
            .await,
        Err(StoreError::LeaseLost)
    ));
    assert_eq!(fingerprint(&pool).await, before);
    sever(&pool, event).await;
    let before = fingerprint(&pool).await;
    assert!(
        s.reserve_model_call(&lease, key, &context, allowance())
            .await
            .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
    sqlx::query("UPDATE objects SET blocked=false WHERE id=$1")
        .bind(event)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE developments SET provenance_removed=true WHERE id=$1")
        .bind(accepted.development_id)
        .execute(&pool)
        .await
        .unwrap();
    let before = fingerprint(&pool).await;
    assert!(
        s.reserve_model_call(&lease, key, &context, allowance())
            .await
            .is_err()
    );
    assert_eq!(fingerprint(&pool).await, before);
    let (count, reserved, charged): (i64, i64, i64) = sqlx::query_as(
        "SELECT count(*),sum(reserved_tokens)::bigint,sum(charged_tokens)::bigint FROM usage_reservations WHERE run_id=$1",
    ).bind(lease.run_id).fetch_one(&pool).await.unwrap();
    assert_eq!((count, reserved, charged), (1, 150, 0));
}
