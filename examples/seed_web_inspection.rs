//! Explicit synthetic browser fixture. Never runs against the production database.
use alia::{
    adapters::{connect_database, store::Store},
    domain::{
        entities::{MemoryPersonAssertion, PersonRole, SourceSpan},
        history::{MemoryKind, NewMemory},
    },
};
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let url = std::env::var("DATABASE_URL").map_err(|_| "test database required")?;
    let parsed = reqwest::Url::parse(&url).map_err(|_| "invalid test database")?;
    if parsed.host_str() != Some("127.0.0.1") || !parsed.path().starts_with("/alia_web_test_") {
        return Err("refusing non-test database".into());
    }
    let pool = connect_database(&url)
        .await
        .map_err(|_| "test database unavailable")?;
    let s = Store::new(pool.clone());
    let text = "周岚喜欢乌龙茶。这是显式合成浏览器夹具，非生产经历。";
    let source = s
        .submit_message("inspection-source", text, None)
        .await
        .map_err(|_| "fixture message")?
        .event_id;
    let memory = s
        .record_memories(
            "inspection-memory",
            s.snapshot()
                .await
                .map_err(|_| "fixture snapshot")?
                .ok_or("test individual absent")?
                .expected(),
            &[source],
            &[NewMemory {
                content: text.into(),
                kind: MemoryKind::Semantic,
                scope: "general".into(),
                source_event_ids: vec![source],
                supersedes: None,
            }],
        )
        .await
        .map_err(|_| "fixture memory")?
        .memory_ids[0];
    let proof = SourceSpan {
        event_id: source,
        start_char: 0,
        end_char: 2,
    };
    let person = s
        .register_person(
            "inspection-person",
            s.snapshot()
                .await
                .map_err(|_| "fixture snapshot")?
                .ok_or("test individual absent")?
                .expected(),
            "inspection-person",
            "周岚",
            proof.clone(),
        )
        .await
        .map_err(|_| "fixture person")?
        .entity_id;
    s.assert_memory_person(
        "inspection-link",
        s.snapshot()
            .await
            .map_err(|_| "fixture snapshot")?
            .ok_or("test individual absent")?
            .expected(),
        MemoryPersonAssertion {
            memory_id: memory,
            memory_version: 1,
            entity_id: person,
            role: PersonRole::Subject,
            source: proof,
            memory_start_char: 0,
        },
    )
    .await
    .map_err(|_| "fixture annotation")?;
    pool.close().await;
    println!(
        "Browser memory inspection fixture saved; synthetic, owner/reported_claim, no live model call"
    );
    Ok(())
}
