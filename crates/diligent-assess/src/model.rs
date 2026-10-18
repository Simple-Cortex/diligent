//! Types shared by the browser-capture checks and the row emitter.
//!
//! [`CheckResult`] and [`EvidenceRecord`] mirror `schemas/check-result.schema.json`
//! and `schemas/evidence-record.schema.json` field for field. They are the same
//! shapes `diligent-infra` and `diligent-source` emit.

use diligent_capture::Viewport;
use serde::Serialize;
use serde_json::Value;

/// Engine version written into every row, taken from this crate.
pub const ENGINE_VERSION: &str = env!("CARGO_PKG_VERSION");
/// Version of the check logic. Bump when a check changes what it decides.
pub const CHECK_VERSION: &str = "0.1.0";
/// Collector name written into evidence rows (the crate that wrote the captures).
pub const COLLECTOR: &str = "diligent-runtime";
/// Key under `extensions` that holds assess-specific fields.
pub const EXTENSION_KEY: &str = "diligent.assess";
/// Limitation attached to a row when a check has nothing more specific to say.
pub const DEFAULT_LIMITATION: &str = "browser capture of one page state; no interaction beyond Tab";

/// Outcome of one check against one target.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum State {
    /// The condition holds. Never a claim that a success criterion is met.
    Pass,
    /// The condition is violated.
    Fail,
    /// The facts were seen but a person must decide.
    NeedsReview,
    /// The preconditions were absent from the capture.
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
    /// Stable check identifier, `BRW-...`.
    pub id: &'static str,
    /// W3C ACT rule ids the check is derived from; empty when none exists.
    pub act: &'static [&'static str],
    /// Rule ids from `rules/packs/a11y-*.json` that this check informs. The first is primary.
    pub informs: &'static [&'static str],
    /// One-line statement of what the check decides.
    pub summary: &'static str,
    /// Always `browser`.
    pub channel: &'static str,
    /// Why the check records `not_tested` when its preconditions are absent.
    pub absent_reason: &'static str,
    /// True when the check can only say `pass`, `needs_review` or `not_tested`, because its rule is
    /// pending or judged by presence. Each module header says why. A test enforces it.
    pub never_fails: bool,
}

/// Names one capture file a finding rests on: kind, viewport and media mode.
///
/// The emitter resolves it to a manifest entry, so a finding never carries a
/// hash itself.
#[derive(Debug, Clone, PartialEq)]
pub struct EvidenceKey {
    /// Manifest kind: `dom_snapshot`, `ax_tree`, `focus_order` or `axe_result`.
    pub kind: &'static str,
    /// Viewport of the capture.
    pub viewport: Viewport,
    /// Media mode, or `None` for kinds the collector records once per viewport.
    pub media: Option<String>,
}

/// What a check concluded about one target, before it becomes rows.
#[derive(Debug, Clone)]
pub struct Finding {
    /// Check identifier.
    pub check_id: &'static str,
    /// Result state.
    pub state: State,
    /// CSS selector of the element, or `html` for page-level results.
    pub instance: String,
    /// What the check expected.
    pub expected: Value,
    /// What the check observed.
    pub observed: Value,
    /// What this result cannot establish. The emitter adds a default when empty.
    pub limitations: Vec<String>,
    /// Capture files the result rests on.
    pub evidence: Vec<EvidenceKey>,
    /// Pack rule ids this result informs. The first is the primary rule.
    pub wcag: Vec<&'static str>,
    /// Viewport the result was measured at, when a capture was available.
    pub viewport: Option<Viewport>,
    /// Media mode the result was measured in, when a capture was available.
    pub media: Option<String>,
    /// What the optional corroborator said about this result, set by the
    /// [`crate::corroborate`] step. `None` when no corroborator ran.
    pub corroboration: Option<Value>,
    /// Selector of a second element the corroborator may report this result
    /// against, when the check names a focusable descendant but the other
    /// engine names its container. Used only by [`crate::corroborate`].
    pub corroboration_scope: Option<String>,
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
    /// `ev-cap-` plus the first 12 hex digits of the capture file hash.
    pub evidence_id: String,
    /// Run identifier.
    pub run_id: String,
    /// `computed_style`, `accessibility_tree` or `runtime_observation`.
    pub kind: String,
    /// Always `diligent-runtime`.
    pub collector: String,
    /// Collector version from the manifest.
    pub collector_version: String,
    /// RFC 3339 capture time from the manifest.
    pub captured_at: String,
    /// Always null.
    pub snapshot_ref: Option<String>,
    /// Viewport, media mode and final URL.
    pub context: Value,
    /// `captures/` plus the capture file name.
    pub payload_ref: String,
    /// SHA-256 of the capture file, from the manifest.
    pub sha256: String,
    /// Always `public`.
    pub data_classification: String,
    /// Redaction state.
    pub redaction: Redaction,
    /// `direct_observation`, or `inference` for the corroborator's `axe_result`.
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
    /// `BRW-...` check identifier.
    pub check_id: String,
    /// Check version.
    pub check_version: String,
    /// `OBL-<rule_id>` of the primary pack rule this result informs.
    pub obligation_id: String,
    /// `<final url>::<selector>`.
    pub instance_id: String,
    /// Same as the obligation id.
    pub applicability_ref: String,
    /// Result state.
    pub status: State,
    /// Evidence ids backing this result.
    pub evidence_refs: Vec<String>,
    /// What the check expected.
    pub expected: Value,
    /// What the check observed.
    pub observed: Value,
    /// What the result cannot establish. Never empty.
    pub limitations: Vec<String>,
    /// RFC 3339 execution time.
    pub executed_at: String,
    /// Crate version.
    pub engine_version: String,
    /// Always null for a first run.
    pub supersedes_result_id: Option<String>,
    /// Namespaced fields: scope, channel, ACT ids, rule ids, viewport, media, instance.
    pub extensions: Value,
}
