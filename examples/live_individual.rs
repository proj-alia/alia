//! Bounded live test, synthetic data and a disposable database only.
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

#[path = "support/lifecycle.rs"]
mod lifecycle;
#[path = "support/observed_provider.rs"]
mod observed_provider;

async fn execute() -> Result<(), &'static str> {
    let url = std::env::var("DATABASE_URL").map_err(|_| "missing test database")?;
    let parsed = reqwest::Url::parse(&url).map_err(|_| "invalid test database")?;
    if !parsed.path().starts_with("/alia_live_test_") || parsed.host_str() != Some("127.0.0.1") {
        return Err("refusing non-test database");
    }
    let phase = std::env::var("ALIA_LIVE_PHASE").map_err(|_| "missing phase")?;
    if !["first", "first-study", "restore", "unknown"].contains(&phase.as_str()) {
        return Err("invalid phase");
    }
    let pool = connect_database(&url)
        .await
        .map_err(|_| "test database unavailable")?;
    let store = Store::new(pool.clone());
    store.migrate().await.map_err(|_| "test migration failed")?;
    if phase == "first" || phase == "first-study" {
        let persona = PersonaSeed::parse(include_bytes!("../config/persona.example.json"))
            .map_err(|_| "seed invalid")?;
        let mut raw: Value =
            serde_json::from_slice(include_bytes!("../config/runtime.example.json"))
                .map_err(|_| "rules invalid")?;
        if phase == "first-study" {
            raw["autonomy"]["idle_check_minutes"] = json!(5);
        }
        let rules = RuntimeRules::parse(
            &serde_json::to_vec(&raw).map_err(|_| "rules invalid")?,
            IMPLEMENTED_CAPABILITIES,
        )
        .map_err(|_| "rules invalid")?;
        store
            .initialize("live-first", &persona, &rules)
            .await
            .map_err(|_| "initialization failed")?;
    }
    if phase == "first-study" {
        store
            .import_document(
                "source",
                "紫杉手账：保存方式的限定资料",
                lifecycle::SOURCE,
                "synthetic fixture facts, live model",
            )
            .await
            .map_err(|_| "import failed")?;
    }
    let provider = ChatProvider::from_env().map_err(|_| "live provider unavailable")?;
    if !matches!(provider.status().readiness, ModelReadiness::Ready) {
        return Err("live budget profile unavailable");
    }
    let input = match phase.as_str() {
        "first-study" => {
            "这是隔离的开发验收，不是生产共同经历。我一般喜欢先研究再行动；我们的项目叫紫杉手账，是本地读书笔记工具。请先实际阅读刚导入的保存方式资料，简短告诉我其中一个发现，并记住我们的项目和一般偏好。我还不太分得清原子可见性与断电持久性；如果你认为值得进一步比较直接覆写和临时文件替换，可以保留一个后续研究目标，自主整理有来源的建议和限制。本轮先完成阅读并回应，深入研究留到之后；不要把比较分析缩小成逐字摘录，也不能声称已做实际断电实验。"
        }
        "first" => {
            "这是隔离的开发验收，不是生产用户经历。我一般喜欢先研究再行动。我们的试验项目叫紫杉手账，计划做一个本地读书笔记工具。请记住一般偏好和项目名称，简单回应即可；这次不需要搜索或新建目标。"
        }
        "restore" => {
            "请回忆我们试验项目的名字，以及我的一般做事偏好。这次是局部例外：请先给读书笔记工具做一个最简原型方案，不要先做完整研究。不要把这次要求永久覆盖我的一般偏好。"
        }
        _ => "上次我们一起去海边露营时，我给你取的绰号是什么？没有记录就说不知道，不要猜。",
    };
    let submitted = store
        .submit_message(&format!("live-{phase}"), input, Some(uuid::Uuid::new_v4()))
        .await
        .map_err(|_| "message failed")?;
    let input_accounting = provider.input_accounting();
    let provider = Arc::new(observed_provider::ObservedProvider::new(provider));
    let worker = Worker::new(
        store,
        provider.clone(),
        Arc::new(HttpWebReader),
        Arc::new(Notify::new()),
    );
    let result = worker
        .run_once()
        .await
        .map_err(|_| "worker storage failure")?;
    let replies:Vec<Value> = sqlx::query_scalar("SELECT payload FROM events WHERE kind='message.sent' AND correlation_id=$1 ORDER BY sequence")
        .bind(submitted.operation_id).fetch_all(&pool).await.map_err(|_| "reply report failed")?;
    let memory:Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'version',version,'content',content,'scope',scope,'validity',validity,'sources',(SELECT jsonb_agg(event_id) FROM memory_sources WHERE memory_id=memories.id AND individual_id=memories.individual_id)) FROM memories ORDER BY created_at")
        .fetch_all(&pool).await.map_err(|_| "memory report failed")?;
    let usage:Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('reserved',reserved_tokens,'input_bound',input_bound,'output_cap',output_cap,'input',actual_input,'output',actual_output,'charged',charged_tokens,'unknown',usage_unknown,'status',status) FROM usage_reservations ORDER BY created_at")
        .fetch_all(&pool).await.map_err(|_| "usage report failed")?;
    let individual: (String, bool) =
        sqlx::query_as("SELECT id::text,budget_frozen FROM individuals")
            .fetch_one(&pool)
            .await
            .map_err(|_| "identity report failed")?;
    let goals: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',id,'description',description,'criterion',success_criterion,'status',status,'completion',completion) FROM goals ORDER BY created_at").fetch_all(&pool).await.map_err(|_| "goal report")?;
    let observations: Vec<Value> =
        sqlx::query_scalar("SELECT payload FROM events WHERE kind='tool.result' ORDER BY sequence")
            .fetch_all(&pool)
            .await
            .map_err(|_| "observations report")?;
    let models: Vec<String> = sqlx::query_scalar(
        "SELECT DISTINCT payload->>'provider_model' FROM events WHERE kind='decision.accepted'",
    )
    .fetch_all(&pool)
    .await
    .map_err(|_| "model report failed")?;
    let diagnostics: Vec<Value> = sqlx::query_scalar("SELECT jsonb_build_object('event_id',id,'kind',kind,'payload',payload) FROM events WHERE kind IN ('run.retry_scheduled','run.stopped','decision.accepted','decision.reviewed','decision.review_retry_scheduled') ORDER BY sequence")
        .fetch_all(&pool).await.map_err(|_| "diagnostic report failed")?;
    println!(
        "{}",
        json!({"input_accounting":input_accounting,"phase":phase,"individual_id":individual.0,"budget_frozen":individual.1,"worker":format!("{result:?}"),"replies":replies,"memories":memory,"usage":usage,"models":models,"goals":goals,"observations":observations,"diagnostics":diagnostics,"author_candidates":provider.candidates(),"review_diagnostics":provider.reviews(),"data":"synthetic only; parsed author candidates are diagnostic proposals, not personal history"})
    );
    if !matches!(result, WorkerOutcome::Committed(id) if id==submitted.operation_id)
        || replies.is_empty()
        || individual.1
    {
        return Err("live cycle did not commit");
    }
    for record in usage {
        if record["unknown"] != false
            || record["status"] != "settled"
            || record["input"].as_u64().is_none()
            || record["output"].as_u64().is_none()
            || record["input"].as_u64() > record["input_bound"].as_u64()
            || record["output"].as_u64() > record["output_cap"].as_u64()
        {
            return Err("live usage did not satisfy reviewed bound");
        }
    }
    pool.close().await;
    Ok(())
}
#[tokio::main]
async fn main() {
    if let Err(code) = execute().await {
        eprintln!("Live isolated verification failed: {code}");
        std::process::exit(1);
    }
}
