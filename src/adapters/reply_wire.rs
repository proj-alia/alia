//! Author wire protocol: construct text and its citations together, then convert
//! once into the existing canonical decision. Never add sources or drop text.
use crate::domain::{
    cognition::{DECISION_SCHEMA, DecisionProposal, ProposalError},
    config::parse_strict_json,
};
use serde_json::{Value, json};
use std::sync::LazyLock;

pub(super) fn schema(mut canonical: Value) -> Value {
    let properties = canonical["properties"]
        .as_object_mut()
        .expect("decision properties");
    let ids = properties
        .remove("reply_evidence_ids")
        .expect("reply evidence schema");
    let text = properties.remove("reply").expect("reply schema");
    properties.insert("reply_segments".into(),json!({"type":"array","maxItems":32,"items":{"type":"object","additionalProperties":false,"required":["text","evidence_ids"],"properties":{"text":text,"evidence_ids":ids}}}));
    let required = canonical["required"]
        .as_array_mut()
        .expect("decision fields");
    required.retain(|v| v != "reply" && v != "reply_evidence_ids");
    required.push(json!("reply_segments"));
    canonical
}
static VALIDATOR: LazyLock<jsonschema::Validator> = LazyLock::new(|| {
    jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&schema(
            serde_json::from_str(DECISION_SCHEMA).expect("decision schema"),
        ))
        .expect("author wire schema")
});
pub(super) fn parse(raw: &[u8]) -> Result<DecisionProposal, ProposalError> {
    let mut value = parse_strict_json(raw, 128 * 1024).map_err(|_| ProposalError::Schema)?;
    if !VALIDATOR.is_valid(&value) {
        return Err(ProposalError::Schema);
    }
    let segments = value
        .as_object_mut()
        .ok_or(ProposalError::Schema)?
        .remove("reply_segments")
        .ok_or(ProposalError::Schema)?;
    let mut text = Vec::new();
    let mut ids = Vec::new();
    for segment in segments.as_array().ok_or(ProposalError::Schema)? {
        let part = segment["text"].as_str().ok_or(ProposalError::Schema)?;
        if part.trim().is_empty() {
            return Err(ProposalError::Schema);
        }
        text.push(part);
        for id in segment["evidence_ids"]
            .as_array()
            .ok_or(ProposalError::Schema)?
        {
            if !ids.contains(id) {
                ids.push(id.clone());
            }
        }
    }
    value["reply"] = json!(text.join("\n\n"));
    value["reply_evidence_ids"] = json!(ids);
    DecisionProposal::parse(&serde_json::to_vec(&value).map_err(|_| ProposalError::Schema)?)
}
#[cfg(test)]
mod tests {
    use super::*;
    fn candidate() -> Value {
        let mut v: Value =
            serde_json::from_str(include_str!("../../evals/decision.example.json")).unwrap();
        let o = v.as_object_mut().unwrap();
        o.remove("reply");
        o.remove("reply_evidence_ids");
        v["reply_segments"] = json!([]);
        v
    }
    #[test]
    fn rendering_preserves_every_segment_and_only_declared_citations() {
        let mut v = candidate();
        let id = "20000000-0000-4000-8000-000000000001";
        v["reply_segments"] = json!([{"text":"目前无法确认。","evidence_ids":[]},{"text":"已知资料的具体内容。","evidence_ids":[id]},{"text":"另一项推断。","evidence_ids":[id]}]);
        let p = parse(v.to_string().as_bytes()).unwrap();
        assert_eq!(
            p.reply,
            "目前无法确认。\n\n已知资料的具体内容。\n\n另一项推断。"
        );
        assert_eq!(p.reply_evidence_ids.len(), 1);
        v["reply"] = json!("另一份可竞争修改的文本");
        assert!(parse(v.to_string().as_bytes()).is_err());
    }
    #[test]
    fn empty_tool_reply_and_invalid_or_oversized_segments_are_distinct() {
        let mut v = candidate();
        assert!(parse(v.to_string().as_bytes()).unwrap().reply.is_empty());
        for segments in [
            json!([{"text":" ","evidence_ids":[]}]),
            json!([{"text":"事实","evidence_ids":["not-a-uuid"]}]),
            json!([{"text":"事实","evidence_ids":[],"approved":true}]),
            json!([{"text":"a".repeat(16000),"evidence_ids":[]},{"text":"b","evidence_ids":[]}]),
        ] {
            v["reply_segments"] = segments;
            assert!(parse(v.to_string().as_bytes()).is_err());
        }
    }
}
