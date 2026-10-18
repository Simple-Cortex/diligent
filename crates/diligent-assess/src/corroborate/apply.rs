//! The corroboration step: findings in, findings out, plus summary counts.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

use diligent_capture::{AxeNode, AxeOutcome, AxeResult, AxeRule, CaptureSet, Viewport};
use serde::Serialize;
use serde_json::{json, Value};

use super::map;
use crate::capture_view::BASELINE;
use crate::checks::{self, clip};
use crate::emit::same_viewport;
use crate::model::{EvidenceKey, Finding, State};

/// Engine name written into every corroboration record.
pub const ENGINE: &str = "axe-core";
/// Limitation added to a finding that a disagreement moved to `needs_review`.
pub const DISAGREE_LIMITATION: &str =
    "corroborator disagreed with the deterministic result; a person must decide";
/// Limitation on a finding that only the corroborator raised.
pub const LEAD_LIMITATION: &str = "not confirmed by the deterministic check; a person must decide";

/// Longest corroborator summary text copied into a row, in characters.
const SUMMARY_MAX: usize = 200;

/// One `axe_result` capture with the viewport it was recorded at (baseline media).
pub struct AxeAt<'a> {
    /// Viewport of the capture.
    pub viewport: Viewport,
    /// The parsed payload.
    pub result: &'a AxeResult,
}

/// Counts over one corroborated assessment. `None` in [`crate::Summary`] when no `axe_result` exists.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CorroborationSummary {
    /// Always `axe-core`.
    pub engine: String,
    /// Version of the first payload that reported one.
    pub version: Option<String>,
    /// Findings the corroborator agreed with.
    pub agree: usize,
    /// Findings the corroborator disagreed with; each was moved to `needs_review`.
    pub disagree: usize,
    /// Leads only the corroborator raised.
    pub axe_only: usize,
    /// Findings of mapped checks where the corroborator said nothing usable.
    pub no_signal: usize,
    /// Findings of checks with no axe-core counterpart.
    pub no_coverage: usize,
    /// Findings already `needs_review` or `not_tested` where axe-core had something to say.
    pub informational: usize,
    /// Findings of mapped checks whose capture reported a corroborator error.
    pub unavailable: usize,
    /// Rule ids axe-core flagged that no check maps. Never emitted as findings.
    pub unmapped_corroborator_violations: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Out {
    Violation,
    Pass,
    Incomplete,
    Inapplicable,
}

fn out(o: &AxeOutcome) -> Out {
    match o {
        AxeOutcome::Violation => Out::Violation,
        AxeOutcome::Pass => Out::Pass,
        AxeOutcome::Incomplete => Out::Incomplete,
        AxeOutcome::Inapplicable => Out::Inapplicable,
    }
}

fn out_str(o: Out) -> &'static str {
    match o {
        Out::Violation => "violation",
        Out::Pass => "pass",
        Out::Incomplete => "incomplete",
        Out::Inapplicable => "inapplicable",
    }
}

/// Lookups over one axe payload.
struct Index<'a> {
    at: &'a AxeAt<'a>,
    by_id: HashMap<&'a str, Vec<&'a AxeRule>>,
    /// Selector to (rule id, node), in payload order.
    by_selector: HashMap<&'a str, Vec<(&'a str, &'a AxeNode)>>,
}

impl<'a> Index<'a> {
    fn new(at: &'a AxeAt<'a>) -> Index<'a> {
        let mut by_id: HashMap<&str, Vec<&AxeRule>> = HashMap::new();
        let mut by_selector: HashMap<&str, Vec<(&str, &AxeNode)>> = HashMap::new();
        for rule in &at.result.rules {
            by_id.entry(rule.id.as_str()).or_default().push(rule);
            for node in &rule.nodes {
                if let Some(sel) = node.selector.as_deref() {
                    by_selector
                        .entry(sel)
                        .or_default()
                        .push((rule.id.as_str(), node));
                }
            }
        }
        Index {
            at,
            by_id,
            by_selector,
        }
    }
}

/// What axe-core said about one finding's instance.
struct Judgement {
    /// `Violation` or `Pass` when the corroborator took a side.
    signal: Option<Out>,
    /// The nodes that matched, as row values.
    nodes: Vec<Value>,
}

fn node_value(selector: &str, node: &AxeNode) -> Value {
    json!({
        "selector": selector,
        "outcome": out_str(out(&node.outcome)),
        "summary": node.summary.as_deref().map(|s| clip(s, SUMMARY_MAX)),
    })
}

/// Violation beats pass; incomplete and inapplicable take no side.
fn fold(outcomes: impl Iterator<Item = Out>) -> Option<Out> {
    let mut signal = None;
    for o in outcomes {
        match o {
            Out::Violation => return Some(Out::Violation),
            Out::Pass => signal = Some(Out::Pass),
            Out::Incomplete | Out::Inapplicable => {}
        }
    }
    signal
}

fn judge(idx: &Index<'_>, mapped: &[&str], instance: &str, scope: Option<&str>) -> Judgement {
    // The instance first, then the container the other engine may have named instead.
    let mut hits: Vec<(&str, &str, &AxeNode)> = Vec::new();
    for sel in std::iter::once(instance).chain(scope.filter(|s| *s != instance)) {
        if let Some(list) = idx.by_selector.get(sel) {
            hits.extend(
                list.iter()
                    .filter(|(rule, _)| mapped.contains(rule))
                    .map(|(r, n)| (sel, *r, *n)),
            );
        }
    }
    if !hits.is_empty() {
        return Judgement {
            signal: fold(hits.iter().map(|(_, _, n)| out(&n.outcome))),
            nodes: hits.iter().map(|(sel, _, n)| node_value(sel, n)).collect(),
        };
    }
    if instance == "html" {
        // A page-level finding with no node of its own is judged by the rule outcomes.
        let rules = mapped.iter().filter_map(|id| idx.by_id.get(id)).flatten();
        return Judgement {
            signal: fold(rules.map(|r| out(&r.outcome))),
            nodes: Vec::new(),
        };
    }
    Judgement {
        signal: None,
        nodes: Vec::new(),
    }
}

fn record(
    version: Option<&str>,
    rule_ids: &[&str],
    outcome: &str,
    original: Option<State>,
    media_mismatch: bool,
    nodes: Vec<Value>,
) -> Value {
    let mut rec = serde_json::Map::new();
    rec.insert("engine".into(), json!(ENGINE));
    rec.insert("version".into(), json!(version));
    rec.insert("rule_ids".into(), json!(rule_ids));
    rec.insert("outcome".into(), json!(outcome));
    if let Some(s) = original {
        rec.insert("original_state".into(), json!(s.as_str()));
    }
    if media_mismatch {
        rec.insert("media_mismatch".into(), json!(true));
    }
    if !nodes.is_empty() {
        rec.insert("nodes".into(), Value::Array(nodes));
    }
    Value::Object(rec)
}

fn axe_key(viewport: Viewport) -> EvidenceKey {
    EvidenceKey {
        kind: "axe_result",
        viewport,
        media: Some(BASELINE.to_string()),
    }
}

fn corroborate_one(idx: &Index<'_>, mut f: Finding) -> Finding {
    let mapped = map::rules_for(f.check_id);
    let axe = idx.at.result;
    let version = axe.version.as_deref();
    let vp = idx.at.viewport;
    let mismatch = f.media.as_deref().is_some_and(|m| m != BASELINE);

    if mapped.is_empty() {
        f.corroboration = Some(record(
            version,
            &[],
            "no_coverage",
            None,
            mismatch,
            Vec::new(),
        ));
        return f.with(axe_key(vp));
    }
    if let Some(err) = &axe.error {
        let mut rec = record(version, mapped, "unavailable", None, mismatch, Vec::new());
        rec["error"] = json!({ "kind": err.kind, "message": clip(&err.message, SUMMARY_MAX) });
        f.corroboration = Some(rec);
        return f.with(axe_key(vp));
    }

    let j = judge(idx, mapped, &f.instance, f.corroboration_scope.as_deref());
    let (outcome, downgrade) = match (f.state, j.signal) {
        (State::NeedsReview | State::NotTested, s) => {
            if s.is_some() || !j.nodes.is_empty() {
                ("informational", false)
            } else {
                ("no_signal", false)
            }
        }
        (State::Fail, Some(Out::Violation)) | (State::Pass, Some(Out::Pass)) => ("agree", false),
        (State::Fail, Some(Out::Pass)) | (State::Pass, Some(Out::Violation)) => ("disagree", true),
        (_, _) => ("no_signal", false),
    };
    let original = downgrade.then_some(f.state);
    if downgrade {
        // A direct assignment: the corroborator can only ask for review.
        f.state = State::NeedsReview;
        f.limitations.push(DISAGREE_LIMITATION.to_string());
    }
    f.corroboration = Some(record(
        version, mapped, outcome, original, mismatch, j.nodes,
    ));
    f.with(axe_key(vp))
}

/// Which payload judges a finding: the one at its viewport, else the widest when it has none.
fn pick<'a, 'b>(indexes: &'b [Index<'a>], viewport: Option<Viewport>) -> Option<&'b Index<'a>> {
    match viewport {
        Some(v) => indexes.iter().find(|i| same_viewport(i.at.viewport, v)),
        None => indexes.iter().max_by_key(|i| i.at.viewport.width),
    }
}

#[derive(Default)]
struct Lead {
    rule_ids: BTreeSet<&'static str>,
    summary: Option<String>,
}

/// Leads for one payload: violation nodes of mapped rules that no Rust finding of that check covers.
fn leads_for(idx: &Index<'_>, findings: &[Finding]) -> Vec<Finding> {
    let axe = idx.at.result;
    let vp = idx.at.viewport;
    let mut out_leads: Vec<Finding> = Vec::new();
    if axe.error.is_some() {
        return out_leads;
    }
    for (check_id, rules) in map::MAP {
        if rules.is_empty() {
            continue;
        }
        let Some(info) = checks::all().iter().find(|i| i.id == *check_id) else {
            continue;
        };
        let ran_here: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.check_id == *check_id && f.viewport.is_some_and(|v| same_viewport(v, vp)))
            .collect();
        if ran_here.is_empty() {
            continue;
        }
        let covered: HashSet<&str> = ran_here
            .iter()
            .flat_map(|f| {
                std::iter::once(f.instance.as_str()).chain(f.corroboration_scope.as_deref())
            })
            .collect();
        let mut by_selector: BTreeMap<&str, Lead> = BTreeMap::new();
        let mut unresolved: BTreeMap<&'static str, (usize, Option<String>)> = BTreeMap::new();
        for rule_id in rules.iter() {
            let Some(found) = idx.by_id.get(rule_id) else {
                continue;
            };
            for rule in found {
                for node in rule
                    .nodes
                    .iter()
                    .filter(|n| out(&n.outcome) == Out::Violation)
                {
                    match node.selector.as_deref() {
                        Some(sel) if covered.contains(sel) => {}
                        Some(sel) => {
                            let lead = by_selector.entry(sel).or_default();
                            lead.rule_ids.insert(rule_id);
                            if lead.summary.is_none() {
                                lead.summary =
                                    node.summary.as_deref().map(|s| clip(s, SUMMARY_MAX));
                            }
                        }
                        None => {
                            let e = unresolved.entry(rule_id).or_insert((0, None));
                            e.0 += 1;
                            if e.1.is_none() {
                                e.1 = node.summary.as_deref().map(|s| clip(s, SUMMARY_MAX));
                            }
                        }
                    }
                }
            }
        }
        for (sel, lead) in by_selector {
            let ids: Vec<&str> = lead.rule_ids.iter().copied().collect();
            let what = lead.summary.clone().unwrap_or_else(|| ids.join(", "));
            let node = json!({ "selector": sel, "outcome": "violation", "summary": lead.summary });
            out_leads.push(lead_finding(
                info,
                vp,
                sel.to_string(),
                &what,
                axe,
                &ids,
                vec![node],
            ));
        }
        for (rule_id, (count, summary)) in unresolved {
            let what = format!(
                "{} (target could not be resolved to one element; {count} node(s))",
                summary.clone().unwrap_or_else(|| rule_id.to_string())
            );
            let node =
                json!({ "selector": Value::Null, "outcome": "violation", "summary": summary });
            out_leads.push(lead_finding(
                info,
                vp,
                "html".to_string(),
                &what,
                axe,
                &[rule_id],
                vec![node],
            ));
        }
    }
    out_leads
}

fn lead_finding(
    info: &'static crate::model::CheckInfo,
    vp: Viewport,
    instance: String,
    what: &str,
    axe: &AxeResult,
    rule_ids: &[&str],
    nodes: Vec<Value>,
) -> Finding {
    Finding {
        check_id: info.id,
        state: State::NeedsReview,
        instance,
        expected: Value::String(info.summary.to_string()),
        observed: Value::String(format!("flagged only by the corroborator: {what}")),
        limitations: vec![LEAD_LIMITATION.to_string()],
        evidence: vec![
            EvidenceKey {
                kind: "dom_snapshot",
                viewport: vp,
                media: Some(BASELINE.to_string()),
            },
            axe_key(vp),
        ],
        wcag: info.informs.to_vec(),
        viewport: Some(vp),
        media: Some(BASELINE.to_string()),
        corroboration: Some(record(
            axe.version.as_deref(),
            rule_ids,
            "axe_only",
            None,
            false,
            nodes,
        )),
        corroboration_scope: None,
    }
}

/// Rule ids axe-core flagged that no check maps.
fn unmapped_violations(indexes: &[Index<'_>]) -> Vec<String> {
    let mut ids: BTreeSet<String> = BTreeSet::new();
    for idx in indexes {
        if idx.at.result.error.is_some() {
            continue;
        }
        for rule in &idx.at.result.rules {
            let flagged = out(&rule.outcome) == Out::Violation
                || rule.nodes.iter().any(|n| out(&n.outcome) == Out::Violation);
            if flagged && !map::is_mapped_rule(&rule.id) {
                ids.insert(rule.id.clone());
            }
        }
    }
    ids.into_iter().collect()
}

fn summarize(findings: &[Finding], indexes: &[Index<'_>]) -> CorroborationSummary {
    let mut s = CorroborationSummary {
        engine: ENGINE.to_string(),
        version: indexes.iter().find_map(|i| i.at.result.version.clone()),
        agree: 0,
        disagree: 0,
        axe_only: 0,
        no_signal: 0,
        no_coverage: 0,
        informational: 0,
        unavailable: 0,
        unmapped_corroborator_violations: unmapped_violations(indexes),
    };
    for f in findings {
        match f.corroboration.as_ref().and_then(|c| c["outcome"].as_str()) {
            Some("agree") => s.agree += 1,
            Some("disagree") => s.disagree += 1,
            Some("axe_only") => s.axe_only += 1,
            Some("no_signal") => s.no_signal += 1,
            Some("no_coverage") => s.no_coverage += 1,
            Some("informational") => s.informational += 1,
            Some("unavailable") => s.unavailable += 1,
            _ => {}
        }
    }
    s
}

/// Corroborates `findings` against the given axe payloads.
///
/// Returns the findings (originals in order, then axe-only leads) and the
/// summary; both are unchanged and `None` when `axe` is empty.
pub fn apply_with(
    axe: &[AxeAt<'_>],
    findings: Vec<Finding>,
) -> (Vec<Finding>, Option<CorroborationSummary>) {
    if axe.is_empty() {
        return (findings, None);
    }
    let indexes: Vec<Index<'_>> = axe.iter().map(Index::new).collect();
    let mut result: Vec<Finding> = Vec::with_capacity(findings.len());
    for f in findings {
        match pick(&indexes, f.viewport) {
            Some(idx) => result.push(corroborate_one(idx, f)),
            None => result.push(f),
        }
    }
    let mut leads: Vec<Finding> = Vec::new();
    for idx in &indexes {
        leads.extend(leads_for(idx, &result));
    }
    result.extend(leads);
    let summary = summarize(&result, &indexes);
    (result, Some(summary))
}

/// Corroborates `findings` with the `axe_result` captures of `set` (baseline media, every viewport).
pub fn apply(
    set: &CaptureSet,
    findings: Vec<Finding>,
) -> (Vec<Finding>, Option<CorroborationSummary>) {
    let mut axe: Vec<AxeAt<'_>> = Vec::new();
    for vp in set.viewports() {
        if let Some(result) = set.axe(vp, BASELINE) {
            axe.push(AxeAt {
                viewport: vp,
                result,
            });
        }
    }
    apply_with(&axe, findings)
}

#[cfg(test)]
mod tests {
    use super::*;
    use diligent_capture::{AxeError, AxeNode, AxeRule};

    fn vp() -> Viewport {
        Viewport {
            width: 1280,
            height: 800,
            dpr: 1.0,
        }
    }

    fn node(sel: Option<&str>, outcome: AxeOutcome) -> AxeNode {
        AxeNode {
            selector: sel.map(str::to_string),
            outcome,
            summary: Some("Fix any of the following".into()),
        }
    }

    fn rule(id: &str, outcome: AxeOutcome, nodes: Vec<AxeNode>) -> AxeRule {
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

    fn result(rules: Vec<AxeRule>) -> AxeResult {
        AxeResult {
            engine: "axe-core".into(),
            version: Some("4.14.0".into()),
            url: "http://example.com/".into(),
            rules,
            error: None,
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
            media: Some(BASELINE.to_string()),
            corroboration: None,
            corroboration_scope: None,
        }
    }

    fn run(r: &AxeResult, findings: Vec<Finding>) -> (Vec<Finding>, CorroborationSummary) {
        let at = [AxeAt {
            viewport: vp(),
            result: r,
        }];
        let (f, s) = apply_with(&at, findings);
        (f, s.expect("summary"))
    }

    fn outcome(f: &Finding) -> &str {
        f.corroboration.as_ref().unwrap()["outcome"]
            .as_str()
            .unwrap()
    }

    #[test]
    fn no_axe_result_changes_nothing() {
        let (f, s) = apply_with(&[], vec![finding("BRW-TEXT-CONTRAST", State::Fail, "p")]);
        assert!(s.is_none());
        assert!(f[0].corroboration.is_none());
    }

    #[test]
    fn fail_with_violation_agrees() {
        let r = result(vec![rule(
            "color-contrast",
            AxeOutcome::Violation,
            vec![node(Some("p"), AxeOutcome::Violation)],
        )]);
        let (f, s) = run(&r, vec![finding("BRW-TEXT-CONTRAST", State::Fail, "p")]);
        assert_eq!(outcome(&f[0]), "agree");
        assert_eq!(f[0].state, State::Fail);
        assert_eq!(s.agree, 1);
    }

    #[test]
    fn fail_with_pass_node_disagrees_and_downgrades() {
        let r = result(vec![rule(
            "color-contrast",
            AxeOutcome::Pass,
            vec![node(Some("p"), AxeOutcome::Pass)],
        )]);
        let (f, s) = run(&r, vec![finding("BRW-TEXT-CONTRAST", State::Fail, "p")]);
        assert_eq!(outcome(&f[0]), "disagree");
        assert_eq!(f[0].state, State::NeedsReview);
        assert_eq!(
            f[0].corroboration.as_ref().unwrap()["original_state"],
            "fail"
        );
        assert!(f[0].limitations.iter().any(|l| l == DISAGREE_LIMITATION));
        assert_eq!(s.disagree, 1);
    }

    #[test]
    fn pass_with_violation_disagrees_and_downgrades() {
        let r = result(vec![rule(
            "color-contrast",
            AxeOutcome::Violation,
            vec![node(Some("p"), AxeOutcome::Violation)],
        )]);
        let (f, _) = run(&r, vec![finding("BRW-TEXT-CONTRAST", State::Pass, "p")]);
        assert_eq!(outcome(&f[0]), "disagree");
        assert_eq!(f[0].state, State::NeedsReview);
        assert_eq!(
            f[0].corroboration.as_ref().unwrap()["original_state"],
            "pass"
        );
    }

    #[test]
    fn pass_with_pass_agrees() {
        let r = result(vec![rule(
            "color-contrast",
            AxeOutcome::Pass,
            vec![node(Some("p"), AxeOutcome::Pass)],
        )]);
        let (f, _) = run(&r, vec![finding("BRW-TEXT-CONTRAST", State::Pass, "p")]);
        assert_eq!(outcome(&f[0]), "agree");
        assert_eq!(f[0].state, State::Pass);
    }

    #[test]
    fn incomplete_missing_node_and_inapplicable_are_no_signal() {
        let r = result(vec![
            rule(
                "color-contrast",
                AxeOutcome::Incomplete,
                vec![node(Some("p"), AxeOutcome::Incomplete)],
            ),
            rule("bypass", AxeOutcome::Inapplicable, vec![]),
        ]);
        let (f, s) = run(
            &r,
            vec![
                finding("BRW-TEXT-CONTRAST", State::Fail, "p"),
                finding("BRW-TEXT-CONTRAST", State::Fail, "span"),
                finding("BRW-BYPASS-BLOCKS", State::Fail, "html"),
            ],
        );
        assert!(f
            .iter()
            .all(|x| outcome(x) == "no_signal" && x.state == State::Fail));
        assert_eq!(s.no_signal, 3);
    }

    #[test]
    fn needs_review_and_not_tested_never_change() {
        let r = result(vec![rule(
            "color-contrast",
            AxeOutcome::Violation,
            vec![node(Some("p"), AxeOutcome::Violation)],
        )]);
        let (f, s) = run(
            &r,
            vec![
                finding("BRW-TEXT-CONTRAST", State::NeedsReview, "p"),
                finding("BRW-TEXT-CONTRAST", State::NotTested, "p"),
            ],
        );
        assert!(f.iter().take(2).all(|x| outcome(x) == "informational"));
        assert_eq!(f[0].state, State::NeedsReview);
        assert_eq!(f[1].state, State::NotTested);
        assert_eq!(s.informational, 2);
    }

    #[test]
    fn page_level_finding_uses_rule_outcomes() {
        let r = result(vec![rule("bypass", AxeOutcome::Violation, vec![])]);
        let (f, _) = run(&r, vec![finding("BRW-BYPASS-BLOCKS", State::Fail, "html")]);
        assert_eq!(outcome(&f[0]), "agree");
        let r = result(vec![rule("bypass", AxeOutcome::Pass, vec![])]);
        let (f, _) = run(&r, vec![finding("BRW-BYPASS-BLOCKS", State::Fail, "html")]);
        assert_eq!(outcome(&f[0]), "disagree");
        assert_eq!(f[0].state, State::NeedsReview);
    }

    #[test]
    fn violation_beats_pass_across_mapped_rules() {
        let r = result(vec![
            rule(
                "aria-valid-attr",
                AxeOutcome::Pass,
                vec![node(Some("div"), AxeOutcome::Pass)],
            ),
            rule(
                "aria-roles",
                AxeOutcome::Violation,
                vec![node(Some("div"), AxeOutcome::Violation)],
            ),
        ]);
        let (f, _) = run(&r, vec![finding("BRW-ARIA-ATTR-VALID", State::Pass, "div")]);
        assert_eq!(outcome(&f[0]), "disagree");
    }

    #[test]
    fn unmapped_check_is_no_coverage_and_unchanged() {
        let r = result(vec![]);
        let (f, s) = run(&r, vec![finding("BRW-FOCUS-VISIBLE", State::Fail, "a")]);
        assert_eq!(outcome(&f[0]), "no_coverage");
        assert_eq!(f[0].state, State::Fail);
        assert_eq!(s.no_coverage, 1);
    }

    #[test]
    fn error_payload_marks_mapped_findings_unavailable() {
        let mut r = result(vec![]);
        r.version = None;
        r.error = Some(AxeError {
            kind: "timeout".into(),
            message: "axe took too long".into(),
        });
        let (f, s) = run(
            &r,
            vec![
                finding("BRW-TEXT-CONTRAST", State::Fail, "p"),
                finding("BRW-FOCUS-VISIBLE", State::Fail, "a"),
            ],
        );
        assert_eq!(outcome(&f[0]), "unavailable");
        assert_eq!(
            f[0].corroboration.as_ref().unwrap()["error"]["kind"],
            "timeout"
        );
        assert_eq!(f[0].state, State::Fail);
        assert_eq!(outcome(&f[1]), "no_coverage");
        assert_eq!(s.unavailable, 1);
        assert!(f.len() == 2, "an error payload raises no leads");
    }

    #[test]
    fn media_mismatch_is_recorded() {
        let r = result(vec![rule(
            "color-contrast",
            AxeOutcome::Violation,
            vec![node(Some("p"), AxeOutcome::Violation)],
        )]);
        let mut x = finding("BRW-TEXT-CONTRAST", State::Fail, "p");
        x.media = Some("text-spacing".into());
        let (f, _) = run(&r, vec![x]);
        assert_eq!(f[0].corroboration.as_ref().unwrap()["media_mismatch"], true);
    }

    #[test]
    fn axe_only_lead_is_needs_review_and_deduplicated() {
        let r = result(vec![
            rule(
                "aria-roles",
                AxeOutcome::Violation,
                vec![node(Some("div#x"), AxeOutcome::Violation)],
            ),
            rule(
                "aria-valid-attr",
                AxeOutcome::Violation,
                vec![node(Some("div#x"), AxeOutcome::Violation)],
            ),
            rule(
                "color-contrast",
                AxeOutcome::Violation,
                vec![node(Some("p"), AxeOutcome::Violation)],
            ),
        ]);
        let (f, s) = run(
            &r,
            vec![
                finding("BRW-ARIA-ATTR-VALID", State::NotTested, "html"),
                finding("BRW-TEXT-CONTRAST", State::Fail, "p"),
            ],
        );
        let leads: Vec<&Finding> = f.iter().filter(|x| outcome(x) == "axe_only").collect();
        assert_eq!(leads.len(), 1);
        assert_eq!(leads[0].state, State::NeedsReview);
        assert_eq!(leads[0].instance, "div#x");
        assert_eq!(leads[0].check_id, "BRW-ARIA-ATTR-VALID");
        assert_eq!(
            leads[0].corroboration.as_ref().unwrap()["rule_ids"],
            json!(["aria-roles", "aria-valid-attr"])
        );
        assert!(leads[0]
            .observed
            .as_str()
            .unwrap()
            .starts_with("flagged only by the corroborator: "));
        assert_eq!(s.axe_only, 1);
    }

    #[test]
    fn leads_need_the_check_to_have_run_at_that_viewport() {
        let r = result(vec![rule(
            "color-contrast",
            AxeOutcome::Violation,
            vec![node(Some("p"), AxeOutcome::Violation)],
        )]);
        let (f, s) = run(&r, vec![finding("BRW-BYPASS-BLOCKS", State::Pass, "html")]);
        assert!(f.iter().all(|x| outcome(x) != "axe_only"));
        assert_eq!(s.axe_only, 0);
    }

    #[test]
    fn unmapped_violations_are_counted_not_emitted() {
        let r = result(vec![
            rule(
                "image-alt",
                AxeOutcome::Violation,
                vec![node(Some("img"), AxeOutcome::Violation)],
            ),
            rule("region", AxeOutcome::Violation, vec![]),
            rule(
                "html-has-lang",
                AxeOutcome::Pass,
                vec![node(Some("html"), AxeOutcome::Pass)],
            ),
        ]);
        let (f, s) = run(&r, vec![finding("BRW-TEXT-CONTRAST", State::Pass, "p")]);
        assert_eq!(f.len(), 1);
        assert_eq!(
            s.unmapped_corroborator_violations,
            vec!["image-alt".to_string(), "region".to_string()]
        );
    }

    #[test]
    fn unresolved_violation_node_becomes_one_page_level_lead() {
        let r = result(vec![rule(
            "color-contrast",
            AxeOutcome::Violation,
            vec![
                node(None, AxeOutcome::Violation),
                node(None, AxeOutcome::Violation),
            ],
        )]);
        let (f, _) = run(&r, vec![finding("BRW-TEXT-CONTRAST", State::Pass, "p")]);
        let leads: Vec<&Finding> = f.iter().filter(|x| outcome(x) == "axe_only").collect();
        assert_eq!(leads.len(), 1);
        assert_eq!(leads[0].instance, "html");
    }

    #[test]
    fn a_container_named_by_axe_covers_a_descendant_finding() {
        let r = result(vec![rule(
            "nested-interactive",
            AxeOutcome::Violation,
            vec![
                node(Some("button"), AxeOutcome::Violation),
                node(Some("span"), AxeOutcome::Pass),
            ],
        )]);
        let mut x = finding("BRW-PRESENTATIONAL-CHILDREN", State::Fail, "span");
        x.corroboration_scope = Some("button".to_string());
        let (f, s) = run(&r, vec![x]);
        assert_eq!(outcome(&f[0]), "agree");
        assert_eq!(f[0].state, State::Fail);
        assert_eq!(f.len(), 1, "the container is not an axe-only lead");
        assert_eq!(s.axe_only, 0);
    }

    #[test]
    fn many_nodes_and_duplicate_rule_ids_do_not_panic() {
        let nodes: Vec<AxeNode> = (0..10_000)
            .map(|i| node(Some(&format!("div:nth-child({i})")), AxeOutcome::Violation))
            .collect();
        let r = result(vec![
            rule("color-contrast", AxeOutcome::Violation, nodes),
            rule(
                "color-contrast",
                AxeOutcome::Pass,
                vec![node(Some("p"), AxeOutcome::Pass)],
            ),
        ]);
        let (f, s) = run(&r, vec![finding("BRW-TEXT-CONTRAST", State::Fail, "p")]);
        assert_eq!(outcome(&f[0]), "disagree");
        assert_eq!(s.axe_only, 10_000);
    }
}
