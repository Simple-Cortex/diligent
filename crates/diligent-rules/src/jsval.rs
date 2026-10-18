//! Small helpers that reproduce the JavaScript value semantics the Node
//! validator relies on: truthiness, string conversion, `JSON.stringify`,
//! `.length` and `String.prototype.trim`. They exist so that error text built
//! from arbitrary (possibly invalid) JSON matches the Node output.

use serde_json::{Number, Value};

/// JavaScript number-to-string for a JSON number.
pub fn number_to_js(n: &Number) -> String {
    if let Some(i) = n.as_i64() {
        return i.to_string();
    }
    if let Some(u) = n.as_u64() {
        return u.to_string();
    }
    let f = n.as_f64().unwrap_or(f64::NAN);
    if f.is_finite() && f.fract() == 0.0 && f.abs() < 1e21 {
        return format!("{f:.0}");
    }
    format!("{f}")
}

/// `String(value)` for a present JSON value.
pub fn to_js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number_to_js(n),
        Value::String(s) => s.clone(),
        Value::Array(items) => items
            .iter()
            .map(|v| match v {
                Value::Null => String::new(),
                other => to_js_string(other),
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".to_string(),
    }
}

/// `String(value)` for a possibly absent value (`undefined`).
pub fn opt_to_js_string(value: Option<&Value>) -> String {
    match value {
        None => "undefined".to_string(),
        Some(v) => to_js_string(v),
    }
}

/// JavaScript truthiness for a possibly absent value.
pub fn truthy(value: Option<&Value>) -> bool {
    match value {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().map(|f| f != 0.0 && !f.is_nan()).unwrap_or(true),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    }
}

/// `value.length` as used by `!x?.length`: arrays and strings only.
/// Strings count UTF-16 code units like JavaScript.
pub fn length(value: Option<&Value>) -> Option<usize> {
    match value {
        Some(Value::Array(a)) => Some(a.len()),
        Some(Value::String(s)) => Some(s.encode_utf16().count()),
        _ => None,
    }
}

/// JavaScript `WhiteSpace` or `LineTerminator` code point (what `trim` removes).
pub fn is_js_whitespace(c: char) -> bool {
    matches!(
        c,
        '\u{09}'
            | '\u{0A}'
            | '\u{0B}'
            | '\u{0C}'
            | '\u{0D}'
            | '\u{20}'
            | '\u{A0}'
            | '\u{1680}'
            | '\u{2000}'
            ..='\u{200A}'
                | '\u{2028}'
                | '\u{2029}'
                | '\u{202F}'
                | '\u{205F}'
                | '\u{3000}'
                | '\u{FEFF}'
    )
}

/// `s.trim() === ''`.
pub fn trims_to_empty(s: &str) -> bool {
    s.chars().all(is_js_whitespace)
}

/// `JSON.stringify(value)` without indentation.
pub fn stringify(value: &Value) -> String {
    match value {
        Value::Null => "null".to_string(),
        Value::Bool(b) => b.to_string(),
        Value::Number(n) => number_to_js(n),
        Value::String(s) => serde_json::to_string(s).unwrap_or_default(),
        Value::Array(items) => {
            let inner: Vec<String> = items.iter().map(stringify).collect();
            format!("[{}]", inner.join(","))
        }
        Value::Object(map) => {
            let inner: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    format!(
                        "{}:{}",
                        serde_json::to_string(k).unwrap_or_default(),
                        stringify(v)
                    )
                })
                .collect();
            format!("{{{}}}", inner.join(","))
        }
    }
}

/// Structural equality with JavaScript number semantics (`1 === 1.0`) and
/// key-order-insensitive objects, like the Node `deepEqual`.
pub fn deep_equal(a: &Value, b: &Value) -> bool {
    match (a, b) {
        (Value::Number(x), Value::Number(y)) => x.as_f64() == y.as_f64(),
        (Value::Array(x), Value::Array(y)) => {
            x.len() == y.len() && x.iter().zip(y).all(|(p, q)| deep_equal(p, q))
        }
        (Value::Object(x), Value::Object(y)) => {
            x.len() == y.len()
                && x.iter()
                    .all(|(k, v)| y.get(k).is_some_and(|w| deep_equal(v, w)))
        }
        _ => a == b,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn string_conversion_matches_javascript() {
        assert_eq!(to_js_string(&json!(5)), "5");
        assert_eq!(to_js_string(&json!(5.0)), "5");
        assert_eq!(to_js_string(&json!(null)), "null");
        assert_eq!(to_js_string(&json!([1, null, "a"])), "1,,a");
        assert_eq!(to_js_string(&json!({"a": 1})), "[object Object]");
        assert_eq!(opt_to_js_string(None), "undefined");
    }

    #[test]
    fn truthiness_matches_javascript() {
        assert!(!truthy(None));
        assert!(!truthy(Some(&json!(""))));
        assert!(!truthy(Some(&json!(0))));
        assert!(truthy(Some(&json!([]))));
        assert!(truthy(Some(&json!("x"))));
    }

    #[test]
    fn trim_uses_javascript_whitespace() {
        assert!(trims_to_empty(" \t\u{FEFF}\u{2028}"));
        // U+0085 is White_Space in Rust but not trimmed by JavaScript.
        assert!(!trims_to_empty("\u{85}"));
    }

    #[test]
    fn stringify_keeps_order_and_integers() {
        assert_eq!(
            stringify(&json!({"b": 1.0, "a": [true, null]})),
            r#"{"b":1,"a":[true,null]}"#
        );
    }
}
