//! Turns check outcomes into evidence records and check results.

use std::collections::HashMap;

use scraper::ElementRef;
use serde_json::{json, Value};

use crate::dom::{self, Ctx};
use crate::model::{
    CheckInfo, CheckResult, EvidenceRecord, Finding, Redaction, SourceRef, State, COLLECTOR,
    ENGINE_VERSION, EXTENSION_KEY,
};
use crate::sha256::sha256_hex;
use crate::template::MARK;

const SNIPPET_CHARS: usize = 400;
const EVIDENCE_LIMITATION: &str =
    "Static parse of the markup text only: no rendering, computed styles, or scripts. Line, column and byte offset are reported when the source text contains the element; elements the parser implied have none.";
const TEMPLATE_LIMITATION: &str =
    "Template file: the snippet and attributes show the markup after template syntax was masked, and {expr} stands for a template expression.";
const EXPRESSION_NOTE: &str =
    "A template expression stands in for part of the value or name; its runtime value is not known.";
const TAINT_NOTE: &str =
    "The element carries a template expression or attribute spread that could supply what is missing.";
const CONDITIONAL_NOTE: &str =
    "The template holds a conditional or loop, so duplicates may not render together.";
/// Attributes whose value an expression can change the outcome for.
const TAINT_ATTRS: &[&str] = &[
    "alt",
    "aria-label",
    "aria-labelledby",
    "title",
    "label",
    "name",
    "lang",
    "href",
    "value",
    "placeholder",
    "for",
    "id",
    "role",
    "type",
    "content",
    "http-equiv",
    "scope",
    "headers",
    "autocomplete",
    "tabindex",
    "autoplay",
    "controls",
    "muted",
];
const RESULT_LIMITATION: &str =
    "Static pass. Does not establish that any WCAG success criterion is met.";

/// What a check concluded about one target.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Result state.
    pub state: State,
    /// What the check expected.
    pub expected: Value,
    /// What the check observed.
    pub observed: Value,
    /// Extra limitations specific to this result.
    pub limitations: Vec<String>,
    /// Primary pack rule when it differs from the check's first `informs` entry.
    pub rule: Option<&'static str>,
}

impl Outcome {
    /// An outcome with a textual expectation and a JSON observation.
    pub fn new(state: State, expected: &str, observed: Value) -> Self {
        Outcome {
            state,
            expected: Value::String(expected.to_string()),
            observed,
            limitations: Vec::new(),
            rule: None,
        }
    }

    /// Adds a limitation.
    pub fn limit(mut self, text: impl Into<String>) -> Self {
        self.limitations.push(text.into());
        self
    }

    /// Overrides the primary pack rule.
    pub fn rule(mut self, rule: &'static str) -> Self {
        self.rule = Some(rule);
        self
    }
}

/// Collects findings for one document.
pub struct Emitter<'a> {
    /// The parsed document the checks read.
    pub ctx: &'a Ctx<'a>,
    source: &'a SourceRef,
    doc_sha: String,
    prefix: String,
    findings: Vec<Finding>,
    counts: HashMap<&'static str, usize>,
}

impl<'a> Emitter<'a> {
    /// Starts an emitter for one document whose bytes hash to `doc_sha`.
    pub fn new(ctx: &'a Ctx<'a>, source: &'a SourceRef, doc_sha: String) -> Self {
        let prefix =
            sha256_hex(format!("{}\0{}", source.path, doc_sha).as_bytes())[..10].to_string();
        Emitter {
            ctx,
            source,
            doc_sha,
            prefix,
            findings: Vec::new(),
            counts: HashMap::new(),
        }
    }

    /// Findings recorded so far for `check_id`.
    pub fn count(&self, check_id: &str) -> usize {
        self.counts.get(check_id).copied().unwrap_or(0)
    }

    /// Records a finding anchored at `el`.
    pub fn emit(&mut self, info: &'static CheckInfo, el: ElementRef<'a>, outcome: Outcome) {
        if dom::is_component(el) {
            return;
        }
        let outcome = self.adjust(info, el, outcome);
        let pos = self.ctx.position(el);
        let templated = self.ctx.template.templated;
        let selector = self.ctx.selector(el);
        let (snippet, truncated) = dom::outer_html_snippet(el, SNIPPET_CHARS);
        let snippet = snippet.replace(MARK, "{expr}");
        let mut attributes = dom::attributes_json(el);
        show_expressions(&mut attributes);
        let evidence_id = format!(
            "ev-{}-{}",
            self.prefix,
            &sha256_hex(selector.as_bytes())[..10]
        );
        let evidence = EvidenceRecord {
            schema_version: "1.0.0".to_string(),
            evidence_id: evidence_id.clone(),
            run_id: self.source.run_id.clone(),
            kind: "source_span".to_string(),
            collector: COLLECTOR.to_string(),
            collector_version: ENGINE_VERSION.to_string(),
            captured_at: self.source.captured_at.clone(),
            snapshot_ref: None,
            context: json!({
                "scope": "static",
                "source_path": self.source.path,
                "selector": selector,
                "tag": dom::tag(el),
                "outer_html": snippet,
                "outer_html_truncated": truncated,
                "attributes": attributes,
                "byte_offset": pos.map(|p| p.offset),
                "line": pos.map(|p| p.line),
                "column": pos.map(|p| p.column),
            }),
            payload_ref: self.source.path.clone(),
            sha256: self.doc_sha.clone(),
            data_classification: "source_code".to_string(),
            redaction: Redaction {
                applied: false,
                flags: Vec::new(),
            },
            trust: "direct_observation".to_string(),
            limitations: {
                let mut l = vec![EVIDENCE_LIMITATION.to_string()];
                if templated {
                    l.push(TEMPLATE_LIMITATION.to_string());
                }
                l
            },
            derived_from: None,
        };

        let rule = outcome.rule.or_else(|| info.informs.first().copied());
        let obligation_id = match rule {
            Some(r) => format!("OBL-{r}"),
            None => format!("OBL-UNMAPPED-{}", info.id),
        };
        let rule_ids: Vec<&str> = match outcome.rule {
            Some(r) => vec![r],
            None => info.informs.to_vec(),
        };
        let seq = self.findings.len() + 1;
        let mut limitations = vec![RESULT_LIMITATION.to_string()];
        limitations.extend(outcome.limitations);
        let result = CheckResult {
            schema_version: "1.0.0".to_string(),
            result_id: format!("res-{}-{seq:04}", self.prefix),
            check_id: info.id.to_string(),
            check_version: ENGINE_VERSION.to_string(),
            obligation_id: obligation_id.clone(),
            instance_id: format!("{}::{}", self.source.path, selector),
            applicability_ref: obligation_id,
            status: outcome.state,
            evidence_refs: vec![evidence_id],
            expected: outcome.expected,
            observed: outcome.observed,
            limitations,
            executed_at: self.source.captured_at.clone(),
            engine_version: ENGINE_VERSION.to_string(),
            supersedes_result_id: None,
            extensions: {
                let mut ext = serde_json::Map::new();
                ext.insert(
                    EXTENSION_KEY.to_string(),
                    json!({
                        "scope": "static",
                        "instance": selector,
                        "source_path": self.source.path,
                        "line": pos.map(|p| p.line),
                        "act_rule_ids": info.act,
                        "rule_ids": rule_ids,
                    }),
                );
                Value::Object(ext)
            },
        };
        *self.counts.entry(info.id).or_insert(0) += 1;
        self.findings.push(Finding { evidence, result });
    }

    /// Records a finding anchored at the document root.
    pub fn emit_root(&mut self, info: &'static CheckInfo, outcome: Outcome) {
        let root = self.ctx.root();
        self.emit(info, root, outcome);
    }

    /// Downgrades `pass` and `fail` to `needs_review` where a template
    /// expression, spread or conditional means the markup cannot settle it.
    fn adjust(&self, info: &CheckInfo, el: ElementRef<'a>, mut o: Outcome) -> Outcome {
        let facts = self.ctx.template;
        if !facts.templated {
            return o;
        }
        if matches!(o.state, State::Pass | State::Fail) {
            let fail = o.state == State::Fail;
            let note = if contains_mark(&o.observed) {
                Some(EXPRESSION_NOTE)
            } else if fail && tainted(el) {
                Some(TAINT_NOTE)
            } else if fail && info.id == "SRC-CONTROL-NAME" && self.ctx.label_for_expr {
                Some(EXPRESSION_NOTE)
            } else if fail && info.id == "SRC-DUP-ID" && facts.has_conditionals {
                Some(CONDITIONAL_NOTE)
            } else {
                None
            };
            if let Some(note) = note {
                o.state = State::NeedsReview;
                o.limitations.push(note.to_string());
            }
        }
        show_expressions(&mut o.observed);
        o
    }

    /// Records `not_tested` for a document-level check on a template fragment.
    pub fn skip_fragment(&mut self, info: &'static CheckInfo) {
        let reason = "The template is a fragment (no html or body tag), so a document-level check does not apply.";
        let outcome =
            Outcome::new(State::NotTested, info.summary, json!({ "reason": reason })).limit(reason);
        self.emit_root(info, outcome);
    }

    /// Records `not_tested` once when the check produced nothing.
    pub fn ensure_recorded(&mut self, info: &'static CheckInfo) {
        if self.count(info.id) == 0 {
            let outcome = Outcome::new(
                State::NotTested,
                info.summary,
                json!({ "reason": info.absent_reason }),
            )
            .limit(info.absent_reason);
            self.emit_root(info, outcome);
        }
    }

    /// Consumes the emitter and returns its findings in emission order.
    pub fn finish(self) -> Vec<Finding> {
        self.findings
    }
}

fn contains_mark(v: &Value) -> bool {
    match v {
        Value::String(s) => dom::has_mark(s),
        Value::Array(a) => a.iter().any(contains_mark),
        Value::Object(m) => m.values().any(contains_mark),
        _ => false,
    }
}

fn show_expressions(v: &mut Value) {
    match v {
        Value::String(s) if dom::has_mark(s) => *s = s.replace(MARK, "{expr}"),
        Value::Array(a) => a.iter_mut().for_each(show_expressions),
        Value::Object(m) => m.values_mut().for_each(show_expressions),
        _ => {}
    }
}

fn tainted(el: ElementRef<'_>) -> bool {
    dom::has_spread(el)
        || TAINT_ATTRS
            .iter()
            .any(|a| dom::attr_value(el, a) == dom::AttrValue::Expression)
}
