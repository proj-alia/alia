//! Three-stage, single-generation probe with synthetic coding context only.
use alia::{
    adapters::model::ChatProvider,
    domain::cognition::ModelContext,
    runtime::model::{CallLimits, ModelCallMetadata, ModelFailure, ModelProvider},
};
use serde_json::json;

fn failed(stage: &str, failure: ModelFailure) -> ! {
    println!(
        "{}",
        json!({"stage":stage,"status":"failed","error":failure.error.to_string(),
        "http":failure.http,"usage":failure.usage,"request_sent":failure.request_sent})
    );
    std::process::exit(1)
}

#[tokio::main]
async fn main() {
    let context: ModelContext =
        serde_json::from_str(include_str!("../evals/model_context.example.json"))
            .expect("synthetic probe context");
    let provider =
        ChatProvider::from_env().unwrap_or_else(|failure| failed("configuration", failure));
    let is_go = std::env::var("ALIA_MODEL_PROVIDER").as_deref() == Ok("opencode_go");
    if is_go {
        let count = provider
            .check_go_catalog()
            .await
            .unwrap_or_else(|failure| failed("public_catalog", failure));
        println!(
            "{}",
            json!({"stage":"public_catalog","status":"passed","model_count":count,"selected_model_present":true,"authenticated":false})
        );
    } else {
        println!(
            "{}",
            json!({"stage":"public_catalog","status":"skipped","reason":"not_opencode_go"})
        );
    }
    let metadata = ModelCallMetadata::foreground(&context)
        .unwrap_or_else(|failure| failed("configuration", failure));
    let allowance = provider
        .token_allowance(&context, CallLimits::default())
        .ok();
    println!(
        "{}",
        json!({"stage":"local_count","profile":provider.input_accounting(),"allowance":allowance})
    );
    let generated = provider
        .generate(&context, CallLimits::default(), metadata)
        .await
        .unwrap_or_else(|failure| failed("synthetic_generation", failure));
    println!(
        "{}",
        json!({"stage":"synthetic_generation","status":"passed","http":generated.http})
    );
    let result = generated
        .into_result()
        .unwrap_or_else(|failure| failed("proposal_validation", failure));
    let count_matches = result
        .usage
        .zip(allowance)
        .map(|(u, a)| u.input <= a.input_upper_bound);
    println!(
        "{}",
        json!({"stage":"count_comparison","input_within_bound":count_matches,"usage":result.usage,"allowance":allowance})
    );
    let valid = result.proposal.validate_context(&context);
    println!(
        "{}",
        json!({"stage":"proposal_validation","status":if valid.is_ok(){"passed"}else{"failed"},
        "provider_mode":result.provider_mode,"provider_model":result.provider_model,"usage":result.usage,
        "proposal_valid":valid.is_ok(),"error":valid.as_ref().err().map(|e|e.to_string()),
        "reply_characters":result.proposal.reply.chars().count(),
        "note":"Synthetic protocol/source validation only; not behavioral acceptance or budget certification"})
    );
    if valid.is_err() || count_matches == Some(false) {
        std::process::exit(1);
    }
}
