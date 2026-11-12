//! Independent synthetic development scenarios; real E5, paid model and normal worker.
//! Controlled owner assertions are fixtures, not evidence of autonomous learning.
#[cfg(feature = "local-embeddings")]
#[tokio::main]
async fn main() {
    if run().await.is_err() {
        eprintln!(
            "Live person retrieval did not satisfy engineering checks; private errors withheld"
        );
        std::process::exit(1);
    }
}
#[cfg(feature = "local-embeddings")]
async fn run() -> Result<(), Box<dyn std::error::Error>> {
    use alia::{
        adapters::{
            connect_database,
            embedding::{CpuMemoryEncoder, LocalEmbedding},
            model::ChatProvider,
            store::Store,
            web::HttpWebReader,
        },
        domain::{
            config::{PersonaSeed, RuntimeRules},
            entities::{MemoryPersonAssertion, PersonRole, SourceSpan},
            history::{MemoryKind, NewMemory},
        },
        runtime::{
            capability::IMPLEMENTED_CAPABILITIES,
            model::{ModelProvider, ModelReadiness},
            worker::{Worker, WorkerOutcome},
        },
    };
    use serde_json::{Value, json};
    use std::sync::Arc;
    use tokio::sync::Notify;
    use uuid::Uuid;
    let url = std::env::var("DATABASE_URL")?;
    let parsed = reqwest::Url::parse(&url)?;
    if parsed.host_str() != Some("127.0.0.1") || !parsed.path().starts_with("/alia_live_test_") {
        return Err("test database required".into());
    }
    let pool = connect_database(&url).await?;
    let s = Store::new(pool.clone());
    s.migrate().await?;
    s.initialize(
        "init",
        &PersonaSeed::parse(include_bytes!("../config/persona.example.json"))?,
        &RuntimeRules::parse(
            include_bytes!("../config/runtime.example.json"),
            IMPLEMENTED_CAPABILITIES,
        )?,
    )
    .await?;
    let mut people = Vec::new();
    for name in ["陆宁", "陆凝"] {
        let source = s
            .import_document(
                &Uuid::new_v4().to_string(),
                "合成人物来源",
                name,
                "independent owner fixture",
            )
            .await?
            .event_id;
        let p = s
            .register_person(
                &Uuid::new_v4().to_string(),
                s.snapshot().await?.ok_or("identity")?.expected(),
                &Uuid::new_v4().to_string(),
                name,
                SourceSpan {
                    event_id: source,
                    start_char: 0,
                    end_char: name.chars().count(),
                },
            )
            .await?
            .entity_id;
        people.push(p);
    }
    for (i, name) in [(0, "宁宁"), (0, "小陆"), (1, "小陆")] {
        let source = s
            .import_document(
                &Uuid::new_v4().to_string(),
                "合成别名来源",
                name,
                "independent owner fixture",
            )
            .await?
            .event_id;
        s.add_person_alias(
            &Uuid::new_v4().to_string(),
            s.snapshot().await?.ok_or("identity")?.expected(),
            people[i],
            SourceSpan {
                event_id: source,
                start_char: 0,
                end_char: name.chars().count(),
            },
        )
        .await?;
    }
    let mut model =
        LocalEmbedding::load(std::path::Path::new("data/models/multilingual-e5-small"))?;
    let mut seeds = Vec::new();
    for (i, text) in [
        (
            0,
            "陆宁晚饭后喜欢去河边散步；陆凝只是这段记录中被提到的朋友。",
        ),
        (
            1,
            "陆凝晚饭后喜欢整理摄影作品；陆宁只是这段记录中被提到的朋友。",
        ),
    ] {
        let source = s
            .import_document(
                &Uuid::new_v4().to_string(),
                "合成活动偏好来源",
                text,
                "independent owner fixture",
            )
            .await?
            .event_id;
        let m = s
            .record_memories(
                &Uuid::new_v4().to_string(),
                s.snapshot().await?.ok_or("identity")?.expected(),
                &[source],
                &[NewMemory {
                    content: text.into(),
                    kind: MemoryKind::Semantic,
                    scope: "general".into(),
                    source_event_ids: vec![source],
                    supersedes: None,
                }],
            )
            .await?
            .memory_ids[0];
        for (j, name) in ["陆宁", "陆凝"].iter().enumerate() {
            let start = text[..text.find(name).ok_or("fixture name")?]
                .chars()
                .count();
            s.assert_memory_person(
                &Uuid::new_v4().to_string(),
                s.snapshot().await?.ok_or("identity")?.expected(),
                MemoryPersonAssertion {
                    memory_id: m,
                    memory_version: 1,
                    entity_id: people[j],
                    role: if i == j {
                        PersonRole::Subject
                    } else {
                        PersonRole::Mentioned
                    },
                    source: SourceSpan {
                        event_id: source,
                        start_char: start,
                        end_char: start + name.chars().count(),
                    },
                    memory_start_char: start,
                },
            )
            .await?;
        }
        let snapshot = s.prepare_memory_index(m).await?;
        s.commit_memory_index(&snapshot, &model.encode_memory(snapshot.content())?)
            .await?;
        seeds.push(json!({"memory_id":m,"source_event_id":source,"subject_id":people[i],"text":text,"recorded_by":"controlled owner fixture"}));
    }
    let s = s.with_memory_encoder(Arc::new(CpuMemoryEncoder::new(model)));
    let provider = ChatProvider::from_env()?;
    if !matches!(provider.status().readiness, ModelReadiness::Ready) {
        return Err("model not ready".into());
    }
    let worker = Worker::new(
        s.clone(),
        Arc::new(provider),
        Arc::new(HttpWebReader),
        Arc::new(Notify::new()),
    );
    let cases = [
        ("exact", "陆宁晚饭后喜欢做什么？请根据已保存档案核实。"),
        ("alias", "宁宁晚饭后通常更喜欢什么活动？"),
        ("near_name", "陆凝晚饭后的爱好是什么？请分清人物。"),
        ("ambiguous", "小陆晚饭后喜欢做什么？"),
        (
            "unknown_longer_name",
            "陆宁安晚饭后喜欢做什么？没有确切记录就如实说明。",
        ),
        (
            "mention_not_subject",
            "那份提到陆凝、并记录河边散步的档案，散步是谁的爱好？",
        ),
    ];
    let mut results = Vec::new();
    let mut engine_pass = true;
    let mut qualified_searches = 0usize;
    for (case, input) in cases {
        let submission = s
            .submit_message(&format!("case-{case}"), input, Some(Uuid::new_v4()))
            .await?;
        let outcome = worker.run_once().await?;
        let replies:Vec<Value>=sqlx::query_scalar("SELECT payload FROM events WHERE kind='message.sent' AND correlation_id=$1 ORDER BY sequence").bind(submission.operation_id).fetch_all(&pool).await?;
        let searches:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('action_id',a.id,'input',a.arguments,'version',a.capability_version,'status',a.status,'result_event_id',a.result_event_id,'result',e.payload) FROM actions a LEFT JOIN events e ON(e.individual_id,e.id)=(a.individual_id,a.result_event_id) WHERE a.run_id=$1 AND a.capability_id='memory.search' ORDER BY a.sequence").bind(submission.operation_id).fetch_all(&pool).await?;
        qualified_searches += searches
            .iter()
            .filter(|r| {
                r["input"]["person_name"].is_string() || r["input"]["person_id"].is_string()
            })
            .count();
        let usage:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('status',status,'input',actual_input,'output',actual_output,'input_bound',input_bound,'output_cap',output_cap,'charged',charged_tokens,'unknown',usage_unknown) FROM usage_reservations WHERE run_id=$1 ORDER BY created_at").bind(submission.operation_id).fetch_all(&pool).await?;
        let diagnostics:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('kind',kind,'payload',payload) FROM events WHERE correlation_id=$1 AND kind IN('run.retry_scheduled','run.stopped') ORDER BY sequence").bind(submission.operation_id).fetch_all(&pool).await?;
        let passed = matches!(outcome,WorkerOutcome::Committed(id) if id==submission.operation_id)
            && !replies.is_empty()
            && !usage.is_empty()
            && usage.iter().all(|u| {
                u["status"] == "settled"
                    && u["unknown"] == false
                    && u["input"]
                        .as_u64()
                        .is_some_and(|v| Some(v) <= u["input_bound"].as_u64())
                    && u["output"]
                        .as_u64()
                        .is_some_and(|v| Some(v) <= u["output_cap"].as_u64())
            });
        engine_pass &= passed;
        results.push(json!({"case":case,"input":input,"operation_id":submission.operation_id,"outcome":format!("{outcome:?}"),"replies":replies,"searches":searches,"usage":usage,"diagnostics":diagnostics,"engineering_pass":passed}));
        if !passed {
            break;
        }
    }
    let current = s.snapshot().await?.ok_or("identity")?;
    let frozen: bool = sqlx::query_scalar("SELECT budget_frozen FROM individuals WHERE singleton")
        .fetch_one(&pool)
        .await?;
    engine_pass &= !frozen && results.len() == cases.len() && qualified_searches > 0;
    let all_usage:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('run_id',run_id,'status',status,'input',actual_input,'output',actual_output,'charged',charged_tokens,'unknown',usage_unknown) FROM usage_reservations ORDER BY created_at").fetch_all(&pool).await?;
    println!(
        "{}",
        json!({"phase":"person-retrieval","individual_id":current.id,"data":"independent synthetic development scenarios; owner-seeded claims/roles, real E5 and paid model worker","seeds":seeds,"people":people,"cases":results,"all_usage":all_usage,"qualified_searches":qualified_searches,"budget_frozen":frozen,"engine_pass":engine_pass,"behavior_review":"pending; inspect actual wording and final evidence separately","limitations":["Not frozen holdout acceptance or a learning experiment","Controlled owner roles do not prove model role extraction","No production history or model-session memory used"]})
    );
    pool.close().await;
    if engine_pass {
        Ok(())
    } else {
        Err("engineering check failed".into())
    }
}
#[cfg(not(feature = "local-embeddings"))]
fn main() {
    eprintln!("requires --features local-embeddings");
    std::process::exit(1);
}
