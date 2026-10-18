//! Turns findings into `RESULTS.jsonl` and `EVIDENCE.jsonl` rows.
//!
//! One evidence row per capture file a finding rests on, one result row per
//! finding. Row shapes follow `schemas/check-result.schema.json` and
//! `schemas/evidence-record.schema.json`.

use diligent_capture::{CaptureSet, ManifestEntry, Viewport};
use serde_json::{json, Value};

use crate::checks;
use crate::clock::sha256_hex;
use crate::corroborate;
use crate::model::{
    CheckResult, EvidenceKey, EvidenceRecord, Finding, Redaction, CHECK_VERSION, COLLECTOR,
    DEFAULT_LIMITATION, ENGINE_VERSION, EXTENSION_KEY,
};

pub(crate) fn same_viewport(a: Viewport, b: Viewport) -> bool {
    a.width == b.width && a.height == b.height && (a.dpr - b.dpr).abs() < 1e-6
}

pub(crate) fn viewport_json(v: Viewport) -> Value {
    json!({ "width": v.width, "height": v.height, "dpr": v.dpr })
}

/// Index of the manifest entry an evidence key names.
pub(crate) fn entry_for(set: &CaptureSet, key: &EvidenceKey) -> Option<usize> {
    set.entries().iter().position(|e| {
        e.kind == key.kind
            && same_viewport(e.viewport, key.viewport)
            && key.media.as_deref().is_none_or(|m| e.media == m)
    })
}

fn evidence_kind(manifest_kind: &str) -> &'static str {
    match manifest_kind {
        "ax_tree" => "accessibility_tree",
        "focus_order" | "screenshot" | "network_log" | "axe_result" | "perf_metrics" => {
            "runtime_observation"
        }
        _ => "computed_style",
    }
}

fn evidence_limitation(manifest_kind: &str) -> &'static str {
    match manifest_kind {
        "ax_tree" => "Accessibility tree as Chrome reported it for one page state at one viewport; no interaction.",
        "axe_result" => "Third-party corroborator output for one page state at one viewport, top frame only; an inference that never replaces a deterministic result.",
        "perf_metrics" => "Lab measurement of one unthrottled headless page load at one viewport; not a field measurement, and timing varies between runs.",
        "network_log" => "Requests the browser made while loading one page at one viewport; scripts blocked by the collector's origin policy and requests made after load are not logged.",
        "focus_order" => "Tab-only walk of one page state, limited in length; Shift+Tab, Escape and arrow keys were not tried.",
        _ => "Computed styles and layout boxes of one page state at one viewport; pseudo-elements and interaction states are not captured.",
    }
}

pub(crate) fn evidence_record(
    set: &CaptureSet,
    entry: &ManifestEntry,
    run_id: &str,
) -> EvidenceRecord {
    let corroborator = (entry.kind == "axe_result").then(|| {
        let version = set
            .axe(entry.viewport, &entry.media)
            .and_then(|a| a.version.clone());
        json!({ "engine": corroborate::ENGINE, "version": version })
    });
    let mut context = json!({
        "viewport": viewport_json(entry.viewport),
        "media": entry.media,
        "final_url": entry.final_url,
    });
    if let Some(c) = corroborator {
        context["corroborator"] = c;
    }
    if entry.kind == "perf_metrics" {
        context["label"] = json!("lab");
    }
    EvidenceRecord {
        schema_version: "1.0.0".to_string(),
        evidence_id: format!("ev-cap-{}", &entry.sha256[..entry.sha256.len().min(12)]),
        run_id: run_id.to_string(),
        kind: evidence_kind(&entry.kind).to_string(),
        collector: COLLECTOR.to_string(),
        collector_version: if entry.collector_version.is_empty() {
            "unknown".to_string()
        } else {
            entry.collector_version.clone()
        },
        captured_at: entry.captured_at.clone(),
        snapshot_ref: None,
        context,
        payload_ref: format!("captures/{}", entry.path),
        sha256: entry.sha256.clone(),
        data_classification: "public".to_string(),
        redaction: Redaction {
            applied: false,
            flags: Vec::new(),
        },
        trust: if entry.kind == "axe_result" {
            "inference"
        } else {
            "direct_observation"
        }
        .to_string(),
        limitations: vec![evidence_limitation(&entry.kind).to_string()],
        derived_from: None,
    }
}

/// Builds the result and evidence rows for `findings`.
///
/// Evidence rows come out in order of first use, one per distinct capture
/// file. Result ids are `res-<10 hex of sha256(final_url NUL run_id)>-<NNNN>`.
pub fn rows(
    set: &CaptureSet,
    findings: &[Finding],
    run_id: &str,
    executed_at: &str,
) -> (Vec<CheckResult>, Vec<EvidenceRecord>) {
    let final_url = set
        .entries()
        .first()
        .map(|e| e.final_url.clone())
        .unwrap_or_default();
    let prefix = sha256_hex(format!("{final_url}\0{run_id}").as_bytes());
    let prefix = &prefix[..10];

    let mut evidence: Vec<EvidenceRecord> = Vec::new();
    let mut used: Vec<usize> = Vec::new();
    let mut results: Vec<CheckResult> = Vec::with_capacity(findings.len());

    for (n, f) in findings.iter().enumerate() {
        let mut refs: Vec<String> = Vec::new();
        for key in &f.evidence {
            let Some(idx) = entry_for(set, key) else {
                continue;
            };
            let entry = &set.entries()[idx];
            let id = format!("ev-cap-{}", &entry.sha256[..entry.sha256.len().min(12)]);
            if !used.contains(&idx) {
                used.push(idx);
                evidence.push(evidence_record(set, entry, run_id));
            }
            if !refs.contains(&id) {
                refs.push(id);
            }
        }
        let info = checks::all().iter().find(|i| i.id == f.check_id);
        let primary = f.wcag.first().copied();
        let obligation_id = match primary {
            Some(rule) => format!("OBL-{rule}"),
            None => format!("OBL-UNMAPPED-{}", f.check_id),
        };
        let limitations = if f.limitations.is_empty() {
            vec![DEFAULT_LIMITATION.to_string()]
        } else {
            f.limitations.clone()
        };
        let instance_id = if final_url.is_empty() {
            f.instance.clone()
        } else {
            format!("{final_url}::{}", f.instance)
        };
        results.push(CheckResult {
            schema_version: "1.0.0".to_string(),
            result_id: format!("res-{prefix}-{:04}", n + 1),
            check_id: f.check_id.to_string(),
            check_version: CHECK_VERSION.to_string(),
            obligation_id: obligation_id.clone(),
            instance_id,
            applicability_ref: obligation_id,
            status: f.state,
            evidence_refs: refs,
            expected: f.expected.clone(),
            observed: f.observed.clone(),
            limitations,
            executed_at: executed_at.to_string(),
            engine_version: ENGINE_VERSION.to_string(),
            supersedes_result_id: None,
            extensions: {
                let mut ext = serde_json::Map::new();
                ext.insert(
                    EXTENSION_KEY.to_string(),
                    json!({
                        "scope": "rendered",
                        "channel": "browser",
                        "act_rule_ids": info.map(|i| i.act).unwrap_or(&[]),
                        "rule_ids": f.wcag,
                        "viewport": f.viewport.map(viewport_json),
                        "media": f.media,
                        "instance": f.instance,
                    }),
                );
                if let Some(c) = &f.corroboration {
                    ext.insert(corroborate::EXTENSION_KEY.to_string(), c.clone());
                }
                Value::Object(ext)
            },
        });
    }
    (results, evidence)
}
