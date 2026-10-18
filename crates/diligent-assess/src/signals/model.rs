//! Types for signals: advisory practice readings that are never results.
//!
//! A signal has its own state vocabulary (`lead`, `within_practice`,
//! `informational`, `not_tested`), a threshold that is a stated practice with
//! a citation and a confidence, and a label saying where the number came from
//! (`static` page analysis, `lab` measurement). It never enters
//! `RESULTS.jsonl`, the applicability result or the gate.

use diligent_capture::Viewport;
use serde::Serialize;
use serde_json::Value;

use crate::capture_view::CaptureView;
use crate::model::EvidenceKey;

/// The family a signal belongs to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Family {
    /// Plain language and link text.
    Content,
    /// Typography, spacing, colour margins and motion.
    Craft,
    /// Form patterns.
    FormFlow,
    /// Lab performance measurements.
    LabPerformance,
}

impl Family {
    /// Schema string.
    pub fn as_str(self) -> &'static str {
        match self {
            Family::Content => "content",
            Family::Craft => "craft",
            Family::FormFlow => "form_flow",
            Family::LabPerformance => "lab_performance",
        }
    }

    /// Where the numbers of this family come from: `lab` or `static`.
    pub fn label(self) -> &'static str {
        match self {
            Family::LabPerformance => "lab",
            _ => "static",
        }
    }
}

/// How sure the source of a threshold makes us.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Confidence {
    /// A published standard or technique.
    High,
    /// Published guidance that is common practice, not a target.
    Medium,
    /// A rule of thumb.
    Low,
}

/// A stated practice with its source.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Threshold {
    /// The rule in words, such as `fk_grade <= 8`.
    pub rule: &'static str,
    /// Citation.
    pub source: &'static str,
    /// Confidence in the source.
    pub confidence: Confidence,
}

/// Static description of one signal.
#[derive(Debug)]
pub struct SignalInfo {
    /// Stable key, such as `content.readability.fk_grade`.
    pub key: &'static str,
    /// Family.
    pub family: Family,
    /// One-line statement of what the signal reads.
    pub summary: &'static str,
    /// The practice the state is judged against, when there is one.
    pub threshold: Option<Threshold>,
    /// Why the signal is `not_tested` when its inputs are absent.
    pub absent_reason: &'static str,
}

/// Signal state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum SignalState {
    /// Worth a person's look.
    Lead,
    /// Within the stated practice.
    WithinPractice,
    /// A number with no threshold.
    Informational,
    /// The inputs were absent.
    NotTested,
}

impl SignalState {
    /// Schema string.
    pub fn as_str(self) -> &'static str {
        match self {
            SignalState::Lead => "lead",
            SignalState::WithinPractice => "within_practice",
            SignalState::Informational => "informational",
            SignalState::NotTested => "not_tested",
        }
    }

    /// Parses a schema string.
    pub fn parse(s: &str) -> Option<SignalState> {
        match s {
            "lead" => Some(SignalState::Lead),
            "within_practice" => Some(SignalState::WithinPractice),
            "informational" => Some(SignalState::Informational),
            "not_tested" => Some(SignalState::NotTested),
            _ => None,
        }
    }

    fn rank(self) -> u8 {
        match self {
            SignalState::NotTested => 0,
            SignalState::WithinPractice => 1,
            SignalState::Informational => 2,
            SignalState::Lead => 3,
        }
    }

    /// The more notable of two states: lead, then informational, then within practice, then not tested.
    pub fn worst(self, other: SignalState) -> SignalState {
        if other.rank() > self.rank() {
            other
        } else {
            self
        }
    }
}

/// One reading, before it becomes a row.
#[derive(Debug, Clone)]
pub struct Signal {
    /// Signal key.
    pub key: &'static str,
    /// State.
    pub state: SignalState,
    /// CSS selector, or `html` for page-level readings.
    pub instance: String,
    /// The measured value.
    pub value: Value,
    /// Unit of the value.
    pub unit: Option<&'static str>,
    /// One sentence saying what was seen.
    pub observed: String,
    /// What the reading cannot establish.
    pub limitations: Vec<String>,
    /// Capture files the reading rests on.
    pub evidence: Vec<EvidenceKey>,
    /// Viewport measured at.
    pub viewport: Option<Viewport>,
    /// Media mode measured in.
    pub media: Option<String>,
}

impl Signal {
    /// A page-level reading with no capture attached yet.
    pub fn new(
        key: &'static str,
        state: SignalState,
        value: Value,
        observed: impl Into<String>,
    ) -> Signal {
        Signal {
            key,
            state,
            instance: "html".to_string(),
            value,
            unit: None,
            observed: observed.into(),
            limitations: Vec::new(),
            evidence: Vec::new(),
            viewport: None,
            media: None,
        }
    }

    /// Sets the instance selector.
    pub fn at_instance(mut self, selector: String) -> Signal {
        self.instance = selector;
        self
    }

    /// Sets the unit.
    pub fn unit(mut self, unit: &'static str) -> Signal {
        self.unit = Some(unit);
        self
    }

    /// Adds a limitation.
    pub fn limit(mut self, text: impl Into<String>) -> Signal {
        self.limitations.push(text.into());
        self
    }

    /// Adds an evidence key unless present.
    pub fn with(mut self, key: EvidenceKey) -> Signal {
        if !self.evidence.contains(&key) {
            self.evidence.push(key);
        }
        self
    }

    /// Rests the reading on `v`: its viewport, media, DOM snapshot and, when present, accessibility tree.
    pub fn on(mut self, v: &CaptureView<'_>) -> Signal {
        self.viewport = Some(v.viewport);
        self.media = Some(v.media.clone());
        self = self.with(v.key("dom_snapshot"));
        if v.ax.is_some() {
            self = self.with(v.key("ax_tree"));
        }
        self
    }
}

const fn t(rule: &'static str, source: &'static str, confidence: Confidence) -> Option<Threshold> {
    Some(Threshold {
        rule,
        source,
        confidence,
    })
}

const TYPE_USWDS: &str = "USWDS typography, designsystem.digital.gov/components/typography";
const WEBDEV: &str = "web.dev Core Web Vitals thresholds";

macro_rules! info {
    ($key:expr, $fam:expr, $sum:expr, $thr:expr, $absent:expr) => {
        SignalInfo {
            key: $key,
            family: $fam,
            summary: $sum,
            threshold: $thr,
            absent_reason: $absent,
        }
    };
}

/// Every signal, in report order.
pub static ALL: [SignalInfo; 30] = [
    info!("content.readability.fk_grade", Family::Content, "Flesch-Kincaid grade of the main text",
        t("fk_grade <= 8", "plainlanguage.gov, write for your audience; common practice, not a government target", Confidence::Medium),
        "The main text has fewer than 100 words."),
    info!("content.readability.mean_sentence_length", Family::Content, "Mean sentence length of the main text, in words",
        t("mean sentence length <= 20 words", "plainlanguage.gov, keep sentences short", Confidence::Medium),
        "The main text has fewer than 100 words."),
    info!("content.readability.passive_share", Family::Content, "Share of sentences in the passive voice",
        t("passive share <= 0.20", "plainlanguage.gov, use active voice", Confidence::Medium),
        "The main text has fewer than 100 words."),
    info!("content.acronyms_undefined", Family::Content, "Acronyms used without an expansion",
        t("no undefined acronyms", "plainlanguage.gov, minimize abbreviations", Confidence::High),
        "The page has no main text."),
    info!("content.link_text_generic", Family::Content, "Links named by generic phrases, bare URLs or nothing",
        t("no generic, bare URL or empty link text", "plainlanguage.gov, and WCAG 2.4.4 Understanding", Confidence::High),
        "The page has no visible links."),
    info!("content.heading_density", Family::Content, "Words of main text per heading", None,
        "The main text has no words or no headings."),
    info!("craft.body_font_size_floor", Family::Craft, "Smallest body text size, in CSS px",
        t("smallest body text >= 16 px", TYPE_USWDS, Confidence::Medium),
        "No main text node has 20 characters with a parseable size."),
    info!("craft.line_length_ch", Family::Craft, "Ninetieth percentile line length of paragraphs and list items, in approximate ch",
        t("p90 line length <= 80 ch", "USWDS measure guidance of 45 to 90 characters, designsystem.digital.gov/components/typography", Confidence::Medium),
        "No paragraph or list item has 80 characters."),
    info!("craft.font_family_count", Family::Craft, "Distinct first font families used by visible text",
        t("at most 3 families", "practice", Confidence::Low),
        "The page has no visible text."),
    info!("craft.spacing_grid_share", Family::Craft, "Share of margins and paddings that are multiples of 4 px",
        t("at least 0.80 on the 4 px grid", "USWDS spacing units on a 4 px grid, designsystem.digital.gov/design-tokens/spacing-units", Confidence::Medium),
        "No visible block has a non-zero margin or padding."),
    info!("craft.border_radius_distinct", Family::Craft, "Distinct non-zero border radii",
        t("at most 4 distinct radii", "practice", Confidence::Low),
        "The page has no visible elements."),
    info!("craft.heading_hierarchy_skips", Family::Craft, "Heading level jumps greater than one",
        t("no skipped levels", "WCAG technique G141, as practice", Confidence::High),
        "The page has no visible headings."),
    info!("craft.contrast_margin", Family::Craft, "Smallest margin of a text pair above its AA ratio",
        t("no pair within 10% of its AA ratio", "practice", Confidence::Medium),
        "No visible text pair resolved to colours that reach the AA ratio."),
    info!("craft.reduced_motion_persists", Family::Craft, "Elements that still animate or transition under reduced motion",
        t("none under prefers-reduced-motion", "WCAG 2.3.3, as practice at AA", Confidence::High),
        "The capture has no reduced-motion snapshot at the primary viewport."),
    info!("craft.svg_icons_unnamed", Family::Craft, "Visible inline svg with no name that are not hidden from assistive technology", None,
        "The page has no visible inline svg."),
    info!("form.placeholder_only_labels", Family::FormFlow, "Inputs named only by their placeholder",
        t("no placeholder-only names", "USWDS form guidance and plainlanguage.gov", Confidence::High),
        "The page has no form with an input."),
    info!("form.autocomplete_missing_identity", Family::FormFlow, "Identity inputs without an autocomplete attribute",
        t("identity inputs carry autocomplete", "WCAG 1.3.5, as practice", Confidence::High),
        "The page has no form with an identity input."),
    info!("form.required_marking", Family::FormFlow, "Required inputs with no visible required marking",
        t("required inputs are marked", "USWDS form guidance", Confidence::Medium),
        "The page has no form with a required input."),
    info!("form.error_summary_pattern", Family::FormFlow, "Whether invalid fields have an error summary that links to them",
        t("an error summary links to invalid fields", "GOV.UK and USWDS error summary pattern", Confidence::Medium),
        "The page has no field marked aria-invalid."),
    info!("form.step_indicator", Family::FormFlow, "Whether a long form shows a step indicator",
        t("forms of 8 or more inputs show steps", "GOV.UK question pages and USWDS step indicator", Confidence::Medium),
        "The page has no form with an input."),
    info!("form.memorable_date_inputs", Family::FormFlow, "Whether dates use three separate fields",
        t("three fields for memorable dates", "GOV.UK date input and USWDS memorable date", Confidence::Medium),
        "The page has no date input."),
    info!("form.upload_guidance", Family::FormFlow, "File inputs with nearby size or format guidance",
        t("file inputs say what is accepted", "USWDS file input guidance", Confidence::Medium),
        "The page has no file input."),
    info!("perf.lcp_ms", Family::LabPerformance, "Largest Contentful Paint in a lab load, in ms",
        t("good <= 2500 ms, poor > 4000 ms", WEBDEV, Confidence::High),
        "The capture has no perf_metrics, or no LCP value."),
    info!("perf.cls", Family::LabPerformance, "Cumulative Layout Shift in a lab load",
        t("good <= 0.1, poor > 0.25", WEBDEV, Confidence::High),
        "The capture has no perf_metrics, or no CLS value."),
    info!("perf.fcp_ms", Family::LabPerformance, "First Contentful Paint in a lab load, in ms", None,
        "The capture has no perf_metrics, or no FCP value."),
    info!("perf.ttfb_ms", Family::LabPerformance, "Time to first byte in a lab load, in ms", None,
        "The capture has no perf_metrics, or no TTFB value."),
    info!("perf.transfer_bytes", Family::LabPerformance, "Bytes transferred by the page load", None,
        "The capture has no perf_metrics, or no transfer size."),
    info!("perf.resource_count", Family::LabPerformance, "Resources requested by the page load", None,
        "The capture has no perf_metrics, or no resource count."),
    info!("perf.long_tasks_ms", Family::LabPerformance, "Total long task time in a lab load, in ms", None,
        "The capture has no perf_metrics, or no long task value."),
    info!("perf.inp_ms", Family::LabPerformance, "Interaction to Next Paint", None,
        "INP needs real user interaction; import from CrUX is not implemented."),
];

/// The description of a signal key.
pub fn info(key: &str) -> Option<&'static SignalInfo> {
    ALL.iter().find(|i| i.key == key)
}
