//! Bounded synthetic UI catalog. Never a production verifier or growth verdict.
#![allow(dead_code)]
include!("../tests/support/development_context_fixture.rs");

use serde::Deserialize;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Input {
    database_url: String,
    database: String,
    individual_id: Uuid,
    service_url: String,
    service_token: String,
}

async fn build_catalog(input: Input) -> Value {
    assert!(input.database.starts_with("alia_p02_"));
    let parsed = reqwest::Url::parse(&input.database_url).unwrap();
    assert_eq!(parsed.host_str(), Some("127.0.0.1"));
    assert_eq!(parsed.path(), format!("/{}", input.database));
    let service = reqwest::Url::parse(&input.service_url).unwrap();
    assert_eq!(service.scheme(), "http");
    assert_eq!(service.host_str(), Some("127.0.0.1"));
    let pool = sqlx::postgres::PgPoolOptions::new()
        .max_connections(2)
        .connect(&input.database_url)
        .await
        .unwrap();
    let database: String = sqlx::query_scalar("SELECT current_database()")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(database, input.database);
    // Hold one catalog-producer admission lock for this DB for the whole attempt.
    let mut guard = pool.acquire().await.unwrap();
    let locked: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock(71301, 208)")
        .fetch_one(&mut *guard)
        .await
        .unwrap();
    assert!(locked);
    let s = Store::new(pool.clone());
    let state = s.snapshot().await.unwrap().unwrap();
    assert_eq!(state.id, input.individual_id);
    assert_eq!(state.runtime_config["autonomy"]["enabled"], false);
    let owners: i64 = sqlx::query_scalar("SELECT count(*) FROM individuals")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(owners, 1);
    let count: i64 = sqlx::query_scalar("SELECT (SELECT count(*) FROM developments)+(SELECT count(*) FROM development_verifiers)+(SELECT count(*) FROM development_observations)+(SELECT count(*) FROM development_review_records)")
        .fetch_one(&pool).await.unwrap();
    assert_eq!(count, 0, "nonempty or partially seeded database refused");
    let reservations_before: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reservations_before, 0);
    let response = Client::new()
        .get(format!("{}/state", input.service_url.trim_end_matches('/')))
        .bearer_auth(&input.service_token)
        .send()
        .await
        .unwrap();
    assert_eq!(response.status(), StatusCode::OK);
    let remote: Value = response.json().await.unwrap();
    assert_eq!(remote["individual_id"], state.id.to_string());
    // This registration/measurement setup is explicitly the existing QA helper.
    // All development statuses below are produced exclusively through Store.
    let verifier = registered(&pool).await;
    // GoalDraft is private to the adapter; use its real authenticated HTTP API.
    let response = Client::new()
        .post(format!("{}/goals", input.service_url.trim_end_matches('/')))
        .bearer_auth(&input.service_token)
        .header("idempotency-key", "synthetic-ui-goal")
        .json(
            &json!({"description":"合成 UI 作用域","success_criterion":"读取真实夹具记录",
            "next_step":"检查目录","stop_condition":"工程验证结束"}),
        )
        .send()
        .await
        .unwrap();
    assert!(response.status().is_success());
    let goal: Value = response.json().await.unwrap();
    let goal_id: Uuid = serde_json::from_value(goal["goal_id"].clone()).unwrap();
    let task = s
        .submit_message("synthetic-ui-task", "合成 task 作用域，禁止云调用", None)
        .await
        .unwrap();
    let task_scope = StrategyScope::Task(task.operation_id);
    let task_value = json!({"scope":task_scope,"description":"仅此合成任务先核查来源"});
    let p = propose(
        &s,
        &pool,
        &draft(
            message(&s).await,
            DevelopmentTarget::TaskStrategy,
            task_value.clone(),
        ),
    )
    .await;
    let t = propose(
        &s,
        &pool,
        &draft(
            message(&s).await,
            DevelopmentTarget::TaskStrategy,
            task_value.clone(),
        ),
    )
    .await;
    trial(&s, &pool, t.development_id).await;
    for outcome in [Outcome::Failed, Outcome::Unverified] {
        let r = propose(
            &s,
            &pool,
            &draft(
                message(&s).await,
                DevelopmentTarget::TaskStrategy,
                task_value.clone(),
            ),
        )
        .await;
        trial(&s, &pool, r.development_id).await;
        let cmd = review(&pool, r.development_id, verifier, outcome).await;
        transition(&s, &pool, r.development_id, &cmd).await;
        if outcome == Outcome::Failed {
            transition(
                &s,
                &pool,
                r.development_id,
                &DevelopmentCommand::Reject {
                    reason: "合成失败必须保留".into(),
                },
            )
            .await;
        }
    }
    let mut expiring = draft(
        message(&s).await,
        DevelopmentTarget::TaskStrategy,
        task_value.clone(),
    );
    expiring.deadline = Utc::now() + Duration::seconds(2);
    let e = propose(&s, &pool, &expiring).await;
    tokio::time::sleep(std::time::Duration::from_secs(3)).await;
    transition(&s, &pool, e.development_id, &DevelopmentCommand::Expire).await;
    let r = accepted(
        &s,
        &pool,
        &draft(
            message(&s).await,
            DevelopmentTarget::TaskStrategy,
            task_value,
        ),
        verifier,
    )
    .await;
    transition(
        &s,
        &pool,
        r.development_id,
        &DevelopmentCommand::Revert {
            reason: "合成真实撤回".into(),
        },
    )
    .await;
    let mut old = draft(message(&s).await, DevelopmentTarget::Traits, traits(0.61));
    accepted(&s, &pool, &old, verifier).await;
    old = draft(message(&s).await, DevelopmentTarget::Traits, traits(0.73));
    old.before_version = 2;
    let mut role_receipts = vec![accepted(&s, &pool, &old, verifier).await];
    let current = s.snapshot().await.unwrap().unwrap();
    let mut values = current.persona["personality"]["values"].clone();
    values[0]["priority"] = json!(0.83);
    let mut expression = current.persona["expression"].clone();
    expression["tone"] = json!("合成工程场景的清晰表达");
    let preferences = json!(["合成工程场景先核查来源与反证"]);
    for (target, value) in [
        (DevelopmentTarget::Values, values),
        (DevelopmentTarget::Expression, expression),
        (DevelopmentTarget::CognitivePreferences, preferences),
        (
            DevelopmentTarget::TaskStrategy,
            json!({"scope":StrategyScope::Goal(goal_id),"description":"此合成goal核查来源"}),
        ),
    ] {
        role_receipts.push(
            accepted(
                &s,
                &pool,
                &draft(message(&s).await, target, value),
                verifier,
            )
            .await,
        );
    }
    let catalog = s.developments(true, 100).await.unwrap();
    assert_eq!(catalog["has_more"], false);
    let items = catalog["items"].as_array().unwrap();
    assert!(items.len() <= 100);
    for status in [
        "proposed", "trial", "accepted", "rejected", "expired", "reverted",
    ] {
        assert!(items.iter().any(|v| v["status"] == status));
    }
    let roles: Vec<Value> = items
        .iter()
        .filter(|v| {
            role_receipts
                .iter()
                .any(|r| v["id"] == r.development_id.to_string())
        })
        .map(|v| {
            json!({"development_id":v["id"],"target_id":v["target_id"],
        "head_id":v["current_target_head"],"evidence_ids":v["evidence_ids"],"version":v["version"]})
        })
        .collect();
    assert_eq!(roles.len(), 5);
    assert_ne!(roles[0]["version"], catalog["state_version"]);
    let reservations_after: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(reservations_before, reservations_after);
    assert_eq!(
        s.snapshot().await.unwrap().unwrap().runtime_config["autonomy"]["enabled"],
        false
    );
    json!({"format":"alia-development-ui-fixture/1","synthetic":true,
        "production_verifier":false,"formal_acceptance":null,"individual_id":state.id,
        "database":database,"items":items.len(),"roles_order":"commit,lost200,rollback,source_removed,survivor",
        "roles":roles,"goal_id":goal_id,"task_id":task.operation_id,
        "proposed_id":p.development_id,"reservations_before":reservations_before,
        "reservations_after":reservations_after,"cleanup_owner":"parent"})
}

#[tokio::main]
async fn main() {
    // Underlying errors may contain connection strings; only the wrapper sees
    // fixed failure text. Never print a panic payload or a provider/DB error.
    std::panic::set_hook(Box::new(|info| {
        eprintln!(
            "synthetic fixture failed at line {}",
            info.location().map_or(0, |l| l.line())
        )
    }));
    let input: Input =
        serde_json::from_str(&std::env::var("ALIA_DEVELOPMENT_UI_PRIVATE").unwrap()).unwrap();
    let report = build_catalog(input).await;
    println!("{}", report);
}
