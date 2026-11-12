//! Portable personal archive, not a database backup or an import command.
use chrono::{DateTime, Utc};
use serde::{
    Deserialize, Serialize,
    de::{self, MapAccess, SeqAccess, Visitor},
};
use serde_json::Value;
use std::{
    collections::{BTreeMap, HashSet},
    fmt,
    io::Write,
};
use uuid::Uuid;
pub const EXPORT_FORMAT: &str = "alia-individual-export/1";
pub const EXPORT_MAX_BYTES: usize = 64 * 1024 * 1024;
pub const EXPORT_MAX_TABLE_ROWS: usize = 100_000;
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ExportOmission {
    pub table: String,
    pub columns: Vec<String>,
    pub reason: String,
}
#[derive(Debug, Clone, Serialize)]
pub struct IndividualExport {
    pub format: String,
    pub individual_id: Uuid,
    pub state_version: i64,
    pub privacy_epoch: i64,
    pub observed_at: DateTime<Utc>,
    pub schema_fingerprint: String,
    pub tables: BTreeMap<String, Vec<Value>>,
    pub omissions: Vec<ExportOmission>,
}
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum ExportError {
    #[error("invalid personal export structure")]
    Invalid,
    #[error("personal export size limit exceeded")]
    TooLarge,
    #[error("personal export table row limit exceeded")]
    TooManyRows,
}
struct BoundedWriter {
    bytes: Vec<u8>,
    count: usize,
    limit: usize,
    exceeded: bool,
    retain: bool,
}
impl Write for BoundedWriter {
    fn write(&mut self, bytes: &[u8]) -> std::io::Result<usize> {
        let Some(next) = self
            .count
            .checked_add(bytes.len())
            .filter(|v| *v <= self.limit)
        else {
            self.exceeded = true;
            return Err(std::io::ErrorKind::FileTooLarge.into());
        };
        self.count = next;
        if self.retain {
            self.bytes.extend_from_slice(bytes);
        }
        Ok(bytes.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}
/// Counts the real compact UTF8 encoding without constructing a second row copy.
pub fn json_size(value: &impl Serialize, limit: usize) -> Result<usize, ExportError> {
    let mut writer = BoundedWriter {
        bytes: vec![],
        count: 0,
        limit,
        exceeded: false,
        retain: false,
    };
    if serde_json::to_writer(&mut writer, value).is_err() {
        return Err(if writer.exceeded {
            ExportError::TooLarge
        } else {
            ExportError::Invalid
        });
    }
    Ok(writer.count)
}
// Discarding visitor: detects repeated object keys at every level without
// retaining another copy of private row bodies. This is validation, not import.
struct StrictJson;
impl<'de> Deserialize<'de> for StrictJson {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        struct V;
        impl<'de> Visitor<'de> for V {
            type Value = StrictJson;
            fn expecting(&self, f: &mut fmt::Formatter) -> fmt::Result {
                f.write_str("JSON with unique object fields")
            }
            fn visit_bool<E: de::Error>(self, _: bool) -> Result<Self::Value, E> {
                Ok(StrictJson)
            }
            fn visit_i64<E: de::Error>(self, _: i64) -> Result<Self::Value, E> {
                Ok(StrictJson)
            }
            fn visit_u64<E: de::Error>(self, _: u64) -> Result<Self::Value, E> {
                Ok(StrictJson)
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Self::Value, E> {
                if v.is_finite() {
                    Ok(StrictJson)
                } else {
                    Err(E::custom("invalid JSON number"))
                }
            }
            fn visit_str<E: de::Error>(self, _: &str) -> Result<Self::Value, E> {
                Ok(StrictJson)
            }
            fn visit_none<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictJson)
            }
            fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
                Ok(StrictJson)
            }
            fn visit_seq<A: SeqAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                while a.next_element::<StrictJson>()?.is_some() {}
                Ok(StrictJson)
            }
            fn visit_map<A: MapAccess<'de>>(self, mut a: A) -> Result<Self::Value, A::Error> {
                let mut keys = HashSet::new();
                while let Some(key) = a.next_key::<String>()? {
                    if !keys.insert(key) {
                        return Err(de::Error::custom("duplicate JSON field"));
                    }
                    a.next_value::<StrictJson>()?;
                }
                Ok(StrictJson)
            }
        }
        deserializer.deserialize_any(V)
    }
}
/// Strict syntax/duplicate/size check only. It never constructs a restored DB.
pub fn validate_json_document(bytes: &[u8]) -> Result<(), ExportError> {
    if bytes.len() > EXPORT_MAX_BYTES {
        return Err(ExportError::TooLarge);
    }
    let mut parser = serde_json::Deserializer::from_slice(bytes);
    StrictJson::deserialize(&mut parser).map_err(|_| ExportError::Invalid)?;
    parser.end().map_err(|_| ExportError::Invalid)
}
impl IndividualExport {
    pub fn to_json_bytes(&self) -> Result<Vec<u8>, ExportError> {
        if self.format != EXPORT_FORMAT
            || self.individual_id.is_nil()
            || self.state_version <= 0
            || self.privacy_epoch < 0
            || self.schema_fingerprint.len() != 64
            || !self
                .schema_fingerprint
                .bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        {
            return Err(ExportError::Invalid);
        }
        for (table, rows) in &self.tables {
            if table.is_empty() || !table.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
                return Err(ExportError::Invalid);
            }
            if rows.len() > EXPORT_MAX_TABLE_ROWS {
                return Err(ExportError::TooManyRows);
            }
            if rows.iter().any(|r| !r.is_object()) {
                return Err(ExportError::Invalid);
            }
        }
        let mut writer = BoundedWriter {
            bytes: vec![],
            count: 0,
            limit: EXPORT_MAX_BYTES,
            exceeded: false,
            retain: true,
        };
        if serde_json::to_writer(&mut writer, self).is_err() {
            return Err(if writer.exceeded {
                ExportError::TooLarge
            } else {
                ExportError::Invalid
            });
        }
        validate_json_document(&writer.bytes)?;
        Ok(writer.bytes)
    }
}
