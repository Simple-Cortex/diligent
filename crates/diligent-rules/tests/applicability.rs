//! Predicate language and applicability engine tests.
//!
//! Fixture cases live in `fixtures/applicability/<case>/` with their own mini
//! data root (`data/`), `SCOPE.json`, `CASE.json` (run parameters) and
//! `expected.json`. Every output is also validated against
//! `schemas/applicability.schema.json`.

use std::fs;
use std::path::{Path, PathBuf};

use diligent_rules::predicate::{
    evaluate, parse_str, parse_structured, Date, Expr, FactSet, Truth,
};
use diligent_rules::{
    lint_predicates, load_data_root, resolve_applicability, ApplicabilityOptions, ObligationState,
};
use jsonschema::Draft;
use serde_json::{json, Value};

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("{}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{}: {e}", path.display()))
}

fn validator(schema_file: &str) -> jsonschema::Validator {
    let schema = read_json(&repo_root().join("schemas").join(schema_file));
    jsonschema::options()
        .with_draft(Draft::Draft202012)
        .should_validate_formats(true)
        .build(&schema)
        .expect("schema compiles")
}

fn assert_valid(validator: &jsonschema::Validator, instance: &Value, what: &str) {
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{what} is invalid: {errors:?}");
}

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
// Fixtures
// ---------------------------------------------------------------------------

const FIXTURE_CASES: [&str; 8] = [
    "disputed-fact-unknown",
    "exists-never-false",
    "federal-all-known",
    "invalid-date-unknown",
    "missing-fact-unknown",
    "not-applicable-with-exclusion",
    "pending-date",
    "type-mismatch",
];

#[test]
fn every_fixture_matches_expected_and_validates_against_the_schema() {
    let base = repo_root().join("fixtures/applicability");
    let applicability_schema = validator("applicability.schema.json");
    let scope_schema = validator("scope.schema.json");

    let mut found: Vec<String> = fs::read_dir(&base)
        .expect("fixtures/applicability exists")
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().into_owned())
        .collect();
    found.sort();
    assert_eq!(found, FIXTURE_CASES.to_vec());

    for name in found {
        let dir = base.join(&name);
        let case = read_json(&dir.join("CASE.json"));
        let scope = read_json(&dir.join("SCOPE.json"));
        assert_valid(&scope_schema, &scope, &format!("{name}/SCOPE.json"));

        let data = load_data_root(&dir.join("data")).expect("data root lists");
        assert_eq!(data.typed_problems(), Vec::<String>::new(), "{name}");

        let as_of = Date::parse(case["as_of"].as_str().unwrap()).expect("as_of is a date");
        let options = ApplicabilityOptions {
            as_of,
            evaluated_at: case["evaluated_at"].as_str().unwrap().to_string(),
            run_id: case["run_id"].as_str().map(str::to_string),
        };
        let file =
            resolve_applicability(&data, case["profile"].as_str().unwrap(), &scope, &options)
                .unwrap_or_else(|e| panic!("{name}: {e}"));
        let actual: Value = serde_json::from_str(&file.to_json()).unwrap();

        assert_valid(&applicability_schema, &actual, &format!("{name} output"));
        let expected = read_json(&dir.join("expected.json"));
        assert_valid(
            &applicability_schema,
            &expected,
            &format!("{name}/expected.json"),
        );
        assert_eq!(actual, expected, "fixture {name}");
        assert!(dir.join("README.md").is_file(), "{name} has a README");
    }
}

#[test]
fn fixture_states_cover_every_obligation_state() {
    let dir = repo_root().join("fixtures/applicability");
    let mut seen = std::collections::BTreeSet::new();
    for name in FIXTURE_CASES {
        let expected = read_json(&dir.join(name).join("expected.json"));
        for o in expected["obligations"].as_array().unwrap() {
            seen.insert(o["state"].as_str().unwrap().to_string());
        }
    }
    let all: Vec<&str> = seen.iter().map(String::as_str).collect();
    assert_eq!(
        all,
        vec![
            "applicable",
            "not_applicable",
            "pending_effective_date",
            "unknown"
        ]
    );
}

#[test]
fn shipped_data_resolves_against_the_schema_with_known_and_empty_facts() {
    let root = repo_root();
    let data = load_data_root(&root).unwrap();
    let schema = validator("applicability.schema.json");
    let options = ApplicabilityOptions {
        as_of: Date::parse("2026-10-17").unwrap(),
        evaluated_at: "2026-10-17T12:00:00Z".to_string(),
        run_id: None,
    };
    let scope = json!({
        "audit_id": "shipped-audit",
        "run_id": "run-2026-10-17-abcdef",
        "profile_id": "us-federal-public-web",
        "facts": [
            {"fact_id": "federal_executive_agency", "state": "known", "value": false,
             "evidence_refs": ["ev-1"]}
        ]
    });
    let file = resolve_applicability(&data, "us-federal-public-web", &scope, &options).unwrap();
    let value: Value = serde_json::from_str(&file.to_json()).unwrap();
    assert_valid(&schema, &value, "shipped-data output");
    assert!(file
        .obligations
        .iter()
        .any(|o| o.state == ObligationState::NotApplicable));
    // Rows are sorted by pack_id then rule_id.
    let keys: Vec<_> = file
        .obligations
        .iter()
        .map(|o| (o.pack_id.clone(), o.rule_id.clone()))
        .collect();
    let mut sorted = keys.clone();
    sorted.sort();
    assert_eq!(keys, sorted);
}

#[test]
fn profile_mismatch_and_missing_profile_are_errors() {
    let data = load_data_root(&repo_root()).unwrap();
    let options = ApplicabilityOptions {
        as_of: Date::parse("2026-10-17").unwrap(),
        evaluated_at: "2026-10-17T12:00:00Z".to_string(),
        run_id: Some("not-a-run-id".to_string()),
    };
    let scope = json!({
        "audit_id": "a", "run_id": "run-2026-10-17-abcdef",
        "profile_id": "us-federal-public-web", "facts": []
    });
    assert!(resolve_applicability(&data, "other-profile", &scope, &options).is_err());
    assert!(resolve_applicability(&data, "us-federal-public-web", &scope, &options).is_err());
    assert!(resolve_applicability(&data, "us-federal-public-web", &json!([]), &options).is_err());
}

// ---------------------------------------------------------------------------
// Lint
// ---------------------------------------------------------------------------

#[test]
fn shipped_predicates_lint_clean() {
    let data = load_data_root(&repo_root()).unwrap();
    assert_eq!(lint_predicates(&data), Vec::<String>::new());
}

#[test]
fn every_shipped_predicate_round_trips_through_its_canonical_form() {
    let data = load_data_root(&repo_root()).unwrap();
    let mut count = 0;
    for pack in data.typed_packs() {
        for rule in &pack.rules {
            let expr = diligent_rules::predicate::parse_predicate(&rule.applicability.predicate)
                .unwrap_or_else(|e| panic!("{}: {e}", rule.rule_id));
            let again = parse_str(&expr.to_string()).unwrap();
            assert_eq!(expr, again, "{}", rule.rule_id);
            count += 1;
        }
    }
    assert!(count > 100);
}

// ---------------------------------------------------------------------------
// Parsing
// ---------------------------------------------------------------------------

#[test]
fn string_and_structured_forms_build_identical_trees() {
    let structured = json!({"all": [{"fact": "a"}, {"fact": "b"}]});
    assert_eq!(
        parse_str("a && b").unwrap(),
        parse_structured(structured.as_object().unwrap()).unwrap()
    );
    let structured = json!({"any": [
        {"all": [{"fact": "a"}, {"gte": {"fact": "n", "value": 3}}]},
        {"not": {"in": {"fact": "v", "values": ["x", "y"]}}}
    ]});
    assert_eq!(
        parse_str("(a && n >= 3) || !(v in ['x', 'y'])").unwrap(),
        parse_structured(structured.as_object().unwrap()).unwrap()
    );
    let structured = json!({"eq": {"fact": "v", "value": "2.2"}});
    assert_eq!(
        parse_str("v == \"2.2\"").unwrap(),
        parse_structured(structured.as_object().unwrap()).unwrap()
    );
}

#[test]
fn chained_operators_flatten_and_precedence_holds() {
    match parse_str("a && b && c").unwrap() {
        Expr::All(items) => assert_eq!(items.len(), 3),
        other => panic!("expected all, got {other:?}"),
    }
    // && binds tighter than ||.
    assert_eq!(parse_str("a || b && c").unwrap().to_string(), "a || b && c");
    assert_eq!(
        parse_str("(a || b) && c").unwrap().to_string(),
        "(a || b) && c"
    );
    // ! binds tighter than && and applies to a whole comparison.
    assert_eq!(parse_str("!x == 1").unwrap().to_string(), "!x == 1");
    assert_eq!(parse_str("!(a && b)").unwrap().to_string(), "!(a && b)");
}

#[test]
fn canonical_rendering_uses_single_quotes_and_lowercase_ops() {
    let e = parse_str("v in [\"a\", 'b\\'c'] && n != 2.5").unwrap();
    assert_eq!(e.to_string(), "v in ['a', 'b\\'c'] && n != 2.5");
}

#[test]
fn bad_predicates_are_errors_not_panics() {
    for bad in [
        "",
        "   ",
        "a &",
        "a | b",
        "a = b",
        "a === b",
        "(a && b",
        "a && b)",
        "a && ",
        "&& a",
        "'unterminated",
        "\"also unterminated",
        "a == ",
        "a == b == c",
        "x in 'abc'",
        "x in ['a'",
        "x in [a]",
        "A && b",
        "a ^ b",
        "a >= true",
        "'just a string'",
        "42",
        "null",
        "a && 'x'",
        "x == 1e500",
        "f(a)",
        "exists(a, b)",
        "date_before(a, 'not-a-date')",
        "date_before(a, '2026-02-30')",
        "-",
        "x in",
        "!",
    ] {
        assert!(parse_str(bad).is_err(), "should reject {bad:?}");
    }
}

#[test]
fn deep_nesting_is_rejected_without_overflow() {
    assert!(parse_str(&format!("{}a{}", "(".repeat(100), ")".repeat(100))).is_err());
    let deep = format!("{}a{}", "(".repeat(5000), ")".repeat(5000));
    assert!(parse_str(&deep).is_err());
    let nots = format!("{}a", "!".repeat(5000));
    assert!(parse_str(&nots).is_err());
}

#[test]
fn structured_form_rejects_unknown_operators_and_bad_shapes() {
    for bad in [
        json!({"xor": [{"fact": "a"}]}),
        json!({"all": []}),
        json!({"all": {"fact": "a"}}),
        json!({"any": [], "all": []}),
        json!({}),
        json!({"fact": 3}),
        json!({"fact": "Upper"}),
        json!({"eq": {"fact": "a"}}),
        json!({"eq": {"fact": "a", "value": 1, "extra": 2}}),
        json!({"eq": {"fact": "a", "value": [1]}}),
        json!({"gte": {"fact": "a", "value": true}}),
        json!({"in": {"fact": "a", "values": "x"}}),
        json!({"in": {"fact": "a", "values": [[1]]}}),
        json!({"exists": 5}),
        json!({"date_before": {"fact": "a", "date": "2026-13-01"}}),
        json!({"date_before": {"fact": "a"}}),
        json!({"not": 1}),
        json!({"eval": "process.exit()"}),
    ] {
        assert!(
            parse_structured(bad.as_object().unwrap()).is_err(),
            "should reject {bad}"
        );
    }
}

// ---------------------------------------------------------------------------
// Three-valued evaluation
// ---------------------------------------------------------------------------

#[test]
fn all_any_not_truth_tables() {
    let set = facts(&[
        ("t", "known", json!(true)),
        ("f", "known", json!(false)),
        ("u", "unknown", Value::Null),
    ]);
    let cases = [
        ("t && t", Truth::True),
        ("t && f", Truth::False),
        ("t && u", Truth::Unknown),
        ("f && u", Truth::False),
        ("u && f", Truth::False),
        ("u && u", Truth::Unknown),
        ("t || f", Truth::True),
        ("f || f", Truth::False),
        ("f || u", Truth::Unknown),
        ("t || u", Truth::True),
        ("u || t", Truth::True),
        ("!t", Truth::False),
        ("!f", Truth::True),
        ("!u", Truth::Unknown),
        ("!(f || u)", Truth::Unknown),
        ("absent_fact", Truth::Unknown),
        ("true", Truth::True),
        ("false", Truth::False),
    ];
    for (predicate, want) in cases {
        assert_eq!(truth(predicate, &set), want, "{predicate}");
    }
}

#[test]
fn fact_state_rules() {
    let set = facts(&[
        ("known_true", "known", json!(true)),
        ("known_null", "known", Value::Null),
        ("disputed", "disputed", json!(true)),
        ("unknown_with_value", "unknown", json!(true)),
    ]);
    assert_eq!(truth("known_true", &set), Truth::True);
    assert_eq!(truth("known_null", &set), Truth::Unknown);
    assert_eq!(truth("disputed", &set), Truth::Unknown);
    assert_eq!(truth("unknown_with_value", &set), Truth::Unknown);
    // Conflicting duplicates become disputed.
    let mut dup = FactSet::new();
    dup.insert(
        "x",
        FactSet::entry_from_json("known", &json!(true), vec![]).unwrap(),
    );
    dup.insert(
        "x",
        FactSet::entry_from_json("known", &json!(false), vec![]).unwrap(),
    );
    assert_eq!(truth("x", &dup), Truth::Unknown);
}

#[test]
fn comparisons_and_type_mismatch() {
    let set = facts(&[
        ("s", "known", json!("2.2")),
        ("n", "known", json!(25)),
        ("b", "known", json!(true)),
        ("a", "known", json!([1])),
    ]);
    let cases = [
        ("s == '2.2'", Truth::True),
        ("s != '2.2'", Truth::False),
        ("s >= '2.1'", Truth::True),
        ("s < '10'", Truth::False), // bytewise: '2' > '1'
        ("n >= 25", Truth::True),
        ("n > 25", Truth::False),
        ("n < 100", Truth::True),
        ("n <= 24.5", Truth::False),
        ("n == 25.0", Truth::True),
        ("b == true", Truth::True),
        ("b != false", Truth::True),
        ("s == 2.2", Truth::Unknown),
        ("n == '25'", Truth::Unknown),
        ("b >= 1", Truth::Unknown),
        ("n == null", Truth::Unknown),
        ("a == 1", Truth::Unknown),
        ("n", Truth::Unknown),
        ("s", Truth::Unknown),
        ("s in ['2.1', '2.2']", Truth::True),
        ("s in ['2.0', '2.1']", Truth::False),
        ("s in []", Truth::False),
        ("s in ['2.2', 3]", Truth::True),
        ("s in ['2.1', 3]", Truth::Unknown),
        ("missing in ['a']", Truth::Unknown),
        ("n == missing", Truth::Unknown),
    ];
    for (predicate, want) in cases {
        assert_eq!(truth(predicate, &set), want, "{predicate}");
    }
    let outcome = evaluate(&parse_str("s == 2.2").unwrap(), &set);
    assert_eq!(outcome.trace[0].reason.as_deref(), Some("type_mismatch"));
}

#[test]
fn exists_is_never_false() {
    let set = facts(&[
        ("present", "known", json!("x")),
        ("falsy", "known", json!(false)),
        ("zero", "known", json!(0)),
        ("null_fact", "known", Value::Null),
        ("unk", "unknown", Value::Null),
        ("disp", "disputed", json!(1)),
    ]);
    assert_eq!(truth("exists(present)", &set), Truth::True);
    assert_eq!(truth("exists(falsy)", &set), Truth::True);
    assert_eq!(truth("exists(zero)", &set), Truth::True);
    for name in ["null_fact", "unk", "disp", "absent"] {
        assert_eq!(
            truth(&format!("exists({name})"), &set),
            Truth::Unknown,
            "{name}"
        );
    }
    assert_eq!(truth("!exists(absent)", &set), Truth::Unknown);
}

#[test]
fn date_operators() {
    let set = facts(&[
        ("d", "known", json!("2026-03-15")),
        ("bad", "known", json!("2026-02-30")),
        ("word", "known", json!("soon")),
        ("num", "known", json!(20260315)),
    ]);
    assert_eq!(truth("date_before(d, '2026-04-01')", &set), Truth::True);
    assert_eq!(truth("date_before(d, '2026-03-15')", &set), Truth::False);
    assert_eq!(
        truth("date_on_or_after(d, '2026-03-15')", &set),
        Truth::True
    );
    assert_eq!(
        truth("date_on_or_after(d, '2026-03-16')", &set),
        Truth::False
    );
    for name in ["bad", "word"] {
        let outcome = evaluate(
            &parse_str(&format!("date_before({name}, '2026-04-01')")).unwrap(),
            &set,
        );
        assert_eq!(outcome.truth, Truth::Unknown);
        assert_eq!(outcome.trace[0].reason.as_deref(), Some("invalid_date"));
    }
    let outcome = evaluate(&parse_str("date_before(num, '2026-04-01')").unwrap(), &set);
    assert_eq!(outcome.trace[0].reason.as_deref(), Some("type_mismatch"));
}

#[test]
fn trace_is_post_order_with_stable_paths_and_key_order() {
    let set = facts(&[("a", "known", json!(true))]);
    let outcome = evaluate(&parse_str("a && (b || !a)").unwrap(), &set);
    let paths: Vec<&str> = outcome.trace.iter().map(|t| t.path.as_str()).collect();
    assert_eq!(paths, vec!["0.0", "0.1.0", "0.1.1.0", "0.1.1", "0.1", "0"]);
    assert_eq!(outcome.trace[1].fact_state.as_deref(), Some("absent"));
    assert_eq!(outcome.trace[1].reason.as_deref(), Some("missing_fact"));
    let line = serde_json::to_string(&outcome.trace[0]).unwrap();
    assert_eq!(
        line,
        r#"{"path":"0.0","op":"fact","expr":"a","result":"true","fact":"a","fact_state":"known","reason":null}"#
    );
    // Deterministic across runs.
    assert_eq!(
        outcome,
        evaluate(&parse_str("a && (b || !a)").unwrap(), &set)
    );
}

#[test]
fn facts_used_collects_every_referenced_fact() {
    let e =
        parse_str("a && (b || n >= 3) && v in ['x'] && exists(c) && date_before(d, '2026-01-01')")
            .unwrap();
    let used: Vec<String> = e.facts().into_iter().collect();
    assert_eq!(used, vec!["a", "b", "c", "d", "n", "v"]);
}
