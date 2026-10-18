//! `BRW-FOCUS-OBSCURED` (WCAG 2.4.11; authored fixtures, no ACT rule).
//!
//! For each Tab stop, compares the box it had when it took focus with the
//! boxes of visible `position: fixed` and `position: sticky` elements that are
//! not the stop's own ancestors and do not sit behind the page (negative
//! `z-index`):
//!
//! * `needs_review` when one covers the focused box entirely;
//! * `needs_review` when one covers part of it, since AA only requires that
//!   the element is not entirely hidden;
//! * `pass` when none overlaps it.
//!
//! `not_tested` when the page has no visible fixed or sticky element. Never
//! `fail` in this version: the focus-time scroll position is not captured, so
//! sticky boxes measured at scroll position zero may not be where they were
//! when the element took focus.

use diligent_capture::BoundingBox;
use serde_json::json;

use super::{finding, not_tested, Ctx};
use crate::capture_view::CaptureView;
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FOCUS-OBSCURED",
    act: &[],
    informs: &["A11Y-2.4.11"],
    summary: "A focused element is not entirely hidden behind fixed or sticky content",
    channel: "browser",
    never_fails: true,
    absent_reason:
        "The page has no visible fixed or sticky element, or the capture has no focus_order file.",
};

const LIMIT: &str = "Fixed and sticky boxes are taken from the DOM snapshot at scroll position zero and compared with the focused box measured at focus time. The scroll position at focus time is not captured, so this check never fails.";

fn overlap(a: &BoundingBox, b: &BoundingBox) -> f64 {
    let w = (a.x + a.width).min(b.x + b.width) - a.x.max(b.x);
    let h = (a.y + a.height).min(b.y + b.height) - a.y.max(b.y);
    if w > 0.0 && h > 0.0 {
        w * h
    } else {
        0.0
    }
}

fn covers(outer: &BoundingBox, inner: &BoundingBox) -> bool {
    outer.x <= inner.x + 0.5
        && outer.y <= inner.y + 0.5
        && outer.x + outer.width + 0.5 >= inner.x + inner.width
        && outer.y + outer.height + 0.5 >= inner.y + inner.height
}

fn overlays(v: &CaptureView<'_>) -> Vec<(usize, BoundingBox)> {
    v.elements()
        .filter(|el| {
            matches!(
                v.style(*el, "position").map(str::trim),
                Some("fixed" | "sticky")
            )
        })
        .filter(|el| v.visible(*el))
        .filter(|el| {
            v.style(*el, "z-index")
                .and_then(|z| z.trim().parse::<i64>().ok())
                .is_none_or(|z| z >= 0)
        })
        .filter_map(|el| v.bbox(el).map(|b| (el, b)))
        .collect()
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let overlays = overlays(v);
    if overlays.is_empty() {
        return Vec::new();
    }
    let Some(walk) = v.focus else {
        return vec![not_tested(
            &INFO,
            Some(v),
            "The capture has no focus_order file.",
        )];
    };
    let mut out = Vec::new();
    for (k, stop) in walk.stops.iter().enumerate() {
        let Some(sb) = stop.bounding_box else {
            continue;
        };
        let el = v.dom_index(stop.backend_node_id);
        let instance = match el {
            Some(e) => v.selector(e),
            None => format!("{}[backend-node-id=\"{}\"]", stop.tag, stop.backend_node_id),
        };
        let mut full: Vec<String> = Vec::new();
        let mut partial: Vec<String> = Vec::new();
        for (o, ob) in &overlays {
            if el.is_some_and(|e| e == *o || v.is_ancestor(*o, e)) {
                continue;
            }
            if overlap(&sb, ob) <= 0.5 {
                continue;
            }
            if covers(ob, &sb) {
                full.push(v.selector(*o));
            } else {
                partial.push(v.selector(*o));
            }
        }
        let (state, note) = if !full.is_empty() {
            (
                State::NeedsReview,
                Some("focused element may be fully obscured"),
            )
        } else if !partial.is_empty() {
            (
                State::NeedsReview,
                Some("partially obscured; AA requires not entirely hidden"),
            )
        } else {
            (State::Pass, None)
        };
        out.push(
            finding(
                &INFO,
                v,
                instance,
                state,
                "no fixed or sticky element entirely covers the focused element",
                json!({
                    "stop_index": k,
                    "focused_box": { "x": sb.x, "y": sb.y, "width": sb.width, "height": sb.height },
                    "fully_covered_by": full,
                    "partially_covered_by": partial,
                    "note": note,
                }),
            )
            .with(v.key("focus_order"))
            .limit(LIMIT),
        );
    }
    out
}
