//! Real local E5 + PostgreSQL smoke in a caller-created disposable DB only.
#[cfg(feature = "local-embeddings")]
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    use alia::{
        adapters::{
            connect_database,
            embedding::{InputKind, LocalEmbedding},
            store::Store,
        },
        domain::{
            config::{PersonaSeed, RuntimeRules},
            history::{MemoryKind, NewMemory},
            retrieval::*,
        },
        runtime::capability::IMPLEMENTED_CAPABILITIES,
    };
    use chrono::Utc;
    let url = std::env::var("DATABASE_URL")?;
    let parsed = reqwest::Url::parse(&url)?;
    if parsed.host_str() != Some("127.0.0.1") || !parsed.path().starts_with("/alia_semantic_test_")
    {
        return Err("refusing non-test database".into());
    }
    let directory = std::env::args().nth(1).ok_or("model directory required")?;
    let pool = connect_database(&url)
        .await
        .map_err(|_| "test database unavailable")?;
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
    let mut model = LocalEmbedding::load(std::path::Path::new(&directory))?;
    let relevant = "保存文件需要考虑写入、同步以及故障恢复。".repeat(25);
    let mut ids = vec![];
    let mut chunk_counts = vec![];
    for text in [relevant, "院子里的花在春天开放。".into()] {
        let key = uuid::Uuid::new_v4().to_string();
        let source = s.submit_message(&key, &text, None).await?.event_id;
        let current = s.snapshot().await?.ok_or("missing identity")?;
        let memory = s
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
            .await?
            .memory_ids[0];
        let snapshot = s.prepare_memory_index(memory).await?;
        let chunks = model.encode_memory(snapshot.content())?;
        chunk_counts.push(chunks.len());
        s.commit_memory_index(&snapshot, &chunks).await?;
        ids.push(memory);
    }
    assert!(chunk_counts[0] > 1);
    let vector = model
        .encode(&["怎样把文件可靠地保存到磁盘？".into()], InputKind::Query)?
        .remove(0);
    pool.close().await;
    let pool = connect_database(&url)
        .await
        .map_err(|_| "test database unavailable")?;
    let restored = Store::new(pool.clone());
    let query = SemanticQuery {
        vector,
        profile: INDEX_PROFILE.into(),
        scope: None,
        subject_id: None,
        person_id: None,
        at: Utc::now(),
        include_history: false,
        limit: 10,
    };
    let found = restored.search_semantic(&query).await?;
    assert_eq!(found.len(), 2);
    assert_eq!(found[0].id, ids[0]);
    println!(
        "{}",
        serde_json::json!({"status":"passed","mode":"real local embedding and PostgreSQL; synthetic development data","profile":INDEX_PROFILE,"chunks":chunk_counts,"ranked_ids":found.iter().map(|m|m.id).collect::<Vec<_>>(),"connection_recreated":true,"recall_acceptance":"not_run"})
    );
    pool.close().await;
    Ok(())
}
#[cfg(not(feature = "local-embeddings"))]
fn main() {
    eprintln!("requires --features local-embeddings");
    std::process::exit(1);
}
