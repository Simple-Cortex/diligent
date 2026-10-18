//! Adversarial tests for the axe-core corroboration step.
//!
//! Each test builds a synthetic capture directory with a DOM snapshot and an
//! `axe_result` payload, then runs `assess_set` or `corroborate::apply_with`
//! directly. The goal is to exercise hostile inputs that the recorded fixtures
//! do not cover: duplicate rule ids, unknown outcomes, selectors that match
//! nothing, very large node lists, null versions, error payloads, state
//! transitions that must not happen, schema validation of every emitted row,
//! and profile-schema rejection of unknown corroborator names.

use std::path::{Path, PathBuf};

use diligent_assess::corroborate::{self, AxeAt};
use diligent_assess::{assess_set, AssessOptions, Finding, State};
use diligent_capture::{AxeNode, AxeOutcome, AxeResult, AxeRule, CaptureSet, Viewport};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const VIEWPORT: &str = r#"{"width":1280,"height":800,"dpr":1.0}"#;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir =
            std::env::temp_dir().join(format!("diligent-corr-adv-{}-{}", name, std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }

    fn path(&self) -> &Path {
        &self.0
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn sha256(content: &str) -> String {
    Sha256::digest(content.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn styled(color: &str) -> Value {
    json!({
        "display": "block",
        "visibility": "visible",
        "opacity": "1",
        "color": color,
        "font-size": "16px",
        "font-weight": "400",
        "background-color": "rgba(0, 0, 0, 0)"
    })
}

fn element(index: usize, tag: &str, parent: usize, id: Option<&str>, color: &str) -> Value {
    let attributes: Vec<Value> = id.map(|i| json!(["id", i])).into_iter().collect();
    json!({
        "index": index, "node_type": 1, "tag": tag, "attributes": attributes, "text": null,
        "parent": parent, "backend_node_id": index + 1,
        "bounding_box": {"x": 0.0, "y": index as f64 * 20.0, "width": 600.0, "height": 20.0},
        "styles": styled(color)
    })
}

fn text(index: usize, parent: usize, content: &str) -> Value {
    json!({
        "index": index, "node_type": 3, "tag": "#text", "attributes": [], "text": content,
        "parent": parent, "backend_node_id": index + 1, "bounding_box": null, "styles": {}
    })
}

fn dom() -> String {
    serde_json::to_string(&json!({
        "document_url": "http://example.com",
        "nodes": [
            {"index": 0, "node_type": 9, "tag": "#document", "attributes": [], "text": null, "parent": null,
             "backend_node_id": 1, "bounding_box": null, "styles": {}},
            element(1, "html", 0, None, "rgb(0, 0, 0)"),
            element(2, "body", 1, None, "rgb(0, 0, 0)"),
            element(3, "p", 2, Some("low"), "rgb(200, 200, 200)"),
            text(4, 3, "Pale grey words on white"),
            element(5, "p", 2, Some("high"), "rgb(0, 0, 0)"),
            text(6, 5, "Black words on white"),
        ],
        "document_metrics": {"scroll_width": 1280, "scroll_height": 800, "client_width": 1280, "client_height": 800}
    }))
    .unwrap()
}

fn entry(kind: &str, sha: &str, bytes: usize) -> String {
    format!(
        r#"{{"evidence_id":"{sha}","kind":"{kind}","viewport":{VIEWPORT},"media":"baseline","path":"{sha}.json","sha256":"{sha}","bytes":{bytes},"captured_at":"2026-10-18T00:00:00Z","final_url":"http://example.com","collector_version":"0.1.0","protocol_version":"1.2.0"}}"#
    )
}

fn write(dir: &Path, axe: &str) {
    let dom = dom();
    let dom_sha = sha256(&dom);
    let axe_sha = sha256(axe);
    let lines = [
        entry("dom_snapshot", &dom_sha, dom.len()),
        entry("axe_result", &axe_sha, axe.len()),
    ];
    std::fs::write(dir.join(format!("{dom_sha}.json")), &dom).unwrap();
    std::fs::write(dir.join(format!("{axe_sha}.json")), axe).unwrap();
    std::fs::write(dir.join("manifest.jsonl"), lines.join("\n")).unwrap();
}

fn axe_rule(id: &str, outcome: &str, nodes: Vec<Value>) -> Value {
    json!({
        "id": id, "act_ids": [], "tags": [], "outcome": outcome, "impact": null,
        "nodes": nodes, "truncated": false
    })
}

fn axe_node(selector: Option<&str>, outcome: &str) -> Value {
    json!({"selector": selector, "outcome": outcome, "summary": "Element has insufficient color contrast"})
}

fn axe_payload(version: Option<&str>, rules: Vec<Value>, error: Option<Value>) -> String {
    serde_json::to_string(&json!({
        "engine": "axe-core", "version": version, "url": "http://example.com",
        "rules": rules, "error": error
    }))
    .unwrap()
}

fn opts() -> AssessOptions {
    AssessOptions {
        run_id: Some("run-2026-10-18-abc123".to_string()),
        executed_at: Some("2026-10-18T00:00:00.000Z".to_string()),
        inventory: None,
        no_signals: false,
        corroborate: true,
    }
}

fn load(name: &str, axe: &str) -> (TempDir, CaptureSet) {
    let dir = TempDir::new(name);
    write(dir.path(), axe);
    let set = CaptureSet::load(dir.path()).expect("capture loads");
    (dir, set)
}

fn row<'a>(
    report: &'a diligent_assess::AssessReport,
    check: &str,
    selector: &str,
) -> &'a diligent_assess::CheckResult {
    report
        .results
        .iter()
        .find(|r| r.check_id == check && r.instance_id.ends_with(&format!("::{selector}")))
        .unwrap_or_else(|| panic!("no {check} row for {selector}"))
}

fn corr(r: &diligent_assess::CheckResult) -> &Value {
    &r.extensions["diligent.corroboration"]
}

fn validator(name: &str) -> jsonschema::Validator {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../schemas")
        .join(name);
    let schema: Value = serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .should_validate_formats(true)
        .build(&schema)
        .unwrap_or_else(|e| panic!("{name} does not compile: {e}"))
}

fn vp() -> Viewport {
    Viewport {
        width: 1280,
        height: 800,
        dpr: 1.0,
    }
}

fn finding(check: &'static str, state: State, instance: &str) -> Finding {
    Finding {
        check_id: check,
        state,
        instance: instance.to_string(),
        expected: Value::Null,
        observed: Value::Null,
        limitations: Vec::new(),
        evidence: Vec::new(),
        wcag: Vec::new(),
        viewport: Some(vp()),
        media: Some("baseline".to_string()),
        corroboration: None,
        corroboration_scope: None,
    }
}

fn mk_result(
    rules: Vec<AxeRule>,
    version: Option<&str>,
    error: Option<diligent_capture::AxeError>,
) -> AxeResult {
    AxeResult {
        engine: "axe-core".into(),
        version: version.map(str::to_string),
        url: "http://example.com".into(),
        rules,
        error,
    }
}

fn mk_rule(id: &str, outcome: AxeOutcome, nodes: Vec<AxeNode>) -> AxeRule {
    AxeRule {
        id: id.to_string(),
        act_ids: Vec::new(),
        tags: Vec::new(),
        outcome,
        impact: None,
        nodes,
        truncated: false,
    }
}

fn mk_node(sel: Option<&str>, outcome: AxeOutcome) -> AxeNode {
    AxeNode {
        selector: sel.map(str::to_string),
        outcome,
        summary: Some("summary".into()),
    }
}

// --- Adversarial scenarios ---

#[test]
fn duplicate_rule_ids_in_one_axe_result() {
    let payload = axe_payload(
        Some("4.14.0"),
        vec![
            axe_rule(
                "color-contrast",
                "violation",
                vec![axe_node(Some("p#low"), "violation")],
            ),
            axe_rule(
                "color-contrast",
                "pass",
                vec![axe_node(Some("p#low"), "pass")],
            ),
            axe_rule(
                "color-contrast",
                "violation",
                vec![axe_node(Some("p#low"), "violation")],
            ),
        ],
        None,
    );
    let (_d, set) = load("dup-rules", &payload);
    let report = assess_set(&set, &opts()).unwrap();
    let r = row(&report, "BRW-TEXT-CONTRAST", "p#low");
    assert_eq!(r.status, State::Fail);
    assert_eq!(corr(r)["outcome"], "agree");
}

#[test]
fn unknown_outcome_string_is_a_load_error_not_a_panic() {
    let payload = axe_payload(
        Some("4.14.0"),
        vec![axe_rule(
            "color-contrast",
            "maybe",
            vec![axe_node(Some("p#low"), "violation")],
        )],
        None,
    );
    let dir = TempDir::new("unknown-outcome");
    write(dir.path(), &payload);
    assert!(CaptureSet::load(dir.path()).is_err());
}

#[test]
fn node_selector_that_matches_no_finding() {
    let payload = axe_payload(
        Some("4.14.0"),
        vec![axe_rule(
            "color-contrast",
            "violation",
            vec![axe_node(Some("div#nothing"), "violation")],
        )],
        None,
    );
    let (_d, set) = load("no-match", &payload);
    let report = assess_set(&set, &opts()).unwrap();
    let r = row(&report, "BRW-TEXT-CONTRAST", "p#low");
    assert_eq!(r.status, State::Fail);
    assert_eq!(corr(r)["outcome"], "no_signal");
    let s = report.summary.corroboration.as_ref().unwrap();
    assert_eq!(s.axe_only, 1, "div#nothing becomes an axe-only lead");
}

#[test]
fn ten_thousand_nodes_on_one_rule() {
    let nodes: Vec<Value> = (0..10_000)
        .map(|i| axe_node(Some(&format!("div:nth-child({i})")), "violation"))
        .collect();
    let payload = axe_payload(
        Some("4.14.0"),
        vec![axe_rule("color-contrast", "violation", nodes)],
        None,
    );
    let start = std::time::Instant::now();
    let (_d, set) = load("many-nodes", &payload);
    let report = assess_set(&set, &opts()).unwrap();
    assert!(
        start.elapsed().as_secs() < 20,
        "10,000 nodes must stay cheap"
    );
    let s = report.summary.corroboration.as_ref().unwrap();
    assert_eq!(s.axe_only, 10_000);
}

#[test]
fn version_null_with_no_error() {
    let payload = axe_payload(
        None,
        vec![axe_rule(
            "color-contrast",
            "violation",
            vec![axe_node(Some("p#low"), "violation")],
        )],
        None,
    );
    let (_d, set) = load("null-version", &payload);
    let report = assess_set(&set, &opts()).unwrap();
    let r = row(&report, "BRW-TEXT-CONTRAST", "p#low");
    assert!(corr(r)["version"].is_null());
    assert_eq!(corr(r)["outcome"], "agree");
}

#[test]
fn error_payload_marks_mapped_findings_unavailable_no_state_change() {
    let payload = axe_payload(
        None,
        vec![],
        Some(json!({"kind": "timeout", "message": "axe took too long"})),
    );
    let (_d, set) = load("error-payload", &payload);
    let report = assess_set(&set, &opts()).unwrap();
    let r = row(&report, "BRW-TEXT-CONTRAST", "p#low");
    assert_eq!(r.status, State::Fail, "state must not change on error");
    assert_eq!(corr(r)["outcome"], "unavailable");
    assert_eq!(corr(r)["error"]["kind"], "timeout");
    let s = report.summary.corroboration.as_ref().unwrap();
    assert_eq!(s.disagree, 0);
    assert_eq!(s.axe_only, 0);
    assert!(s.unavailable >= 1);
}

#[test]
fn rust_fail_that_axe_passes_becomes_needs_review_with_original_state_fail() {
    let r = mk_result(
        vec![mk_rule(
            "color-contrast",
            AxeOutcome::Pass,
            vec![mk_node(Some("p#low"), AxeOutcome::Pass)],
        )],
        Some("4.14.0"),
        None,
    );
    let f = finding("BRW-TEXT-CONTRAST", State::Fail, "p#low");
    let (out, _) = corroborate::apply_with(
        &[AxeAt {
            viewport: vp(),
            result: &r,
        }],
        vec![f],
    );
    assert_eq!(out[0].state, State::NeedsReview);
    assert_eq!(
        out[0].corroboration.as_ref().unwrap()["original_state"],
        "fail"
    );
    assert_eq!(
        out[0].corroboration.as_ref().unwrap()["outcome"],
        "disagree"
    );
}

#[test]
fn rust_pass_that_axe_flags_becomes_needs_review_with_original_state_pass() {
    let r = mk_result(
        vec![mk_rule(
            "color-contrast",
            AxeOutcome::Violation,
            vec![mk_node(Some("p#high"), AxeOutcome::Violation)],
        )],
        Some("4.14.0"),
        None,
    );
    let f = finding("BRW-TEXT-CONTRAST", State::Pass, "p#high");
    let (out, _) = corroborate::apply_with(
        &[AxeAt {
            viewport: vp(),
            result: &r,
        }],
        vec![f],
    );
    assert_eq!(out[0].state, State::NeedsReview);
    assert_eq!(
        out[0].corroboration.as_ref().unwrap()["original_state"],
        "pass"
    );
    assert_eq!(
        out[0].corroboration.as_ref().unwrap()["outcome"],
        "disagree"
    );
}

#[test]
fn rust_needs_review_that_axe_flags_is_unchanged() {
    let r = mk_result(
        vec![mk_rule(
            "color-contrast",
            AxeOutcome::Violation,
            vec![mk_node(Some("p#low"), AxeOutcome::Violation)],
        )],
        Some("4.14.0"),
        None,
    );
    let f = finding("BRW-TEXT-CONTRAST", State::NeedsReview, "p#low");
    let (out, s) = corroborate::apply_with(
        &[AxeAt {
            viewport: vp(),
            result: &r,
        }],
        vec![f],
    );
    assert_eq!(out[0].state, State::NeedsReview);
    assert_eq!(
        out[0].corroboration.as_ref().unwrap()["outcome"],
        "informational"
    );
    assert!(out[0]
        .corroboration
        .as_ref()
        .unwrap()
        .get("original_state")
        .is_none());
    assert_eq!(s.unwrap().informational, 1);
}

#[test]
fn rust_not_tested_that_axe_flags_is_unchanged_plus_axe_only_lead() {
    let r = mk_result(
        vec![
            mk_rule(
                "color-contrast",
                AxeOutcome::Violation,
                vec![mk_node(Some("p#low"), AxeOutcome::Violation)],
            ),
            mk_rule(
                "color-contrast",
                AxeOutcome::Violation,
                vec![mk_node(Some("p#ghost"), AxeOutcome::Violation)],
            ),
        ],
        Some("4.14.0"),
        None,
    );
    let f = finding("BRW-TEXT-CONTRAST", State::NotTested, "p#low");
    let (out, s) = corroborate::apply_with(
        &[AxeAt {
            viewport: vp(),
            result: &r,
        }],
        vec![f],
    );
    assert_eq!(out[0].state, State::NotTested, "not_tested must not change");
    assert_eq!(
        out[0].corroboration.as_ref().unwrap()["outcome"],
        "informational"
    );
    let leads: Vec<&Finding> = out
        .iter()
        .skip(1)
        .filter(|x| x.corroboration.as_ref().unwrap()["outcome"] == "axe_only")
        .collect();
    assert_eq!(leads.len(), 1, "p#ghost becomes an axe-only lead");
    assert_eq!(leads[0].instance, "p#ghost");
    assert_eq!(leads[0].state, State::NeedsReview);
    assert_eq!(s.unwrap().axe_only, 1);
}

#[test]
fn corroboration_never_produces_fail_from_a_non_fail() {
    let states = [State::Pass, State::NeedsReview, State::NotTested];
    let outcomes = [
        AxeOutcome::Violation,
        AxeOutcome::Pass,
        AxeOutcome::Incomplete,
        AxeOutcome::Inapplicable,
    ];
    let checks = [
        "BRW-TEXT-CONTRAST",
        "BRW-BYPASS-BLOCKS",
        "BRW-ARIA-ATTR-VALID",
    ];
    let instances = ["p#low", "html", "div"];
    let rules_map: &[(&str, &[(&str, AxeOutcome)])] = &[
        (
            "BRW-TEXT-CONTRAST",
            &[("color-contrast", AxeOutcome::Violation)],
        ),
        ("BRW-BYPASS-BLOCKS", &[("bypass", AxeOutcome::Violation)]),
        (
            "BRW-ARIA-ATTR-VALID",
            &[("aria-roles", AxeOutcome::Violation)],
        ),
    ];
    for &state in &states {
        for &outcome in &outcomes {
            for (i, &check) in checks.iter().enumerate() {
                let mapped_rules = rules_map[i].1;
                let axe_rules: Vec<AxeRule> = mapped_rules
                    .iter()
                    .map(|(id, _)| mk_rule(id, outcome, vec![mk_node(Some(instances[i]), outcome)]))
                    .collect();
                let r = mk_result(axe_rules, Some("4.14.0"), None);
                let f = finding(check, state, instances[i]);
                let (out, _) = corroborate::apply_with(
                    &[AxeAt {
                        viewport: vp(),
                        result: &r,
                    }],
                    vec![f],
                );
                assert_ne!(
                    out[0].state,
                    State::Fail,
                    "corroboration must never produce fail from {:?} + {:?}",
                    state,
                    outcome
                );
                if state == State::NeedsReview || state == State::NotTested {
                    assert_eq!(out[0].state, state, "review/not_tested must not change");
                }
            }
        }
    }
}

#[test]
fn every_emitted_row_validates_against_the_schemas() {
    let result_schema = validator("check-result.schema.json");
    let evidence_schema = validator("evidence-record.schema.json");
    let payloads = [
        axe_payload(
            Some("4.14.0"),
            vec![axe_rule(
                "color-contrast",
                "pass",
                vec![axe_node(Some("p#low"), "pass")],
            )],
            None,
        ),
        axe_payload(
            Some("4.14.0"),
            vec![axe_rule(
                "color-contrast",
                "violation",
                vec![axe_node(Some("p#ghost"), "violation")],
            )],
            None,
        ),
        axe_payload(
            None,
            vec![],
            Some(json!({"kind": "collector_failure", "message": "page redefined a global"})),
        ),
        axe_payload(
            None,
            vec![axe_rule(
                "color-contrast",
                "violation",
                vec![axe_node(Some("p#low"), "violation")],
            )],
            None,
        ),
    ];
    for (n, payload) in payloads.iter().enumerate() {
        let (_d, set) = load(&format!("schema-adv-{n}"), payload);
        let report = assess_set(&set, &opts()).unwrap();
        for r in &report.results {
            let v = serde_json::to_value(r).unwrap();
            let errors: Vec<String> = result_schema
                .iter_errors(&v)
                .map(|e| e.to_string())
                .collect();
            assert!(
                errors.is_empty(),
                "result {} (payload {n}): {errors:?}",
                r.result_id
            );
        }
        for e in &report.evidence {
            let v = serde_json::to_value(e).unwrap();
            let errors: Vec<String> = evidence_schema
                .iter_errors(&v)
                .map(|e| e.to_string())
                .collect();
            assert!(
                errors.is_empty(),
                "evidence {} (payload {n}): {errors:?}",
                e.evidence_id
            );
        }
    }
}

#[test]
fn profile_rejects_unknown_corroborator_browser() {
    let schema = validator("profile.schema.json");
    let mut profile: Value = serde_json::from_str(
        &std::fs::read_to_string(
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../profiles/us-federal-public-web.json"),
        )
        .unwrap(),
    )
    .unwrap();
    profile["corroboration"] = json!({"browser": ["something-else"]});
    let errors: Vec<String> = schema
        .iter_errors(&profile)
        .map(|e| e.to_string())
        .collect();
    assert!(
        !errors.is_empty(),
        "profile with unknown corroborator must be rejected"
    );
    assert!(
        errors.iter().any(|e| e.contains("something-else")
            || e.contains("enum")
            || e.contains("corroboration")),
        "errors should mention the bad value or the enum: {errors:?}"
    );
}

#[test]
fn pinned_digest_test_would_fail_if_bundle_altered() {
    let bundle = include_str!("../../../crates/diligent-runtime/vendor/axe-core/axe.min.js");
    let pinned = "20c09fe157a8a34a30e241aaa1fcdade657734f08ab379ecfbeb7d45cc46e878";
    let actual: String = Sha256::digest(bundle.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    assert_eq!(
        &actual, pinned,
        "vendored axe bundle digest must match the pinned value in scripts.rs"
    );
}

#[test]
fn no_corroborate_skips_the_step_even_when_an_axe_result_exists() {
    // axe passes the pale pair that BRW-TEXT-CONTRAST fails, which would normally move it to needs_review.
    let payload = axe_payload(
        Some("4.14.0"),
        vec![
            axe_rule(
                "color-contrast",
                "pass",
                vec![axe_node(Some("p#low"), "pass")],
            ),
            // A violation the checks never reported: normally becomes an axe-only lead.
            axe_rule(
                "aria-roles",
                "violation",
                vec![axe_node(Some("p#high"), "violation")],
            ),
        ],
        None,
    );
    let (_d, set) = load("no-corroborate", &payload);
    let with = assess_set(&set, &opts()).unwrap();
    assert!(with.summary.corroboration.is_some());
    assert!(!with.summary.corroboration_disabled);
    assert_eq!(
        row(&with, "BRW-TEXT-CONTRAST", "p#low").status,
        State::NeedsReview
    );

    let mut off = opts();
    off.corroborate = false;
    let without = assess_set(&set, &off).unwrap();
    assert!(without.summary.corroboration.is_none());
    assert!(without.summary.corroboration_disabled);
    assert_eq!(
        row(&without, "BRW-TEXT-CONTRAST", "p#low").status,
        State::Fail
    );
    for r in &without.results {
        assert!(
            r.extensions.get("diligent.corroboration").is_none(),
            "{} carries corroboration",
            r.check_id
        );
    }
    assert!(
        without
            .evidence
            .iter()
            .all(|e| e.trust != "inference" && e.context.get("corroborator").is_none()),
        "no axe evidence rows"
    );
    assert!(
        without.results.len() <= with.results.len(),
        "axe-only leads are not added"
    );
}
