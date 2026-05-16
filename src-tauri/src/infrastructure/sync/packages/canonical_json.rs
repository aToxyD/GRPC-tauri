//! Deterministic JSON bytes for sync package integrity and signatures (schema ≥ V2).
//!
//! Contract: UTF-8 JSON with **lexicographically sorted object keys** (Rust `String` / Unicode
//! scalar ordering) at every object nesting level, **no** insignificant whitespace,
//! arrays preserve element order. Scalars follow `serde_json::Value` / JSON semantics.
//!
//! See ADR 0005 and `docs/architecture/0009-sync-package-canonical-json-v2.md`.

use serde_json::Value;

use crate::errors::AppResult;

/// Serialize a JSON value to canonical UTF-8 bytes (no trailing newline).
pub fn canonical_json_bytes(value: &Value) -> AppResult<Vec<u8>> {
    let mut out = Vec::new();
    write_value(&mut out, value)?;
    Ok(out)
}

fn write_value(out: &mut Vec<u8>, value: &Value) -> AppResult<()> {
    match value {
        Value::Null => out.extend_from_slice(b"null"),
        Value::Bool(true) => out.extend_from_slice(b"true"),
        Value::Bool(false) => out.extend_from_slice(b"false"),
        Value::Number(n) => {
            let s = n.to_string();
            out.extend_from_slice(s.as_bytes());
        }
        Value::String(s) => write_json_string(out, s),
        Value::Array(items) => {
            out.push(b'[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_value(out, item)?;
            }
            out.push(b']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push(b'{');
            for (i, k) in keys.iter().enumerate() {
                if i > 0 {
                    out.push(b',');
                }
                write_json_string(out, k);
                out.push(b':');
                write_value(out, &map[*k])?;
            }
            out.push(b'}');
        }
    }
    Ok(())
}

fn write_json_string(out: &mut Vec<u8>, s: &str) {
    // Delegate escaping to serde_json for spec-correct minimal escapes.
    let v = Value::String(s.to_string());
    let encoded = serde_json::to_string(&v).unwrap_or_else(|_| "\"\"".to_string());
    out.extend_from_slice(encoded.as_bytes());
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn same_logical_object_same_bytes_regardless_of_key_insertion_order() {
        let a = json!({"z": 1, "a": 2, "m": {"b": true, "a": null}});
        let b = json!({"m": {"a": null, "b": true}, "a": 2, "z": 1});
        assert_eq!(
            canonical_json_bytes(&a).unwrap(),
            canonical_json_bytes(&b).unwrap()
        );
    }

    #[test]
    fn canonical_bytes_are_stable_across_calls() {
        let v = json!({"metadata": {"x": 1}, "payload": [3, 2, 1]});
        let x = canonical_json_bytes(&v).unwrap();
        let y = canonical_json_bytes(&v).unwrap();
        assert_eq!(x, y);
    }
}
