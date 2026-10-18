//! Output types: rows of `RESULTS.jsonl` and `EVIDENCE.jsonl`.
//!
//! Field names and required keys follow `schemas/check-result.schema.json` and
//! `schemas/evidence-record.schema.json`. Pass-specific data lives under the
//! namespaced `extensions` key [`EXTENSION_KEY`].

use serde::Serialize;
use serde_json::Value;

/// Engine and collector version, taken from this crate.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Collector name written into every evidence record.
pub const COLLECTOR: &str = "diligent-infra";
/// Key under `extensions` that holds pass-specific fields.
pub const EXTENSION_KEY: &str = "diligent.infra";

/// Outcome of one check. All six schema states are used by this pass.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    /// The condition holds.
    Pass,
    /// The condition is violated.
    Fail,
    /// The facts were captured but a person must decide.
    NeedsReview,
    /// The check was not attempted (for example `--skip-dns`).
    NotTested,
    /// The network did not let the check observe anything (timeout, connection failure).
    Blocked,
    /// The observation was ambiguous in a way the check cannot resolve (for example two SPF records).
    Error,
}

impl Status {
    /// Schema string for this state.
    pub fn as_str(self) -> &'static str {
        match self {
            Status::Pass => "pass",
            Status::Fail => "fail",
            Status::NeedsReview => "needs_review",
            Status::NotTested => "not_tested",
            Status::Blocked => "blocked",
            Status::Error => "error",
        }
    }
}

/// What a check concluded for one rule, before ids and timestamps are attached.
#[derive(Debug, Clone)]
pub struct Outcome {
    /// Stable check id, for example `INFRA-02-hsts`.
    pub check_id: &'static str,
    /// The pack rule this row informs.
    pub rule: &'static str,
    /// `automated` or `assisted`, matching `docs/supported-checks.md`.
    pub channel: &'static str,
    /// Result state.
    pub status: Status,
    /// What the check expected.
    pub expected: Value,
    /// What the check observed.
    pub observed: Value,
    /// What this row cannot establish.
    pub limitations: Vec<String>,
    /// Evidence ids backing the row.
    pub evidence: Vec<String>,
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
    /// Stable identifier derived from the URL (or DNS name) and the payload hash.
    pub evidence_id: String,
    /// Run identifier.
    pub run_id: String,
    /// `http_response`, or `runtime_observation` for DNS.
    pub kind: String,
    /// Always `diligent-infra`.
    pub collector: String,
    /// Crate version.
    pub collector_version: String,
    /// RFC 3339 capture time.
    pub captured_at: String,
    /// Always null.
    pub snapshot_ref: Option<String>,
    /// URL, status, filtered headers, sizes; or the DNS name and answers.
    pub context: Value,
    /// `evidence/<sha256>.<ext>` for HTTP bodies, `dns:TXT:<name>` for DNS.
    pub payload_ref: String,
    /// SHA-256 of the stored payload bytes.
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
    /// Result identifier, unique within the run.
    pub result_id: String,
    /// Check id, for example `INFRA-02-hsts`.
    pub check_id: String,
    /// Crate version.
    pub check_version: String,
    /// `OBL-<rule id>`.
    pub obligation_id: String,
    /// The origin URL the row is about.
    pub instance_id: String,
    /// Same as the obligation id; applicability is resolved elsewhere.
    pub applicability_ref: String,
    /// Result state.
    pub status: Status,
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
    /// Always null on a first run.
    pub supersedes_result_id: Option<String>,
    /// Namespaced pass fields: channel, rule id, scope.
    pub extensions: Value,
}

/// A response body to store under `<out>/evidence/`.
#[derive(Debug, Clone)]
pub struct StoredBody {
    /// Path relative to the output directory, `evidence/<sha256>.<ext>`.
    pub path: String,
    /// The bytes to write.
    pub bytes: Vec<u8>,
}

/// Everything one run produced.
#[derive(Debug, Clone)]
pub struct InfraReport {
    /// Check results, in check order.
    pub rows: Vec<CheckResult>,
    /// Evidence records, in capture order, deduplicated.
    pub evidence: Vec<EvidenceRecord>,
    /// Response bodies for the evidence records.
    pub bodies: Vec<StoredBody>,
}
