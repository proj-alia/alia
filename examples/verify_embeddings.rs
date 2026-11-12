//! Public synthetic development smoke; does not consume the frozen holdout.
#[cfg(feature = "local-embeddings")]
fn main() -> Result<(), Box<dyn std::error::Error>> {
    use alia::adapters::embedding::{InputKind, LocalEmbedding, MODEL_VERSION};
    let directory = std::env::args().nth(1).ok_or("model directory required")?;
    let mut model = LocalEmbedding::load(std::path::Path::new(&directory))?;
    let query = model.encode(&["怎样把文件可靠地保存到磁盘？".into()], InputKind::Query)?;
    let passages = model.encode(
        &[
            "保存文件需要考虑写入、同步以及故障恢复。".into(),
            "院子里的花在春天开放。".into(),
        ],
        InputKind::Passage,
    )?;
    let dot = |v: &Vec<f32>| query[0].iter().zip(v).map(|(a, b)| a * b).sum::<f32>();
    assert!(
        dot(&passages[0]) > dot(&passages[1]),
        "development contrast failed"
    );
    let again = model.encode(&["怎样把文件可靠地保存到磁盘？".into()], InputKind::Query)?;
    let max_difference = query[0]
        .iter()
        .zip(&again[0])
        .map(|(a, b)| (a - b).abs())
        .fold(0.0f32, f32::max);
    assert!(max_difference < 1e-5);
    assert!(model.encode(&[" ".into()], InputKind::Query).is_err());
    assert!(
        model
            .encode(&["文件".repeat(2000)], InputKind::Passage)
            .is_err()
    );
    assert!(
        model
            .encode(&vec!["文本".into(); 33], InputKind::Query)
            .is_err()
    );
    println!(
        "{}",
        serde_json::json!({"status":"passed","profile":MODEL_VERSION,"dimension":query[0].len(),"relevant_cosine":dot(&passages[0]),"irrelevant_cosine":dot(&passages[1]),"repeat_max_difference":max_difference,"scope":"local encoder development smoke only; no database or retrieval acceptance"})
    );
    Ok(())
}
#[cfg(not(feature = "local-embeddings"))]
fn main() {
    eprintln!("requires --features local-embeddings");
    std::process::exit(1);
}
