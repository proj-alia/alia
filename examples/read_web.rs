//! One explicit public read, no model/individual/loop. Only safe metadata is printed.
use alia::{
    adapters::web::HttpWebReader,
    runtime::web::{WebReader, WebTarget},
};
use serde_json::json;
#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.len() != 2 {
        return Err("usage: read_web <https-url> <exact-allowed-host>".into());
    }
    let target = WebTarget::from_input(&json!({"url":args[0]}), &[args[1].clone()])?;
    let document = HttpWebReader.read(&target).await?;
    println!(
        "Public HTTPS read PASS: status={} utf8_bytes={} media_type={}",
        document.http_status,
        document.text.len(),
        document.media_type
    );
    Ok(())
}
