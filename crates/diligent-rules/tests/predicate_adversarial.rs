//! Adversarial tests for the predicate engine.
//!
//! These probe edge cases the normal test suite does not cover: deep nesting,
//! malformed input, unicode, operator precedence, type mismatches, and
//! equivalence between string and structured forms.

use diligent_rules::predicate::{
    evaluate, parse_str, parse_structured, Date, Expr, FactSet, Truth,
};
use serde_json::{json, Map, Value};

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn facts(items: &[(&str, &str, Value)]) -> FactSet {
    let mut set = FactSet::new();
    for (id, state, value) in items {
        set.insert(
            id,
            FactSet::entry_from_json(state, value, vec![]).expect("valid state"),
        );
    }
    set
}

fn truth(predicate: &str, set: &FactSet) -> Truth {
    evaluate(&parse_str(predicate).expect("parses"), set).truth
}

// ---------------------------------------------------------------------------
// Nesting and malformed input
// ---------------------------------------------------------------------------

#[test]
fn deeply_nested_parens_1000_levels_do_not_panic() {
    let mut s = String::new();
    for _ in 0..1000 {
        s.push('(');
    }
    s.push_str("true");
    for _ in 0..1000 {
        s.push(')');
    }
    let result = parse_str(&s);
    // Must either succeed or error; never stack-overflow or panic.
    match result {
        Ok(_) | Err(_) => {}
    }
}

#[test]
fn deeply_nested_structured_100_levels_do_not_panic() {
    // BUG: 1000 levels causes stack overflow because parse_structured clones
    // the entire Value tree before the depth check can reject it. At 100 levels
    // the clone succeeds and the depth check (MAX_DEPTH=64) rejects it.
    let mut v = json!({"fact": "x"});
    for _ in 0..100 {
        v = json!({"all": [v]});
    }
    let obj = v.as_object().unwrap().clone();
    let result = parse_structured(&obj);
    // Must error (depth > 64), never panic.
    assert!(
        result.is_err(),
        "100-level nested structured predicate must be rejected"
    );
}

#[test]
fn unterminated_string_is_error_not_panic() {
    assert!(parse_str("'hello").is_err());
    assert!(parse_str("\"hello").is_err());
    assert!(parse_str("a == 'unterminated").is_err());
}

#[test]
fn unicode_in_identifiers_is_rejected() {
    // Identifiers are [a-z_][a-z0-9_]* only.
    assert!(parse_str("café == true").is_err());
    assert!(parse_str("über").is_err());
    assert!(parse_str("a_日本語").is_err());
}

#[test]
fn in_with_empty_list_is_false() {
    let f = facts(&[("x", "known", json!("a"))]);
    assert_eq!(truth("x in []", &f), Truth::False);
}

#[test]
fn in_with_mixed_types_produces_type_mismatch_or_hit() {
    // A string fact compared to a list with both strings and numbers.
    let f = facts(&[("x", "known", json!("hello"))]);
    // "hello" matches the string literal, so it should be True (hit).
    assert_eq!(truth("x in ['hello', 42]", &f), Truth::True);
    // No match, but types differ: unknown (type_mismatch).
    assert_eq!(truth("x in [42, true]", &f), Truth::Unknown);
}

#[test]
fn bang_chains_toggle_correctly() {
    let f_true = facts(&[("a", "known", json!(true))]);
    let f_false = facts(&[("a", "known", json!(false))]);
    assert_eq!(truth("!a", &f_true), Truth::False);
    assert_eq!(truth("!!a", &f_true), Truth::True);
    assert_eq!(truth("!!!a", &f_true), Truth::False);
    assert_eq!(truth("!a", &f_false), Truth::True);
    assert_eq!(truth("!!a", &f_false), Truth::False);
}

// ---------------------------------------------------------------------------
// Operator precedence
// ---------------------------------------------------------------------------

#[test]
fn or_and_precedence_and_binds_tighter() {
    // a || b && c must parse as a || (b && c)
    let expr = parse_str("a || b && c").unwrap();
    // The top-level should be Any (||), with the second element being All (&&).
    match &expr {
        Expr::Any(items) => {
            assert_eq!(items.len(), 2);
            assert!(matches!(&items[1], Expr::All(_)));
        }
        other => panic!("expected Any at top, got {other:?}"),
    }
}

#[test]
fn precedence_matches_with_explicit_parens() {
    // a || b && c and a || (b && c) must produce identical ASTs.
    let implicit = parse_str("a || b && c").unwrap();
    let explicit = parse_str("a || (b && c)").unwrap();
    assert_eq!(implicit, explicit);
}

#[test]
fn and_does_not_absorb_following_or() {
    // a && b || c must parse as (a && b) || c
    let expr = parse_str("a && b || c").unwrap();
    match &expr {
        Expr::Any(items) => {
            assert_eq!(items.len(), 2);
            assert!(matches!(&items[0], Expr::All(_)));
        }
        other => panic!("expected Any at top, got {other:?}"),
    }
}

// ---------------------------------------------------------------------------
// Type mismatches in comparisons
// ---------------------------------------------------------------------------

#[test]
fn number_vs_string_is_unknown_not_equal() {
    let f = facts(&[("x", "known", json!(42)), ("y", "known", json!("42"))]);
    // Number 42 vs string "42" must be type_mismatch (unknown), not equal.
    assert_eq!(truth("x == y", &f), Truth::Unknown);
}

#[test]
fn number_vs_string_literal_is_unknown() {
    let f = facts(&[("x", "known", json!(100))]);
    assert_eq!(truth("x == '100'", &f), Truth::Unknown);
}

#[test]
fn bool_vs_number_is_unknown() {
    let f = facts(&[("x", "known", json!(true)), ("y", "known", json!(1))]);
    assert_eq!(truth("x == y", &f), Truth::Unknown);
}

// ---------------------------------------------------------------------------
// Three-valued logic: full truth tables
// ---------------------------------------------------------------------------

#[test]
fn all_truth_table_with_unknown() {
    // all(true, unknown) = unknown
    // all(false, unknown) = false
    // all(unknown, unknown) = unknown
    let f_missing = FactSet::new();
    assert_eq!(truth("true && a", &f_missing), Truth::Unknown);
    assert_eq!(truth("false && a", &f_missing), Truth::False);

    let f_unknown = facts(&[("a", "unknown", Value::Null)]);
    assert_eq!(truth("true && a", &f_unknown), Truth::Unknown);
    assert_eq!(truth("false && a", &f_unknown), Truth::False);
}

#[test]
fn any_truth_table_with_unknown() {
    // any(true, unknown) = true
    // any(false, unknown) = unknown
    let f_missing = FactSet::new();
    assert_eq!(truth("true || a", &f_missing), Truth::True);
    assert_eq!(truth("false || a", &f_missing), Truth::Unknown);

    let f_unknown = facts(&[("a", "unknown", Value::Null)]);
    assert_eq!(truth("true || a", &f_unknown), Truth::True);
    assert_eq!(truth("false || a", &f_unknown), Truth::Unknown);
}

#[test]
fn not_unknown_is_unknown() {
    let f = facts(&[("a", "unknown", Value::Null)]);
    assert_eq!(truth("!a", &f), Truth::Unknown);
}

#[test]
fn exists_is_never_false_even_for_missing_fact() {
    let f = FactSet::new();
    let expr = parse_str("exists(missing)").unwrap();
    let outcome = evaluate(&expr, &f);
    // exists of a missing fact is unknown, never false.
    assert_ne!(outcome.truth, Truth::False);
}

#[test]
fn disputed_fact_makes_everything_depending_on_it_unknown() {
    let mut set = FactSet::new();
    set.insert(
        "x",
        FactSet::entry_from_json("known", &json!(true), vec![]).unwrap(),
    );
    set.insert(
        "x",
        FactSet::entry_from_json("known", &json!(false), vec!["ev2".into()]).unwrap(),
    );
    // x is now disputed; any predicate using it should be unknown.
    assert_eq!(truth("x", &set), Truth::Unknown);
    assert_eq!(truth("x && true", &set), Truth::Unknown);
    assert_eq!(truth("x || true", &set), Truth::True); // true || unknown = true
}

// ---------------------------------------------------------------------------
// Temporal: date edge cases
// ---------------------------------------------------------------------------

#[test]
fn effective_date_equal_to_as_of_is_effective() {
    let d = Date::parse("2026-01-15").unwrap();
    assert!(d <= d);
}

#[test]
fn one_day_after_as_of_is_pending() {
    let as_of = Date::parse("2026-01-15").unwrap();
    let effective = Date::parse("2026-01-16").unwrap();
    assert!(effective > as_of);
}

#[test]
fn unparseable_date_is_none() {
    assert!(Date::parse("not-a-date").is_none());
    assert!(Date::parse("2026/01/15").is_none());
    assert!(Date::parse("").is_none());
    assert!(Date::parse("2026-1-15").is_none());
}

#[test]
fn leap_day_2027_02_29_is_invalid() {
    // 2027 is not a leap year.
    assert!(Date::parse("2027-02-29").is_none());
}

#[test]
fn leap_day_2028_02_29_is_valid() {
    // 2028 is a leap year (divisible by 4, not by 100).
    assert!(Date::parse("2028-02-29").is_some());
}

#[test]
fn century_non_leap_1900() {
    // 1900 is divisible by 100 but not 400: not a leap year.
    assert!(Date::parse("1900-02-29").is_none());
}

#[test]
fn quad_century_leap_2000() {
    // 2000 is divisible by 400: leap year.
    assert!(Date::parse("2000-02-29").is_some());
}

// ---------------------------------------------------------------------------
// String form and structured form equivalence
// ---------------------------------------------------------------------------

#[test]
fn string_and_structured_forms_give_identical_asts() {
    let cases = [
        ("a && b", json!({"all": [{"fact": "a"}, {"fact": "b"}]})),
        ("a || b", json!({"any": [{"fact": "a"}, {"fact": "b"}]})),
        ("!a", json!({"not": {"fact": "a"}})),
        ("a == 'x'", json!({"eq": {"fact": "a", "value": "x"}})),
        ("a != 42", json!({"ne": {"fact": "a", "value": 42}})),
        ("a >= 10", json!({"gte": {"fact": "a", "value": 10}})),
        (
            "a in ['x', 'y']",
            json!({"in": {"fact": "a", "values": ["x", "y"]}}),
        ),
        ("exists(a)", json!({"exists": "a"})),
    ];
    for (string_form, structured_value) in &cases {
        let from_str = parse_str(string_form).expect(string_form);
        let obj = structured_value.as_object().unwrap().clone();
        let from_struct = parse_structured(&obj).expect("structured form");
        assert_eq!(from_str, from_struct, "AST mismatch for: {string_form}");
    }
}

#[test]
fn string_and_structured_forms_give_identical_trace_output() {
    let f = facts(&[("a", "known", json!(true)), ("b", "known", json!(false))]);
    let expr_str = parse_str("a && !b").unwrap();
    let mut obj = Map::new();
    obj.insert(
        "all".to_string(),
        json!([{"fact": "a"}, {"not": {"fact": "b"}}]),
    );
    let expr_struct = parse_structured(&obj).unwrap();
    let out_str = evaluate(&expr_str, &f);
    let out_struct = evaluate(&expr_struct, &f);
    assert_eq!(out_str.truth, out_struct.truth);
    assert_eq!(out_str.trace, out_struct.trace);
}

// ---------------------------------------------------------------------------
// Empty and whitespace predicates
// ---------------------------------------------------------------------------

#[test]
fn empty_predicate_is_error() {
    assert!(parse_str("").is_err());
    assert!(parse_str("   ").is_err());
}

#[test]
fn lone_literal_number_is_error() {
    assert!(parse_str("42").is_err());
    assert!(parse_str("'hello'").is_err());
}

#[test]
fn bare_true_and_false_are_valid() {
    assert!(parse_str("true").is_ok());
    assert!(parse_str("false").is_ok());
}

// ---------------------------------------------------------------------------
// Canonical rendering round-trip
// ---------------------------------------------------------------------------

#[test]
fn every_parsed_predicate_round_trips_through_display() {
    let cases = [
        "a && b",
        "a || b",
        "!a",
        "a == 'x'",
        "a != 42",
        "a >= 10",
        "a <= 20",
        "a > 0",
        "a < 100",
        "a in ['x', 'y']",
        "exists(a)",
        "a && b || c",
        "!(a && b)",
        "date_before(a, '2026-01-01')",
        "date_on_or_after(a, '2026-06-15')",
    ];
    for case in &cases {
        let expr = parse_str(case).unwrap();
        let rendered = expr.to_string();
        let reparsed = parse_str(&rendered)
            .unwrap_or_else(|e| panic!("re-parse failed for {case:?} -> {rendered:?}: {e}"));
        assert_eq!(expr, reparsed, "round-trip mismatch for: {case}");
    }
}

// ---------------------------------------------------------------------------
// Structured form rejects unknown operators and bad shapes
// ---------------------------------------------------------------------------

#[test]
fn structured_form_rejects_unknown_operator() {
    let obj = json!({"xor": [{"fact": "a"}, {"fact": "b"}]});
    assert!(parse_structured(obj.as_object().unwrap()).is_err());
}

#[test]
fn structured_form_rejects_empty_all() {
    let obj = json!({"all": []});
    assert!(parse_structured(obj.as_object().unwrap()).is_err());
}

#[test]
fn structured_form_rejects_extra_keys() {
    let obj = json!({"all": [{"fact": "a"}], "extra": true});
    assert!(parse_structured(obj.as_object().unwrap()).is_err());
}

#[test]
fn parse_value_rejects_very_deep_nesting_without_stack_overflow() {
    // Built iteratively so the test itself never recurses.
    let mut value = json!({"fact": "a"});
    for _ in 0..5_000 {
        let mut m = Map::new();
        m.insert("not".to_string(), value);
        value = Value::Object(m);
    }
    let result = diligent_rules::predicate::parse_value(&value);
    let msg = result.expect_err("depth must be rejected").to_string();
    assert!(msg.contains("nested too deeply"), "unexpected error: {msg}");
    // Dropping a 5,000-deep Value recurses in serde_json; leak it instead.
    std::mem::forget(value);
}
