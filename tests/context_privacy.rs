//! Optional revoked recall must not poison an independent, valid new input.
use alia::{
    adapters::store::{Store, StoreError},
    domain::{
        cognition::ModelContext,
        config::{PersonaSeed, RuntimeRules},
    },
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        scheduling::{RunLease, TokenAllowance},
    },
};
use serde_json::json;
use sqlx::PgPool;
use uuid::Uuid;

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
async fn input(s: &Store, pool: &PgPool) -> (RunLease, Uuid) {
    let msg = s
        .submit_message("current", "独立的新输入", None)
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
async fn edge(pool: &PgPool, owner: Uuid, from: Uuid, to: Uuid) {
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id)VALUES($1,$2,$3)ON CONFLICT DO NOTHING")
        .bind(owner).bind(from).bind(to).execute(pool).await.unwrap();
}
async fn reserve(s: &Store, l: &RunLease, c: &ModelContext) {
    let receipt = s
        .reserve_model_call(
            l,
            Uuid::new_v4(),
            c,
            TokenAllowance {
                input_upper_bound: 100,
                output_cap: 32,
            },
        )
        .await
        .unwrap();
    assert_eq!(receipt.status, "reserved");
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn hidden_relationship_is_omitted_without_disabling_new_independent_cognition(pool: PgPool) {
    let s = setup(&pool).await;
    let old = s
        .submit_message("private", "已撤销的旧关系信息", None)
        .await
        .unwrap();
    let subject: Uuid =
        sqlx::query_scalar("SELECT subject_id FROM relationships WHERE subject_key='owner'")
            .fetch_one(&pool)
            .await
            .unwrap();
    sqlx::query("UPDATE relationships SET expectations=$1 WHERE subject_id=$2")
        .bind(json!(["SYNTHETIC private expectation"]))
        .bind(subject)
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(old.event_id)
        .execute(&pool)
        .await
        .unwrap();
    let (lease, current) = input(&s, &pool).await;
    let c = s.model_context(lease.run_id, vec![]).await.unwrap();
    assert_eq!(
        c.relationship,
        json!({}),
        "the poisoned optional projection must not enter a model request"
    );
    assert!(!c.input_manifest.contains(&subject));
    assert!(!c.input_manifest.contains(&old.event_id));
    assert!(c.input_manifest.contains(&current));
    reserve(&s, &lease, &c).await;
    let calls: i32 = sqlx::query_scalar("SELECT model_calls FROM runs WHERE id=$1")
        .bind(lease.run_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(
        calls, 0,
        "no actual request was dispatched by this provenance test"
    );
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn hidden_optional_goals_and_recent_events_are_filtered_but_visible_peers_survive(
    pool: PgPool,
) {
    let s = setup(&pool).await;
    let root = s.submit_message("root", "待撤销源", None).await.unwrap();
    let mid = s.submit_message("mid", "中间源", None).await.unwrap();
    let leaf = s
        .submit_message("leaf", "SYNTHETIC hidden recent message", None)
        .await
        .unwrap();
    let public = s
        .submit_message("public", "可用的独立资料", None)
        .await
        .unwrap();
    edge(&pool, root.individual_id, root.event_id, mid.event_id).await;
    edge(&pool, root.individual_id, mid.event_id, leaf.event_id).await;
    let hidden = Uuid::new_v4();
    let visible = Uuid::new_v4();
    for (id, source, title) in [
        (hidden, mid.event_id, "SYNTHETIC hidden goal"),
        (visible, public.event_id, "可用目标"),
    ] {
        sqlx::query("INSERT INTO objects(individual_id,id,kind)VALUES($1,$2,'goal')")
            .bind(root.individual_id)
            .bind(id)
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("INSERT INTO goals(individual_id,id,origin_type,origin_ids,description,success_criterion,next_step,stop_condition)VALUES($1,$2,'event',$3,$4,'有实际成果','检查资料','没有资料则停止')")
            .bind(root.individual_id).bind(id).bind(json!([source])).bind(title).execute(&pool).await.unwrap();
        edge(&pool, root.individual_id, source, id).await;
    }
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(root.event_id)
        .execute(&pool)
        .await
        .unwrap();
    let (lease, current) = input(&s, &pool).await;
    let c = s.model_context(lease.run_id, vec![]).await.unwrap();
    assert!(!c.goals.iter().any(|g| g.id == hidden));
    assert!(c.goals.iter().any(|g| g.id == visible));
    for private in [root.event_id, mid.event_id, leaf.event_id, hidden] {
        assert!(!c.input_manifest.contains(&private));
        assert!(!c.events.iter().any(|e| e.id == private));
    }
    assert!(c.events.iter().any(|e| e.id == public.event_id));
    assert!(c.events.iter().any(|e| e.id == current));
    reserve(&s, &lease, &c).await;
}

#[sqlx::test]
#[ignore = "requires isolated PostgreSQL; run python3 scripts/test_database.py"]
async fn a_revoked_required_trigger_still_fails_instead_of_being_silently_omitted(pool: PgPool) {
    let s = setup(&pool).await;
    let old = s.submit_message("old", "撤销源", None).await.unwrap();
    let (lease, current) = input(&s, &pool).await;
    edge(&pool, old.individual_id, old.event_id, current).await;
    sqlx::query("UPDATE objects SET blocked=true WHERE id=$1")
        .bind(old.event_id)
        .execute(&pool)
        .await
        .unwrap();
    assert!(matches!(
        s.model_context(lease.run_id, vec![]).await,
        Err(StoreError::StaleState)
    ));
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(count, 0);
}
