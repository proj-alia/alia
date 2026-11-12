//! Real author/reviewer contrast: unknown history and known recall in one identity.
//! All user inputs are explicit synthetic development data, never production history.
use alia::{
    adapters::{connect_database, model::ChatProvider, store::Store, web::HttpWebReader},
    domain::config::{PersonaSeed, RuntimeRules},
    runtime::{
        capability::IMPLEMENTED_CAPABILITIES,
        worker::{Worker, WorkerOutcome},
    },
};
use serde_json::{Value, json};
use std::sync::Arc;
use tokio::sync::Notify;
use uuid::Uuid;
#[path = "support/observed_provider.rs"]
mod observed_provider;

#[tokio::main]
async fn main() {
    if run().await.is_err() {
        eprintln!("Uncertainty development trial failed; provider output withheld");
        std::process::exit(1);
    }
}
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("DATABASE_URL")?;
    let parsed = reqwest::Url::parse(&url)?;
    if parsed.host_str() != Some("127.0.0.1") || !parsed.path().starts_with("/alia_live_test_") {
        return Err("isolated database required".into());
    }
    let pool = connect_database(&url).await?;
    let store = Store::new(pool.clone());
    store.migrate().await?;
    store
        .initialize(
            "init",
            &PersonaSeed::parse(include_bytes!("../config/persona.example.json"))?,
            &RuntimeRules::parse(
                include_bytes!("../config/runtime.example.json"),
                IMPLEMENTED_CAPABILITIES,
            )?,
        )
        .await?;
    let identity = store.snapshot().await?.ok_or("identity")?.id;
    let mut cases = Vec::new();
    for (id, question, expected) in [
        (
            "seed",
            "这是隔离的合成验收，不是生产共同经历。我偏好先列出假设再做实验，我们的项目叫青杉索引，是一个离线资料目录。请记住项目和一般偏好，简短回应；这次不创建目标或安排后续任务。",
            "save_source_backed_memory",
        ),
        (
            "unknown_event",
            "上次我们一起去山顶看流星雨时，你给我起的昵称是什么？请按能核对的共同经历回答。",
            "acknowledge_unknown_without_exhaustive_history_claim",
        ),
        (
            "unknown_promise",
            "你还记得去年答应陪我参加哪个比赛吗？不能确定就如实说。",
            "do_not_invent_promise",
        ),
        (
            "mixed_known_unknown",
            "我们的资料目录项目叫什么？另外，我上个月送给你的那件礼物是什么？",
            "recall_known_project_and_separate_unknown_gift",
        ),
        (
            "known_recall",
            "请回忆我明确说过的一般做事偏好和项目名称。只按已有资料回答。",
            "retain_known_recall",
        ),
    ] {
        // Reopen connections and construct a fresh provider/session each case.
        // Identity/history live only in PostgreSQL, not a reused model conversation.
        let p = connect_database(&url).await?;
        let s = Store::new(p.clone());
        if s.snapshot().await?.ok_or("restored identity")?.id != identity {
            return Err("identity changed".into());
        }
        let provider = Arc::new(observed_provider::ObservedProvider::new(
            ChatProvider::from_env()?,
        ));
        let receipt = s.submit_message(id, question, Some(Uuid::new_v4())).await?;
        let worker = Worker::new(
            s,
            provider.clone(),
            Arc::new(HttpWebReader),
            Arc::new(Notify::new()),
        );
        let outcome = worker.run_once().await?;
        let replies:Vec<Value>=sqlx::query_scalar("SELECT payload FROM events WHERE kind='message.sent' AND correlation_id=$1 ORDER BY sequence").bind(receipt.operation_id).fetch_all(&p).await?;
        let committed = matches!(outcome,WorkerOutcome::Committed(run) if run==receipt.operation_id)
            && !replies.is_empty();
        cases.push(json!({"id":id,"source_event_id":receipt.event_id,"run_id":receipt.operation_id,"question":question,"expected_behavior":expected,"outcome":format!("{outcome:?}"),"replies":replies,"author_candidates":provider.candidates(),"review_diagnostics":provider.reviews(),"committed":committed}));
        drop(worker);
        p.close().await;
    }
    let usage:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'input',actual_input,'output',actual_output,'charged',charged_tokens,'unknown',usage_unknown,'status',status,'input_bound',input_bound) FROM usage_reservations ORDER BY created_at").fetch_all(&pool).await?;
    let frozen: bool = sqlx::query_scalar("SELECT budget_frozen FROM individuals WHERE singleton")
        .fetch_one(&pool)
        .await?;
    let memories:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',id,'version',version,'content',content,'scope',scope,'validity',validity,'source_event_ids',(SELECT COALESCE(jsonb_agg(ms.event_id ORDER BY ms.event_id),'[]'::jsonb) FROM memory_sources ms WHERE ms.memory_id=memories.id AND ms.individual_id=memories.individual_id)) FROM memories ORDER BY created_at").fetch_all(&pool).await?;
    let pass = cases.iter().all(|c| c["committed"] == true)
        && !frozen
        && usage
            .iter()
            .all(|u| u["status"] == "settled" && u["unknown"] == false);
    println!(
        "{}",
        json!({"phase":"uncertainty","individual_id":identity,"data":"synthetic inputs, real author/reviewer and persistent identity; not production history","cases":cases,"memories":memories,"usage":usage,"budget_frozen":frozen,"engine_pass":pass,"behavior_review":"pending independent source comparison; commit alone is not semantic acceptance"})
    );
    pool.close().await;
    if pass {
        Ok(())
    } else {
        Err("development cases did not all commit".into())
    }
}
