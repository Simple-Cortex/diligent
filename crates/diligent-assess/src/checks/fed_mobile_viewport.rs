//! `BRW-FED-MOBILE-VIEWPORT` (FED-23, human-routed; authored fixtures, no ACT rule).
//!
//! The federal usability framing of a page on a phone. Needs the baseline
//! capture 320 CSS px wide with document metrics.
//!
//! * `pass` when the document does not scroll horizontally (scroll width at
//!   most the client width plus one) and the page has a `meta[name=viewport]`.
//! * `needs_review` when it scrolls horizontally or has no viewport meta,
//!   naming each cause.
//! * `not_tested` when there is no such capture.
//!
//! Never `fail`. `BRW-REFLOW-320` decides WCAG 1.4.10 reflow; this check only
//! reports the usability signals. Does not decide: touch target sizes, text
//! size, or zoom restrictions in the viewport meta content.

use serde_json::json;

use super::fed_common::with_ax;
use super::{finding, Ctx};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FED-MOBILE-VIEWPORT",
    act: &[],
    informs: &["FED-23"],
    summary: "At 320 CSS px the page has a viewport meta and does not scroll sideways",
    channel: "browser",
    never_fails: true,
    absent_reason: "The capture has no baseline snapshot at 320 CSS px wide with document metrics.",
};

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.narrow.as_ref() else {
        return Vec::new();
    };
    let Some(metrics) = v.dom.document_metrics else {
        return Vec::new();
    };
    let overflow = f64::from(metrics.scroll_width) > f64::from(metrics.client_width) + 1.0;
    let viewport_meta = v.elements().find(|e| {
        v.tag(*e) == "meta"
            && v.attr(*e, "name")
                .is_some_and(|n| n.trim().eq_ignore_ascii_case("viewport"))
    });
    let mut causes: Vec<&str> = Vec::new();
    if overflow {
        causes.push("the document scrolls horizontally at 320 CSS px");
    }
    if viewport_meta.is_none() {
        causes.push("the page has no meta viewport");
    }
    let state = if causes.is_empty() {
        State::Pass
    } else {
        State::NeedsReview
    };
    let observed = json!({
        "scroll_width": metrics.scroll_width,
        "client_width": metrics.client_width,
        "viewport_width": v.viewport.width,
        "horizontal_overflow": overflow,
        "viewport_meta": viewport_meta.map(|e| v.attr(e, "content").unwrap_or("").to_string()),
        "causes": causes,
    });
    vec![with_ax(
        finding(
            &INFO,
            v,
            "html".to_string(),
            state,
            "a viewport meta and no horizontal scrolling at 320 CSS px",
            observed,
        ),
        v,
    )
    .limit("One 320 CSS px capture of one page state. Reflow under WCAG 1.4.10 is BRW-REFLOW-320's decision; this reports usability signals only.")]
}
