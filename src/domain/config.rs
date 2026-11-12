//! Strict configuration parsing using the same versioned schemas as the M0 checker.
//! No remote schema retrieval and no implicit application of seeds to existing state.
use serde::{
    Deserialize, Deserializer,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::{Map, Number, Value};
use std::{collections::HashSet, fmt, sync::LazyLock};

static PERSONA: LazyLock<jsonschema::Validator> =
    LazyLock::new(|| compiled(include_str!("../../schemas/persona.schema.json")));
static RULES: LazyLock<jsonschema::Validator> =
    LazyLock::new(|| compiled(include_str!("../../schemas/runtime.schema.json")));

fn compiled(raw: &str) -> jsonschema::Validator {
    jsonschema::draft202012::options()
        .should_validate_formats(true)
        .build(&serde_json::from_str::<Value>(raw).expect("embedded schema JSON"))
        .expect("embedded schema must compile")
}

#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ConfigurationError {
    #[error("invalid JSON, duplicate keys, nonfinite number, or invalid encoding")]
    Json,
    #[error("configuration violates the schema")]
    Schema,
    #[error("JSON document exceeds the configured size limit")]
    TooLarge,
    #[error("duplicate value identifiers")]
    DuplicateValue,
    #[error("background token budget exceeds total token budget")]
    Budget,
    #[error("unrecognized IANA timezone")]
    Timezone,
    #[error("configured capability has no implementation in this runtime")]
    UnimplementedCapability,
    #[error("local hostnames are not allowed")]
    LocalHostname,
}

/// Keeps duplicate-key detection at the parsing boundary, before serde_json::Value loses it.
struct StrictJson(Value);
impl<'de> Deserialize<'de> for StrictJson {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct StrictVisitor;
        impl<'de> Visitor<'de> for StrictVisitor {
            type Value = StrictJson;
            fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
                formatter.write_str("strict JSON")
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictJson(Value::Null))
            }
            fn visit_bool<E: de::Error>(self, v: bool) -> Result<Self::Value, E> {
                Ok(StrictJson(Value::Bool(v)))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
                Ok(StrictJson(v.into()))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
                Ok(StrictJson(v.into()))
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                Number::from_f64(v)
                    .map(|n| StrictJson(Value::Number(n)))
                    .ok_or_else(|| E::custom("nonfinite number"))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
                Ok(StrictJson(Value::String(v.into())))
            }
            fn visit_string<E: de::Error>(self, v: String) -> Result<Self::Value, E> {
                Ok(StrictJson(Value::String(v)))
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut seq: A) -> Result<Self::Value, A::Error> {
                let mut items = Vec::new();
                while let Some(StrictJson(item)) = seq.next_element()? {
                    items.push(item);
                }
                Ok(StrictJson(Value::Array(items)))
            }
            fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut values = Map::new();
                while let Some(key) = map.next_key::<String>()? {
                    if values.contains_key(&key) {
                        return Err(de::Error::custom("duplicate key"));
                    }
                    let StrictJson(value) = map.next_value()?;
                    values.insert(key, value);
                }
                Ok(StrictJson(Value::Object(values)))
            }
        }
        deserializer.deserialize_any(StrictVisitor)
    }
}

pub fn parse_strict_json(bytes: &[u8], maximum: usize) -> Result<Value, ConfigurationError> {
    if bytes.len() > maximum {
        return Err(ConfigurationError::TooLarge);
    }
    serde_json::from_slice::<StrictJson>(bytes)
        .map(|v| v.0)
        .map_err(|_| ConfigurationError::Json)
}

#[derive(Clone)]
pub struct PersonaSeed(Value);
impl PersonaSeed {
    pub fn parse(bytes: &[u8]) -> Result<Self, ConfigurationError> {
        Self::validate(parse_strict_json(bytes, 64 * 1024)?)
    }
    pub fn validate(value: Value) -> Result<Self, ConfigurationError> {
        if serde_json::to_vec(&value)
            .map_err(|_| ConfigurationError::Json)?
            .len()
            > 64 * 1024
        {
            return Err(ConfigurationError::TooLarge);
        }
        if !PERSONA.is_valid(&value) {
            return Err(ConfigurationError::Schema);
        }
        let mut ids = HashSet::new();
        for item in value["personality"]["values"].as_array().unwrap() {
            if !ids.insert(item["id"].as_str().unwrap()) {
                return Err(ConfigurationError::DuplicateValue);
            }
        }
        Ok(Self(value))
    }
    pub fn as_value(&self) -> &Value {
        &self.0
    }
}

#[derive(Clone)]
pub struct RuntimeRules(Value);
impl RuntimeRules {
    pub fn parse(bytes: &[u8], implemented: &[&str]) -> Result<Self, ConfigurationError> {
        Self::validate(parse_strict_json(bytes, 64 * 1024)?, implemented)
    }
    pub fn validate(value: Value, implemented: &[&str]) -> Result<Self, ConfigurationError> {
        if serde_json::to_vec(&value)
            .map_err(|_| ConfigurationError::Json)?
            .len()
            > 64 * 1024
        {
            return Err(ConfigurationError::TooLarge);
        }
        if !RULES.is_valid(&value) {
            return Err(ConfigurationError::Schema);
        }
        if value["budgets"]["background_tokens_per_day"]
            .as_u64()
            .unwrap()
            > value["budgets"]["total_tokens_per_day"].as_u64().unwrap()
        {
            return Err(ConfigurationError::Budget);
        }
        if value["contact"]["timezone"]
            .as_str()
            .unwrap()
            .parse::<chrono_tz::Tz>()
            .is_err()
        {
            return Err(ConfigurationError::Timezone);
        }
        for item in value["capabilities"]["enabled"].as_array().unwrap() {
            if !implemented.contains(&item.as_str().unwrap()) {
                return Err(ConfigurationError::UnimplementedCapability);
            }
        }
        for item in value["capabilities"]["allowed_web_hosts"]
            .as_array()
            .unwrap()
        {
            let host = item.as_str().unwrap();
            if host == "localhost"
                || host == "localhost.localdomain"
                || host.ends_with(".localhost")
            {
                return Err(ConfigurationError::LocalHostname);
            }
        }
        Ok(Self(value))
    }
    pub fn as_value(&self) -> &Value {
        &self.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    const CAPABILITIES: &[&str] = &[
        "memory.search",
        "documents.read",
        "notes.write",
        "web.read_allowed",
    ];
    const PERSONA_EXAMPLE: &[u8] = include_bytes!("../../config/persona.example.json");
    const RUNTIME_EXAMPLE: &[u8] = include_bytes!("../../config/runtime.example.json");

    #[test]
    fn reference_examples_validate_with_an_explicit_implemented_registry() {
        assert!(PersonaSeed::parse(PERSONA_EXAMPLE).is_ok());
        assert!(RuntimeRules::parse(RUNTIME_EXAMPLE, CAPABILITIES).is_ok());
        assert!(matches!(
            RuntimeRules::parse(RUNTIME_EXAMPLE, &[]),
            Err(ConfigurationError::UnimplementedCapability)
        ));
    }

    #[test]
    fn strict_parser_rejects_ambiguity_nonfinite_and_invalid_unicode_at_every_depth() {
        for invalid in [
            r#"{"a":1,"a":2}"#,
            r#"{"outer":{"a":1,"a":2}}"#,
            r#"[ {"a":1,"a":2} ]"#,
            "NaN",
            "Infinity",
            "1e999",
            r#""\ud800""#,
            "{} trailing",
        ] {
            assert!(
                parse_strict_json(invalid.as_bytes(), 65536).is_err(),
                "{invalid}"
            );
        }
        assert!(parse_strict_json(&[0xff], 65536).is_err());
        assert!(parse_strict_json(b"{}", 1).is_err());
    }

    #[test]
    fn schema_and_semantic_rules_reject_invalid_personas() {
        for (pointer, replacement) in [
            ("/schema_version", json!(2)),
            ("/personality/traits/curiosity", json!(1.1)),
            ("/relationship/assume_intimacy", json!(true)),
            ("/identity/nature", json!("human")),
        ] {
            let mut value = PersonaSeed::parse(PERSONA_EXAMPLE).unwrap().0;
            *value.pointer_mut(pointer).unwrap() = replacement;
            assert!(PersonaSeed::validate(value).is_err());
        }
        let mut value = PersonaSeed::parse(PERSONA_EXAMPLE).unwrap().0;
        value["identity"]["unrecognized"] = json!("not allowed");
        assert!(PersonaSeed::validate(value).is_err());
        let mut value = PersonaSeed::parse(PERSONA_EXAMPLE).unwrap().0;
        value["personality"]["values"][1]["id"] = value["personality"]["values"][0]["id"].clone();
        assert!(matches!(
            PersonaSeed::validate(value),
            Err(ConfigurationError::DuplicateValue)
        ));
    }

    #[test]
    fn runtime_rejects_invalid_limits_timezones_privilege_changes_and_unknown_fields() {
        for (pointer, replacement) in [
            ("/autonomy/max_model_calls_per_cycle", json!(4)),
            ("/autonomy/max_tool_calls_per_cycle", json!(0)),
            ("/budgets/background_tokens_per_day", json!(200001)),
            ("/contact/timezone", json!("Mars/Olympus")),
            ("/contact/quiet_start", json!("25:00")),
            ("/development/allow_value_change", json!(false)),
            ("/capabilities/enabled", json!(["shell.execute"])),
            ("/capabilities/self_grant_permissions", json!(true)),
            ("/capabilities/allowed_web_hosts", json!(["*.example.com"])),
            ("/capabilities/allowed_web_hosts", json!(["127.0.0.1"])),
            ("/capabilities/allowed_web_hosts", json!(["a.localhost"])),
        ] {
            let mut value = RuntimeRules::parse(RUNTIME_EXAMPLE, CAPABILITIES)
                .unwrap()
                .0;
            *value.pointer_mut(pointer).unwrap() = replacement;
            assert!(
                RuntimeRules::validate(value, CAPABILITIES).is_err(),
                "{pointer}"
            );
        }
        let mut value = RuntimeRules::parse(RUNTIME_EXAMPLE, CAPABILITIES)
            .unwrap()
            .0;
        value["contact"]["ignored_misspelling"] = json!(true);
        assert!(RuntimeRules::validate(value, CAPABILITIES).is_err());
    }
}
