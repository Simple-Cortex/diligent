//! The browser-capture accessibility checks.
//!
//! Each module decides one narrow question over a [`CaptureView`] and states
//! in its own header what it does not decide. A check returns findings; when
//! it returns none, [`run`] records one `not_tested` finding with the check's
//! `absent_reason`, so every check speaks exactly once per capture at least.

use diligent_capture::{CaptureSet, Viewport};
use serde_json::{json, Value};

use crate::capture_view::{CaptureView, BASELINE};
use crate::contrast::Srgb;
use crate::inventory::InventoryState;
use crate::model::{CheckInfo, EvidenceKey, Finding, State};

pub mod aria_attr_valid;
pub mod aria_hidden_focusable;
pub mod ax_name_required;
pub mod bypass_blocks;
pub mod element_lang_valid;
pub mod fed_accessibility_statement;
pub mod fed_banner;
pub mod fed_common;
pub mod fed_contact;
pub mod fed_dap;
pub mod fed_identifier;
pub mod fed_language_access;
pub mod fed_meta_description;
pub mod fed_mobile_viewport;
pub mod fed_privacy_link;
pub mod fed_search;
pub mod fed_title_criteria;
pub mod focus_cycle;
pub mod focus_obscured;
pub mod focus_visible;
pub mod label_in_name;
pub mod link_same_name;
pub mod non_text_contrast;
pub mod presentational_children;
pub mod reflow_320;
pub mod table_header_assoc;
pub mod target_size;
pub mod text_contrast;
pub mod text_spacing_style;

/// Media mode name of the text-spacing capture.
pub const TEXT_SPACING: &str = "text-spacing";

/// The views a check may read.
pub struct Ctx<'a> {
    /// The capture directory.
    pub set: &'a CaptureSet,
    /// Baseline view at the widest viewport that has a baseline DOM snapshot.
    pub primary: Option<CaptureView<'a>>,
    /// Baseline view at the narrowest viewport of 320 CSS px or less.
    pub narrow: Option<CaptureView<'a>>,
    /// `text-spacing` view, preferring the primary viewport.
    pub spacing: Option<CaptureView<'a>>,
    /// The site inventory `assess --inventory` supplied, read or not.
    pub inventory: &'a InventoryState,
}

static NO_INVENTORY: InventoryState = InventoryState::NotSupplied;

impl<'a> Ctx<'a> {
    /// Selects the views from `set`.
    pub fn new(set: &'a CaptureSet) -> Ctx<'a> {
        Self::with_inventory(set, &NO_INVENTORY)
    }

    /// Selects the views from `set` and keeps `inventory` for the checks that compare across pages.
    pub fn with_inventory(set: &'a CaptureSet, inventory: &'a InventoryState) -> Ctx<'a> {
        let viewports = set.viewports();
        let baseline: Vec<Viewport> = viewports
            .iter()
            .copied()
            .filter(|v| set.dom(*v, BASELINE).is_some())
            .collect();
        let primary_vp = baseline
            .iter()
            .copied()
            .fold(None::<Viewport>, |best, v| match best {
                Some(b) if b.width >= v.width => Some(b),
                _ => Some(v),
            });
        let narrow_vp = baseline.iter().copied().filter(|v| v.width <= 320).fold(
            None::<Viewport>,
            |best, v| match best {
                Some(b) if b.width <= v.width => Some(b),
                _ => Some(v),
            },
        );
        let spacing_vp = primary_vp
            .filter(|v| set.dom(*v, TEXT_SPACING).is_some())
            .or_else(|| {
                viewports
                    .iter()
                    .copied()
                    .find(|v| set.dom(*v, TEXT_SPACING).is_some())
            });
        Ctx {
            set,
            primary: primary_vp.and_then(|v| CaptureView::new(set, v, BASELINE)),
            narrow: narrow_vp.and_then(|v| CaptureView::new(set, v, BASELINE)),
            spacing: spacing_vp.and_then(|v| CaptureView::new(set, v, TEXT_SPACING)),
            inventory,
        }
    }
}

impl Finding {
    /// Adds a limitation.
    pub fn limit(mut self, text: impl Into<String>) -> Self {
        self.limitations.push(text.into());
        self
    }

    /// Adds an evidence key unless it is already present.
    pub fn with(mut self, key: EvidenceKey) -> Self {
        if !self.evidence.contains(&key) {
            self.evidence.push(key);
        }
        self
    }

    /// Names the container element a corroborator would report instead of this instance.
    pub fn scoped_to(mut self, selector: String) -> Self {
        self.corroboration_scope = Some(selector);
        self
    }

    /// [`Finding::scoped_to`] when a container is known.
    pub fn scoped_to_opt(self, selector: Option<String>) -> Self {
        match selector {
            Some(s) => self.scoped_to(s),
            None => self,
        }
    }

    /// Replaces the pack rules the finding informs. The first is primary.
    pub fn rules(mut self, rules: &[&'static str]) -> Self {
        self.wcag = rules.to_vec();
        self
    }
}

/// Starts a finding for `view`, informing the check's rules and resting on its DOM snapshot.
pub fn finding(
    info: &CheckInfo,
    view: &CaptureView<'_>,
    instance: String,
    state: State,
    expected: impl Into<Value>,
    observed: Value,
) -> Finding {
    Finding {
        check_id: info.id,
        state,
        instance,
        expected: expected.into(),
        observed,
        limitations: Vec::new(),
        evidence: vec![view.key("dom_snapshot")],
        wcag: info.informs.to_vec(),
        viewport: Some(view.viewport),
        media: Some(view.media.clone()),
        corroboration: None,
        corroboration_scope: None,
    }
}

/// A page-level `not_tested` finding with a reason.
pub fn not_tested(info: &CheckInfo, view: Option<&CaptureView<'_>>, reason: &str) -> Finding {
    Finding {
        check_id: info.id,
        state: State::NotTested,
        instance: "html".to_string(),
        expected: Value::String(info.summary.to_string()),
        observed: json!({ "reason": reason }),
        limitations: vec![reason.to_string()],
        evidence: view
            .map(|v| vec![v.key("dom_snapshot")])
            .unwrap_or_default(),
        wcag: info.informs.to_vec(),
        viewport: view.map(|v| v.viewport),
        media: view.map(|v| v.media.clone()),
        corroboration: None,
        corroboration_scope: None,
    }
}

/// `#rrggbb` for a colour.
pub fn hex(c: Srgb) -> String {
    format!("#{:02x}{:02x}{:02x}", c.r, c.g, c.b)
}

/// A ratio rounded to two decimals, for rows. Never used to decide a state.
pub fn round2(x: f64) -> f64 {
    (x * 100.0).round() / 100.0
}

/// Shortens text to `max` characters for rows.
pub fn clip(text: &str, max: usize) -> String {
    if text.chars().count() <= max {
        text.to_string()
    } else {
        let mut out: String = text.chars().take(max).collect();
        out.push('…');
        out
    }
}

/// Every check's description, in work-order order.
pub static ALL: [CheckInfo; 28] = [
    text_contrast::INFO,
    focus_visible::INFO,
    focus_cycle::INFO,
    aria_hidden_focusable::INFO,
    label_in_name::INFO,
    aria_attr_valid::INFO,
    presentational_children::INFO,
    bypass_blocks::INFO,
    link_same_name::INFO,
    text_spacing_style::INFO,
    reflow_320::INFO,
    target_size::INFO,
    focus_obscured::INFO,
    non_text_contrast::INFO,
    ax_name_required::INFO,
    element_lang_valid::INFO,
    table_header_assoc::INFO,
    fed_banner::INFO,
    fed_identifier::INFO,
    fed_search::INFO,
    fed_dap::INFO,
    fed_privacy_link::INFO,
    fed_accessibility_statement::INFO,
    fed_title_criteria::INFO,
    fed_meta_description::INFO,
    fed_contact::INFO,
    fed_language_access::INFO,
    fed_mobile_viewport::INFO,
];

/// Every check's description.
pub fn all() -> &'static [CheckInfo] {
    &ALL
}

/// True when the check can never produce `fail`, from [`CheckInfo::never_fails`].
pub fn never_fails(check_id: &str) -> bool {
    ALL.iter().any(|i| i.id == check_id && i.never_fails)
}

/// Reason recorded on a document-level check in component mode.
pub const COMPONENT_REASON: &str = "component mode: whole-page obligation";

/// Checks whose subject is the whole document (landmarks, page chrome, page metadata,
/// page-wide layout or link sets), not an element. Component mode records these as `not_tested`.
pub const DOCUMENT_CHECKS: [&str; 14] = [
    "BRW-BYPASS-BLOCKS",
    "BRW-FED-BANNER",
    "BRW-FED-IDENTIFIER",
    "BRW-FED-TITLE-CRITERIA",
    "BRW-FED-META-DESCRIPTION-CRITERIA",
    "BRW-FED-SEARCH",
    "BRW-FED-PRIVACY-LINK",
    "BRW-FED-ACCESSIBILITY-STATEMENT",
    "BRW-FED-CONTACT",
    "BRW-FED-LANGUAGE-ACCESS",
    "BRW-FED-DAP",
    "BRW-FED-MOBILE-VIEWPORT",
    "BRW-REFLOW-320",
    "BRW-LINK-SAME-NAME",
];

/// `"document"` for a whole-page check, else `"element"`.
pub fn scope(check_id: &str) -> &'static str {
    if DOCUMENT_CHECKS.contains(&check_id) {
        "document"
    } else {
        "element"
    }
}

type Runner = for<'a> fn(&Ctx<'a>) -> Vec<Finding>;

fn runners() -> [Runner; 28] {
    [
        text_contrast::run,
        focus_visible::run,
        focus_cycle::run,
        aria_hidden_focusable::run,
        label_in_name::run,
        aria_attr_valid::run,
        presentational_children::run,
        bypass_blocks::run,
        link_same_name::run,
        text_spacing_style::run,
        reflow_320::run,
        target_size::run,
        focus_obscured::run,
        non_text_contrast::run,
        ax_name_required::run,
        element_lang_valid::run,
        table_header_assoc::run,
        fed_banner::run,
        fed_identifier::run,
        fed_search::run,
        fed_dap::run,
        fed_privacy_link::run,
        fed_accessibility_statement::run,
        fed_title_criteria::run,
        fed_meta_description::run,
        fed_contact::run,
        fed_language_access::run,
        fed_mobile_viewport::run,
    ]
}

/// Runs every check over `set`.
///
/// A check that finds nothing to test records one `not_tested` finding with
/// its `absent_reason`.
pub fn run(set: &CaptureSet) -> Vec<Finding> {
    run_with(set, &NO_INVENTORY)
}

/// [`run`] with the site inventory `assess --inventory` supplied.
pub fn run_with(set: &CaptureSet, inventory: &InventoryState) -> Vec<Finding> {
    run_scoped(set, inventory, false)
}

/// [`run_with`] for a component capture: each document-level check records one `not_tested`
/// finding with [`COMPONENT_REASON`] and runs nothing else.
pub fn run_component(set: &CaptureSet, inventory: &InventoryState) -> Vec<Finding> {
    run_scoped(set, inventory, true)
}

fn run_scoped(set: &CaptureSet, inventory: &InventoryState, component: bool) -> Vec<Finding> {
    let ctx = Ctx::with_inventory(set, inventory);
    let mut out: Vec<Finding> = Vec::new();
    for (info, runner) in ALL.iter().zip(runners()) {
        if component && scope(info.id) == "document" {
            out.push(not_tested(info, ctx.primary.as_ref(), COMPONENT_REASON));
            continue;
        }
        let mut found = runner(&ctx);
        if found.is_empty() {
            found.push(not_tested(info, ctx.primary.as_ref(), info.absent_reason));
        }
        out.extend(found);
    }
    out
}
