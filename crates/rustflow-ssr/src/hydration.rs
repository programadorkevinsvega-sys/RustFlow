//! Bounded, HTML-script-safe JSON data for a future hydration protocol.
//!
//! This module deliberately accepts only JSON values. Callers must project explicitly
//! selected public fields into a `serde_json::Value`; the framework cannot infer whether
//! a string or field is semantically secret. This type does not render a script element or
//! implement hydration.

use serde_json::Value;
use std::{error::Error, fmt};

const MAX_SERIALIZED_BYTES: usize = 1_048_576;
const MAX_NESTING_DEPTH: usize = 64;

/// A bounded compact JSON payload escaped for safe use as HTML script-element text.
///
/// The returned string remains valid JSON: HTML-significant characters are represented
/// with JSON Unicode escapes. It is data, not HTML, and is not a hydration protocol by itself.
/// Construct the value from an explicit allow-list of public fields; do not pass sessions,
/// credentials, request objects, private server state, or arbitrary application structs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HydrationData {
    serialized: String,
}

/// Validation failures for a future hydration-data payload.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HydrationDataError {
    /// The input is not valid JSON.
    InvalidJson,
    /// The payload exceeds the encoded-byte limit.
    PayloadTooLarge { limit: usize },
    /// The payload exceeds the permitted nesting depth.
    NestingLimitExceeded { limit: usize },
    /// JSON serialization failed.
    SerializationFailed,
}

impl fmt::Display for HydrationDataError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidJson => f.write_str("invalid JSON hydration data"),
            Self::PayloadTooLarge { limit } => {
                write!(f, "hydration data exceeds the {limit}-byte limit")
            }
            Self::NestingLimitExceeded { limit } => {
                write!(f, "hydration data exceeds the {limit}-level nesting limit")
            }
            Self::SerializationFailed => f.write_str("failed to serialize hydration data"),
        }
    }
}

impl Error for HydrationDataError {}

impl HydrationData {
    /// Parse JSON and re-encode it as compact, HTML-script-safe JSON.
    ///
    /// This rejects raw inputs larger than 1 MiB before parsing, as well as values whose
    /// normalized/escaped representation is larger than 1 MiB or exceeds 64 levels of nesting.
    pub fn from_json(input: &str) -> Result<Self, HydrationDataError> {
        if input.len() > MAX_SERIALIZED_BYTES {
            return Err(HydrationDataError::PayloadTooLarge {
                limit: MAX_SERIALIZED_BYTES,
            });
        }
        let value: Value =
            serde_json::from_str(input).map_err(|_| HydrationDataError::InvalidJson)?;
        Self::from_value(&value)
    }

    /// Encode a JSON value as compact, HTML-script-safe JSON under explicit resource limits.
    ///
    /// Only JSON data can cross this boundary. Project public fields into a
    /// `serde_json::Value` explicitly; this API does not inspect application semantics and
    /// cannot detect a secret that a caller deliberately places in the value.
    pub fn from_value(value: &Value) -> Result<Self, HydrationDataError> {
        validate_value(value)?;
        let normalized = normalize_value(value);
        let json = serde_json::to_string(&normalized)
            .map_err(|_| HydrationDataError::SerializationFailed)?;
        let serialized = escape_html_script_data(&json);
        if serialized.len() > MAX_SERIALIZED_BYTES {
            return Err(HydrationDataError::PayloadTooLarge {
                limit: MAX_SERIALIZED_BYTES,
            });
        }
        Ok(Self { serialized })
    }

    /// Return the validated compact JSON string.
    ///
    /// This string is suitable as data inside an HTML script-element context, but is not an HTML
    /// fragment, does not create a script element, and does not imply client hydration support.
    pub fn as_str(&self) -> &str {
        &self.serialized
    }
}

// Do not rely on serde_json::Map's feature-dependent iteration order. The MongoDB
// adapter enables serde_json's preserve_order feature through Cargo feature unification,
// but hydration payloads still need a stable canonical representation.
fn normalize_value(value: &Value) -> Value {
    match value {
        Value::Array(values) => Value::Array(values.iter().map(normalize_value).collect()),
        Value::Object(values) => {
            let mut keys: Vec<_> = values.keys().collect();
            keys.sort_unstable();

            let mut normalized = serde_json::Map::new();
            for key in keys {
                if let Some(child) = values.get(key) {
                    normalized.insert(key.clone(), normalize_value(child));
                }
            }
            Value::Object(normalized)
        }
        _ => value.clone(),
    }
}

fn validate_value(value: &Value) -> Result<(), HydrationDataError> {
    let mut pending = vec![(value, 0_usize)];
    let mut estimated_bytes = 0_usize;

    while let Some((current, depth)) = pending.pop() {
        if depth > MAX_NESTING_DEPTH {
            return Err(HydrationDataError::NestingLimitExceeded {
                limit: MAX_NESTING_DEPTH,
            });
        }

        let additional_bytes = match current {
            Value::Null => 4,
            Value::Bool(true) => 4,
            Value::Bool(false) => 5,
            Value::Number(number) => number.to_string().len(),
            Value::String(value) => value.len().saturating_add(2),
            Value::Array(values) => {
                pending.extend(values.iter().map(|child| (child, depth.saturating_add(1))));
                2_usize.saturating_add(values.len().saturating_sub(1))
            }
            Value::Object(values) => {
                for (key, child) in values {
                    estimated_bytes = estimated_bytes.saturating_add(key.len().saturating_add(3));
                    pending.push((child, depth.saturating_add(1)));
                }
                2_usize.saturating_add(values.len().saturating_sub(1))
            }
        };

        estimated_bytes = estimated_bytes.saturating_add(additional_bytes);
        if estimated_bytes > MAX_SERIALIZED_BYTES {
            return Err(HydrationDataError::PayloadTooLarge {
                limit: MAX_SERIALIZED_BYTES,
            });
        }
    }
    Ok(())
}

fn escape_html_script_data(json: &str) -> String {
    let mut escaped = String::with_capacity(json.len());
    for character in json.chars() {
        match character {
            '<' => escaped.push_str("\\u003c"),
            '>' => escaped.push_str("\\u003e"),
            '&' => escaped.push_str("\\u0026"),
            '\u{2028}' => escaped.push_str("\\u2028"),
            '\u{2029}' => escaped.push_str("\\u2029"),
            other => escaped.push(other),
        }
    }
    escaped
}

#[cfg(test)]
mod tests {
    use super::{HydrationData, HydrationDataError, MAX_NESTING_DEPTH, MAX_SERIALIZED_BYTES};
    use serde_json::{Value, json};

    #[test]
    fn escapes_html_script_sensitive_characters_and_keeps_valid_json() {
        let value = json!({
            "markup": "</script><script>safe & text</script>",
            "separators": "\u{2028}\u{2029}",
        });
        let payload = HydrationData::from_value(&value).expect("valid public JSON value");
        let encoded = payload.as_str();

        assert!(!encoded.contains('<'));
        assert!(!encoded.contains('>'));
        assert!(!encoded.contains('&'));
        assert!(encoded.contains("\\u003c/script\\u003e"));
        assert!(encoded.contains("\\u2028"));
        assert!(encoded.contains("\\u2029"));
        assert_eq!(
            serde_json::from_str::<Value>(encoded).expect("valid JSON"),
            value
        );
    }

    #[test]
    fn normalizes_json_and_repeats_identically_for_the_same_value() {
        let first = HydrationData::from_json(r#"{"z": 1, "a": [true, null]}"#).expect("valid JSON");
        let second =
            HydrationData::from_json(r#"{"z": 1, "a": [true, null]}"#).expect("valid JSON");

        assert_eq!(first, second);
        assert_eq!(first.as_str(), r#"{"a":[true,null],"z":1}"#);

        let nested = HydrationData::from_json(r#"{"z":{"y":2,"b":3},"a":[{"z":0,"c":1}]}"#)
            .expect("valid nested JSON");
        assert_eq!(
            nested.as_str(),
            r#"{"a":[{"c":1,"z":0}],"z":{"b":3,"y":2}}"#
        );
    }

    #[test]
    fn rejects_malformed_and_oversized_input() {
        assert!(matches!(
            HydrationData::from_json("{"),
            Err(HydrationDataError::InvalidJson)
        ));

        let oversized = "x".repeat(MAX_SERIALIZED_BYTES + 1);
        assert!(matches!(
            HydrationData::from_json(&oversized),
            Err(HydrationDataError::PayloadTooLarge { .. })
        ));
    }

    #[test]
    fn rejects_values_that_exceed_nesting_or_encoded_size_limits() {
        let mut deeply_nested = Value::Null;
        for _ in 0..=MAX_NESTING_DEPTH {
            deeply_nested = Value::Array(vec![deeply_nested]);
        }
        assert!(matches!(
            HydrationData::from_value(&deeply_nested),
            Err(HydrationDataError::NestingLimitExceeded { .. })
        ));

        let escaped_overflow = Value::String("<".repeat(200_000));
        assert!(matches!(
            HydrationData::from_value(&escaped_overflow),
            Err(HydrationDataError::PayloadTooLarge { .. })
        ));
    }
}
