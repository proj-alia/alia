//! Synthetic seed/probe for a real service process; never manually writes vectors/jobs.
#[cfg(feature = "local-embeddings")]
#[tokio::main]
async fn main() {
    match run().await {
        Ok(report) => println!("{report}"),
        Err(code) => {
            eprintln!("Index service smoke failed: {code}");
            std::process::exit(1)
        }
    }
}
#[cfg(feature = "local-embeddings")]
async fn run() -> Result<serde_json::Value, &'static str> {
    use alia::{
        adapters::{
            connect_database,
            embedding::{InputKind, LocalEmbedding},
            store::Store,
        },
        domain::{
            config::{PersonaSeed, RuntimeRules},
            history::{MemoryKind, NewMemory},
            retrieval::{INDEX_PROFILE, SemanticQuery},
        },
        runtime::capability::IMPLEMENTED_CAPABILITIES,
    };
    use serde_json::json;
    let url = std::env::var("DATABASE_URL").map_err(|_| "database required")?;
    let parsed = reqwest::Url::parse(&url).map_err(|_| "database invalid")?;
    if parsed.host_str() != Some("127.0.0.1") || !parsed.path().starts_with("/alia_semantic_test_")
    {
        return Err("refusing non-test database");
    }
    let pool = connect_database(&url)
        .await
        .map_err(|_| "database unavailable")?;
    let s = Store::new(pool.clone());
    let phase = std::env::args().nth(1).ok_or("phase required")?;
    let report = if phase == "seed" {
        s.migrate().await.map_err(|_| "migration")?;
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
        .map_err(|_| "initialize")?;
        let mut ids = vec![];
        for (n, text) in [
            "可靠保存文件应考虑磁盘同步、完整写入和中断恢复。".repeat(25),
            "花园中的植物在春天开花。".into(),
        ]
        .into_iter()
        .enumerate()
        {
            let key = format!("index-smoke-{n}");
            let source = s
                .submit_message(&key, &text, None)
                .await
                .map_err(|_| "source")?
                .event_id;
            let current = s
                .snapshot()
                .await
                .map_err(|_| "state")?
                .ok_or("individual absent")?;
            let id = s
                .record_memories(
                    &key,
                    current.expected(),
                    &[source],
                    &[NewMemory {
                        content: text,
                        kind: MemoryKind::Semantic,
                        scope: "general".into(),
                        source_event_ids: vec![source],
                        supersedes: None,
                    }],
                )
                .await
                .map_err(|_| "memory")?
                .memory_ids[0];
            ids.push(id);
        }
        json!({"phase":"seed","memory_ids":ids,"data":"explicit synthetic development data; no model or vector fixtures"})
    } else if phase == "verify" {
        let dir = std::env::args().nth(2).ok_or("assets required")?;
        let mut encoder = LocalEmbedding::load(std::path::Path::new(&dir)).map_err(|_| "assets")?;
        let vector = encoder
            .encode(&["怎样将文件可靠地保存在磁盘上？".into()], InputKind::Query)
            .map_err(|_| "query encoding")?
            .remove(0);
        let found = s
            .search_semantic(&SemanticQuery {
                vector,
                profile: INDEX_PROFILE.into(),
                scope: None,
                subject_id: None,
                person_id: None,
                at: chrono::Utc::now(),
                include_history: false,
                limit: 10,
            })
            .await
            .map_err(|_| "query")?;
        if found.len() != 2 || !found[0].content.contains("磁盘同步") {
            return Err("ranking or coverage");
        }
        let jobs:Vec<serde_json::Value>=sqlx::query_scalar("SELECT jsonb_build_object('memory_id',memory_id,'profile',profile,'status',status,'attempts',attempts,'generation',generation,'updated_at',updated_at) FROM memory_index_jobs ORDER BY memory_id").fetch_all(&pool).await.map_err(|_|"jobs")?;
        let chunks: Vec<i32> = sqlx::query_scalar(
            "SELECT chunk_count FROM memory_vector_sets ORDER BY chunk_count DESC",
        )
        .fetch_all(&pool)
        .await
        .map_err(|_| "sets")?;
        let usage: i64 = sqlx::query_scalar("SELECT count(*) FROM usage_reservations")
            .fetch_one(&pool)
            .await
            .map_err(|_| "usage")?;
        if jobs.len() != 2
            || !jobs
                .iter()
                .all(|j| j["status"] == "ready" && j["attempts"] == 1)
            || chunks.len() != 2
            || chunks[0] < 2
            || usage != 0
        {
            return Err("queue or usage");
        }
        json!({"phase":"verify","ranked_ids":found.iter().map(|m|m.id).collect::<Vec<_>>(),"jobs":jobs,"chunk_counts":chunks,"cloud_calls":usage,"profile":INDEX_PROFILE,"indexing":"real service worker; real local E5; synthetic development data","recall_acceptance":"not_run"})
    } else if phase == "query_windows" {
        let dir = std::env::args().nth(2).ok_or("assets required")?;
        let mut encoder = LocalEmbedding::load(std::path::Path::new(&dir)).map_err(|_| "assets")?;
        let text = "甲".repeat(900);
        if encoder
            .encode(std::slice::from_ref(&text), InputKind::Query)
            .is_ok()
        {
            return Err("fixture did not exceed native token window");
        }
        let encoded = encoder.encode_query(&text).map_err(|_| "windowed query")?;
        if !alia::domain::retrieval::valid_vector(&encoded) {
            return Err("query normalization");
        }
        json!({"phase":"query_windows","characters":text.chars().count(),"direct_input_rejected":true,"full_window_encoding":true,"normalized_dimensions":encoded.len()})
    } else {
        return Err("unknown phase");
    };
    pool.close().await;
    Ok(report)
}
#[cfg(not(feature = "local-embeddings"))]
fn main() {
    eprintln!("requires --features local-embeddings");
    std::process::exit(1)
}
