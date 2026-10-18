//! A faithful port of `scripts/lib/schema.mjs`, the documented-subset JSON
//! Schema validator the Node engine uses.
//!
//! The distribution ADR requires a shadow test proving the Rust and Node
//! validators print identical output before Rust may replace Node. Error text
//! is therefore produced by this port, with the same keyword order and the
//! same wording, rather than by re-phrasing the output of another library.
//! `crate::crosscheck` runs the `jsonschema` crate over the same data as an
//! independent second opinion.
//!
//! Known residual differences are listed in `docs/rust-shadow-notes.md`.

use regex::{Regex, RegexBuilder};
use serde_json::{Map, Value};

use crate::jsval::{deep_equal, number_to_js, stringify, to_js_string};

const ANNOTATIONS: &[&str] = &[
    "$schema",
    "$id",
    "$comment",
    "$defs",
    "definitions",
    "title",
    "description",
    "default",
    "examples",
    "deprecated",
];
const VALIDATION: &[&str] = &[
    "type",
    "required",
    "properties",
    "additionalProperties",
    "items",
    "enum",
    "const",
    "pattern",
    "minLength",
    "maxLength",
    "minimum",
    "maximum",
    "minItems",
    "maxItems",
    "uniqueItems",
    "format",
    "anyOf",
    "oneOf",
    "allOf",
    "not",
    "if",
    "then",
    "else",
    "$ref",
];
const TYPES: &[&str] = &[
    "object", "array", "string", "number", "integer", "boolean", "null",
];
const FORMATS: &[&str] = &["date", "date-time", "uri"];

/// One validation problem: a JSON pointer and a message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Issue {
    pub path: String,
    pub message: String,
}

/// The names of the validation keywords this subset supports.
pub fn supported_keywords() -> Vec<&'static str> {
    let mut v: Vec<&'static str> = VALIDATION.to_vec();
    v.sort_unstable();
    v
}

fn type_of(value: &Value) -> &'static str {
    match value {
        Value::Null => "null",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
        Value::String(_) => "string",
        Value::Bool(_) => "boolean",
        Value::Number(n) => {
            if n.is_i64() || n.is_u64() {
                "integer"
            } else {
                match n.as_f64() {
                    Some(f) if f.is_finite() && f.fract() == 0.0 => "integer",
                    _ => "number",
                }
            }
        }
    }
}

fn matches_type(value: &Value, ty: &str) -> bool {
    let actual = type_of(value);
    if ty == "number" {
        return actual == "number" || actual == "integer";
    }
    actual == ty
}

fn days_in_month(year: u32, month: u32) -> u32 {
    match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        _ => {
            if (year % 4 == 0 && year % 100 != 0) || year % 400 == 0 {
                29
            } else {
                28
            }
        }
    }
}

fn ascii_digits(s: &str) -> bool {
    !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit())
}

/// `YYYY-MM-DD` naming a real calendar day.
pub fn is_date(value: &str) -> bool {
    if !value.is_ascii() {
        return false;
    }
    let b = value.as_bytes();
    if b.len() != 10 || b[4] != b'-' || b[7] != b'-' {
        return false;
    }
    let (y, m, d) = (&value[0..4], &value[5..7], &value[8..10]);
    if !(ascii_digits(y) && ascii_digits(m) && ascii_digits(d)) {
        return false;
    }
    let (y, m, d): (u32, u32, u32) = (y.parse().unwrap(), m.parse().unwrap(), d.parse().unwrap());
    (1..=12).contains(&m) && d >= 1 && d <= days_in_month(y, m)
}

/// RFC 3339 date-time with a UTC offset.
pub fn is_date_time(value: &str) -> bool {
    if !value.is_ascii() {
        return false;
    }
    let Some((date, rest)) = value.split_once('T') else {
        return false;
    };
    if !is_date(date) {
        return false;
    }
    let (time, offset_ok) = if let Some(t) = rest.strip_suffix('Z') {
        (t, true)
    } else if rest.len() >= 6 {
        let (t, off) = rest.split_at(rest.len() - 6);
        let ob = off.as_bytes();
        let ok = (ob[0] == b'+' || ob[0] == b'-')
            && ascii_digits(&off[1..3])
            && ob[3] == b':'
            && ascii_digits(&off[4..6]);
        (t, ok)
    } else {
        return false;
    };
    if !offset_ok {
        return false;
    }
    let (hms, frac) = match time.split_once('.') {
        Some((h, f)) => (h, Some(f)),
        None => (time, None),
    };
    if let Some(f) = frac {
        if !ascii_digits(f) {
            return false;
        }
    }
    let parts: Vec<&str> = hms.split(':').collect();
    if parts.len() != 3 || parts.iter().any(|p| p.len() != 2 || !ascii_digits(p)) {
        return false;
    }
    let (h, m, s): (u32, u32, u32) = (
        parts[0].parse().unwrap(),
        parts[1].parse().unwrap(),
        parts[2].parse().unwrap(),
    );
    h <= 23 && m <= 59 && s <= 60
}

/// Approximation of `new URL(value)` succeeding (WHATWG URL parsing).
///
/// It accepts a syntactically valid scheme followed by something, and for the
/// special schemes (http, https, ws, wss, ftp) it also requires a non-empty
/// host without forbidden characters and a numeric port. It does not
/// implement IDNA, IPv4/IPv6 host validation or percent-decoding. See
/// `docs/rust-shadow-notes.md`.
pub fn is_uri(value: &str) -> bool {
    let trimmed = value.trim_matches(|c: char| c <= ' ');
    let cleaned: String = trimmed
        .chars()
        .filter(|c| !matches!(c, '\t' | '\n' | '\r'))
        .collect();
    if cleaned.is_empty() {
        return false;
    }
    let Some(colon) = cleaned.find(':') else {
        return false;
    };
    let scheme = &cleaned[..colon];
    let mut chars = scheme.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() => {}
        _ => return false,
    }
    if !chars.all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.')) {
        return false;
    }
    let scheme = scheme.to_ascii_lowercase();
    let rest = &cleaned[colon + 1..];
    match scheme.as_str() {
        "http" | "https" | "ws" | "wss" | "ftp" => {
            let after = rest.trim_start_matches(['/', '\\']);
            let authority_end = after.find(['/', '\\', '?', '#']).unwrap_or(after.len());
            let authority = &after[..authority_end];
            let host_port = authority.rsplit_once('@').map_or(authority, |(_, h)| h);
            let (host, port) = match host_port.rsplit_once(':') {
                Some((h, p)) if !h.ends_with(']') || h.starts_with('[') => (h, Some(p)),
                _ => (host_port, None),
            };
            if let Some(p) = port {
                if !p.is_empty()
                    && (!ascii_digits(p) || p.parse::<u32>().map_or(true, |n| n > 65535))
                {
                    return false;
                }
            }
            if host.is_empty() {
                return false;
            }
            !host.chars().any(|c| {
                c <= ' '
                    || matches!(
                        c,
                        '#' | '/' | '<' | '>' | '?' | '@' | '\\' | '^' | '|' | '\u{7f}'
                    )
            })
        }
        _ => true,
    }
}

fn pointer(path: &[String]) -> String {
    if path.is_empty() {
        return "/".to_string();
    }
    path.iter()
        .map(|part| format!("/{}", part.replace('~', "~0").replace('/', "~1")))
        .collect()
}

fn resolve_ref<'a>(root: &'a Value, reference: &str) -> Option<&'a Value> {
    if reference == "#" {
        return Some(root);
    }
    let rest = reference.strip_prefix("#/")?;
    let mut node = root;
    for raw in rest.split('/') {
        let key = raw.replace("~1", "/").replace("~0", "~");
        node = match node {
            Value::Object(map) => map.get(&key)?,
            Value::Array(items) => items.get(key.parse::<usize>().ok()?)?,
            _ => return None,
        };
    }
    Some(node)
}

fn build_regex(pattern: &str) -> Result<Regex, regex::Error> {
    // ASCII mode keeps `\d` and `\w` ASCII-only, like a JavaScript `u` regex.
    RegexBuilder::new(pattern).unicode(false).build()
}

/// Problems with the schema itself (empty means usable). Mirrors `checkSchema`.
pub fn check_schema(schema: &Value) -> Vec<String> {
    let mut problems = Vec::new();
    check_inner(schema, "#", schema, &mut problems);
    problems
}

fn check_inner(schema: &Value, location: &str, root: &Value, problems: &mut Vec<String>) {
    if schema.is_boolean() {
        return;
    }
    let Some(obj) = schema.as_object() else {
        problems.push(format!("{location}: schema must be an object or boolean"));
        return;
    };
    for key in obj.keys() {
        if !ANNOTATIONS.contains(&key.as_str()) && !VALIDATION.contains(&key.as_str()) {
            problems.push(format!("{location}: unsupported keyword \"{key}\""));
        }
    }
    if let Some(t) = obj.get("type") {
        let types: Vec<&Value> = match t {
            Value::Array(a) => a.iter().collect(),
            other => vec![other],
        };
        for ty in types {
            if !ty.as_str().is_some_and(|s| TYPES.contains(&s)) {
                problems.push(format!(
                    "{location}/type: unknown type \"{}\"",
                    to_js_string(ty)
                ));
            }
        }
    }
    if let Some(f) = obj.get("format") {
        if !f.as_str().is_some_and(|s| FORMATS.contains(&s)) {
            problems.push(format!(
                "{location}/format: unsupported format \"{}\"",
                to_js_string(f)
            ));
        }
    }
    if let Some(p) = obj.get("pattern") {
        match p.as_str().map(build_regex) {
            Some(Ok(_)) => {}
            Some(Err(e)) => problems.push(format!("{location}/pattern: {e}")),
            None => problems.push(format!("{location}/pattern: pattern must be a string")),
        }
    }
    if let Some(r) = obj.get("$ref") {
        if r.as_str().and_then(|s| resolve_ref(root, s)).is_none() {
            problems.push(format!(
                "{location}/$ref: unresolved reference \"{}\"",
                to_js_string(r)
            ));
        }
    }
    for key in ["properties", "$defs", "definitions"] {
        if let Some(Value::Object(children)) = obj.get(key) {
            for (name, child) in children {
                check_inner(child, &format!("{location}/{key}/{name}"), root, problems);
            }
        }
    }
    for key in ["items", "additionalProperties", "not", "if", "then", "else"] {
        if let Some(child) = obj.get(key) {
            if !child.is_boolean() {
                check_inner(child, &format!("{location}/{key}"), root, problems);
            }
        }
    }
    for key in ["anyOf", "oneOf", "allOf"] {
        if let Some(Value::Array(children)) = obj.get(key) {
            for (i, child) in children.iter().enumerate() {
                check_inner(child, &format!("{location}/{key}/{i}"), root, problems);
            }
        }
    }
}

fn keyword_number(obj: &Map<String, Value>, key: &str) -> Option<(f64, String)> {
    obj.get(key)
        .and_then(Value::as_number)
        .map(|n| (n.as_f64().unwrap_or(f64::NAN), number_to_js(n)))
}

fn closest(best: &[Issue]) -> String {
    best.iter()
        .take(3)
        .map(|e| format!("{} {}", e.path, e.message))
        .collect::<Vec<_>>()
        .join("; ")
}

fn run(
    schema: &Value,
    value: &Value,
    path: &mut Vec<String>,
    root: &Value,
    errors: &mut Vec<Issue>,
) {
    let obj = match schema {
        Value::Bool(true) => return,
        Value::Bool(false) => {
            errors.push(Issue {
                path: pointer(path),
                message: "no value is allowed here".to_string(),
            });
            return;
        }
        Value::Object(o) => o,
        _ => return,
    };
    let here = pointer(path);
    let fail = |errors: &mut Vec<Issue>, message: String| {
        errors.push(Issue {
            path: here.clone(),
            message,
        })
    };

    if let Some(reference) = obj.get("$ref") {
        match reference.as_str().and_then(|s| resolve_ref(root, s)) {
            Some(target) => run(target, value, path, root, errors),
            None => fail(
                errors,
                format!("unresolved reference {}", to_js_string(reference)),
            ),
        }
    }

    if let Some(t) = obj.get("type") {
        let types: Vec<&Value> = match t {
            Value::Array(a) => a.iter().collect(),
            other => vec![other],
        };
        if !types
            .iter()
            .any(|ty| ty.as_str().is_some_and(|s| matches_type(value, s)))
        {
            let names: Vec<String> = types.iter().map(|t| to_js_string(t)).collect();
            fail(
                errors,
                format!("expected {}, got {}", names.join(" or "), type_of(value)),
            );
            return;
        }
    }
    if let Some(c) = obj.get("const") {
        if !deep_equal(value, c) {
            fail(errors, format!("must equal {}", stringify(c)));
        }
    }
    if let Some(Value::Array(options)) = obj.get("enum") {
        if !options.iter().any(|o| deep_equal(value, o)) {
            let list: Vec<String> = options.iter().map(stringify).collect();
            fail(
                errors,
                format!(
                    "must be one of {}, got {}",
                    list.join(", "),
                    stringify(value)
                ),
            );
        }
    }

    if let Value::String(s) = value {
        if let Some((n, text)) = keyword_number(obj, "minLength") {
            if (s.chars().count() as f64) < n {
                fail(errors, format!("must be at least {text} characters"));
            }
        }
        if let Some((n, text)) = keyword_number(obj, "maxLength") {
            if (s.chars().count() as f64) > n {
                fail(errors, format!("must be at most {text} characters"));
            }
        }
        if let Some(Value::String(pattern)) = obj.get("pattern") {
            match build_regex(pattern) {
                Ok(re) => {
                    if !re.is_match(s) {
                        fail(errors, format!("must match pattern {pattern}"));
                    }
                }
                Err(_) => fail(errors, format!("must match pattern {pattern}")),
            }
        }
        match obj.get("format").and_then(Value::as_str) {
            Some("date") if !is_date(s) => {
                fail(errors, "must be an ISO date (YYYY-MM-DD)".to_string())
            }
            Some("date-time") if !is_date_time(s) => fail(
                errors,
                "must be an RFC 3339 date-time with a UTC offset".to_string(),
            ),
            Some("uri") if !is_uri(s) => fail(errors, "must be an absolute URI".to_string()),
            _ => {}
        }
    }
    if let Value::Number(n) = value {
        let v = n.as_f64().unwrap_or(f64::NAN);
        if let Some((m, text)) = keyword_number(obj, "minimum") {
            if v < m {
                fail(errors, format!("must be >= {text}"));
            }
        }
        if let Some((m, text)) = keyword_number(obj, "maximum") {
            if v > m {
                fail(errors, format!("must be <= {text}"));
            }
        }
    }
    if let Value::Array(items) = value {
        if let Some((n, text)) = keyword_number(obj, "minItems") {
            if (items.len() as f64) < n {
                fail(errors, format!("must have at least {text} items"));
            }
        }
        if let Some((n, text)) = keyword_number(obj, "maxItems") {
            if (items.len() as f64) > n {
                fail(errors, format!("must have at most {text} items"));
            }
        }
        if obj.get("uniqueItems") == Some(&Value::Bool(true)) {
            'outer: for i in 0..items.len() {
                for j in (i + 1)..items.len() {
                    if deep_equal(&items[i], &items[j]) {
                        fail(errors, format!("items {i} and {j} are duplicates"));
                        break 'outer;
                    }
                }
            }
        }
        if let Some(item_schema) = obj.get("items") {
            for (i, item) in items.iter().enumerate() {
                path.push(i.to_string());
                run(item_schema, item, path, root, errors);
                path.pop();
            }
        }
    }
    if let Value::Object(map) = value {
        if let Some(Value::Array(required)) = obj.get("required") {
            for key in required {
                let name = to_js_string(key);
                if !map.contains_key(&name) {
                    path.push(name);
                    errors.push(Issue {
                        path: pointer(path),
                        message: "required field is missing".to_string(),
                    });
                    path.pop();
                }
            }
        }
        let empty = Map::new();
        let properties = obj
            .get("properties")
            .and_then(Value::as_object)
            .unwrap_or(&empty);
        for (key, child) in map {
            if let Some(prop_schema) = properties.get(key) {
                path.push(key.clone());
                run(prop_schema, child, path, root, errors);
                path.pop();
            } else if obj.get("additionalProperties") == Some(&Value::Bool(false)) {
                path.push(key.clone());
                errors.push(Issue {
                    path: pointer(path),
                    message: "unknown field (move it under \"extensions\" or remove it)"
                        .to_string(),
                });
                path.pop();
            } else if let Some(extra @ Value::Object(_)) = obj.get("additionalProperties") {
                path.push(key.clone());
                run(extra, child, path, root, errors);
                path.pop();
            }
        }
    }

    if let Some(Value::Array(children)) = obj.get("allOf") {
        for child in children {
            run(child, value, path, root, errors);
        }
    }
    if let Some(not) = obj.get("not") {
        let mut sub = Vec::new();
        run(not, value, path, root, &mut sub);
        if sub.is_empty() {
            fail(errors, "must not match the excluded shape".to_string());
        }
    }
    if let Some(Value::Array(children)) = obj.get("anyOf") {
        let branches: Vec<Vec<Issue>> = children
            .iter()
            .map(|child| {
                let mut sub = Vec::new();
                run(child, value, path, root, &mut sub);
                sub
            })
            .collect();
        if !branches.iter().any(Vec::is_empty) {
            if let Some(best) = pick_best(&branches) {
                fail(
                    errors,
                    format!(
                        "does not match any allowed shape; closest: {}",
                        closest(best)
                    ),
                );
            }
        }
    }
    if let Some(Value::Array(children)) = obj.get("oneOf") {
        let branches: Vec<Vec<Issue>> = children
            .iter()
            .map(|child| {
                let mut sub = Vec::new();
                run(child, value, path, root, &mut sub);
                sub
            })
            .collect();
        let matched = branches.iter().filter(|s| s.is_empty()).count();
        if matched != 1 {
            if let Some(best) = pick_best(&branches) {
                if matched == 0 {
                    fail(
                        errors,
                        format!(
                            "does not match any allowed shape; closest: {}",
                            closest(best)
                        ),
                    );
                } else {
                    fail(
                        errors,
                        format!("matches {matched} shapes but must match exactly one"),
                    );
                }
            }
        }
    }
    if let Some(cond) = obj.get("if") {
        let mut sub = Vec::new();
        run(cond, value, path, root, &mut sub);
        if sub.is_empty() {
            if let Some(then) = obj.get("then") {
                run(then, value, path, root, errors);
            }
        } else if let Some(otherwise) = obj.get("else") {
            run(otherwise, value, path, root, errors);
        }
    }
}

/// First branch with the fewest issues, like `reduce((a, b) => b.length < a.length ? b : a)`.
fn pick_best(branches: &[Vec<Issue>]) -> Option<&Vec<Issue>> {
    branches
        .iter()
        .reduce(|a, b| if b.len() < a.len() { b } else { a })
}

/// Validate `value` against `schema`.
///
/// Returns the schema's own problems as `Err` when it uses an unsupported
/// keyword, so a constraint can never be skipped without notice.
pub fn validate_value(schema: &Value, value: &Value) -> Result<Vec<Issue>, Vec<String>> {
    let problems = check_schema(schema);
    if !problems.is_empty() {
        return Err(problems);
    }
    let mut errors = Vec::new();
    run(schema, value, &mut Vec::new(), schema, &mut errors);
    Ok(errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn issues(schema: Value, value: Value) -> Vec<String> {
        validate_value(&schema, &value)
            .unwrap()
            .into_iter()
            .map(|i| format!("{}: {}", i.path, i.message))
            .collect()
    }

    #[test]
    fn required_and_unknown_fields_use_node_wording() {
        let schema = json!({
            "type": "object",
            "properties": {"a": {"type": "string"}},
            "required": ["a", "b"],
            "additionalProperties": false
        });
        assert_eq!(
            issues(schema, json!({"a": 1, "z": 2})),
            vec![
                "/b: required field is missing",
                "/a: expected string, got integer",
                "/z: unknown field (move it under \"extensions\" or remove it)",
            ]
        );
    }

    #[test]
    fn enum_const_pattern_and_format_messages() {
        assert_eq!(
            issues(json!({"enum": ["x", "y"]}), json!("z")),
            vec!["/: must be one of \"x\", \"y\", got \"z\""]
        );
        assert_eq!(
            issues(json!({"const": "1.0.0"}), json!("2")),
            vec!["/: must equal \"1.0.0\""]
        );
        assert_eq!(
            issues(json!({"type": "string", "pattern": "^\\d+$"}), json!("a1")),
            vec!["/: must match pattern ^\\d+$"]
        );
        assert_eq!(
            issues(json!({"format": "date"}), json!("2026-02-30")),
            vec!["/: must be an ISO date (YYYY-MM-DD)"]
        );
    }

    #[test]
    fn any_of_reports_closest_branch() {
        let schema = json!({"anyOf": [{"type": "string", "minLength": 1}, {"type": "object"}]});
        assert_eq!(
            issues(schema, json!(3)),
            vec!["/: does not match any allowed shape; closest: / expected string, got integer"]
        );
    }

    #[test]
    fn unsupported_keywords_are_refused() {
        let err = validate_value(&json!({"patternProperties": {}}), &json!({})).unwrap_err();
        assert_eq!(err, vec!["#: unsupported keyword \"patternProperties\""]);
    }

    #[test]
    fn dates_follow_the_calendar() {
        assert!(is_date("2024-02-29"));
        assert!(!is_date("2023-02-29"));
        assert!(!is_date("2024-13-01"));
        assert!(!is_date("2024-1-01"));
    }

    #[test]
    fn uri_requires_a_scheme_and_host_for_web_schemes() {
        assert!(is_uri("https://www.ada.gov/law-and-regs/"));
        assert!(is_uri("mailto:a@b.example"));
        assert!(!is_uri("www.ada.gov"));
        assert!(!is_uri("https://"));
        assert!(!is_uri(""));
    }
}
