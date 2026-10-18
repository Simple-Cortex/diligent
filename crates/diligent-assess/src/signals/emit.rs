//! Turns signals into `SIGNALS.jsonl` rows.
//!
//! A row cites the capture files it rests on by the same `ev-cap-` ids the
//! result rows use. A file that no result row cited yet, such as a
//! `perf_metrics` capture, gets its evidence record appended to the evidence
//! list, so every `evidence_refs` entry resolves in `EVIDENCE.jsonl`.

use std::collections::HashSet;

use diligent_capture::CaptureSet;
use serde::Serialize;
use serde_json::{json, Value};

use super::model::{info, Signal, Threshold};
use crate::clock::sha256_hex;
use crate::emit::{entry_for, evidence_record, viewport_json};
use crate::model::{EvidenceRecord, ENGINE_VERSION};

/// Key under `extensions` that holds signal-specific fields.
pub const EXTENSION_KEY: &str = "diligent.signals";

/// One line of `SIGNALS.jsonl`.
#[derive(Debug, Clone, Serialize)]
pub struct SignalRow {
    /// Always `1.0.0`.
    pub schema_version: String,
    /// `sig-` plus 12 hex digits of sha256(final url NUL run id NUL key NUL instance).
    pub signal_id: String,
    /// Signal key.
    pub signal_key: String,
    /// `content`, `craft`, `form_flow` or `lab_performance`.
    pub family: String,
    /// `<final url>::<selector>`.
    pub instance_id: String,
    /// `lead`, `within_practice`, `informational` or `not_tested`.
    pub state: String,
    /// The measured value.
    pub value: Value,
    /// Unit of the value.
    pub unit: Option<String>,
    /// One sentence saying what was seen.
    pub observed: String,
    /// The practice the state was judged against.
    pub threshold: Option<Threshold>,
    /// `lab`, `field` or `static`.
    pub label: String,
    /// Always false: a signal is advice, not an obligation.
    pub normative: bool,
    /// Evidence ids.
    pub evidence_refs: Vec<String>,
    /// What the reading cannot establish. Never empty.
    pub limitations: Vec<String>,
    /// RFC 3339 time.
    pub executed_at: String,
    /// Crate version.
    pub engine_version: String,
    /// Run identifier.
    pub run_id: String,
    /// Namespaced fields.
    pub extensions: Value,
}

/// Limitation used when a signal states none.
pub const DEFAULT_LIMITATION: &str =
    "Advisory reading of one captured page state; not a legal obligation.";

/// Builds the rows for `signals`, appending evidence records for files not yet in `evidence`.
pub fn rows(
    set: &CaptureSet,
    signals: &[Signal],
    run_id: &str,
    executed_at: &str,
    evidence: &mut Vec<EvidenceRecord>,
) -> Vec<SignalRow> {
    let final_url = set
        .entries()
        .first()
        .map(|e| e.final_url.clone())
        .unwrap_or_default();
    let mut known: HashSet<String> = evidence.iter().map(|e| e.evidence_id.clone()).collect();
    let mut ids: HashSet<String> = HashSet::new();
    let mut out = Vec::with_capacity(signals.len());
    for s in signals {
        let meta = info(s.key);
        let mut refs: Vec<String> = Vec::new();
        for key in &s.evidence {
            let Some(idx) = entry_for(set, key) else {
                continue;
            };
            let entry = &set.entries()[idx];
            let record = evidence_record(set, entry, run_id);
            if known.insert(record.evidence_id.clone()) {
                evidence.push(record.clone());
            }
            if !refs.contains(&record.evidence_id) {
                refs.push(record.evidence_id);
            }
        }
        let mut instance = s.instance.clone();
        let mut hash =
            sha256_hex(format!("{final_url}\0{run_id}\0{}\0{instance}", s.key).as_bytes());
        let mut n = 1;
        while !ids.insert(format!("sig-{}", &hash[..12])) {
            n += 1;
            instance = format!("{}#{n}", s.instance);
            hash = sha256_hex(format!("{final_url}\0{run_id}\0{}\0{instance}", s.key).as_bytes());
        }
        let family = meta.map(|i| i.family);
        out.push(SignalRow {
            schema_version: "1.0.0".to_string(),
            signal_id: format!("sig-{}", &hash[..12]),
            signal_key: s.key.to_string(),
            family: family.map_or("content", |f| f.as_str()).to_string(),
            instance_id: if final_url.is_empty() {
                instance.clone()
            } else {
                format!("{final_url}::{instance}")
            },
            state: s.state.as_str().to_string(),
            value: s.value.clone(),
            unit: s.unit.map(str::to_string),
            observed: s.observed.clone(),
            threshold: meta.and_then(|i| i.threshold),
            label: family.map_or("static", |f| f.label()).to_string(),
            normative: false,
            evidence_refs: refs,
            limitations: if s.limitations.is_empty() {
                vec![DEFAULT_LIMITATION.to_string()]
            } else {
                s.limitations.clone()
            },
            executed_at: executed_at.to_string(),
            engine_version: ENGINE_VERSION.to_string(),
            run_id: run_id.to_string(),
            extensions: json!({
                EXTENSION_KEY: {
                    "viewport": s.viewport.map(viewport_json),
                    "media": s.media,
                    "instance": s.instance,
                }
            }),
        });
    }
    out
}
