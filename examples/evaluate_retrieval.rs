//! Frozen synthetic holdout through actual encoder and production vector store.
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
    use serde_json::{Value, json};
    use sha2::{Digest, Sha256};
    use std::collections::{HashMap, HashSet};
    let url = std::env::var("DATABASE_URL")?;
    let parsed = reqwest::Url::parse(&url)?;
    if parsed.host_str() != Some("127.0.0.1") || !parsed.path().starts_with("/alia_semantic_test_")
    {
        return Err("refusing non-test database".into());
    }
    let bytes = std::fs::read("evals/retrieval/zh-v1.json")?;
    let hash = format!("{:x}", Sha256::digest(&bytes));
    if hash
        != include_str!("../evals/retrieval/zh-v1.sha256")
            .split_whitespace()
            .next()
            .ok_or("missing corpus hash")?
    {
        return Err("holdout changed".into());
    }
    let corpus: Value = serde_json::from_slice(&bytes)?;
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
    let mut events = HashMap::new();
    let mut ids = HashMap::new();
    let mut original = HashMap::new();
    for e in corpus["events"].as_array().ok_or("events missing")? {
        let id = e["id"].as_str().ok_or("event id")?;
        events.insert(
            id.to_owned(),
            s.submit_message(id, e["text"].as_str().ok_or("event text")?, None)
                .await?
                .event_id,
        );
    }
    let map_scope = |scope: &str| -> Result<String, Box<dyn std::error::Error>> {
        if scope == "general" {
            Ok(scope.to_owned())
        } else {
            Ok(format!(
                "event:{}",
                events
                    .get(scope.strip_prefix("event:").ok_or("scope")?)
                    .ok_or("scope event")?
            ))
        }
    };
    let mut model =
        LocalEmbedding::load(std::path::Path::new("data/models/multilingual-e5-small"))?;
    for m in corpus["memories"].as_array().ok_or("memories missing")? {
        let id = m["id"].as_str().ok_or("memory id")?;
        let source = *events
            .get(m["source_event_ids"][0].as_str().ok_or("source")?)
            .ok_or("source absent")?;
        let current = s.snapshot().await?.ok_or("identity")?;
        let memory = s
            .record_memories(
                id,
                current.expected(),
                &[source],
                &[NewMemory {
                    content: m["content"].as_str().ok_or("content")?.into(),
                    kind: MemoryKind::Semantic,
                    scope: map_scope(m["scope"].as_str().ok_or("scope")?)?,
                    source_event_ids: vec![source],
                    supersedes: None,
                }],
            )
            .await?
            .memory_ids[0];
        // Controlled validity fixtures, not historical life claims or a production mutation API.
        let from: chrono::DateTime<chrono::Utc> =
            m["valid_from"].as_str().ok_or("from")?.parse()?;
        let until: Option<chrono::DateTime<chrono::Utc>> =
            m["valid_until"].as_str().map(str::parse).transpose()?;
        sqlx::query("UPDATE memories SET validity=$2,valid_from=$3,valid_until=$4,version=version+1 WHERE id=$1").bind(memory).bind(m["validity"].as_str().ok_or("validity")?).bind(from).bind(until).execute(&pool).await?;
        let snapshot = s.prepare_memory_index(memory).await?;
        let chunks = model.encode_memory(snapshot.content())?;
        s.commit_memory_index(&snapshot, &chunks).await?;
        ids.insert(id.to_owned(), memory);
        original.insert(memory, id.to_owned());
    }
    pool.close().await;
    let pool = connect_database(&url)
        .await
        .map_err(|_| "test database unavailable")?;
    let shared = std::env::args().any(|a| a == "--shared");
    let mut model = Some(model);
    let s = if shared {
        Store::new(pool.clone()).with_memory_encoder(std::sync::Arc::new(
            alia::adapters::embedding::CpuMemoryEncoder::new(model.take().ok_or("encoder")?),
        ))
    } else {
        Store::new(pool.clone())
    };
    let mut results = vec![];
    let mut recall = 0.0;
    let mut forbidden_total = 0;
    for q in corpus["queries"].as_array().ok_or("queries missing")? {
        let hits = if shared {
            s.retrieve_memories(&RetrievalRequest {
                text: q["text"].as_str().ok_or("query text")?.into(),
                scope: Some(map_scope(q["scope"].as_str().ok_or("scope")?)?),
                subject_id: None,
                person_id: None,
                person_name: None,
                at: q["at"].as_str().ok_or("query at")?.parse()?,
                include_history: q["include_history"].as_bool().unwrap_or(false),
                limit: 10,
            })
            .await?
            .memories()
            .to_vec()
        } else {
            let vector = model
                .as_mut()
                .ok_or("encoder")?
                .encode(
                    &[q["text"].as_str().ok_or("query text")?.into()],
                    InputKind::Query,
                )?
                .remove(0);
            s.search_semantic(&SemanticQuery {
                vector,
                profile: INDEX_PROFILE.into(),
                scope: Some(map_scope(q["scope"].as_str().ok_or("scope")?)?),
                subject_id: None,
                person_id: None,
                at: q["at"].as_str().ok_or("query at")?.parse()?,
                include_history: q["include_history"].as_bool().unwrap_or(false),
                limit: 10,
            })
            .await?
        };
        let ranked: Vec<String> = hits.iter().map(|m| original[&m.id].clone()).collect();
        let selected: HashSet<&str> = ranked.iter().map(String::as_str).collect();
        let relevant = q["relevant_ids"].as_array().ok_or("labels")?;
        let score = relevant
            .iter()
            .filter(|id| selected.contains(id.as_str().unwrap_or("")))
            .count() as f64
            / relevant.len() as f64;
        let forbidden: Vec<_> = q["forbidden_ids"]
            .as_array()
            .ok_or("forbidden labels")?
            .iter()
            .filter(|id| selected.contains(id.as_str().unwrap_or("")))
            .cloned()
            .collect();
        forbidden_total += forbidden.len();
        recall += score;
        results.push(json!({"id":q["id"],"ranked_ids":ranked,"recall_at_10":score,"forbidden_raw_hits":forbidden}));
    }
    let mean = recall / results.len() as f64;
    println!(
        "{}",
        json!({"status":"measured","mode":if shared {"actual local E5, shared hybrid retrieval; controlled synthetic validity fixtures"} else {"actual local E5 and PostgreSQL exact search; controlled synthetic validity fixtures"},"corpus_sha256":hash,"profile":INDEX_PROFILE,"queries":results.len(),"recall_at_10":mean,"recall_gate":mean>=0.9,"forbidden_raw_hits_total":forbidden_total,"limitations":["No entity resolver: subject annotation is not supplied to ranking; the source relationship is the single owner.","Raw forbidden retrieval is reported, not a measurement of final answer use.","Corpus is directly indexed: this report is not service or model-behavior acceptance.","No tuning against this frozen holdout."],"results":results})
    );
    pool.close().await;
    Ok(())
}
#[cfg(not(feature = "local-embeddings"))]
fn main() {
    eprintln!("requires --features local-embeddings");
    std::process::exit(1);
}
