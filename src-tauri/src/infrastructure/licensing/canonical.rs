//! Deterministic Canonical JSON serializer (artifact-spec §3).
//!
//! Generic over `serde::Serialize` — callers never see `serde_json::Value`. The
//! canonicalization mechanism is confined to this file so it can be swapped in
//! isolation.

use serde::ser::Serialize;
use serde_json::Value;
use std::fmt::Write as _;

/// Errors produced by canonical serialization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CanonicalError {
    /// A float was encountered; the contract allows integers only (artifact-spec §3).
    UnsupportedFloat,
    /// Underlying serde_json failure.
    Serialization(String),
}

impl std::fmt::Display for CanonicalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFloat => write!(f, "canonical JSON: floats are not allowed"),
            Self::Serialization(msg) => write!(f, "canonical JSON: {msg}"),
        }
    }
}

impl std::error::Error for CanonicalError {}

/// Serializes `value` to canonical JSON bytes.
///
/// Guarantees (artifact-spec §3):
/// - object keys sorted in byte order,
/// - no insignificant whitespace,
/// - integers only (floats are rejected),
/// - minimal escaping (`"`, `\`, control chars escaped; `/` and non-ASCII left as-is),
/// - same input always produces identical bytes.
pub fn canonical_serialize<T: Serialize>(value: &T) -> Result<Vec<u8>, CanonicalError> {
    let json =
        serde_json::to_value(value).map_err(|e| CanonicalError::Serialization(e.to_string()))?;
    let mut out = String::with_capacity(256);
    write_canonical(&json, &mut out)?;
    Ok(out.into_bytes())
}

fn write_canonical(value: &Value, out: &mut String) -> Result<(), CanonicalError> {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                write!(out, "{i}").map_err(serialization_err)?;
            } else if let Some(u) = n.as_u64() {
                write!(out, "{u}").map_err(serialization_err)?;
            } else {
                return Err(CanonicalError::UnsupportedFloat);
            }
        }
        Value::String(s) => {
            let encoded = serde_json::to_string(&Value::String(s.clone()))
                .map_err(|e| CanonicalError::Serialization(e.to_string()))?;
            out.push_str(&encoded);
        }
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_canonical(item, out)?;
            }
            out.push(']');
        }
        Value::Object(map) => {
            out.push('{');
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            for (i, key) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                let encoded = serde_json::to_string(&Value::String((*key).clone()))
                    .map_err(|e| CanonicalError::Serialization(e.to_string()))?;
                out.push_str(&encoded);
                out.push(':');
                write_canonical(map.get(*key).expect("key present"), out)?;
            }
            out.push('}');
        }
    }
    Ok(())
}

fn serialization_err(e: std::fmt::Error) -> CanonicalError {
    CanonicalError::Serialization(e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn deterministic_for_same_input() {
        let input = json!({"z": 1, "a": {"y": 2, "b": [true, null, "s"]}, "m": -3});
        let first = canonical_serialize(&input).unwrap();
        let second = canonical_serialize(&input).unwrap();
        assert_eq!(first, second);
    }

    #[test]
    fn sorts_object_keys_in_byte_order() {
        let input = json!({"b": 1, "a": 2, "c": 3});
        let bytes = canonical_serialize(&input).unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), r#"{"a":2,"b":1,"c":3}"#);
    }

    #[test]
    fn no_whitespace() {
        let bytes = canonical_serialize(&json!({"a": [1, 2, {"b": true}]})).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(!s.chars().any(char::is_whitespace));
    }

    #[test]
    fn rejects_floats() {
        assert_eq!(
            canonical_serialize(&json!(1.5)),
            Err(CanonicalError::UnsupportedFloat)
        );
        assert_eq!(
            canonical_serialize(&json!({"a": 1.0})),
            Err(CanonicalError::UnsupportedFloat)
        );
    }

    #[test]
    fn minimal_escaping() {
        let bytes = canonical_serialize(&json!({"k": "a/b \"q\" \n"})).unwrap();
        let s = String::from_utf8(bytes).unwrap();
        assert!(s.contains("a/b"));
        assert!(s.contains(r#"\""#));
        assert!(s.contains(r"\n"));
        assert!(!s.contains(r"\/"));
    }

    #[test]
    fn round_trip_parses_back_to_same_value() {
        let input = json!({"a": [1, "x", null, true], "b": {"c": -5}});
        let bytes = canonical_serialize(&input).unwrap();
        let parsed: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(parsed, input);
    }

    #[test]
    fn large_u64_is_supported() {
        let bytes = canonical_serialize(&json!(u64::MAX)).unwrap();
        assert_eq!(String::from_utf8(bytes).unwrap(), u64::MAX.to_string());
    }
}
