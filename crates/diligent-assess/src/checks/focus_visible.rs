//! `BRW-FOCUS-VISIBLE` (WCAG 2.4.7; ACT oj04fd).
//!
//! Decides, for each stop of the Tab walk, whether focusing the element
//! changed how it paints:
//!
//! * `fail` when the collector measured no change in outline, shadow, border
//!   colour, background, colour or text decoration;
//! * `pass` when it measured a change;
//! * `needs_review` when the collector could not measure.
//!
//! Does not decide: whether a change is large or contrasting enough to see,
//! indicators drawn by an ancestor or a pseudo-element, or elements that only
//! take focus by script or by `tabindex="-1"`.

use serde_json::json;

use super::{finding, Ctx};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FOCUS-VISIBLE",
    act: &["oj04fd"],
    informs: &["A11Y-2.4.7"],
    summary: "Each element reached with Tab shows a visible focus indicator",
    channel: "browser",
    never_fails: false,
    absent_reason:
        "The focus walk found no focusable element, or the capture has no focus_order file.",
};

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let Some(walk) = v.focus else {
        return Vec::new();
    };
    let mut out = Vec::new();
    for (k, stop) in walk.stops.iter().enumerate() {
        let instance = match v.dom_index(stop.backend_node_id) {
            Some(el) => v.selector(el),
            None => format!("{}[backend-node-id=\"{}\"]", stop.tag, stop.backend_node_id),
        };
        let (state, note) = match stop.had_visible_indicator {
            Some(true) => (State::Pass, None),
            Some(false) => (State::Fail, None),
            None => (State::NeedsReview, Some("indicator diff unavailable")),
        };
        let observed = json!({
            "stop_index": k,
            "tag": stop.tag,
            "name": stop.name,
            "had_visible_indicator": stop.had_visible_indicator,
            "note": note,
        });
        out.push(
            finding(
                &INFO,
                v,
                instance,
                state,
                "focusing the element changes its outline, shadow, border, background, colour or text decoration",
                observed,
            )
            .with(v.key("focus_order"))
            .limit("The collector compares computed styles before and after focus. A change is not proof that the indicator is large or contrasting enough to see.")
            .limit("Indicators drawn by an ancestor or a pseudo-element are not detected."),
        );
    }
    out
}
