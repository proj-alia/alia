//! Actual TCP HTTP + isolated PostgreSQL; synthetic rows, no worker/provider.
use alia::{runtime::Runtime, service};
use chrono::{DateTime, Duration, Utc};
use reqwest::{Client, StatusCode};
use serde_json::{Value, json};
use sqlx::PgPool;
use std::{path::PathBuf, time::Duration as StdDuration};
use uuid::Uuid;
const TOKEN: &str = "overview-isolated-http-test-token-long-enough";
struct Server {
    base: String,
    client: Client,
    task: tokio::task::JoinHandle<()>,
}
impl Drop for Server {
    fn drop(&mut self) {
        self.task.abort();
    }
}
impl Server {
    async fn get(&self, path: &str) -> (StatusCode, Value) {
        let response = self
            .client
            .get(format!("{}{path}", self.base))
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap();
        assert_eq!(response.headers()["cache-control"], "no-store");
        let status = response.status();
        (status, response.json().await.unwrap())
    }
    async fn overview(&self) -> Value {
        let (s, v) = self.get("/api/v1/overview").await;
        assert_eq!(s, StatusCode::OK, "{v}");
        v
    }
    async fn init(&self) {
        let response=self.client.post(format!("{}/api/v1/initialize",self.base)).bearer_auth(TOKEN).header("Idempotency-Key","overview-birth").json(&json!({
            "persona":serde_json::from_str::<Value>(include_str!("../config/persona.example.json")).unwrap(),
            "runtime":serde_json::from_str::<Value>(include_str!("../config/runtime.example.json")).unwrap()
        })).send().await.unwrap();
        assert_eq!(response.status(), StatusCode::CREATED);
    }
}
async fn server(pool: &PgPool) -> Server {
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let base = format!("http://{}", listener.local_addr().unwrap());
    let router = service::router(
        Runtime::new(pool.clone(), TOKEN).unwrap(),
        PathBuf::from("web/dist"),
    );
    let task = tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    Server {
        base,
        client: Client::new(),
        task,
    }
}
async fn seed(pool: &PgPool, number: i32) {
    sqlx::query(r#"DO $$ DECLARE ind uuid; e uuid; m uuid; g uuid; r uuid; n integer; BEGIN
        SELECT id INTO ind FROM individuals;
        FOR n IN 0..$seed_last LOOP e:=gen_random_uuid();m:=gen_random_uuid();g:=gen_random_uuid();r:=gen_random_uuid();
          INSERT INTO objects(individual_id,id,kind) VALUES(ind,e,'event'),(ind,m,'memory'),(ind,g,'goal'),(ind,r,'run');
          INSERT INTO events(individual_id,id,kind,source_kind,actor_id,occurred_at,payload,correlation_id)
            VALUES(ind,e,'overview.fixture','reported_claim','fixture-'||n,clock_timestamp(),'{"text":"overview-private-body-marker"}',r);
          INSERT INTO memories(individual_id,id,kind,content,scope,validity)
            VALUES(ind,m,'semantic','overview-private-memory-marker','general',(ARRAY['active','disputed','superseded','stale'])[n%4+1]);
          INSERT INTO memory_sources(individual_id,memory_id,event_id) VALUES(ind,m,e);
          INSERT INTO goals(individual_id,id,origin_type,description,success_criterion,next_step,stop_condition,status)
            VALUES(ind,g,'event','synthetic goal '||n,'synthetic criterion','synthetic next','synthetic stop',(ARRAY['proposed','active','waiting','completed','abandoned'])[n%5+1]);
          INSERT INTO runs(individual_id,id,trigger_event_id,mode,state_version,privacy_epoch,config_version,status,created_at,finished_at,error_code)
            VALUES(ind,r,e,CASE WHEN n%2=0 THEN 'background' ELSE 'foreground' END,1,0,1,(ARRAY['failed','committed','cancelled','stale'])[n%4+1],
                (date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC')+n*interval '1 second',NULL,
                CASE WHEN n=64 THEN 'private-raw-provider-error-marker' ELSE 'model_failure' END);
          INSERT INTO derivation_edges(individual_id,source_id,target_id) VALUES(ind,e,m),(ind,e,g),(ind,e,r);
        END LOOP;END $$;"#.replace("$seed_last",&(number-1).to_string()).as_str()).execute(pool).await.unwrap();
}
async fn invariant(pool: &PgPool) -> Value {
    sqlx::query_scalar("SELECT jsonb_build_object('state',state_version,'epoch',privacy_epoch,'config',config_version,'events',(SELECT count(*) FROM events),'usage',(SELECT count(*) FROM usage_reservations),'objects',(SELECT count(*) FROM objects)) FROM individuals")
        .fetch_one(pool).await.unwrap()
}
#[sqlx::test]
#[ignore = "isolated PostgreSQL and TCP HTTP; P02-02 locked runner"]
async fn overview_uninitialized_empty_strict_queries_auth_and_read_only(pool: PgPool) {
    let s = server(&pool).await;
    let (code, v) = s.get("/api/v1/overview").await;
    assert_eq!(code, StatusCode::CONFLICT);
    assert_eq!(v["error"]["code"], "not_initialized");
    s.init().await;
    let before = invariant(&pool).await;
    let v = s.overview().await;
    assert_eq!(v["scope"], "full_visible_metadata");
    assert_eq!(v["state_version"], 1);
    assert_eq!(v["privacy_epoch"], 0);
    for (name, keys) in [
        (
            "memories",
            vec!["total", "active", "disputed", "superseded", "stale"],
        ),
        (
            "goals",
            vec![
                "total",
                "proposed",
                "active",
                "waiting",
                "completed",
                "abandoned",
            ],
        ),
        (
            "runs",
            vec![
                "total",
                "queued",
                "running",
                "committed",
                "failed",
                "cancelled",
                "stale",
            ],
        ),
    ] {
        assert_eq!(v["totals"][name].as_object().unwrap().len(), keys.len());
        for key in keys {
            assert_eq!(v["totals"][name][key], 0);
        }
    }
    let days = v["trend"]["days"].as_array().unwrap();
    assert_eq!(days.len(), 7);
    assert_eq!(v["trend"]["timezone"], "UTC");
    assert_eq!(v["trend"]["group_by"], "created_at");
    let observed: DateTime<Utc> = serde_json::from_value(v["observed_at"].clone()).unwrap();
    for (i, day) in days.iter().enumerate() {
        assert_eq!(
            day["date"],
            (observed.date_naive() - Duration::days(6 - i as i64)).to_string()
        );
        assert_eq!(day["partial"], i == 6);
        for key in ["created", "failed", "foreground", "background"] {
            assert_eq!(day[key], 0);
        }
    }
    assert_eq!(v["recent_failed_runs"], json!([]));
    for path in [
        "/api/v1/overview?q=",
        "/api/v1/overview?limit=1",
        "/api/v1/overview?q=x&q=y",
        "/api/v1/overview?x",
        "/api/v1/overview?from=x&from=y",
    ] {
        assert_eq!(s.get(path).await.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
    assert_eq!(s.get("/api/v1/overview?").await.0, StatusCode::OK);
    assert_eq!(
        s.client
            .get(format!("{}/api/v1/overview", s.base))
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::UNAUTHORIZED
    );
    assert_eq!(
        s.client
            .get(format!("{}/api/v1/overview", s.base))
            .bearer_auth(TOKEN)
            .header("Origin", "https://foreign.invalid")
            .send()
            .await
            .unwrap()
            .status(),
        StatusCode::FORBIDDEN
    );
    assert_eq!(
        before,
        invariant(&pool).await,
        "public overview must not mutate state/events/usage"
    );
    sqlx::query("ALTER TABLE memories RENAME TO overview_unavailable_fixture")
        .execute(&pool)
        .await
        .unwrap();
    let (code, error) = s.get("/api/v1/overview").await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE);
    assert!(error.get("totals").is_none());
    assert!(error.get("error").is_some());
    assert!(!error.to_string().contains("overview_unavailable_fixture"));
}
#[sqlx::test]
#[ignore = "isolated PostgreSQL and TCP HTTP; P02-02 locked runner"]
async fn overview_counts_beyond_page_limit_and_projects_only_safe_recent_failures(pool: PgPool) {
    let s = server(&pool).await;
    s.init().await;
    seed(&pool, 65).await;
    let before = invariant(&pool).await;
    let v = s.overview().await;
    for resource in ["memories", "goals", "runs"] {
        assert_eq!(v["totals"][resource]["total"], 65);
        let (_, page) = s.get(&format!("/api/v1/{resource}")).await;
        assert_eq!(page["items"].as_array().unwrap().len(), 30);
        assert!(page["next_cursor"].is_string());
    }
    assert_eq!(
        v["totals"]["memories"],
        json!({"total":65,"active":17,"disputed":16,"superseded":16,"stale":16})
    );
    assert_eq!(
        v["totals"]["goals"],
        json!({"total":65,"proposed":13,"active":13,"waiting":13,"completed":13,"abandoned":13})
    );
    assert_eq!(
        v["totals"]["runs"],
        json!({"total":65,"failed":17,"committed":16,"cancelled":16,"stale":16,"queued":0,"running":0})
    );
    assert_eq!(v["trend"]["days"][6]["created"], 65);
    assert_eq!(v["trend"]["days"][6]["failed"], 17);
    let expected: Vec<Uuid> = sqlx::query_scalar(
        "SELECT id FROM runs WHERE status='failed' ORDER BY created_at DESC,id DESC LIMIT 5",
    )
    .fetch_all(&pool)
    .await
    .unwrap();
    let recent = v["recent_failed_runs"].as_array().unwrap();
    assert_eq!(recent.len(), 5);
    for (i, r) in recent.iter().enumerate() {
        assert_eq!(r["id"], expected[i].to_string());
        assert_eq!(r.as_object().unwrap().len(), 5);
        assert!(r["finished_at"].is_null());
    }
    assert_eq!(recent[0]["error_code"], "unclassified_failure");
    for secret in [
        "overview-private-body-marker",
        "overview-private-memory-marker",
        "private-raw-provider-error-marker",
        TOKEN,
    ] {
        assert!(!v.to_string().contains(secret));
    }
    // Same created_at still uses UUID as a deterministic recent-failure tiebreaker.
    sqlx::query("UPDATE runs SET created_at=date_trunc('day',clock_timestamp() AT TIME ZONE 'UTC') AT TIME ZONE 'UTC'").execute(&pool).await.unwrap();
    let sorted: Vec<Uuid> =
        sqlx::query_scalar("SELECT id FROM runs WHERE status='failed' ORDER BY id DESC LIMIT 5")
            .fetch_all(&pool)
            .await
            .unwrap();
    let tied = s.overview().await;
    for (i, r) in tied["recent_failed_runs"]
        .as_array()
        .unwrap()
        .iter()
        .enumerate()
    {
        assert_eq!(r["id"], sorted[i].to_string());
    }
    assert_eq!(before, invariant(&pool).await);
}
#[sqlx::test]
#[ignore = "isolated PostgreSQL and TCP HTTP; P02-02 locked runner"]
async fn overview_seven_utc_days_have_precise_boundaries_and_current_failed_status(pool: PgPool) {
    let s = server(&pool).await;
    s.init().await;
    seed(&pool, 6).await;
    let initial = s.overview().await;
    let observed: DateTime<Utc> = serde_json::from_value(initial["observed_at"].clone()).unwrap();
    let today = observed
        .date_naive()
        .and_hms_opt(0, 0, 0)
        .unwrap()
        .and_utc();
    let ids: Vec<Uuid> = sqlx::query_scalar("SELECT id FROM runs ORDER BY created_at,id")
        .fetch_all(&pool)
        .await
        .unwrap();
    let points = [
        today - Duration::days(6),
        today - Duration::days(6) - Duration::microseconds(1),
        today - Duration::microseconds(1),
        today,
        today + Duration::days(1),
        today - Duration::days(5) - Duration::microseconds(1),
    ];
    for (i, id) in ids.iter().enumerate() {
        sqlx::query("UPDATE runs SET created_at=$2,status='failed',mode=$3 WHERE id=$1")
            .bind(id)
            .bind(points[i])
            .bind(if i % 2 == 0 {
                "foreground"
            } else {
                "background"
            })
            .execute(&pool)
            .await
            .unwrap();
    }
    let v = s.overview().await;
    let days = v["trend"]["days"].as_array().unwrap();
    assert_eq!(days[0]["created"], 2);
    assert_eq!(days[5]["created"], 1);
    assert_eq!(days[6]["created"], 1);
    assert_eq!(
        days.iter()
            .map(|d| d["created"].as_i64().unwrap())
            .sum::<i64>(),
        4
    );
    for day in days {
        let n = day["created"].as_i64().unwrap();
        assert_eq!(day["failed"], n);
        assert_eq!(
            day["foreground"].as_i64().unwrap() + day["background"].as_i64().unwrap(),
            n
        );
    }
    assert_eq!(
        v["totals"]["runs"]["total"], 6,
        "whole history is independent of trend window"
    );
    sqlx::query("UPDATE runs SET status='committed' WHERE id=$1")
        .bind(ids[0])
        .execute(&pool)
        .await
        .unwrap();
    let v = s.overview().await;
    assert_eq!(v["trend"]["days"][0]["created"], 2);
    assert_eq!(
        v["trend"]["days"][0]["failed"], 1,
        "failed buckets use current status, not finished_at"
    );
}
#[sqlx::test]
#[ignore = "isolated PostgreSQL and TCP HTTP; P02-02 locked runner"]
async fn overview_recursive_ancestor_blocking_hides_counts_trend_and_failure_ids(pool: PgPool) {
    let s = server(&pool).await;
    s.init().await;
    seed(&pool, 65).await;
    let before = s.overview().await;
    let (run, source): (Uuid, Uuid) = sqlx::query_as(
        "SELECT id,trigger_event_id FROM runs ORDER BY created_at DESC,id DESC LIMIT 1",
    )
    .fetch_one(&pool)
    .await
    .unwrap();
    let ancestor = Uuid::new_v4();
    let middle = Uuid::new_v4();
    sqlx::query("INSERT INTO objects(individual_id,id,kind,blocked) SELECT individual_id,$2,'event',true FROM objects WHERE id=$1 UNION ALL SELECT individual_id,$3,'event',false FROM objects WHERE id=$1").bind(source).bind(ancestor).bind(middle).execute(&pool).await.unwrap();
    sqlx::query("INSERT INTO derivation_edges(individual_id,source_id,target_id) SELECT individual_id,$2,$3 FROM objects WHERE id=$1 UNION ALL SELECT individual_id,$3,$1 FROM objects WHERE id=$1").bind(source).bind(ancestor).bind(middle).execute(&pool).await.unwrap();
    let v = s.overview().await;
    for resource in ["memories", "goals", "runs"] {
        assert_eq!(v["totals"][resource]["total"], 64);
    }
    assert_eq!(before["trend"]["days"][6]["created"], 65);
    assert_eq!(v["trend"]["days"][6]["created"], 64);
    assert_eq!(v["totals"]["runs"]["failed"], 16);
    assert!(!v.to_string().contains(&run.to_string()));
    assert!(!v.to_string().contains(&source.to_string()));
    assert_eq!(v["recent_failed_runs"].as_array().unwrap().len(), 5);
    assert_eq!(
        s.get(&format!("/api/v1/runs/{run}")).await.0,
        StatusCode::NOT_FOUND
    );
    sqlx::query("UPDATE objects SET blocked=true WHERE id=(SELECT id FROM individuals)")
        .execute(&pool)
        .await
        .unwrap();
    assert_eq!(s.get("/api/v1/overview").await.0, StatusCode::NOT_FOUND);
}
#[sqlx::test]
#[ignore = "isolated PostgreSQL and TCP HTTP; P02-02 locked runner"]
async fn overview_uses_one_repeatable_read_snapshot_across_aggregates(pool: PgPool) {
    let s = server(&pool).await;
    s.init().await;
    seed(&pool, 1).await;
    let mut lock = pool.begin().await.unwrap();
    sqlx::query("LOCK TABLE memories IN ACCESS EXCLUSIVE MODE")
        .execute(&mut *lock)
        .await
        .unwrap();
    let client = s.client.clone();
    let url = format!("{}/api/v1/overview", s.base);
    let response = tokio::spawn(async move {
        client
            .get(url)
            .bearer_auth(TOKEN)
            .send()
            .await
            .unwrap()
            .json::<Value>()
            .await
            .unwrap()
    });
    let mut blocked = false;
    for _ in 0..100 {
        let waiting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%FROM memories m WHERE%')").fetch_one(&pool).await.unwrap();
        if waiting {
            blocked = true;
            break;
        }
        tokio::time::sleep(StdDuration::from_millis(20)).await;
    }
    assert!(
        blocked,
        "overview reached first aggregate after snapshot establishment"
    );
    sqlx::query("UPDATE runs SET status='committed'")
        .execute(&pool)
        .await
        .unwrap();
    sqlx::query("UPDATE goals SET status='completed'")
        .execute(&pool)
        .await
        .unwrap();
    lock.commit().await.unwrap();
    let v = response.await.unwrap();
    assert_eq!(v["totals"]["runs"]["failed"], 1);
    assert_eq!(v["totals"]["goals"]["proposed"], 1);
    assert_eq!(v["recent_failed_runs"].as_array().unwrap().len(), 1);
    let new = s.overview().await;
    assert_eq!(new["totals"]["runs"]["failed"], 0);
    assert_eq!(new["totals"]["goals"]["completed"], 1);
}
