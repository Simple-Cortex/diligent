//! `BRW-FOCUS-CYCLE` (WCAG 2.1.2; ACT 80af7b, a1b64e, ebe86a).
//!
//! Decides how the Tab walk ended:
//!
//! * `pass` when Tab itself wrapped to the first stop or left the page content;
//! * `needs_review` when the page moved focus back to an earlier stop (any
//!   index, the first included), or the 200-stop limit was reached;
//! * `not_tested` when nothing was focusable.
//!
//! Never `fail`: a Tab-only walk cannot tell a keyboard trap from an
//! intentional focus group, and cannot try Shift+Tab or Escape.

use serde_json::json;

use super::{finding, Ctx};
use crate::model::{CheckInfo, Finding, State};
use diligent_capture::FocusWalkEnd;

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FOCUS-CYCLE",
    act: &["80af7b", "a1b64e", "ebe86a"],
    informs: &["A11Y-2.1.2"],
    summary: "Focus can move away from every element with the keyboard",
    channel: "browser",
    never_fails: true,
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
    let stops = walk.stops.len();
    let (state, how, note) = match walk.terminated_by {
        FocusWalkEnd::NoFocusable => return Vec::new(),
        FocusWalkEnd::ReturnedToFirst => (State::Pass, "returned_to_first", None),
        FocusWalkEnd::ReturnedToBody => (State::Pass, "returned_to_body", None),
        FocusWalkEnd::RepeatOf(i) => (
            State::NeedsReview,
            "repeat_of_earlier_stop",
            Some(format!(
                "page moved focus back to stop {i} after Tab; a trap or an intentional focus group; verify with Shift+Tab and Escape"
            )),
        ),
        FocusWalkEnd::StopLimit => (
            State::NeedsReview,
            "stop_limit",
            Some("200 stops; the walk ended at the limit without returning".to_string()),
        ),
    };
    let observed = json!({
        "terminated_by": how,
        "repeat_of": match walk.terminated_by { FocusWalkEnd::RepeatOf(i) => json!(i), _ => json!(null) },
        "stops": stops,
        "note": note,
    });
    vec![finding(
        &INFO,
        v,
        "html".to_string(),
        state,
        "Tab leaves the page content or returns to the first stop",
        observed,
    )
    .with(v.key("focus_order"))
    .limit("A Tab-only walk cannot prove a keyboard trap; it cannot try Shift+Tab, Escape or arrow keys.")]
}
