//! `BRW-TARGET-SIZE` (WCAG 2.5.8; authored fixtures, no ACT rule).
//!
//! Decides, for each interactive element with a layout box in the baseline
//! capture (1280 CSS px wide by default):
//!
//! * `pass` when the box is at least 24 by 24 CSS px;
//! * `pass` when it is smaller but a 24 CSS px diameter circle centred on its
//!   box touches no other interactive element's box (the spacing exception);
//! * `fail` when it is smaller and the circle touches another target. A target
//!   nested inside another target always touches it, so an undersized nested
//!   target never gets the spacing exception;
//! * `needs_review` instead of `fail` when another passing control with the
//!   same link target or form action exists, or the element is a native
//!   checkbox or radio whose size may be the browser default;
//! * `not_tested` for links inside running text (the inline exception),
//!   disabled controls, and elements 1 CSS px or less in a dimension.
//!
//! Does not decide: targets that are equivalent by purpose rather than by
//! link target, targets whose size is essential, or how user-agent controls
//! are drawn.

use serde_json::json;

use super::{finding, Ctx};
use crate::capture_view::CaptureView;
use crate::model::{CheckInfo, Finding, State};
use diligent_capture::BoundingBox;

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-TARGET-SIZE",
    act: &[],
    informs: &["A11Y-2.5.8"],
    summary: "Pointer targets are at least 24 by 24 CSS px, or have enough space around them",
    channel: "browser",
    never_fails: false,
    absent_reason: "No interactive element with a layout box in the baseline capture.",
};

const MIN: f64 = 24.0;
const TOLERANCE: f64 = 0.05;
const RADIUS: f64 = 12.0;

const LIMIT: &str = "Sizes come from layout boxes at one viewport. Equivalent controls are matched by link target or form action only. The 24 px circle is tested against other targets' boxes, not against their circles.";

fn big_enough(b: &BoundingBox) -> bool {
    b.width + TOLERANCE >= MIN && b.height + TOLERANCE >= MIN
}

/// True when a circle of `RADIUS` centred at the middle of `b` overlaps `other`.
fn circle_touches(b: &BoundingBox, other: &BoundingBox) -> bool {
    let cx = b.x + b.width / 2.0;
    let cy = b.y + b.height / 2.0;
    let dx = (other.x - cx).max(0.0).max(cx - (other.x + other.width));
    let dy = (other.y - cy).max(0.0).max(cy - (other.y + other.height));
    dx * dx + dy * dy < RADIUS * RADIUS
}

/// The nearest ancestor that is a block container.
fn text_container(v: &CaptureView<'_>, el: usize) -> Option<usize> {
    v.ancestors(el).filter(|a| v.is_element(*a)).find(|a| {
        !matches!(
            v.style(*a, "display").map(str::trim),
            Some("inline" | "inline-block" | "contents") | None
        )
    })
}

fn in_running_text(v: &CaptureView<'_>, el: usize) -> bool {
    if v.ax_role(el).as_deref() != Some("link") {
        return false;
    }
    let Some(container) = text_container(v, el) else {
        return false;
    };
    v.visible_text_nodes(container)
        .into_iter()
        .any(|t| !v.is_ancestor(el, t))
}

fn equivalence_key(v: &CaptureView<'_>, el: usize) -> Option<String> {
    if v.tag(el) == "a" {
        if let Some(href) = v.attr(el, "href") {
            return Some(format!("href:{}", v.resolve_url(href)));
        }
    }
    v.self_and_ancestors(el)
        .find(|a| v.is_element(*a) && v.tag(*a) == "form")
        .and_then(|f| {
            v.attr(f, "action")
                .map(|a| format!("action:{}", v.resolve_url(a)))
        })
}

fn native_ua_control(v: &CaptureView<'_>, el: usize) -> bool {
    v.tag(el) == "input"
        && matches!(
            v.attr(el, "type").map(str::to_ascii_lowercase).as_deref(),
            Some("checkbox" | "radio")
        )
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    if v.ax.is_none() {
        return Vec::new();
    }
    let targets: Vec<(usize, BoundingBox)> = v
        .interactive()
        .into_iter()
        .filter_map(|el| v.bbox(el).map(|b| (el, b)))
        .filter(|(_, b)| b.width > 0.0 && b.height > 0.0)
        .collect();
    let mut out = Vec::new();
    for (el, b) in &targets {
        let make = |state: State, expected: &str, observed: serde_json::Value| {
            finding(&INFO, v, v.selector(*el), state, expected, observed)
                .with(v.key("ax_tree"))
                .limit(LIMIT)
        };
        let size = json!({ "width": (b.width * 100.0).round() / 100.0, "height": (b.height * 100.0).round() / 100.0 });
        let expected = "at least 24 by 24 CSS px, or no other target within a 24 CSS px circle";
        if v.is_disabled(*el) {
            out.push(make(
                State::NotTested,
                expected,
                json!({ "reason": "disabled control", "size": size }),
            ));
            continue;
        }
        if b.width <= 1.0 || b.height <= 1.0 {
            out.push(make(
                State::NotTested,
                expected,
                json!({ "reason": "1 CSS px or less in a dimension; visually hidden", "size": size }),
            ));
            continue;
        }
        if in_running_text(v, *el) {
            out.push(make(
                State::NotTested,
                expected,
                json!({ "reason": "inline link in running text", "size": size }),
            ));
            continue;
        }
        if big_enough(b) {
            out.push(make(State::Pass, expected, json!({ "size": size })));
            continue;
        }
        let touching: Vec<usize> = targets
            .iter()
            .filter(|(o, ob)| *o != *el && circle_touches(b, ob))
            .map(|(o, _)| *o)
            .collect();
        if touching.is_empty() {
            out.push(make(
                State::Pass,
                expected,
                json!({ "size": size, "exception": "spacing: no other target within 24 CSS px of the centre circle" }),
            ));
            continue;
        }
        let key = equivalence_key(v, *el);
        let equivalent = key.as_ref().and_then(|k| {
            targets
                .iter()
                .find(|(o, ob)| {
                    *o != *el && big_enough(ob) && equivalence_key(v, *o).as_ref() == Some(k)
                })
                .map(|(o, _)| *o)
        });
        let mut observed = json!({
            "size": size,
            "touching": touching.iter().take(5).map(|t| v.selector(*t)).collect::<Vec<_>>(),
        });
        if let Some(outer) = touching.iter().find(|t| v.is_ancestor(**t, *el)) {
            observed["note"] = json!(format!(
                "nested inside another target: {}",
                v.selector(*outer)
            ));
        }
        if let Some(o) = equivalent {
            out.push(make(
                State::NeedsReview,
                expected,
                json!({ "size": size, "touching": observed["touching"], "note": "equivalent control may exist", "equivalent": v.selector(o) }),
            ));
        } else if native_ua_control(v, *el) {
            out.push(make(
                State::NeedsReview,
                expected,
                json!({ "size": size, "touching": observed["touching"], "note": "native checkbox or radio; the size may be the browser default, which WCAG excepts" }),
            ));
        } else {
            out.push(make(State::Fail, expected, observed));
        }
    }
    out
}
