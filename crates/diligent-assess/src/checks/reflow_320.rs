//! `BRW-REFLOW-320` (WCAG 1.4.10; authored fixtures, no ACT rule).
//!
//! Needs a baseline capture 320 CSS px wide with document metrics.
//!
//! * `pass` when the document does not scroll horizontally;
//! * `needs_review` when it does and every element that sticks out is a data
//!   table with header cells, an `img`, `video`, `canvas` or `pre`, or sits
//!   inside one of those or inside an `overflow-x: auto|scroll` ancestor, which
//!   WCAG allows to need two-dimensional layout; also when no culprit is found;
//! * `fail` when some other element sticks out, naming the culprits.
//!
//! Does not decide: the 256 CSS px height case for horizontal scrolling,
//! content that is clipped rather than scrolled, or whether an excepted
//! element really needs two-dimensional layout.

use serde_json::json;

use super::{finding, Ctx};
use crate::capture_view::CaptureView;
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-REFLOW-320",
    act: &[],
    informs: &["A11Y-1.4.10"],
    summary: "Content reflows at 320 CSS px wide without horizontal scrolling",
    channel: "browser",
    never_fails: false,
    absent_reason: "The capture has no baseline snapshot at 320 CSS px wide with document metrics.",
};

const MAX_LISTED: usize = 10;

fn excusable(v: &CaptureView<'_>, el: usize) -> Option<String> {
    for n in v.self_and_ancestors(el) {
        if !v.is_element(n) {
            continue;
        }
        match v.tag(n) {
            "img" | "video" | "canvas" | "pre" => {
                return Some(format!("inside <{}>", v.tag(n)));
            }
            "table" => {
                let has_headers = v
                    .descendants(n)
                    .into_iter()
                    .any(|d| v.is_element(d) && v.tag(d) == "th");
                if has_headers {
                    return Some("inside a data table with headers".to_string());
                }
            }
            _ => {}
        }
        if n != el
            && matches!(
                v.style(n, "overflow-x").map(str::trim),
                Some("auto" | "scroll")
            )
        {
            return Some("inside an overflow-x scroll container".to_string());
        }
    }
    None
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.narrow.as_ref() else {
        return Vec::new();
    };
    let Some(metrics) = v.dom.document_metrics else {
        return Vec::new();
    };
    let client = f64::from(metrics.client_width);
    let limit = client + 1.0;
    let expected = "the document does not scroll horizontally at 320 CSS px";
    let make = |state: State, observed: serde_json::Value| {
        finding(&INFO, v, "html".to_string(), state, expected, observed)
            .limit("Judged from document scroll width and element boxes at one 320 CSS px capture; the 256 CSS px height case for horizontal scrolling is not checked.")
    };
    if f64::from(metrics.scroll_width) <= limit {
        return vec![make(
            State::Pass,
            json!({
                "scroll_width": metrics.scroll_width,
                "client_width": metrics.client_width,
                "viewport_width": v.viewport.width,
            }),
        )];
    }

    let sticks_out = |el: usize| v.visible(el) && v.bbox(el).is_some_and(|b| b.x + b.width > limit);
    let candidates: Vec<usize> = v
        .elements()
        .filter(|el| !matches!(v.tag(*el), "html" | "body") && sticks_out(*el))
        .collect();
    // An element clipped by an overflow:hidden ancestor that fits does not cause scrolling.
    let clipped = |el: usize| {
        v.ancestors(el).any(|a| {
            v.is_element(a)
                && matches!(
                    v.style(a, "overflow-x").map(str::trim),
                    Some("hidden" | "clip")
                )
                && v.bbox(a).is_some_and(|b| b.x + b.width <= limit)
        })
    };
    // Keep the deepest elements that stick out: those with no child that also sticks out.
    let sticking: std::collections::HashSet<usize> = candidates
        .iter()
        .copied()
        .filter(|el| !clipped(*el))
        .collect();
    let culprits: Vec<usize> = candidates
        .iter()
        .copied()
        .filter(|el| sticking.contains(el))
        .filter(|el| !v.children(*el).iter().any(|c| sticking.contains(c)))
        .collect();
    let described: Vec<serde_json::Value> = culprits
        .iter()
        .take(MAX_LISTED)
        .map(|el| {
            let reason = excusable(v, *el);
            json!({
                "selector": v.selector(*el),
                "tag": v.tag(*el),
                "right_edge": v.bbox(*el).map(|b| (b.x + b.width).round()),
                "exception_candidate": reason,
            })
        })
        .collect();
    let all_excused = !culprits.is_empty() && culprits.iter().all(|el| excusable(v, *el).is_some());
    let observed = json!({
        "scroll_width": metrics.scroll_width,
        "client_width": metrics.client_width,
        "viewport_width": v.viewport.width,
        "culprit_count": culprits.len(),
        "culprits": described,
    });
    if culprits.is_empty() {
        return vec![make(
            State::NeedsReview,
            json!({
                "scroll_width": metrics.scroll_width,
                "client_width": metrics.client_width,
                "note": "the document scrolls horizontally but no element was found sticking out; overflow source not identified",
            }),
        )];
    }
    if all_excused {
        return vec![make(State::NeedsReview, observed)
            .limit("reflow exception candidates: content that may need two-dimensional layout")];
    }
    vec![make(State::Fail, observed)]
}
