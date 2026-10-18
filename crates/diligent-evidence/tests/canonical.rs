//! `canonical_json` and cache-key hashing.

use diligent_evidence::{canonical_json, sha256_hex, CacheKey};
use serde_json::{json, Value};

#[test]
fn keys_are_sorted_at_every_depth_and_whitespace_is_gone() {
    let value = json!({ "b": 1, "a": { "z": [3, { "y": 1, "x": 2 }], "c": null }, "A": true });
    assert_eq!(
        canonical_json(&value),
        r#"{"A":true,"a":{"c":null,"z":[3,{"x":2,"y":1}]},"b":1}"#
    );
}

#[test]
fn insertion_order_does_not_matter_even_with_preserve_order() {
    let one: Value = serde_json::from_str(r#"{"a":1,"b":{"c":2,"d":3}}"#).unwrap();
    let two: Value = serde_json::from_str(r#"{"b":{"d":3,"c":2},"a":1}"#).unwrap();
    assert_ne!(
        one.to_string(),
        two.to_string(),
        "preserve_order keeps insertion order"
    );
    assert_eq!(canonical_json(&one), canonical_json(&two));
}

#[test]
fn arrays_keep_their_order() {
    assert_eq!(canonical_json(&json!([3, 1, 2])), "[3,1,2]");
    assert_ne!(
        canonical_json(&json!([1, 2])),
        canonical_json(&json!([2, 1]))
    );
}

#[test]
fn nulls_are_kept_and_distinct_from_absent() {
    assert_eq!(canonical_json(&json!({ "a": null })), r#"{"a":null}"#);
    assert_ne!(
        canonical_json(&json!({ "a": null })),
        canonical_json(&json!({}))
    );
}

#[test]
fn unicode_is_written_as_is_and_sorted_by_utf8_bytes() {
    let value = json!({ "é": "ü", "z": "日本", "a": "\u{1F600}" });
    let text = canonical_json(&value);
    assert_eq!(text, "{\"a\":\"\u{1F600}\",\"z\":\"日本\",\"é\":\"ü\"}");
    // Control characters and quotes are escaped the way serde_json escapes them.
    assert_eq!(canonical_json(&json!("a\"b\\c\n")), r#""a\"b\\c\n""#);
}

#[test]
fn integral_floats_become_integers_and_other_floats_stay() {
    assert_eq!(canonical_json(&json!(1.0)), "1");
    assert_eq!(canonical_json(&json!(1)), "1");
    assert_eq!(canonical_json(&json!(-0.0)), "0");
    assert_eq!(canonical_json(&json!(2.5)), "2.5");
    assert_eq!(canonical_json(&json!(0.1)), "0.1");
    assert_eq!(canonical_json(&json!(-3.0)), "-3");
    assert_eq!(canonical_json(&json!(u64::MAX)), u64::MAX.to_string());
    assert_eq!(canonical_json(&json!(i64::MIN)), i64::MIN.to_string());
    let from_text: Value = serde_json::from_str("[1.0, 1, 1e2, 100.0]").unwrap();
    assert_eq!(canonical_json(&from_text), "[1,1,100,100]");
}

fn key_from(text: &str) -> CacheKey {
    serde_json::from_str(text).expect("key parses")
}

const KEY_A: &str = r#"{
  "deployment": {"commit": "abc", "deployment_id": null},
  "route": "https://example.gov/",
  "role": null,
  "state": null,
  "viewport": {"width": 1280, "height": 800, "dpr": 1.0},
  "media": "baseline",
  "locale": "en-US",
  "rule_version": "r1",
  "engine_version": "0.1.0",
  "protocol_version": "1.1.0",
  "collector": "diligent-runtime",
  "kind": "dom_snapshot"
}"#;

const KEY_A_REORDERED: &str = r#"{
  "kind": "dom_snapshot",
  "collector": "diligent-runtime",
  "protocol_version": "1.1.0",
  "engine_version": "0.1.0",
  "rule_version": "r1",
  "locale": "en-US",
  "media": "baseline",
  "viewport": {"dpr": 1, "height": 800, "width": 1280},
  "state": null,
  "role": null,
  "route": "https://example.gov/",
  "deployment": {"deployment_id": null, "commit": "abc"}
}"#;

#[test]
fn key_hash_is_stable_across_field_order_and_number_spelling() {
    let a = key_from(KEY_A);
    let b = key_from(KEY_A_REORDERED);
    assert_eq!(a.hash(), b.hash());
    assert_eq!(a.canonical(), b.canonical());
}

#[test]
fn key_hash_is_the_sha256_of_the_canonical_form() {
    let key = key_from(KEY_A);
    let expected = concat!(
        r#"{"collector":"diligent-runtime","deployment":{"commit":"abc","deployment_id":null},"#,
        r#""engine_version":"0.1.0","kind":"dom_snapshot","locale":"en-US","media":"baseline","#,
        r#""protocol_version":"1.1.0","role":null,"route":"https://example.gov/","rule_version":"r1","#,
        r#""state":null,"viewport":{"dpr":1,"height":800,"width":1280}}"#
    );
    assert_eq!(key.canonical(), expected);
    assert_eq!(key.hash(), sha256_hex(expected.as_bytes()));
    assert_eq!(key.hash().len(), 64);
}

#[test]
fn every_dimension_changes_the_hash() {
    let base = key_from(KEY_A);
    let mut variants: Vec<CacheKey> = Vec::new();
    let mut v = base.clone();
    v.deployment.commit = Some("abd".into());
    variants.push(v);
    let mut v = base.clone();
    v.deployment.deployment_id = Some("d1".into());
    variants.push(v);
    let mut v = base.clone();
    v.route.push('x');
    variants.push(v);
    let mut v = base.clone();
    v.role = Some("admin".into());
    variants.push(v);
    let mut v = base.clone();
    v.state = Some("logged-in".into());
    variants.push(v);
    let mut v = base.clone();
    v.viewport.as_mut().unwrap().width = 1281;
    variants.push(v);
    let mut v = base.clone();
    v.viewport.as_mut().unwrap().dpr = 2.0;
    variants.push(v);
    let mut v = base.clone();
    v.media = Some("dark".into());
    variants.push(v);
    let mut v = base.clone();
    v.locale = None;
    variants.push(v);
    let mut v = base.clone();
    v.rule_version = Some("r2".into());
    variants.push(v);
    let mut v = base.clone();
    v.engine_version = "0.2.0".into();
    variants.push(v);
    let mut v = base.clone();
    v.protocol_version = None;
    variants.push(v);
    let mut v = base.clone();
    v.collector = "other".into();
    variants.push(v);
    let mut v = base.clone();
    v.kind = "ax_tree".into();
    variants.push(v);
    let mut hashes: Vec<String> = variants.iter().map(CacheKey::hash).collect();
    hashes.push(base.hash());
    let count = hashes.len();
    hashes.sort();
    hashes.dedup();
    assert_eq!(hashes.len(), count, "no two keys may share a hash");
}

#[test]
fn empty_dimensions_name_what_needs_a_reason() {
    let key = key_from(KEY_A);
    assert_eq!(key.empty_dimensions(), vec!["role", "state"]);
    let mut none = key.clone();
    none.deployment.commit = None;
    none.locale = None;
    assert_eq!(
        none.empty_dimensions(),
        vec!["role", "state", "locale", "deployment"]
    );
}
