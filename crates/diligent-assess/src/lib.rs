//! Deterministic checks and bounded model-task adapters.
//!
//! Audit specification section 9.5 responsibility: Typed model tasks, abstention, calibration and disagreement handling (review-only by default).
//!
//! The pure checks in this crate take values and return typed results. They
//! do no I/O and need no browser. A measurement never returns a bare result
//! state: mapping a measurement to a state is a separate function so a caller
//! can apply exceptions first.
//!
//! * [`contrast`] and [`color`]: WCAG contrast ratio, text size class and CSS color parsing (TC-01).
//! * [`label`]: label-in-name comparison and the label normalization helper (TC-02).
//! * [`content`]: sentences, syllables, readability, passive voice, link text quality and undefined acronyms.
//!
//! The browser-capture checks sit on top of those functions:
//!
//! * [`checks`]: 17 accessibility checks (`BRW-*`) and 11 federal public web checks (`BRW-FED-*`) over a `diligent-rs collect` capture directory.
//! * [`inventory`]: the optional site inventory one federal check reads.
//! * [`capture_view`]: the DOM, accessibility tree and focus walk joined into one read-only view.
//! * [`aria12`] and [`lang_tags`]: embedded WAI-ARIA 1.2 and language subtag tables.
//! * [`corroborate`]: optional axe-core corroboration; a disagreement moves a result to `needs_review`, never to `pass` or `fail`.
//! * [`emit`], [`assess`] and [`cli`]: findings to `RESULTS.jsonl` and `EVIDENCE.jsonl` rows, and `diligent-rs assess`.

pub mod aria12;
pub mod capture_view;
pub mod checks;
pub mod cli;
pub mod clock;
pub mod color;
pub mod content;
pub mod contrast;
pub mod corroborate;
pub mod emit;
pub mod inventory;
pub mod label;
pub mod lang_names;
pub mod lang_tags;
pub mod model;
pub mod signals;

use std::collections::BTreeMap;
use std::fmt;
use std::path::{Path, PathBuf};

use diligent_capture::{CaptureError, CaptureSet};
use serde::Serialize;

pub use cli::cli;
pub use color::{composite_over, parse_css_color, Color, Rgba};
pub use content::{
    acronyms_undefined, link_text_quality, readability, sentences, syllables, words,
    LinkTextQuality, Readability,
};
pub use contrast::{
    classify_text, contrast_ratio, display_ratio, relative_luminance, required_ratio,
    text_contrast_verdict, ContrastLevel, ContrastVerdict, Srgb, TextSize,
};
pub use corroborate::CorroborationSummary;
pub use label::{label_in_name, normalize_label, LabelMatch};
pub use model::{CheckInfo, CheckResult, EvidenceRecord, Finding, State};

/// The responsibility this crate owns in the audit specification's module boundaries.
pub const CRATE_ROLE: &str = "Typed model tasks, abstention, calibration and disagreement handling (review-only by default).";

/// Options for [`assess`].
#[derive(Debug, Clone)]
pub struct AssessOptions {
    /// Run identifier, `run-YYYY-MM-DD-xxxxxx`. A fresh one is made when absent.
    pub run_id: Option<String>,
    /// RFC 3339 execution time. The current time is used when absent.
    pub executed_at: Option<String>,
    /// Path of an `INVENTORY.json`. `BRW-FED-TITLE-CRITERIA` compares the page title with the
    /// titles of the other fetched routes. A file that cannot be read becomes a limitation.
    pub inventory: Option<PathBuf>,
    /// Skip the signals step. Signals are on by default and never change a result.
    pub no_signals: bool,
    /// Run the axe-core corroboration step when the capture has an `axe_result`. Default true.
    /// False skips the step entirely: no corroboration extensions, no axe evidence rows.
    pub corroborate: bool,
}

impl Default for AssessOptions {
    fn default() -> Self {
        AssessOptions {
            run_id: None,
            executed_at: None,
            inventory: None,
            no_signals: false,
            corroborate: true,
        }
    }
}

/// Counts over one assessment.
#[derive(Debug, Clone, Serialize)]
pub struct Summary {
    /// Run identifier.
    pub run_id: String,
    /// Number of result rows.
    pub results: usize,
    /// Number of evidence rows.
    pub evidence: usize,
    /// Rows with state `pass`.
    pub pass: usize,
    /// Rows with state `fail`.
    pub fail: usize,
    /// Rows with state `needs_review`.
    pub needs_review: usize,
    /// Rows with state `not_tested`.
    pub not_tested: usize,
    /// Worst state per check id.
    pub by_check: BTreeMap<String, String>,
    /// Corroborator counts, or null when the capture has no `axe_result`.
    pub corroboration: Option<CorroborationSummary>,
    /// True when `--no-corroborate` skipped the corroboration step.
    pub corroboration_disabled: bool,
    /// Signal counts, or null when `--no-signals` was given.
    pub signals: Option<signals::SignalSummary>,
}

/// Everything [`assess`] produces.
#[derive(Debug, Clone)]
pub struct AssessReport {
    /// Rows for `RESULTS.jsonl`.
    pub results: Vec<CheckResult>,
    /// Rows for `EVIDENCE.jsonl`.
    pub evidence: Vec<EvidenceRecord>,
    /// Counts.
    pub summary: Summary,
    /// Rows for `SIGNALS.jsonl`; empty with `no_signals`.
    pub signals: Vec<signals::SignalRow>,
}

/// Why an assessment could not run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AssessError {
    /// The capture directory could not be loaded.
    Capture(CaptureError),
    /// The run id does not match `run-YYYY-MM-DD-xxxxxx`.
    InvalidRunId(String),
}

impl fmt::Display for AssessError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AssessError::Capture(e) => write!(f, "{e}"),
            AssessError::InvalidRunId(id) => {
                write!(f, "run id `{id}` does not match run-YYYY-MM-DD-xxxxxx")
            }
        }
    }
}

impl std::error::Error for AssessError {}

impl From<CaptureError> for AssessError {
    fn from(e: CaptureError) -> Self {
        AssessError::Capture(e)
    }
}

/// Runs every browser-capture check over a loaded capture set.
pub fn assess_set(set: &CaptureSet, opts: &AssessOptions) -> Result<AssessReport, AssessError> {
    let run_id = match &opts.run_id {
        Some(id) if clock::is_valid_run_id(id) => id.clone(),
        Some(id) => return Err(AssessError::InvalidRunId(id.clone())),
        None => clock::new_run_id(&set.dir().display().to_string()),
    };
    let executed_at = opts.executed_at.clone().unwrap_or_else(clock::now_rfc3339);
    let inventory = match &opts.inventory {
        None => inventory::InventoryState::NotSupplied,
        Some(path) => inventory::load(path),
    };
    let checked = checks::run_with(set, &inventory);
    let (findings, corroboration) = if opts.corroborate {
        corroborate::apply(set, checked)
    } else {
        (checked, None)
    };
    let (results, mut evidence) = emit::rows(set, &findings, &run_id, &executed_at);
    let (signal_rows, signal_summary) = if opts.no_signals {
        (Vec::new(), None)
    } else {
        let found = signals::run(set);
        let summary = signals::summarize(&found);
        (
            signals::emit::rows(set, &found, &run_id, &executed_at, &mut evidence),
            Some(summary),
        )
    };
    let mut by_check: BTreeMap<String, State> = BTreeMap::new();
    let (mut pass, mut fail, mut needs_review, mut not_tested) = (0, 0, 0, 0);
    for f in &findings {
        by_check
            .entry(f.check_id.to_string())
            .and_modify(|s| *s = s.worst(f.state))
            .or_insert(f.state);
        match f.state {
            State::Pass => pass += 1,
            State::Fail => fail += 1,
            State::NeedsReview => needs_review += 1,
            State::NotTested => not_tested += 1,
        }
    }
    let summary = Summary {
        run_id,
        results: results.len(),
        evidence: evidence.len(),
        pass,
        fail,
        needs_review,
        not_tested,
        by_check: by_check
            .into_iter()
            .map(|(k, s)| (k, s.as_str().to_string()))
            .collect(),
        corroboration,
        corroboration_disabled: !opts.corroborate,
        signals: signal_summary,
    };
    Ok(AssessReport {
        results,
        evidence,
        summary,
        signals: signal_rows,
    })
}

/// Loads the capture directory `dir` and runs every browser-capture check over it.
pub fn assess(dir: &Path, opts: &AssessOptions) -> Result<AssessReport, AssessError> {
    let set = CaptureSet::load(dir)?;
    assess_set(&set, opts)
}

#[cfg(test)]
mod tests {
    use super::CRATE_ROLE;
    use super::*;
    use std::path::Path;

    #[test]
    fn crate_role_is_not_empty() {
        assert!(!CRATE_ROLE.is_empty());
    }

    fn hex(c: Srgb) -> String {
        format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
    }

    /// Run one contrast case the way a caller would: parse, composite,
    /// measure, classify, then map to a state.
    fn run_case(
        case: &serde_json::Value,
    ) -> (Option<f64>, Option<TextSize>, Option<String>, &'static str) {
        let fg = parse_css_color(case["fg"].as_str().unwrap());
        let bg = parse_css_color(case["bg"].as_str().unwrap());
        let (Some(fg), Some(Color::Opaque(bg))) = (fg, bg) else {
            return (None, None, None, "needs_review");
        };
        let (fg_rgb, composite) = match fg {
            Color::Opaque(c) => (c, None),
            Color::Translucent(t) => {
                let c = composite_over(t, bg);
                (c, Some(hex(c)))
            }
        };
        let ratio = contrast_ratio(fg_rgb, bg);
        let size = classify_text(
            case["font_px"].as_f64().unwrap(),
            case["bold"].as_bool().unwrap(),
        );
        let level = match case["level"].as_str().unwrap() {
            "AA" => ContrastLevel::Aa,
            "AAA" => ContrastLevel::Aaa,
            other => panic!("unknown level {other}"),
        };
        let state = match text_contrast_verdict(ratio, size, level) {
            ContrastVerdict::Pass => "pass",
            ContrastVerdict::Fail { .. } => "fail",
        };
        (Some(ratio), Some(size), composite, state)
    }

    #[test]
    fn contrast_fixture_cases() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/contrast/cases.json");
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let tolerance = doc["ratio_tolerance"].as_f64().unwrap();
        let cases = doc["cases"].as_array().unwrap();
        assert!(cases.len() >= 24, "fixture needs at least 24 cases");
        for case in cases {
            let id = case["id"].as_str().unwrap();
            let (ratio, size, composite, state) = run_case(case);
            assert_eq!(
                state,
                case["expected_verdict"].as_str().unwrap(),
                "{id}: verdict"
            );
            match case["expected_ratio"].as_f64() {
                Some(want) => {
                    let got = ratio.unwrap_or_else(|| panic!("{id}: no ratio"));
                    assert!(
                        (got - want).abs() <= tolerance,
                        "{id}: ratio {got} vs {want}"
                    );
                }
                None => assert!(ratio.is_none(), "{id}: expected no ratio"),
            }
            match case["expected_size"].as_str() {
                Some("large") => assert_eq!(size, Some(TextSize::Large), "{id}: size"),
                Some("normal") => assert_eq!(size, Some(TextSize::Normal), "{id}: size"),
                _ => assert!(size.is_none(), "{id}: expected no size"),
            }
            assert_eq!(
                composite.as_deref(),
                case["expected_composite"].as_str(),
                "{id}: composite"
            );
        }
    }

    #[test]
    fn contrast_fixture_parse_cases() {
        let path = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/contrast/cases.json");
        let doc: serde_json::Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        for case in doc["parse_cases"].as_array().unwrap() {
            let input = case["input"].as_str().unwrap();
            let parsed = parse_css_color(input);
            match case["kind"].as_str().unwrap() {
                "none" => assert!(parsed.is_none(), "{input:?} should not parse"),
                "opaque" => match parsed {
                    Some(Color::Opaque(c)) => {
                        assert_eq!(hex(c), case["hex"].as_str().unwrap(), "{input:?}")
                    }
                    other => panic!("{input:?}: expected opaque, got {other:?}"),
                },
                "translucent" => match parsed {
                    Some(Color::Translucent(t)) => {
                        assert_eq!(
                            hex(Srgb {
                                r: t.r,
                                g: t.g,
                                b: t.b
                            }),
                            case["hex"].as_str().unwrap(),
                            "{input:?}"
                        );
                        let alpha = case["alpha"].as_f64().unwrap();
                        assert!(
                            (t.a - alpha).abs() < 1e-9,
                            "{input:?}: alpha {} vs {alpha}",
                            t.a
                        );
                    }
                    other => panic!("{input:?}: expected translucent, got {other:?}"),
                },
                other => panic!("unknown kind {other}"),
            }
        }
    }
}
