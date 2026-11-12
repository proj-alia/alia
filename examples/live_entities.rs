//! Independent synthetic entity development case through the real paid worker.
use alia::{
    adapters::{connect_database, model::ChatProvider, store::Store, web::HttpWebReader},
    domain::config::{PersonaSeed, RuntimeRules},
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        model::{ModelProvider, ModelReadiness},
        worker::{Worker, WorkerOutcome},
    },
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Notify;
#[tokio::main]
async fn main() {
    if let Err(code) = run().await {
        eprintln!("Live entity development case failed: {code}");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), &'static str> {
    let url = std::env::var("DATABASE_URL").map_err(|_| "test database required")?;
    let parsed = reqwest::Url::parse(&url).map_err(|_| "test database invalid")?;
    if parsed.host_str() != Some("127.0.0.1") || !parsed.path().starts_with("/alia_live_test_") {
        return Err("refusing production database");
    }
    let pool = connect_database(&url)
        .await
        .map_err(|_| "database unavailable")?;
    let s = Store::new(pool.clone());
    s.migrate().await.map_err(|_| "migration failed")?;
    s.initialize(
        "init",
        &PersonaSeed::parse(include_bytes!("../config/persona.example.json"))
            .map_err(|_| "seed")?,
        &RuntimeRules::parse(
            include_bytes!("../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )
        .map_err(|_| "rules")?,
    )
    .await
    .map_err(|_| "init failed")?;
    let text = "沈遥喜欢红茶。沈瑶喜欢咖啡。这是隔离开发验收的合成资料，不是实际共同经历，两人不同。请分别记住两人的偏好，并用当前支持的人物引用保存主体关联。区间必须来自本条实际文本，不确定则如实说明，不编造来源。";
    s.submit_message("entity-case", text, None)
        .await
        .map_err(|_| "input failed")?;
    let individual = s
        .snapshot()
        .await
        .map_err(|_| "snapshot")?
        .ok_or("identity absent")?
        .id;
    let provider = ChatProvider::from_env().map_err(|_| "model config")?;
    if !matches!(provider.status().readiness, ModelReadiness::Ready) {
        return Err("model not ready");
    }
    let worker = Worker::new(
        s,
        Arc::new(provider),
        Arc::new(HttpWebReader),
        Arc::new(Notify::new()),
    );
    let outcome = worker.run_once().await.map_err(|_| "worker storage")?;
    let people: Vec<Value> = sqlx::query_scalar(
        "SELECT jsonb_build_object('id',id,'label',label) FROM person_entities ORDER BY label",
    )
    .fetch_all(&pool)
    .await
    .map_err(|_| "people report")?;
    let links:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('entity',p.label,'memory',m.content,'role',l.role,'actor',e.actor_id,'source_kind',e.source_kind) FROM memory_person_links l JOIN person_entities p ON (p.individual_id,p.id)=(l.individual_id,l.entity_id) JOIN memories m ON (m.individual_id,m.id)=(l.individual_id,l.memory_id) JOIN events e ON (e.individual_id,e.id)=(l.individual_id,l.assertion_event_id) ORDER BY p.label").fetch_all(&pool).await.map_err(|_|"links report")?;
    let memories: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('content',content,'scope',scope,'version',version) FROM memories ORDER BY created_at")
        .fetch_all(&pool).await.map_err(|_| "memory report")?;
    let usage:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('input',actual_input,'output',actual_output,'input_bound',input_bound,'output_cap',output_cap,'charged',charged_tokens,'status',status,'unknown',usage_unknown) FROM usage_reservations ORDER BY created_at").fetch_all(&pool).await.map_err(|_|"usage report")?;
    let replies: Vec<Value> = sqlx::query_scalar(
        "SELECT payload FROM events WHERE kind='message.sent' ORDER BY sequence",
    )
    .fetch_all(&pool)
    .await
    .map_err(|_| "reply report")?;
    let diagnostics:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('kind',kind,'payload',payload) FROM events WHERE kind IN ('run.retry_scheduled','run.stopped') ORDER BY sequence").fetch_all(&pool).await.map_err(|_|"diagnostic report")?;
    let frozen: bool = sqlx::query_scalar("SELECT budget_frozen FROM individuals")
        .fetch_one(&pool)
        .await
        .map_err(|_| "budget report")?;
    let engine_pass = matches!(outcome, WorkerOutcome::Committed(_))
        && people.len() == 2
        && links.len() == 2
        && links.iter().all(|l| {
            l["actor"] == "alia" && l["source_kind"] == "inference" && l["role"] == "subject"
        })
        && !frozen
        && usage.iter().all(|u| {
            u["status"] == "settled"
                && u["unknown"] == false
                && u["input"].as_u64().is_some()
                && u["input"].as_u64() <= u["input_bound"].as_u64()
                && u["output"].as_u64() <= u["output_cap"].as_u64()
        });
    println!(
        "{}",
        json!({"phase":"entities","individual_id":individual,"data":"synthetic independent development case; real worker and paid model","input":text,"outcome":format!("{outcome:?}"),"people":people,"memories":memories,"links":links,"usage":usage,"replies":replies,"diagnostics":diagnostics,"budget_frozen":frozen,"engine_pass":engine_pass,"behavior_review":"pending"})
    );
    pool.close().await;
    if engine_pass {
        Ok(())
    } else {
        Err("entity engine criteria not met")
    }
}
