//! Canonical JSON for hashing cache keys.

use serde_json::Value;

/// Serialises `value` as canonical JSON: object keys sorted by their UTF-8
/// bytes at every depth, no whitespace, nulls kept, and numbers with no
/// fractional part written as integers (`1.0` and `1` are the same number).
///
/// Strings are written exactly as `serde_json` escapes them, so non-ASCII
/// characters stay as they are. Never hash `Value::to_string`: the workspace
/// builds `serde_json` with `preserve_order`, which keeps insertion order.
pub fn canonical_json(value: &Value) -> String {
    let mut out = String::new();
    write_value(value, &mut out);
    out
}

fn write_value(value: &Value, out: &mut String) {
    match value {
        Value::Null => out.push_str("null"),
        Value::Bool(true) => out.push_str("true"),
        Value::Bool(false) => out.push_str("false"),
        Value::Number(n) => {
            if let Some(i) = n.as_i64() {
                out.push_str(&i.to_string());
            } else if let Some(u) = n.as_u64() {
                out.push_str(&u.to_string());
            } else if let Some(f) = n.as_f64() {
                // 2^53: below this every integral f64 is exact as an i64.
                if f.is_finite() && f.fract() == 0.0 && f.abs() < 9_007_199_254_740_992.0 {
                    out.push_str(&(f as i64).to_string());
                } else {
                    out.push_str(&n.to_string());
                }
            } else {
                out.push_str(&n.to_string());
            }
        }
        Value::String(s) => out.push_str(&serde_json::to_string(s).unwrap_or_default()),
        Value::Array(items) => {
            out.push('[');
            for (i, item) in items.iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                write_value(item, out);
            }
            out.push(']');
        }
        Value::Object(map) => {
            let mut keys: Vec<&String> = map.keys().collect();
            keys.sort();
            out.push('{');
            for (i, key) in keys.into_iter().enumerate() {
                if i > 0 {
                    out.push(',');
                }
                out.push_str(&serde_json::to_string(key).unwrap_or_default());
                out.push(':');
                write_value(&map[key.as_str()], out);
            }
            out.push('}');
        }
    }
}
