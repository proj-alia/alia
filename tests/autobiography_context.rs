//! Structural history/provenance and actual HTTP/PG checks, not model semantics.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        autobiography::{AutobiographyContext, AutobiographyDraft, CurrentAutobiography},
        beliefs::{BeliefDraft, BeliefPosition},
        cognition::{DecisionProposal, ModelContext},
        config::{PersonaSeed, RuntimeRules},
        grounding,
    },
    runtime::{
        Runtime,
        capability::IMPLEMENTED_CAPABILITIES,
        scheduling::{RunLease, TokenAllowance},
    },
};
use serde_json::{Value, json};
use sqlx::PgPool;
use uuid::Uuid;
fn sample() -> ModelContext {
    serde_json::from_str(include_str!("../evals/model_context.example.json")).unwrap()
}
fn proposal(id: Uuid) -> DecisionProposal {
    DecisionProposal::parse(serde_json::to_vec(&json!({"schema_version":1,"reply":"我现在对那次经历有了不同的理解。","reply_evidence_ids":[id],"memory_changes":[],"goal_changes":[],"tool_intents":[],"development":null,"brief_reason":"当前历史解释不是原事件"})).unwrap().as_slice()).unwrap()
}
fn selected() -> ModelContext {
    let mut c = sample();
    let id = Uuid::new_v4();
    let event = c.events[0].id;
    c.input_manifest.extend([id, event]);
    c.input_manifest.sort();
    c.input_manifest.dedup();
    c.autobiographical_context = Some(AutobiographyContext {
        entries: vec![CurrentAutobiography {
            id,
            version: 1,
            topic: "learning.project".into(),
            title: "共同探索".into(),
            interpretation: "当前对早期讨论的理解".into(),
            event_ids: vec![event],
            belief_ids: vec![],
            supersedes: None,
            provenance_ids: vec![id, event],
            basis_event_ids: vec![event],
        }],
        belief_basis: vec![],
        has_more: false,
    });
    c
}
#[test]
fn historical_interpretation_is_reply_evidence_not_a_new_observed_event() {
    let c = selected();
    let id = c.autobiographical_context.as_ref().unwrap().entries[0].id;
    assert!(proposal(id).validate_context(&c).is_ok());
    let mut p = proposal(id);
    p.memory_changes=serde_json::from_value(json!([{"content":"我们亲自经历过这件事","kind":"episodic","scope":"general","source_event_ids":[id],"supersedes":null}])).unwrap();
    assert!(p.validate_context(&c).is_err());
    let mut review = c.clone();
    review.decision_review = Some(grounding::DecisionReviewDraft {
        author_reservation_id: Uuid::new_v4(),
        expected_targets: grounding::targets(&proposal(id)),
        proposal: proposal(id),
    });
    let packet = grounding::evidence_packet(&review).unwrap();
    let text = packet.to_string();
    assert!(text.contains("current_historical_interpretation_not_event_or_world_fact"));
    assert!(text.contains(&c.events[0].id.to_string()));
}
#[test]
fn missing_actual_basis_or_manifest_cannot_be_hidden_by_a_history_id() {
    let mut c = selected();
    let id = c.autobiographical_context.as_ref().unwrap().entries[0].id;
    c.events.remove(0);
    assert!(!c.autobiographical_context.as_ref().unwrap().valid_for(&c));
    assert!(proposal(id).validate_context(&c).is_err());
    let mut c = selected();
    c.input_manifest
        .retain(|v| *v != c.autobiographical_context.as_ref().unwrap().entries[0].id);
    assert!(!c.autobiographical_context.as_ref().unwrap().valid_for(&c));
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
async fn msg(s: &Store, key: &str, text: &str) -> Uuid {
    s.submit_message(key, text, None).await.unwrap().event_id
}
async fn history(
    s: &Store,
    key: &str,
    topic: &str,
    events: Vec<Uuid>,
    beliefs: Vec<Uuid>,
    previous: Option<Uuid>,
    mut sources: Vec<Uuid>,
) -> Uuid {
    sources.extend(&events);
    sources.extend(&beliefs);
    sources.extend(previous);
    sources.sort();
    sources.dedup();
    s.record_autobiography(
        key,
        s.snapshot().await.unwrap().unwrap().expected(),
        &sources,
        &AutobiographyDraft {
            topic: topic.into(),
            title: "合成测试中的一次重新理解".into(),
            interpretation: format!("{key}: 当前解释保留原始事件与旧认识，不是世界事实证明"),
            event_ids: events,
            belief_ids: beliefs,
            supersedes: previous,
        },
    )
    .await
    .unwrap()
    .autobiography_id
}
async fn context(s: &Store, pool: &PgPool) -> (ModelContext, RunLease) {
    let m = s
        .submit_message(&Uuid::new_v4().to_string(), "新的独立输入", None)
        .await
        .unwrap();
    sqlx::query("UPDATE runs SET status='cancelled' WHERE status='queued' AND id<>$1")
        .bind(m.operation_id)
        .execute(pool)
        .await
        .unwrap();
    let lease = s.claim_run(Uuid::new_v4()).await.unwrap().unwrap();
    assert_eq!(lease.run_id, m.operation_id);
    (s.model_context(lease.run_id, vec![]).await.unwrap(), lease)
}
async fn belief(
    s: &Store,
    key: &str,
    source: Uuid,
    counter: Uuid,
    previous: Option<Uuid>,
    mut inputs: Vec<Uuid>,
) -> Uuid {
    inputs.extend([source, counter]);
    inputs.extend(previous);
    inputs.sort();
    inputs.dedup();
    s.record_belief(
        key,
        s.snapshot().await.unwrap().unwrap().expected(),
        &inputs,
        &BeliefDraft {
            statement: "合成测试中有待修正的认识".into(),
            scope: "general".into(),
            position: BeliefPosition::Questioned,
            support_event_ids: vec![source],
            counter_event_ids: vec![counter],
            supersedes: previous,
            brief_basis: "保留支持和反对".into(),
        },
    )
    .await
    .unwrap()
    .belief_id
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn restart_selects_current_interpretation_with_complete_old_stances_and_counterevidence(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let a = msg(&s, "a", "先研究的陈述").await;
    let b = msg(&s, "b", "关于相反条件的陈述").await;
    let old_b = belief(&s, "old-b", a, b, None, vec![]).await;
    let new_b = belief(&s, "new-b", a, b, Some(old_b), vec![]).await;
    let old = history(
        &s,
        "old-history",
        "learning.project",
        vec![a, b],
        vec![old_b],
        None,
        vec![old_b, a, b],
    )
    .await;
    let new = history(
        &s,
        "new-history",
        "learning.project",
        vec![a, b],
        vec![new_b],
        Some(old),
        vec![old, old_b, new_b, a, b],
    )
    .await;
    let restored = Store::new(pool.clone());
    let (c, _) = context(&restored, &pool).await;
    let h = c.autobiographical_context.as_ref().unwrap();
    assert!(h.valid_for(&c));
    assert_eq!(h.entries.len(), 1);
    assert_eq!(h.entries[0].id, new);
    assert_eq!(h.entries[0].supersedes, Some(old));
    assert!(
        h.belief_basis
            .iter()
            .any(|v| v.id == old_b && v.status == "superseded")
    );
    assert!(
        h.belief_basis
            .iter()
            .any(|v| v.id == new_b && v.counter_event_ids.contains(&b))
    );
    for id in [a, b, old, old_b, new_b, new] {
        assert!(c.input_manifest.contains(&id));
    }
    assert!(
        c.events
            .iter()
            .any(|e| e.id == b && e.payload["text"] == "关于相反条件的陈述")
    );
    assert!(serde_json::to_vec(&c).unwrap().len() <= 96 * 1024);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn missing_edges_do_not_leak_revoked_history_or_disable_independent_topics(pool: PgPool) {
    let s = setup(&pool).await;
    let private = msg(&s, "private", "待撤销的记录").await;
    let public = msg(&s, "public", "独立记录").await;
    let hidden = history(
        &s,
        "hidden",
        "private.topic",
        vec![private],
        vec![],
        None,
        vec![],
    )
    .await;
    let visible = history(
        &s,
        "visible",
        "public.topic",
        vec![public],
        vec![],
        None,
        vec![],
    )
    .await;
    sqlx::query("DELETE FROM derivation_edges WHERE target_id=$1")
        .bind(hidden)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(private)
        .execute(&pool)
        .await
        .unwrap();
    let (c, l) = context(&s, &pool).await;
    let h = c.autobiographical_context.as_ref().unwrap();
    assert_eq!(
        h.entries.iter().map(|a| a.id).collect::<Vec<_>>(),
        vec![visible]
    );
    assert!(!c.input_manifest.contains(&private));
    assert!(!c.input_manifest.contains(&hidden));
    assert!(
        s.reserve_model_call(
            &l,
            Uuid::new_v4(),
            &c,
            TokenAllowance {
                input_upper_bound: 100,
                output_cap: 32
            }
        )
        .await
        .is_ok()
    );
    let count: i64 =
        sqlx::query_scalar("SELECT count(*) FROM usage_reservations WHERE status<>'reserved'")
            .fetch_one(&pool)
            .await
            .unwrap();
    assert_eq!(count, 0);
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn oversized_full_basis_is_omitted_whole_with_explicit_incomplete_flag(pool: PgPool) {
    let s = setup(&pool).await;
    let mut events = vec![];
    for i in 0..12 {
        events.push(msg(&s, &format!("large-{i}"), &"x".repeat(11000)).await);
    }
    let id = history(
        &s,
        "large-history",
        "large.topic",
        events.clone(),
        vec![],
        None,
        vec![],
    )
    .await;
    let (c, _) = context(&s, &pool).await;
    let h = c.autobiographical_context.as_ref().unwrap();
    assert!(h.entries.is_empty());
    assert!(h.has_more);
    assert!(!c.input_manifest.contains(&id));
    assert!(serde_json::to_vec(&c).unwrap().len() <= 96 * 1024);
    assert!(
        c.events
            .iter()
            .any(|e| e.id == c.current_event_id && e.payload["text"] == "新的独立输入")
    );
}
#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn revocation_after_selection_blocks_reservation_and_later_dispatch_lease(pool: PgPool) {
    let s = setup(&pool).await;
    let source = msg(&s, "source", "旧依据").await;
    history(&s, "h", "sample.topic", vec![source], vec![], None, vec![]).await;
    let (c, l) = context(&s, &pool).await;
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
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
    sqlx::query("UPDATE objects SET blocked=false WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    let reserved = s
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
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.dispatch_model_call(&l, reserved.id).await,
        Err(StoreError::LeaseLost)
    ));
    let (sent,calls):(bool,i32)=sqlx::query_as("SELECT u.dispatched_at IS NOT NULL,r.model_calls FROM usage_reservations u JOIN runs r ON r.id=u.run_id WHERE u.id=$1")
        .bind(reserved.id).fetch_one(&pool).await.unwrap();
    assert!(!sent);
    assert_eq!(calls, 0);
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL"]
async fn timeline_reads_actual_interpretations_with_auth_strict_parameters_and_no_side_effects(
    pool: PgPool,
) {
    use axum::{
        body::Body,
        http::{Request, StatusCode},
    };
    use http_body_util::BodyExt;
    use tower::ServiceExt;
    let s = setup(&pool).await;
    let source = msg(&s, "source", "实际收到的合成测试陈述").await;
    let old = history(
        &s,
        "old",
        "timeline.topic",
        vec![source],
        vec![],
        None,
        vec![],
    )
    .await;
    let current = history(
        &s,
        "current",
        "timeline.topic",
        vec![source],
        vec![],
        Some(old),
        vec![old, source],
    )
    .await;
    let before:Value=sqlx::query_scalar("SELECT jsonb_build_object('i',(SELECT jsonb_agg(to_jsonb(i)) FROM individuals i),'e',(SELECT count(*) FROM events),'outbox',(SELECT count(*) FROM outbox),'usage',(SELECT count(*) FROM usage_reservations))").fetch_one(&pool).await.unwrap();
    let app = alia::service::router(
        Runtime::new(
            pool.clone(),
            "synthetic-isolated-test-access-token-0123456789",
        )
        .unwrap(),
        std::path::PathBuf::from("web/dist"),
    );
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/timeline?include_history=true&limit=30")
                .header(
                    "authorization",
                    "Bearer synthetic-isolated-test-access-token-0123456789",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    assert_eq!(response.headers()["cache-control"], "no-store");
    let result: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert_eq!(result["view"], "autobiographical_interpretations");
    assert_eq!(result["items"].as_array().unwrap().len(), 2);
    assert_eq!(result["items"][0]["id"], current.to_string());
    assert_eq!(result["items"][1]["id"], old.to_string());
    for query in [
        "limit=0",
        "limit=101",
        "unknown=yes",
        "limit=1&limit=2",
        "include_history=maybe",
    ] {
        let response = app
            .clone()
            .oneshot(
                Request::builder()
                    .uri(format!("/api/v1/timeline?{query}"))
                    .header(
                        "authorization",
                        "Bearer synthetic-isolated-test-access-token-0123456789",
                    )
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(response.status(), StatusCode::UNPROCESSABLE_ENTITY);
    }
    let response = app
        .clone()
        .oneshot(
            Request::builder()
                .uri("/api/v1/timeline")
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::UNAUTHORIZED);
    let after:Value=sqlx::query_scalar("SELECT jsonb_build_object('i',(SELECT jsonb_agg(to_jsonb(i)) FROM individuals i),'e',(SELECT count(*) FROM events),'outbox',(SELECT count(*) FROM outbox),'usage',(SELECT count(*) FROM usage_reservations))").fetch_one(&pool).await.unwrap();
    assert_eq!(before, after);
    sqlx::query("DELETE FROM derivation_edges WHERE target_id=ANY($1)")
        .bind(vec![old, current])
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(source)
        .execute(&pool)
        .await
        .unwrap();
    let response = app
        .oneshot(
            Request::builder()
                .uri("/api/v1/timeline?include_history=true")
                .header(
                    "authorization",
                    "Bearer synthetic-isolated-test-access-token-0123456789",
                )
                .body(Body::empty())
                .unwrap(),
        )
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let result: Value =
        serde_json::from_slice(&response.into_body().collect().await.unwrap().to_bytes()).unwrap();
    assert!(result["items"].as_array().unwrap().is_empty());
}
