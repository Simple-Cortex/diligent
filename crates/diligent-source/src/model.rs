//! Output types: evidence records, check results, and the bundle that joins them.
//!
//! Field names and required keys follow `schemas/evidence-record.schema.json`
//! and `schemas/check-result.schema.json`. Pass-specific data that the schemas
//! do not name lives under the namespaced `extensions` object.

use serde::Serialize;
use serde_json::Value;

use crate::time;

/// Engine and collector version, taken from this crate.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Collector name written into every evidence record.
pub const COLLECTOR: &str = "diligent-source";
/// Key under `extensions` that holds pass-specific fields.
pub const EXTENSION_KEY: &str = "diligent.source";

/// Outcome of one check against one target.
///
/// The static pass emits only these four states. `blocked` and `error` exist
/// in the schema but are produced by other components.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// The static condition holds. Never a claim that a success criterion passes.
    Pass,
    /// The static condition is violated.
    Fail,
    /// The pass saw the facts but a person or a rendered pass must decide.
    NeedsReview,
    /// The preconditions were absent from the document, so nothing was tested.
    NotTested,
}

impl State {
    /// Schema string for this state.
    pub fn as_str(self) -> &'static str {
        match self {
            State::Pass => "pass",
            State::Fail => "fail",
            State::NeedsReview => "needs_review",
            State::NotTested => "not_tested",
        }
    }

    /// Parses a schema string.
    pub fn parse(s: &str) -> Option<State> {
        match s {
            "pass" => Some(State::Pass),
            "fail" => Some(State::Fail),
            "needs_review" => Some(State::NeedsReview),
            "not_tested" => Some(State::NotTested),
            _ => None,
        }
    }

    /// Roll-up rank: a document-level aggregate takes the highest rank.
    fn rank(self) -> u8 {
        match self {
            State::NotTested => 0,
            State::Pass => 1,
            State::NeedsReview => 2,
            State::Fail => 3,
        }
    }

    /// The worse of two states: fail, then needs_review, then pass, then not_tested.
    pub fn worst(self, other: State) -> State {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }
}

/// Static description of one check.
#[derive(Debug)]
pub struct CheckInfo {
    /// Stable check identifier, `SRC-...`.
    pub id: &'static str,
    /// W3C ACT rule ids the check is derived from; empty when none exists.
    pub act: &'static [&'static str],
    /// Rule ids from `rules/packs/*.json` that this check informs. The first is primary.
    pub informs: &'static [&'static str],
    /// One-line statement of what the check decides.
    pub summary: &'static str,
    /// Why the check records `not_tested` when nothing applicable is found.
    pub absent_reason: &'static str,
}

/// Where a document came from and which run it belongs to.
#[derive(Debug, Clone)]
pub struct SourceRef {
    /// Path written into evidence (`payload_ref`) and result instance identifiers.
    pub path: String,
    /// Run identifier, `run-YYYY-MM-DD-xxxxxx`.
    pub run_id: String,
    /// RFC 3339 capture time, also used as `executed_at`.
    pub captured_at: String,
}

impl SourceRef {
    /// A source reference with a fresh run id and the current time.
    pub fn new(path: impl Into<String>) -> Self {
        let path = path.into();
        let run_id = time::new_run_id(&path);
        SourceRef {
            path,
            run_id,
            captured_at: time::now_rfc3339(),
        }
    }

    /// Replaces the run id.
    pub fn with_run_id(mut self, run_id: impl Into<String>) -> Self {
        self.run_id = run_id.into();
        self
    }

    /// Replaces the capture time.
    pub fn with_captured_at(mut self, captured_at: impl Into<String>) -> Self {
        self.captured_at = captured_at.into();
        self
    }
}

/// Redaction block of an evidence record.
#[derive(Debug, Clone, Serialize)]
pub struct Redaction {
    /// Whether any redaction was applied.
    pub applied: bool,
    /// Redaction flags.
    pub flags: Vec<String>,
}

/// One line of `EVIDENCE.jsonl`.
#[derive(Debug, Clone, Serialize)]
pub struct EvidenceRecord {
    /// Always `1.0.0`.
    pub schema_version: String,
    /// Stable identifier derived from document, run-independent selector.
    pub evidence_id: String,
    /// Run identifier.
    pub run_id: String,
    /// Always `source_span`.
    pub kind: String,
    /// Always `diligent-source`.
    pub collector: String,
    /// Crate version.
    pub collector_version: String,
    /// RFC 3339 capture time.
    pub captured_at: String,
    /// Always null for the static pass.
    pub snapshot_ref: Option<String>,
    /// Selector, snippet, attributes, source path, byte offset, line and column (null when unknown).
    pub context: Value,
    /// Source path of the scanned document.
    pub payload_ref: String,
    /// SHA-256 of the exact bytes scanned.
    pub sha256: String,
    /// Data classification label.
    pub data_classification: String,
    /// Redaction state.
    pub redaction: Redaction,
    /// Always `direct_observation`.
    pub trust: String,
    /// What this evidence cannot show.
    pub limitations: Vec<String>,
    /// Always null.
    pub derived_from: Option<String>,
}

/// One line of `RESULTS.jsonl`.
#[derive(Debug, Clone, Serialize)]
pub struct CheckResult {
    /// Always `1.0.0`.
    pub schema_version: String,
    /// Result identifier, unique within the scan.
    pub result_id: String,
    /// `SRC-...` check identifier.
    pub check_id: String,
    /// Check version, the crate version.
    pub check_version: String,
    /// `OBL-<rule_id>` of the primary pack rule this result informs.
    pub obligation_id: String,
    /// `<source path>::<selector>`.
    pub instance_id: String,
    /// Same as the obligation id; the applicability record is resolved elsewhere.
    pub applicability_ref: String,
    /// Result state.
    pub status: State,
    /// Evidence ids backing this result.
    pub evidence_refs: Vec<String>,
    /// What the check expected.
    pub expected: Value,
    /// What the check observed.
    pub observed: Value,
    /// What the result cannot establish.
    pub limitations: Vec<String>,
    /// RFC 3339 execution time.
    pub executed_at: String,
    /// Crate version.
    pub engine_version: String,
    /// Always null for the first run.
    pub supersedes_result_id: Option<String>,
    /// Namespaced pass fields: ACT ids, rule ids, scope, selector.
    pub extensions: Value,
}

/// One evidence record and the check result that references it.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    /// What was observed.
    pub evidence: EvidenceRecord,
    /// What the check concluded.
    pub result: CheckResult,
}

#[cfg(test)]
mod tests {
    use super::State;

    #[test]
    fn worst_orders_states() {
        assert_eq!(State::Pass.worst(State::Fail), State::Fail);
        assert_eq!(State::NeedsReview.worst(State::Pass), State::NeedsReview);
        assert_eq!(State::NotTested.worst(State::Pass), State::Pass);
        assert_eq!(State::Fail.worst(State::NeedsReview), State::Fail);
    }

    #[test]
    fn parse_round_trips() {
        for s in [
            State::Pass,
            State::Fail,
            State::NeedsReview,
            State::NotTested,
        ] {
            assert_eq!(State::parse(s.as_str()), Some(s));
        }
        assert_eq!(State::parse("passed"), None);
    }
}
