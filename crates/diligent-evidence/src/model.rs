//! Records the store writes. Field order and names follow
//! `schemas/evidence-index.schema.json`, `schemas/cache-key.schema.json` and
//! `schemas/reuse-record.schema.json`.

use crate::canonical::canonical_json;
use crate::error::{EvidenceError, Result};
use crate::io::sha256_hex;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;

/// Version written into every record the store emits.
pub const SCHEMA_VERSION: &str = "1.0.0";
/// Version written into `STORE.json`.
pub const STORE_VERSION: &str = "1";

/// Evidence-record `kind` values (`schemas/evidence-record.schema.json`).
pub const EVIDENCE_KINDS: [&str; 10] = [
    "source_span",
    "runtime_observation",
    "accessibility_tree",
    "computed_style",
    "interaction_trace",
    "http_response",
    "document_extract",
    "owner_assertion",
    "human_test",
    "model_assessment",
];

/// Dimensions a cache key may leave empty, each needing a reason.
pub const OPTIONAL_DIMENSIONS: [&str; 4] = ["role", "state", "locale", "deployment"];

/// Which tool produced a piece of evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Producer {
    /// `diligent-rs collect`.
    Collect,
    /// `diligent-rs assess`.
    Assess,
    /// `diligent-rs infra`.
    Infra,
    /// `diligent-rs source scan`.
    Source,
    /// Anything else.
    Other,
}

impl Producer {
    /// Name used in the index and on the command line.
    pub fn as_str(self) -> &'static str {
        match self {
            Producer::Collect => "collect",
            Producer::Assess => "assess",
            Producer::Infra => "infra",
            Producer::Source => "source",
            Producer::Other => "other",
        }
    }

    /// Parses a command-line name.
    pub fn parse(name: &str) -> Option<Producer> {
        [
            Producer::Collect,
            Producer::Assess,
            Producer::Infra,
            Producer::Source,
            Producer::Other,
        ]
        .into_iter()
        .find(|p| p.as_str() == name)
    }
}

/// Redaction state of a payload.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Redaction {
    /// True when the payload was changed to remove sensitive content.
    pub applied: bool,
    /// What was removed, for example `headers_stripped:set-cookie`.
    pub flags: Vec<String>,
}

/// One evidence record, shaped like `schemas/evidence-record.schema.json`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct EvidenceRecord {
    /// Always `1.0.0`.
    pub schema_version: String,
    /// Stable identifier.
    pub evidence_id: String,
    /// Run that produced or ingested the record.
    pub run_id: String,
    /// Evidence kind, one of [`EVIDENCE_KINDS`].
    pub kind: String,
    /// Producing tool.
    pub collector: String,
    /// Producing tool's version.
    pub collector_version: String,
    /// RFC 3339 capture time.
    pub captured_at: String,
    /// Snapshot reference, when there is one.
    pub snapshot_ref: Option<String>,
    /// Free-form context.
    pub context: Value,
    /// Where the payload lives.
    pub payload_ref: String,
    /// Lowercase hex SHA-256 of the payload.
    pub sha256: String,
    /// Data classification.
    pub data_classification: String,
    /// Redaction state.
    pub redaction: Redaction,
    /// How far the observation can be trusted.
    pub trust: String,
    /// What the evidence cannot show.
    pub limitations: Vec<String>,
    /// Parent evidence id for a derivative.
    pub derived_from: Option<String>,
    /// Instant after which the evidence is stale.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub valid_until: Option<String>,
    /// Namespaced extra fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// Index line for one ingested evidence record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IndexRecord {
    /// Always `1.0.0`.
    pub schema_version: String,
    /// The record's `evidence_id`, never renamed by the store.
    pub evidence_id: String,
    /// Lowercase hex SHA-256 of the payload.
    pub sha256: String,
    /// Evidence kind.
    pub kind: String,
    /// Producing tool.
    pub producer: Producer,
    /// Run the record belongs to.
    pub run_id: String,
    /// `payload_ref` exactly as the producer wrote it.
    pub original_payload_ref: String,
    /// `objects/<sha256>.<ext>` when the store holds the payload, else null.
    pub stored: Option<String>,
    /// Payload size when stored, else null.
    pub bytes: Option<u64>,
    /// When the store ingested the record.
    pub ingested_at: String,
    /// The full evidence record as ingested.
    pub record: Value,
    /// Namespaced extra fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// Index line saying a payload was removed. The record line before it keeps
/// provenance.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Tombstone {
    /// Always `1.0.0`.
    pub schema_version: String,
    /// Evidence the payload belonged to.
    pub evidence_id: String,
    /// Hash of the removed payload.
    pub sha256: String,
    /// Why it was removed.
    pub reason: String,
    /// When it was removed.
    pub marked_at: String,
    /// What can no longer be replayed or re-checked because of it.
    pub replay_limitation: String,
    /// Namespaced extra fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// One line of `index.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[allow(clippy::large_enum_variant)]
#[serde(tag = "entry", rename_all = "lowercase")]
pub enum IndexEntry {
    /// An ingested record.
    Record(IndexRecord),
    /// A removed payload.
    Tombstone(Tombstone),
}

/// Commit or deployment a capture was taken from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Deployment {
    /// Source commit.
    pub commit: Option<String>,
    /// Deployment identifier.
    pub deployment_id: Option<String>,
}

/// Viewport dimension of a cache key.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct ViewportKey {
    /// CSS pixel width.
    pub width: u32,
    /// CSS pixel height.
    pub height: u32,
    /// Device pixel ratio.
    pub dpr: f64,
}

/// The evidence-relevant inputs a piece of work depends on (audit specification 9.5).
///
/// Every field is always serialised, nulls included, so the canonical form
/// of two keys is equal exactly when the keys are equal.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CacheKey {
    /// Commit or deployment.
    pub deployment: Deployment,
    /// Route or URL.
    pub route: String,
    /// User role the page was seen as.
    pub role: Option<String>,
    /// Application state.
    pub state: Option<String>,
    /// Viewport.
    pub viewport: Option<ViewportKey>,
    /// Media mode name.
    pub media: Option<String>,
    /// Locale.
    pub locale: Option<String>,
    /// Rule-pack version the work was judged against.
    pub rule_version: Option<String>,
    /// Engine version.
    pub engine_version: String,
    /// Browser-worker protocol version.
    pub protocol_version: Option<String>,
    /// Collector that produced the evidence.
    pub collector: String,
    /// Capture kind.
    pub kind: String,
}

impl CacheKey {
    /// Canonical JSON of the key (see [`canonical_json`]).
    pub fn canonical(&self) -> String {
        canonical_json(&serde_json::to_value(self).unwrap_or(Value::Null))
    }

    /// `sha256(canonical_json(key))` in lowercase hex.
    pub fn hash(&self) -> String {
        sha256_hex(self.canonical().as_bytes())
    }

    /// Dimensions that are empty and need a reason: any of `role`, `state`,
    /// `locale` and `deployment` that carries no value.
    pub fn empty_dimensions(&self) -> Vec<&'static str> {
        let mut out = Vec::new();
        if self.role.is_none() {
            out.push("role");
        }
        if self.state.is_none() {
            out.push("state");
        }
        if self.locale.is_none() {
            out.push("locale");
        }
        if self.deployment.commit.is_none() && self.deployment.deployment_id.is_none() {
            out.push("deployment");
        }
        out
    }
}

/// Whether a cache key's evidence can be trusted for reuse.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum KeyStatus {
    /// Every object the key selects was captured.
    Complete,
    /// Capture stopped early.
    Partial,
    /// Something the key depends on changed, so reuse is off.
    Invalidated,
}

impl KeyStatus {
    /// Name used in the file.
    pub fn as_str(self) -> &'static str {
        match self {
            KeyStatus::Complete => "complete",
            KeyStatus::Partial => "partial",
            KeyStatus::Invalidated => "invalidated",
        }
    }
}

/// One line of `keys.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct KeyRecord {
    /// Always `1.0.0`.
    pub schema_version: String,
    /// `sha256(canonical_json(key))`.
    pub key_hash: String,
    /// The key.
    pub key: CacheKey,
    /// Reason for each empty dimension among role, state, locale, deployment.
    pub missing_reasons: BTreeMap<String, String>,
    /// Status.
    pub status: KeyStatus,
    /// Hashes of the objects the key selects.
    pub sha256s: Vec<String>,
    /// Run that wrote the record.
    pub run_id: String,
    /// When the record was written.
    pub created_at: String,
    /// Namespaced extra fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// One line of `reuse.jsonl`: a run reused earlier evidence on purpose.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ReuseRecord {
    /// Always `1.0.0`.
    pub schema_version: String,
    /// Run that reused the evidence.
    pub run_id: String,
    /// The key that matched.
    pub key_hash: String,
    /// Run that captured the evidence.
    pub reused_from_run_id: String,
    /// Objects reused.
    pub sha256s: Vec<String>,
    /// True when every object was re-hashed before reuse.
    pub verified: bool,
    /// Why reuse was allowed.
    pub reason: String,
    /// When the decision was recorded.
    pub recorded_at: String,
    /// Namespaced extra fields.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// What a resumed run may skip and what it must redo.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ResumePlan {
    /// Keys served from the store; each has a reuse record on disk.
    pub reuse: Vec<ReuseRecord>,
    /// Keys that need fresh work.
    pub redo: Vec<CacheKey>,
    /// Why each redo key was not reused, in the same order as `redo`.
    pub reasons: Vec<(CacheKey, String)>,
}

/// Dimensions and defaults for [`crate::Store::ingest_captures`].
#[derive(Debug, Clone, PartialEq, Default)]
pub struct IngestContext {
    /// Run identifier, `run-YYYY-MM-DD-xxxxxx`.
    pub run_id: String,
    /// Route or URL the captures belong to.
    pub route: String,
    /// Source commit.
    pub commit: Option<String>,
    /// Deployment identifier.
    pub deployment_id: Option<String>,
    /// User role.
    pub role: Option<String>,
    /// Application state.
    pub state: Option<String>,
    /// Locale.
    pub locale: Option<String>,
    /// Rule-pack version.
    pub rule_version: Option<String>,
    /// Engine version; defaults to the manifest's `collector_version`.
    pub engine_version: Option<String>,
    /// Data classification; defaults to `public`.
    pub data_classification: Option<String>,
    /// Reason for each empty dimension (`role`, `state`, `locale`, `deployment`).
    pub missing_reasons: BTreeMap<String, String>,
}

impl IngestContext {
    /// A context with only the run id and route set.
    pub fn new(run_id: impl Into<String>, route: impl Into<String>) -> Self {
        IngestContext {
            run_id: run_id.into(),
            route: route.into(),
            ..IngestContext::default()
        }
    }

    pub(crate) fn validate(&self) -> Result<()> {
        if !is_run_id(&self.run_id) {
            return Err(EvidenceError::Invalid(format!(
                "run id `{}` is not run-YYYY-MM-DD-xxxxxx (six lowercase hex digits)",
                self.run_id
            )));
        }
        if self.route.is_empty() {
            return Err(EvidenceError::Invalid(
                "route must not be empty".to_string(),
            ));
        }
        for dim in self.missing_reasons.keys() {
            if !OPTIONAL_DIMENSIONS.contains(&dim.as_str()) {
                return Err(EvidenceError::Invalid(format!(
                    "`{dim}` is not a dimension that takes a missing reason; use one of role, state, locale, deployment"
                )));
            }
        }
        Ok(())
    }
}

/// True for `run-YYYY-MM-DD-xxxxxx` with six lowercase hex digits.
pub fn is_run_id(id: &str) -> bool {
    let b = id.as_bytes();
    b.len() == 21
        && id.starts_with("run-")
        && b[8] == b'-'
        && b[11] == b'-'
        && b[14] == b'-'
        && b[4..8].iter().all(u8::is_ascii_digit)
        && b[9..11].iter().all(u8::is_ascii_digit)
        && b[12..14].iter().all(u8::is_ascii_digit)
        && b[15..]
            .iter()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(c))
}

/// A payload that could not be stored.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Problem {
    /// Evidence the row names.
    pub evidence_id: String,
    /// `payload_ref` as written.
    pub payload_ref: String,
    /// What went wrong.
    pub reason: String,
}

/// Outcome of an ingest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct IngestReport {
    /// Index lines appended.
    pub records_indexed: usize,
    /// Rows already in the index for the same id, run and hash.
    pub records_skipped: usize,
    /// Objects written to `objects/`.
    pub objects_written: usize,
    /// Objects already present and intact.
    pub objects_reused: usize,
    /// Existing objects that did not re-hash to their name and were replaced.
    pub objects_replaced: Vec<String>,
    /// Cache-key lines appended.
    pub keys_recorded: usize,
    /// Keys already recorded as complete with the same objects.
    pub keys_skipped: usize,
    /// Payloads that were indexed with `stored: null` and why.
    pub payload_problems: Vec<Problem>,
    /// Input rows that could not be indexed at all, with the reason.
    pub rejected: Vec<String>,
}

/// Outcome of [`crate::Store::verify`].
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct VerifyReport {
    /// Files found under `objects/`.
    pub objects: usize,
    /// Objects whose bytes hash to their name.
    pub ok: usize,
    /// Paths (relative to the store) whose bytes do not hash to their name,
    /// that are not regular files, or whose name is not `<sha256>.<ext>`.
    pub mismatched: Vec<String>,
    /// Evidence ids the index says are stored but whose object is absent.
    pub missing: Vec<String>,
    /// Evidence ids with a tombstone.
    pub tombstoned: usize,
    /// Lines of `index.jsonl`, `keys.jsonl` or `reuse.jsonl` that do not
    /// parse or contradict themselves, as `file:line: reason`.
    pub malformed: Vec<String>,
}

impl VerifyReport {
    /// True when nothing is mismatched, missing or malformed.
    pub fn is_clean(&self) -> bool {
        self.mismatched.is_empty() && self.missing.is_empty() && self.malformed.is_empty()
    }
}

/// True for a lowercase 64-digit hex string.
pub fn is_sha256_hex(text: &str) -> bool {
    text.len() == 64
        && text
            .bytes()
            .all(|c| c.is_ascii_digit() || (b'a'..=b'f').contains(&c))
}

/// True when `record` is still fresh at `now` (both RFC 3339).
///
/// A record with no `valid_until` never goes stale. Otherwise it is fresh
/// while `now` is strictly before `valid_until`. An unparseable `now` or
/// `valid_until` is stale: reuse fails closed.
pub fn is_fresh(record: &EvidenceRecord, now: &str) -> bool {
    let Some(limit) = record.valid_until.as_deref() else {
        return true;
    };
    match (
        crate::time::parse_rfc3339(now),
        crate::time::parse_rfc3339(limit),
    ) {
        (Some(now), Some(limit)) => now < limit,
        _ => false,
    }
}
