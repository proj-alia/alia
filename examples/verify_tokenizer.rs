//! Offline equivalence check. Input is public synthetic system/user pairs.
use alia::adapters::tokenizer::PinnedTokenizer;
use std::io::{self, Read};
fn main() {
    let path = std::env::args_os()
        .nth(1)
        .expect("tokenizer artifact path required");
    let tokenizer = PinnedTokenizer::load(std::path::Path::new(&path)).expect("verified artifact");
    let mut bytes = Vec::new();
    io::stdin()
        .take(4 * 1024 * 1024 + 1)
        .read_to_end(&mut bytes)
        .unwrap();
    assert!(bytes.len() <= 4 * 1024 * 1024);
    let cases: Vec<(String, String)> = serde_json::from_slice(&bytes).unwrap();
    let counts: Vec<_> = cases
        .iter()
        .map(|(system, user)| tokenizer.count(system, user).expect("supported input"))
        .collect();
    // Check adapter selection as well as tokenizer parity, without a network call.
    use alia::{
        adapters::model::{ChatDialect, ChatProvider},
        domain::cognition::ModelContext,
        runtime::model::{CallLimits, ModelProvider},
    };
    let base = "https://opencode.ai/inference/go/openai/v1";
    let make = || {
        ChatProvider::new(
            base,
            "glm-5.3-flash",
            "offline-not-a-real-key",
            ChatDialect::OpenCodeGo,
        )
        .unwrap()
    };
    let ordinary = make();
    let pinned = make()
        .with_pinned_tokenizer(std::path::Path::new(&path))
        .unwrap();
    let context: ModelContext =
        serde_json::from_str(include_str!("../evals/model_context.example.json")).unwrap();
    let before = ordinary
        .token_allowance(&context, CallLimits::default())
        .unwrap();
    let after = pinned
        .token_allowance(&context, CallLimits::default())
        .unwrap();
    assert!(after.input_upper_bound > 0 && after.input_upper_bound < before.input_upper_bound);
    assert_eq!(before.output_cap, after.output_cap);
    assert_eq!(pinned.input_accounting(), "glm-pinned-text/1");
    for (endpoint, model) in [
        ("https://unreviewed.example/v1", "glm-5.3-flash"),
        (base, "another-model"),
    ] {
        assert!(
            ChatProvider::new(endpoint, model, "offline-key", ChatDialect::OpenCodeGo)
                .unwrap()
                .with_pinned_tokenizer(std::path::Path::new(&path))
                .is_err()
        );
    }
    println!("{}", serde_json::to_string(&counts).unwrap());
}
