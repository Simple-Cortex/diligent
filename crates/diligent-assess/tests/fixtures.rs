//! Fixture harness for the browser-capture checks.
//!
//! Every case in `fixtures/a11y/act/manifest.json` and
//! `fixtures/a11y/authored/manifest.json` is loaded from its recorded capture in
//! `fixtures/a11y/captures/<case_id>/`, run through every check, and compared
//! with the state the manifest expects for the case's check. ACT cases are
//! named `<rule_id>-<case>`; skipped ACT cases (non-null `skipped_reason`) are
//! ignored. Cases without a recorded capture fail with their id.
//!
//! The state comparison runs `checks::run`, which is the state BEFORE the
//! optional axe-core corroboration step, so a recorded `axe_result` can never
//! move an expectation. A separate test asserts what the step may and may not do.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use diligent_assess::checks;
use diligent_assess::corroborate;
use diligent_assess::signals::{self, SignalState};
use diligent_assess::{assess_set, AssessOptions, State};
use diligent_capture::CaptureSet;
use serde_json::Value;

// Checks that never emit `fail` need a `needs_review` fixture in its place. The set comes from
// `CheckInfo::never_fails`, set in each check module; there is no second list.

/// Checks whose `pass` cannot be recorded offline. A synthetic-capture unit
/// test in `fedweb.rs` proves the pass instead; the recorded fixtures still
/// need the negative state.
const SYNTHETIC_PASS_ONLY: &[&str] = &["BRW-FED-DAP"];

struct Case {
    id: String,
    check_id: String,
    expected: State,
    /// ACT outcome (`passed`, `failed`, `inapplicable`) for ACT cases.
    act_expected: Option<String>,
    note: Option<String>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn read_json(path: &Path) -> Value {
    let text = std::fs::read_to_string(path)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()))
}

fn text(v: &Value, key: &str) -> Option<String> {
    v.get(key).and_then(Value::as_str).map(str::to_string)
}

fn state_of(label: &str, case: &str) -> State {
    State::parse(label).unwrap_or_else(|| panic!("{case}: unknown expected state `{label}`"))
}

fn cases() -> Vec<Case> {
    let root = repo_root().join("fixtures/a11y");
    let mut out = Vec::new();

    let act = read_json(&root.join("act/manifest.json"));
    for c in act["cases"].as_array().expect("act manifest has cases") {
        if !c["skipped_reason"].is_null() {
            continue;
        }
        let id = format!(
            "{}-{}",
            text(c, "rule_id").expect("rule_id"),
            text(c, "case").expect("case")
        );
        out.push(Case {
            check_id: text(c, "check_id").unwrap_or_else(|| panic!("{id}: no check_id")),
            expected: state_of(
                &text(c, "diligent_expected")
                    .unwrap_or_else(|| panic!("{id}: no diligent_expected")),
                &id,
            ),
            act_expected: text(c, "act_expected"),
            note: text(c, "note"),
            id,
        });
    }

    let authored = read_json(&root.join("authored/manifest.json"));
    for c in authored["cases"]
        .as_array()
        .expect("authored manifest has cases")
    {
        // Craft signal cases name a signal key and a signal state; `signal_cases` reads them.
        if text(c, "check_id").is_some_and(|k| signals::info(&k).is_some()) {
            continue;
        }
        let id = text(c, "case_id").expect("case_id");
        out.push(Case {
            check_id: text(c, "check_id").unwrap_or_else(|| panic!("{id}: no check_id")),
            expected: state_of(
                &text(c, "expected").unwrap_or_else(|| panic!("{id}: no expected")),
                &id,
            ),
            act_expected: None,
            note: text(c, "note"),
            id,
        });
    }
    out
}

fn load(case: &Case) -> Result<CaptureSet, String> {
    let dir = repo_root().join("fixtures/a11y/captures").join(&case.id);
    if !dir.is_dir() {
        return Err(format!(
            "{}: no recorded capture at {}",
            case.id,
            dir.display()
        ));
    }
    CaptureSet::load(&dir).map_err(|e| format!("{}: {e}", case.id))
}

fn validator(name: &str) -> jsonschema::Validator {
    let schema = read_json(&repo_root().join("schemas").join(name));
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .should_validate_formats(true)
        .build(&schema)
        .unwrap_or_else(|e| panic!("{name} does not compile: {e}"))
}

fn natural(act: &str) -> Option<State> {
    match act {
        "passed" => Some(State::Pass),
        "failed" => Some(State::Fail),
        "inapplicable" => Some(State::NotTested),
        _ => None,
    }
}

#[test]
fn recorded_captures_produce_the_expected_states() {
    let mut problems: Vec<String> = Vec::new();
    for case in cases() {
        let set = match load(&case) {
            Ok(set) => set,
            Err(why) => {
                problems.push(why);
                continue;
            }
        };
        let findings = checks::run(&set);
        let mut worst: Option<State> = None;
        for f in findings.iter().filter(|f| f.check_id == case.check_id) {
            worst = Some(worst.map_or(f.state, |w| w.worst(f.state)));
        }
        let Some(got) = worst else {
            problems.push(format!(
                "{}: check {} produced no result",
                case.id, case.check_id
            ));
            continue;
        };
        if got != case.expected {
            problems.push(format!(
                "{}: {} expected {} got {}",
                case.id,
                case.check_id,
                case.expected.as_str(),
                got.as_str()
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{} fixture problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn every_divergence_from_the_natural_act_mapping_has_a_note() {
    let mut problems: Vec<String> = Vec::new();
    for case in cases() {
        let Some(act) = case.act_expected.as_deref() else {
            continue;
        };
        let Some(want) = natural(act) else {
            problems.push(format!("{}: unknown ACT outcome `{act}`", case.id));
            continue;
        };
        if want != case.expected && case.note.as_deref().is_none_or(|n| n.trim().is_empty()) {
            problems.push(format!(
                "{}: ACT {act} but expected {}, with no note",
                case.id,
                case.expected.as_str()
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_check_has_a_pass_and_a_fail_fixture() {
    let mut seen: BTreeMap<String, Vec<State>> = BTreeMap::new();
    for case in cases() {
        seen.entry(case.check_id.clone())
            .or_default()
            .push(case.expected);
    }
    let mut problems: Vec<String> = Vec::new();
    for info in checks::all() {
        let states = seen.get(info.id).cloned().unwrap_or_default();
        if !states.contains(&State::Pass) && !SYNTHETIC_PASS_ONLY.contains(&info.id) {
            problems.push(format!("{}: no fixture expects pass", info.id));
        }
        let negative = if info.never_fails {
            State::NeedsReview
        } else {
            State::Fail
        };
        if !states.contains(&negative) {
            problems.push(format!(
                "{}: no fixture expects {}",
                info.id,
                negative.as_str()
            ));
        }
    }
    for id in seen.keys() {
        if !checks::all().iter().any(|i| i.id == id) {
            problems.push(format!("{id}: fixtures name a check that does not exist"));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn emitted_rows_validate_against_the_schemas() {
    let result_schema = validator("check-result.schema.json");
    let evidence_schema = validator("evidence-record.schema.json");
    let opts = AssessOptions {
        run_id: Some("run-2026-10-18-abc123".to_string()),
        executed_at: Some("2026-10-18T00:00:00.000Z".to_string()),
        inventory: None,
        no_signals: false,
        corroborate: true,
    };
    let mut problems: Vec<String> = Vec::new();
    let mut rows = 0usize;
    for case in cases() {
        let set = match load(&case) {
            Ok(set) => set,
            Err(why) => {
                problems.push(why);
                continue;
            }
        };
        let report = match assess_set(&set, &opts) {
            Ok(r) => r,
            Err(e) => {
                problems.push(format!("{}: {e}", case.id));
                continue;
            }
        };
        for row in &report.results {
            rows += 1;
            let value = serde_json::to_value(row).expect("result serialises");
            for e in result_schema.iter_errors(&value) {
                problems.push(format!(
                    "{}: result {} invalid: {e} at {}",
                    case.id,
                    row.result_id,
                    e.instance_path()
                ));
            }
            if row.limitations.is_empty() {
                problems.push(format!(
                    "{}: result {} has no limitations",
                    case.id, row.result_id
                ));
            }
        }
        for row in &report.evidence {
            rows += 1;
            let value = serde_json::to_value(row).expect("evidence serialises");
            for e in evidence_schema.iter_errors(&value) {
                problems.push(format!(
                    "{}: evidence {} invalid: {e} at {}",
                    case.id,
                    row.evidence_id,
                    e.instance_path()
                ));
            }
            if row.limitations.is_empty() {
                problems.push(format!(
                    "{}: evidence {} has no limitations",
                    case.id, row.evidence_id
                ));
            }
        }
        let ids: Vec<&str> = report
            .evidence
            .iter()
            .map(|e| e.evidence_id.as_str())
            .collect();
        for row in &report.results {
            for r in &row.evidence_refs {
                if !ids.contains(&r.as_str()) {
                    problems.push(format!(
                        "{}: result {} cites missing evidence {r}",
                        case.id, row.result_id
                    ));
                }
            }
        }
    }
    assert!(rows > 0 || !problems.is_empty(), "no rows were validated");
    assert!(
        problems.is_empty(),
        "{} row problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn corroboration_only_asks_for_review_and_never_creates_a_fail() {
    let mut problems: Vec<String> = Vec::new();
    let mut with_axe = 0usize;
    for case in cases() {
        let Ok(set) = load(&case) else { continue };
        let pre = checks::run(&set);
        let (post, summary) = corroborate::apply(&set, pre.clone());
        if summary.is_some() {
            with_axe += 1;
        }
        if post.len() < pre.len() {
            problems.push(format!("{}: corroboration dropped findings", case.id));
            continue;
        }
        for (a, b) in pre.iter().zip(&post) {
            if a.check_id != b.check_id || a.instance != b.instance {
                problems.push(format!("{}: corroboration reordered findings", case.id));
                break;
            }
            if a.state == b.state {
                continue;
            }
            let moved_ok = matches!(a.state, State::Pass | State::Fail)
                && b.state == State::NeedsReview
                && b.corroboration
                    .as_ref()
                    .and_then(|c| c["original_state"].as_str())
                    == Some(a.state.as_str());
            if !moved_ok {
                problems.push(format!(
                    "{}: {} {} moved {} to {} without a recorded original state",
                    case.id,
                    a.check_id,
                    a.instance,
                    a.state.as_str(),
                    b.state.as_str()
                ));
            }
        }
        for lead in &post[pre.len()..] {
            let outcome = lead
                .corroboration
                .as_ref()
                .and_then(|c| c["outcome"].as_str());
            if lead.state != State::NeedsReview || outcome != Some("axe_only") {
                problems.push(format!(
                    "{}: extra finding {} is not an axe-only review lead",
                    case.id, lead.check_id
                ));
            }
        }
    }
    eprintln!("{with_axe} recorded captures carry an axe_result");
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn corroboration_agreement_table_over_recorded_fixtures() {
    // Per-check counts of agree / disagree / axe_only across every recorded
    // capture that carries an axe_result. Also asserts that a manifest-expected
    // `fail` is never moved to anything other than `fail` or `needs_review`.
    let mut problems: Vec<String> = Vec::new();
    let mut table: BTreeMap<String, [usize; 3]> = BTreeMap::new(); // [agree, disagree, axe_only]
    let mut cases_with_axe = 0usize;
    for case in cases() {
        let Ok(set) = load(&case) else { continue };
        let pre = checks::run(&set);
        let (post, summary) = corroborate::apply(&set, pre.clone());
        if summary.is_none() {
            continue;
        }
        cases_with_axe += 1;
        for (a, b) in pre.iter().zip(&post) {
            let outcome = b
                .corroboration
                .as_ref()
                .and_then(|c| c["outcome"].as_str())
                .unwrap_or("");
            let entry = table.entry(a.check_id.to_string()).or_default();
            match outcome {
                "agree" => entry[0] += 1,
                "disagree" => entry[1] += 1,
                "axe_only" => entry[2] += 1,
                _ => {}
            }
        }
        // The manifest expectation is the worst state across all findings for
        // the case's check. After corroboration the worst must still be fail or
        // needs_review when the manifest expected fail — corroboration never
        // promotes a fail to pass.
        if case.expected == State::Fail {
            let post_worst: Option<State> = post
                .iter()
                .filter(|f| f.check_id == case.check_id)
                .map(|f| f.state)
                .reduce(|a, b| a.worst(b));
            if let Some(worst) = post_worst {
                if worst != State::Fail && worst != State::NeedsReview {
                    problems.push(format!(
                        "{}: expected fail moved to {} (only fail or needs_review allowed)",
                        case.id,
                        worst.as_str()
                    ));
                }
            }
        }
    }
    eprintln!("corroboration agreement table ({cases_with_axe} cases with axe_result):");
    eprintln!(
        "{:<35} {:>6} {:>8} {:>9}",
        "check", "agree", "disagree", "axe_only"
    );
    for (check, [agree, disagree, axe_only]) in &table {
        eprintln!("{:<35} {:>6} {:>8} {:>9}", check, agree, disagree, axe_only);
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

/// A craft case: the check id is a signal key and the expectation a signal state.
struct SignalCase {
    id: String,
    key: String,
    expected: SignalState,
}

fn signal_cases() -> Vec<SignalCase> {
    let authored = read_json(&repo_root().join("fixtures/a11y/authored/manifest.json"));
    authored["cases"]
        .as_array()
        .expect("authored manifest has cases")
        .iter()
        .filter(|c| text(c, "check_id").is_some_and(|k| signals::info(&k).is_some()))
        .map(|c| {
            let id = text(c, "case_id").expect("case_id");
            let expected = text(c, "expected").and_then(|e| SignalState::parse(&e));
            SignalCase {
                key: text(c, "check_id").expect("check_id"),
                expected: expected
                    .unwrap_or_else(|| panic!("{id}: expected is not a signal state")),
                id,
            }
        })
        .collect()
}

#[test]
fn craft_cases_produce_the_expected_signal_states() {
    let mut problems: Vec<String> = Vec::new();
    for case in signal_cases() {
        let dir = repo_root().join("fixtures/a11y/captures").join(&case.id);
        let set = match CaptureSet::load(&dir) {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("{}: {e}", case.id));
                continue;
            }
        };
        let got = signals::run(&set)
            .iter()
            .filter(|s| s.key == case.key)
            .map(|s| s.state)
            .reduce(SignalState::worst);
        if got != Some(case.expected) {
            problems.push(format!(
                "{}: {} expected {} got {}",
                case.id,
                case.key,
                case.expected.as_str(),
                got.map_or("nothing", SignalState::as_str)
            ));
        }
    }
    assert!(
        problems.is_empty(),
        "{} craft problems:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

#[test]
fn checks_marked_never_fails_never_emit_fail_over_every_recorded_capture() {
    let root = repo_root().join("fixtures/a11y/captures");
    let mut problems: Vec<String> = Vec::new();
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&root)
        .expect("captures directory")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| p.is_dir())
        .collect();
    dirs.sort();
    assert!(!dirs.is_empty(), "no recorded captures");
    for dir in dirs {
        let Ok(set) = CaptureSet::load(&dir) else {
            continue;
        };
        for f in checks::run(&set) {
            if checks::never_fails(f.check_id) && f.state == State::Fail {
                problems.push(format!(
                    "{}: {} emitted fail on {}",
                    dir.display(),
                    f.check_id,
                    f.instance
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        checks::all().iter().any(|i| i.never_fails),
        "no check is marked never_fails"
    );
}
