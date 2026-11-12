//! Synthetic, bounded, real-provider autonomous research; never marks goals done.
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
use std::{
    sync::Arc,
    time::{Duration, Instant},
};
use tokio::sync::Notify;
#[path = "support/lifecycle.rs"]
mod lifecycle;
use lifecycle::SOURCE;
// An unresolved design tradeoff, not an instruction to the individual.
pub const DISCOVERY_GAP_SOURCE: &str = "隔离研究场景的资料，不是已执行的真实文件系统实验。紫杉手账是一款计划中的单机读书笔记工具，读者的手写笔记难以重建，目标平台尚未确定。设计记录中有一个未解决的分歧：甲主张直接覆写单个JSON文件，认为实现最简单；乙主张同目录临时文件加原子重命名，认为这样就足以保证断电后的数据安全。已知限定事实：直接覆写被打断时可能留下半份内容；同一文件系统的原子重命名只保证读者看到旧或新文件，原子可见性不等于断电持久性。替换前同步临时文件、替换后同步父目录与断电持久性有关，具体保障还依赖操作系统、文件系统和设备。没有性能测试或断电实验数据，跨文件系统的移动不能假定原子。未决问题是：两种说法各自遗漏了什么，这些已知事实能支持怎样的条件性选择，还有哪些问题必须等待平台与实验资料才能判断。";

const QUESTION: &str = "这是隔离的开发验收，项目叫紫杉手账，不是生产共同经历。我们准备为本地读书笔记选择保存方式。我还分不清原子可见性和断电持久性，刚导入的资料值得研究。若你认为这个问题值得延续，请形成一个后续研究目标：比较直接覆写与临时文件替换，说明原子性与持久性的差别，保存有来源的建议及限制。现在先简短回应，随后自主推进。我不会继续发消息。保留这个分析目标，不能将其改成逐字摘录，也不能声称做过实际断电实验。";
async fn execute() -> Result<(), &'static str> {
    let url = std::env::var("DATABASE_URL").map_err(|_| "missing test database")?;
    let parsed = reqwest::Url::parse(&url).map_err(|_| "invalid test database")?;
    if !parsed.path().starts_with("/alia_live_test_") || parsed.host_str() != Some("127.0.0.1") {
        return Err("refusing production database");
    }
    let pool = connect_database(&url)
        .await
        .map_err(|_| "test database unavailable")?;
    let store = Store::new(pool.clone());
    store.migrate().await.map_err(|_| "migration failed")?;
    let phase = std::env::var("ALIA_LIVE_PHASE").unwrap_or_else(|_| "standalone".into());
    let source = match std::env::var("ALIA_LIVE_SOURCE_SCENARIO").as_deref() {
        Ok("gap") if phase == "discover" => DISCOVERY_GAP_SOURCE,
        Ok("facts") | Err(_) => SOURCE,
        _ => return Err("invalid source scenario"),
    };
    match phase.as_str() {
        "standalone" | "discover" => {
            let mut raw: Value =
                serde_json::from_slice(include_bytes!("../config/runtime.example.json"))
                    .map_err(|_| "runtime parse")?;
            raw["autonomy"]["idle_check_minutes"] = json!(5);
            let rules =
                RuntimeRules::parse(&serde_json::to_vec(&raw).unwrap(), IMPLEMENTED_CAPABILITIES)
                    .map_err(|_| "runtime invalid")?;
            let persona = PersonaSeed::parse(include_bytes!("../config/persona.example.json"))
                .map_err(|_| "persona invalid")?;
            store
                .initialize("init", &persona, &rules)
                .await
                .map_err(|_| "init failed")?;
            store
                .import_document(
                    "source",
                    "紫杉手账：保存方式的限定资料",
                    source,
                    "synthetic fixture facts, live model",
                )
                .await
                .map_err(|_| "import failed")?;
            if phase == "standalone" {
                store
                    .submit_message("question", QUESTION, Some(uuid::Uuid::new_v4()))
                    .await
                    .map_err(|_| "input failed")?;
            }
        }
        "observe" => {
            if store
                .snapshot()
                .await
                .map_err(|_| "snapshot failed")?
                .is_none()
            {
                return Err("existing individual required");
            }
        }
        _ => return Err("invalid phase"),
    }
    let provider = ChatProvider::from_env().map_err(|_| "model config")?;
    if !matches!(provider.status().readiness, ModelReadiness::Ready) {
        return Err("model not ready");
    }
    let input_accounting = provider.input_accounting();
    let worker = Worker::new(
        store,
        Arc::new(provider),
        Arc::new(HttpWebReader),
        Arc::new(Notify::new()),
    );
    let start = Instant::now();
    let mut outcomes = vec![];
    let mut engine_success = false;
    for cycle in 0..24 {
        if start.elapsed() > Duration::from_secs(900) {
            break;
        }
        let outcome = worker
            .run_once()
            .await
            .map_err(|_| "worker storage failure")?;
        eprintln!("autonomy cycle {cycle}: {outcome:?}");
        outcomes.push(format!("{outcome:?}"));
        let states: Vec<String> =
            sqlx::query_scalar("SELECT status FROM goals ORDER BY created_at")
                .fetch_all(&pool)
                .await
                .map_err(|_| "goal states")?;
        if !states.is_empty() && states.iter().all(|s| s == "completed") {
            engine_success = true;
            break;
        }
        if states.is_empty() || states.iter().any(|s| s == "waiting" || s == "abandoned") {
            break;
        }
        if matches!(outcome, WorkerOutcome::Idle | WorkerOutcome::Stopped(_, _)) {
            let delay = worker.store.idle_delay().await.map_err(|_| "idle time")?;
            // Respect real scheduler dates. There is no test clock or status mutation.
            tokio::time::sleep(delay.min(Duration::from_secs(60))).await;
        }
    }
    let goals:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'description',description,'criterion',success_criterion,'completion',completion,'status',status,'waiting_reason',waiting_reason,'review_revision',review_revision) FROM goals ORDER BY created_at").fetch_all(&pool).await.map_err(|_|"goals report")?;
    let notes:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('action_id',action_id,'title',title,'body',body,'citations',citations) FROM notes ORDER BY created_at").fetch_all(&pool).await.map_err(|_|"notes report")?;
    let reviews: Vec<Value> = sqlx::query_scalar(
        "SELECT e.payload FROM events e WHERE kind='goal.reviewed' ORDER BY sequence",
    )
    .fetch_all(&pool)
    .await
    .map_err(|_| "reviews report")?;
    let events:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'kind',kind,'source_kind',source_kind,'payload',payload,'correlation_id',correlation_id) FROM events ORDER BY sequence").fetch_all(&pool).await.map_err(|_|"events report")?;
    let usage:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('run_id',run_id,'input_bound',input_bound,'output_cap',output_cap,'input',actual_input,'output',actual_output,'charged',charged_tokens,'unknown',usage_unknown,'status',status) FROM usage_reservations ORDER BY created_at").fetch_all(&pool).await.map_err(|_|"usage report")?;
    let frozen: bool = sqlx::query_scalar("SELECT budget_frozen FROM individuals")
        .fetch_one(&pool)
        .await
        .map_err(|_| "budget report")?;
    let calls:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('mode',mode,'model_calls',model_calls,'tool_calls',tool_calls,'status',status,'error_code',error_code) FROM runs ORDER BY created_at").fetch_all(&pool).await.map_err(|_|"calls report")?;
    let accepted = reviews
        .iter()
        .any(|r| r["assessment"]["verdict"] == "met" && r["provider_mode"] == "live");
    let usage_ok = usage.iter().all(|u| {
        u["status"] == "settled"
            && u["unknown"] == false
            && u["input"].as_u64().is_some()
            && u["input"].as_u64() <= u["input_bound"].as_u64()
            && u["output"].as_u64() <= u["output_cap"].as_u64()
    });
    let individual_id: String = sqlx::query_scalar("SELECT id::text FROM individuals")
        .fetch_one(&pool)
        .await
        .map_err(|_| "identity report")?;
    let success = engine_success && accepted && !notes.is_empty() && !frozen && usage_ok;
    println!(
        "{}",
        json!({"input_accounting":input_accounting,"individual_id":individual_id,"phase":phase,"data":"synthetic only; real provider; real scheduling time","question":if phase=="standalone"{Some(QUESTION)}else{None},"source":source,"outcomes":outcomes,"goals":goals,"notes":notes,"reviews":reviews,"events":events,"usage":usage,"calls":calls,"budget_frozen":frozen,"engine_pass":success,"behavior_review":"pending","elapsed_seconds":start.elapsed().as_secs()})
    );
    pool.close().await;
    if success {
        Ok(())
    } else {
        Err("autonomous completion not demonstrated")
    }
}
#[tokio::main]
async fn main() {
    if let Err(code) = execute().await {
        eprintln!("Live isolated verification failed: {code}");
        std::process::exit(1)
    }
}
