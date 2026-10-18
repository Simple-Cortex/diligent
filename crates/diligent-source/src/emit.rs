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

const SNIPPET_CHARS: usize = 400;
const EVIDENCE_LIMITATION: &str =
    "Static parse of the HTML text only: no rendering, computed styles, scripts, or line and byte offsets.";
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
        let selector = self.ctx.selector(el);
        let (snippet, truncated) = dom::outer_html_snippet(el, SNIPPET_CHARS);
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
                "attributes": dom::attributes_json(el),
                "byte_offset": Value::Null,
            }),
            payload_ref: self.source.path.clone(),
            sha256: self.doc_sha.clone(),
            data_classification: "source_code".to_string(),
            redaction: Redaction {
                applied: false,
                flags: Vec::new(),
            },
            trust: "direct_observation".to_string(),
            limitations: vec![EVIDENCE_LIMITATION.to_string()],
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
