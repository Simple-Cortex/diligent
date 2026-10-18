//! Tests for the optional axe-core corroboration step.
//!
//! Captures are built in a temporary directory: one DOM snapshot with a failing
//! and a passing paragraph, plus an `axe_result` payload that each test shapes.
//! The selectors `p#low` (fails contrast) and `p#high` (passes) are what the
//! text-contrast check reports for that DOM.

use std::path::{Path, PathBuf};

use diligent_assess::corroborate::{self, AxeAt};
use diligent_assess::{assess_set, checks, AssessOptions, Finding, State};
use diligent_capture::{AxeNode, AxeOutcome, AxeResult, AxeRule, CaptureSet, Viewport};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const VIEWPORT: &str = r#"{"width":1280,"height":800,"dpr":1.0}"#;

struct TempDir(PathBuf);

impl TempDir {
    fn new(name: &str) -> TempDir {
        let dir =
            std::env::temp_dir().join(format!("diligent-corr-{}-{}", name, std::process::id()));
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

/// Writes a capture directory: the DOM, and an `axe_result` payload when given.
fn write(dir: &Path, axe: Option<&str>) {
    let dom = dom();
    let dom_sha = sha256(&dom);
    let mut lines = vec![entry("dom_snapshot", &dom_sha, dom.len())];
    std::fs::write(dir.join(format!("{dom_sha}.json")), &dom).unwrap();
    if let Some(axe) = axe {
        let sha = sha256(axe);
        lines.push(entry("axe_result", &sha, axe.len()));
        std::fs::write(dir.join(format!("{sha}.json")), axe).unwrap();
    }
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

fn load(name: &str, axe: Option<String>) -> (TempDir, CaptureSet) {
    let dir = TempDir::new(name);
    write(dir.path(), axe.as_deref());
    let set = CaptureSet::load(dir.path()).expect("capture loads");
    (dir, set)
}

fn contrast(rules: Vec<Value>) -> Option<String> {
    Some(axe_payload(Some("4.14.0"), rules, None))
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

#[test]
fn the_synthetic_dom_gives_one_fail_and_one_pass() {
    let (_d, set) = load("baseline-states", None);
    let report = assess_set(&set, &opts()).unwrap();
    assert_eq!(
        row(&report, "BRW-TEXT-CONTRAST", "p#low").status,
        State::Fail
    );
    assert_eq!(
        row(&report, "BRW-TEXT-CONTRAST", "p#high").status,
        State::Pass
    );
}

#[test]
fn a_missing_axe_capture_changes_nothing_and_adds_no_extension() {
    let (_d, set) = load("missing", None);
    let report = assess_set(&set, &opts()).unwrap();
    assert!(report.summary.corroboration.is_none());
    assert!(report
        .results
        .iter()
        .all(|r| r.extensions.get("diligent.corroboration").is_none()));
    assert!(report
        .evidence
        .iter()
        .all(|e| e.trust == "direct_observation"));
    let pre = checks::run(&set);
    assert_eq!(pre.len(), report.results.len());
    for (f, r) in pre.iter().zip(&report.results) {
        assert_eq!(f.state, r.status);
    }
}

#[test]
fn fail_and_violation_agree() {
    let rules = vec![axe_rule(
        "color-contrast",
        "violation",
        vec![axe_node(Some("p#low"), "violation")],
    )];
    let (_d, set) = load("agree", contrast(rules));
    let report = assess_set(&set, &opts()).unwrap();
    let r = row(&report, "BRW-TEXT-CONTRAST", "p#low");
    assert_eq!(r.status, State::Fail);
    assert_eq!(corr(r)["outcome"], "agree");
    assert_eq!(corr(r)["engine"], "axe-core");
    assert_eq!(corr(r)["version"], "4.14.0");
    assert_eq!(corr(r)["rule_ids"], json!(["color-contrast"]));
    assert_eq!(report.summary.corroboration.as_ref().unwrap().agree, 1);
}

#[test]
fn fail_with_an_explicit_axe_pass_is_downgraded_to_needs_review() {
    let rules = vec![axe_rule(
        "color-contrast",
        "pass",
        vec![axe_node(Some("p#low"), "pass")],
    )];
    let (_d, set) = load("fail-vs-pass", contrast(rules));
    let report = assess_set(&set, &opts()).unwrap();
    let r = row(&report, "BRW-TEXT-CONTRAST", "p#low");
    assert_eq!(r.status, State::NeedsReview);
    assert_eq!(corr(r)["outcome"], "disagree");
    assert_eq!(corr(r)["original_state"], "fail");
    assert!(r
        .limitations
        .iter()
        .any(|l| l == corroborate::DISAGREE_LIMITATION));
    let s = report.summary.corroboration.as_ref().unwrap();
    assert_eq!(s.disagree, 1);
    assert_eq!(report.summary.by_check["BRW-TEXT-CONTRAST"], "needs_review");
}

#[test]
fn pass_with_an_axe_violation_is_downgraded_to_needs_review() {
    let rules = vec![axe_rule(
        "color-contrast",
        "violation",
        vec![axe_node(Some("p#high"), "violation")],
    )];
    let (_d, set) = load("pass-vs-violation", contrast(rules));
    let report = assess_set(&set, &opts()).unwrap();
    let r = row(&report, "BRW-TEXT-CONTRAST", "p#high");
    assert_eq!(r.status, State::NeedsReview);
    assert_eq!(corr(r)["original_state"], "pass");
    assert_eq!(
        row(&report, "BRW-TEXT-CONTRAST", "p#low").status,
        State::Fail
    );
}

#[test]
fn pass_with_an_axe_pass_agrees() {
    let rules = vec![axe_rule(
        "color-contrast",
        "pass",
        vec![axe_node(Some("p#high"), "pass")],
    )];
    let (_d, set) = load("pass-agree", contrast(rules));
    let report = assess_set(&set, &opts()).unwrap();
    let r = row(&report, "BRW-TEXT-CONTRAST", "p#high");
    assert_eq!(r.status, State::Pass);
    assert_eq!(corr(r)["outcome"], "agree");
}

#[test]
fn incomplete_and_unmatched_selectors_give_no_signal() {
    let rules = vec![axe_rule(
        "color-contrast",
        "incomplete",
        vec![
            axe_node(Some("p#low"), "incomplete"),
            axe_node(Some("p#nothing-matches"), "pass"),
        ],
    )];
    let (_d, set) = load("no-signal", contrast(rules));
    let report = assess_set(&set, &opts()).unwrap();
    for sel in ["p#low", "p#high"] {
        let r = row(&report, "BRW-TEXT-CONTRAST", sel);
        assert_eq!(corr(r)["outcome"], "no_signal", "{sel}");
    }
    assert_eq!(
        row(&report, "BRW-TEXT-CONTRAST", "p#low").status,
        State::Fail
    );
    assert_eq!(
        row(&report, "BRW-TEXT-CONTRAST", "p#high").status,
        State::Pass
    );
}

#[test]
fn a_check_without_an_axe_counterpart_is_no_coverage() {
    let (_d, set) = load("no-coverage", contrast(vec![]));
    let report = assess_set(&set, &opts()).unwrap();
    let r = report
        .results
        .iter()
        .find(|r| r.check_id == "BRW-REFLOW-320")
        .expect("reflow row");
    assert_eq!(corr(r)["outcome"], "no_coverage");
    assert!(report.summary.corroboration.as_ref().unwrap().no_coverage >= 1);
}

#[test]
fn an_axe_only_violation_becomes_a_needs_review_lead() {
    let rules = vec![axe_rule(
        "color-contrast",
        "violation",
        vec![
            axe_node(Some("p#ghost"), "violation"),
            axe_node(Some("p#low"), "violation"),
        ],
    )];
    let (_d, set) = load("axe-only", contrast(rules));
    let report = assess_set(&set, &opts()).unwrap();
    let lead = row(&report, "BRW-TEXT-CONTRAST", "p#ghost");
    assert_eq!(lead.status, State::NeedsReview);
    assert_eq!(corr(lead)["outcome"], "axe_only");
    assert!(lead
        .observed
        .as_str()
        .unwrap()
        .starts_with("flagged only by the corroborator: "));
    assert!(lead
        .limitations
        .iter()
        .any(|l| l == corroborate::LEAD_LIMITATION));
    assert_eq!(report.summary.corroboration.as_ref().unwrap().axe_only, 1);
    assert_eq!(
        row(&report, "BRW-TEXT-CONTRAST", "p#low").status,
        State::Fail
    );
}

#[test]
fn unmapped_violations_are_counted_but_never_findings() {
    let rules = vec![
        axe_rule(
            "image-alt",
            "violation",
            vec![axe_node(Some("img"), "violation")],
        ),
        axe_rule("region", "violation", vec![]),
    ];
    let (_d, set) = load("unmapped", contrast(rules));
    let before = checks::run(&set).len();
    let report = assess_set(&set, &opts()).unwrap();
    assert_eq!(report.results.len(), before);
    let s = report.summary.corroboration.as_ref().unwrap();
    assert_eq!(
        s.unmapped_corroborator_violations,
        vec!["image-alt".to_string(), "region".to_string()]
    );
}

#[test]
fn an_error_payload_marks_mapped_checks_unavailable_and_changes_no_state() {
    let payload = axe_payload(
        None,
        vec![],
        Some(json!({"kind": "timeout", "message": "axe-core did not finish"})),
    );
    let (_d, set) = load("error", Some(payload));
    let report = assess_set(&set, &opts()).unwrap();
    let r = row(&report, "BRW-TEXT-CONTRAST", "p#low");
    assert_eq!(r.status, State::Fail);
    assert_eq!(corr(r)["outcome"], "unavailable");
    assert_eq!(corr(r)["error"]["kind"], "timeout");
    assert!(corr(r)["version"].is_null());
    let s = report.summary.corroboration.as_ref().unwrap();
    assert!(s.unavailable >= 2);
    assert_eq!(s.disagree, 0);
    assert_eq!(s.axe_only, 0);
}

#[test]
fn a_finding_from_another_media_mode_is_judged_by_baseline_and_flagged() {
    let vp = Viewport {
        width: 1280,
        height: 800,
        dpr: 1.0,
    };
    let result = AxeResult {
        engine: "axe-core".into(),
        version: Some("4.14.0".into()),
        url: "http://example.com".into(),
        rules: vec![AxeRule {
            id: "avoid-inline-spacing".into(),
            act_ids: vec![],
            tags: vec![],
            outcome: AxeOutcome::Pass,
            impact: None,
            nodes: vec![AxeNode {
                selector: Some("p#low".into()),
                outcome: AxeOutcome::Pass,
                summary: None,
            }],
            truncated: false,
        }],
        error: None,
    };
    let finding = Finding {
        check_id: "BRW-TEXT-SPACING-STYLE",
        state: State::Fail,
        instance: "p#low".into(),
        expected: Value::Null,
        observed: Value::Null,
        limitations: vec![],
        evidence: vec![],
        wcag: vec![],
        viewport: Some(vp),
        media: Some("text-spacing".into()),
        corroboration: None,
        corroboration_scope: None,
    };
    let (out, summary) = corroborate::apply_with(
        &[AxeAt {
            viewport: vp,
            result: &result,
        }],
        vec![finding],
    );
    let c = out[0].corroboration.as_ref().unwrap();
    assert_eq!(c["media_mismatch"], true);
    assert_eq!(c["outcome"], "disagree");
    assert_eq!(out[0].state, State::NeedsReview);
    assert_eq!(summary.unwrap().disagree, 1);
}

#[test]
fn duplicate_rule_ids_version_null_and_many_nodes_are_tolerated() {
    let nodes: Vec<Value> = (0..10_000)
        .map(|i| axe_node(Some(&format!("div:nth-child({i})")), "violation"))
        .collect();
    let rules = vec![
        axe_rule("color-contrast", "violation", nodes),
        axe_rule(
            "color-contrast",
            "pass",
            vec![axe_node(Some("p#high"), "pass")],
        ),
        axe_rule(
            "color-contrast",
            "pass",
            vec![axe_node(Some("p#high"), "pass")],
        ),
    ];
    let (_d, set) = load("hostile", Some(axe_payload(None, rules, None)));
    let start = std::time::Instant::now();
    let report = assess_set(&set, &opts()).unwrap();
    assert!(
        start.elapsed().as_secs() < 20,
        "10,000 nodes must stay cheap"
    );
    assert_eq!(
        corr(row(&report, "BRW-TEXT-CONTRAST", "p#high"))["outcome"],
        "agree"
    );
    assert!(corr(row(&report, "BRW-TEXT-CONTRAST", "p#high"))["version"].is_null());
    assert_eq!(
        report.summary.corroboration.as_ref().unwrap().axe_only,
        10_000
    );
}

#[test]
fn an_unknown_outcome_string_is_a_load_error_not_a_panic() {
    let rules = vec![axe_rule(
        "color-contrast",
        "maybe",
        vec![axe_node(Some("p#low"), "violation")],
    )];
    let dir = TempDir::new("unknown-outcome");
    write(dir.path(), Some(&axe_payload(Some("4.14.0"), rules, None)));
    assert!(CaptureSet::load(dir.path()).is_err());
}

#[test]
fn corroboration_never_makes_a_fail_from_a_non_fail_or_touches_review_states() {
    let shapes = [
        vec![axe_rule(
            "color-contrast",
            "violation",
            vec![
                axe_node(Some("p#low"), "violation"),
                axe_node(Some("p#high"), "violation"),
            ],
        )],
        vec![axe_rule(
            "color-contrast",
            "pass",
            vec![
                axe_node(Some("p#low"), "pass"),
                axe_node(Some("p#high"), "pass"),
            ],
        )],
        vec![
            axe_rule("bypass", "violation", vec![]),
            axe_rule(
                "valid-lang",
                "violation",
                vec![axe_node(Some("html"), "violation")],
            ),
        ],
        vec![
            axe_rule("bypass", "pass", vec![]),
            axe_rule("valid-lang", "pass", vec![axe_node(Some("html"), "pass")]),
        ],
    ];
    for (n, rules) in shapes.into_iter().enumerate() {
        let (_d, set) = load(&format!("invariants-{n}"), contrast(rules));
        let pre = checks::run(&set);
        let (post, _) = corroborate::apply(&set, pre.clone());
        assert!(post.len() >= pre.len());
        for (a, b) in pre.iter().zip(&post) {
            assert_eq!((a.check_id, &a.instance), (b.check_id, &b.instance));
            if a.state != b.state {
                assert!(
                    matches!(a.state, State::Pass | State::Fail),
                    "{}: {:?} moved",
                    a.check_id,
                    a.state
                );
                assert_eq!(b.state, State::NeedsReview);
                assert_eq!(
                    b.corroboration.as_ref().unwrap()["original_state"],
                    a.state.as_str()
                );
            }
        }
        for lead in &post[pre.len()..] {
            assert_eq!(lead.state, State::NeedsReview);
            assert_eq!(lead.corroboration.as_ref().unwrap()["outcome"], "axe_only");
        }
    }
}

#[test]
fn every_row_and_the_corroboration_evidence_validate_against_the_schemas() {
    let result_schema = validator("check-result.schema.json");
    let evidence_schema = validator("evidence-record.schema.json");
    let payloads = [
        contrast(vec![axe_rule(
            "color-contrast",
            "pass",
            vec![axe_node(Some("p#low"), "pass")],
        )]),
        contrast(vec![axe_rule(
            "color-contrast",
            "violation",
            vec![axe_node(Some("p#ghost"), "violation")],
        )]),
        Some(axe_payload(
            None,
            vec![],
            Some(json!({"kind": "collector_failure", "message": "page redefined a global"})),
        )),
    ];
    for (n, payload) in payloads.into_iter().enumerate() {
        let (_d, set) = load(&format!("schema-{n}"), payload);
        let report = assess_set(&set, &opts()).unwrap();
        for r in &report.results {
            let v = serde_json::to_value(r).unwrap();
            let errors: Vec<String> = result_schema
                .iter_errors(&v)
                .map(|e| e.to_string())
                .collect();
            assert!(errors.is_empty(), "result {}: {errors:?}", r.result_id);
        }
        for e in &report.evidence {
            let v = serde_json::to_value(e).unwrap();
            let errors: Vec<String> = evidence_schema
                .iter_errors(&v)
                .map(|e| e.to_string())
                .collect();
            assert!(errors.is_empty(), "evidence {}: {errors:?}", e.evidence_id);
        }
        let axe_ev = report
            .evidence
            .iter()
            .find(|e| e.context.get("corroborator").is_some())
            .expect("an axe evidence row");
        assert_eq!(axe_ev.kind, "runtime_observation");
        assert_eq!(axe_ev.trust, "inference");
        assert_eq!(axe_ev.context["corroborator"]["engine"], "axe-core");
        for r in report
            .results
            .iter()
            .filter(|r| r.extensions.get("diligent.corroboration").is_some())
        {
            assert!(
                r.evidence_refs.contains(&axe_ev.evidence_id),
                "result {} carries a corroboration record but does not cite the axe evidence",
                r.result_id
            );
        }
        assert!(report.summary.corroboration.is_some());
    }
}
