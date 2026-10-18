//! `BRW-NON-TEXT-CONTRAST` (WCAG 1.4.11; authored fixtures, no ACT rule).
//!
//! Decides, for each visible form control, whether its boundary reaches 3:1
//! against what is behind it. The boundary is either of:
//!
//! * the border: the border side with the highest ratio among sides that have
//!   a width and a style, against the opaque background behind the control;
//! * the fill: the control's own `background-color` against the same background.
//!
//! Controls are `input` (except hidden, submit, button, reset and image),
//! `select`, `textarea`, and elements with role textbox, combobox, checkbox,
//! radio or switch.
//!
//! * `pass` when either ratio is at least 3;
//! * `fail` when neither is, every colour involved resolved, and the control
//!   has no `box-shadow` and is not a native checkbox, radio, range, colour or
//!   file input;
//! * `needs_review` when a colour cannot be resolved, a `box-shadow` may draw
//!   the boundary, the control's appearance is the browser's, or it has
//!   opacity below 1;
//! * `not_tested` for disabled controls.
//!
//! Does not decide: focus, hover and checked states, state indicators inside
//! the control, or controls identified by something other than a boundary,
//! such as an adjacent visible label.

use serde_json::json;

use super::{finding, hex, round2, Ctx};
use crate::capture_view::{parse_px, CaptureView};
use crate::color::{composite_over, parse_css_color, Color};
use crate::contrast::{contrast_ratio, Srgb};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-NON-TEXT-CONTRAST",
    act: &[],
    informs: &["A11Y-1.4.11"],
    summary: "Form control boundaries have at least 3:1 contrast against their surroundings",
    channel: "browser",
    never_fails: false,
    absent_reason: "No visible form control in the baseline capture.",
};

const THRESHOLD: f64 = 3.0;
const LIMIT: &str = "Uses the border or the fill of the control against the opaque colour behind it, from computed styles. Shadows, outlines, images, state changes and overlapping elements are not modelled.";

fn is_control(v: &CaptureView<'_>, el: usize) -> bool {
    match v.tag(el) {
        "input" => !matches!(
            v.attr(el, "type")
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "hidden" | "submit" | "button" | "image" | "reset"
        ),
        "select" | "textarea" => true,
        _ => {
            let by_role =
                |r: &str| matches!(r, "textbox" | "combobox" | "checkbox" | "radio" | "switch");
            v.attr(el, "role")
                .and_then(|r| r.split_ascii_whitespace().next())
                .is_some_and(|r| by_role(&r.to_ascii_lowercase()))
        }
    }
}

fn ua_drawn(v: &CaptureView<'_>, el: usize) -> bool {
    v.tag(el) == "input"
        && matches!(
            v.attr(el, "type")
                .unwrap_or("")
                .trim()
                .to_ascii_lowercase()
                .as_str(),
            "checkbox" | "radio" | "range" | "color" | "file"
        )
}

enum Edge {
    /// A ratio and the colour that produced it.
    Ratio(f64, Srgb),
    /// No border side has a width and a style.
    Absent,
    /// A colour could not be parsed.
    Unresolved(String),
}

fn border_edge(v: &CaptureView<'_>, el: usize, behind: Srgb) -> Edge {
    let mut best: Option<(f64, Srgb)> = None;
    let mut unresolved: Option<String> = None;
    for side in ["top", "right", "bottom", "left"] {
        let width = v
            .style(el, &format!("border-{side}-width"))
            .and_then(parse_px)
            .unwrap_or(0.0);
        let style = v
            .style(el, &format!("border-{side}-style"))
            .map(|s| s.trim().to_string())
            .unwrap_or_default();
        if width <= 0.0 || style == "none" || style == "hidden" {
            continue;
        }
        let Some(raw) = v.style(el, &format!("border-{side}-color")) else {
            unresolved = Some(format!("no border-{side}-color"));
            continue;
        };
        match parse_css_color(raw) {
            None => unresolved = Some(format!("unparseable border-{side}-color `{raw}`")),
            Some(color) => {
                let rgb = match color {
                    Color::Opaque(c) => c,
                    Color::Translucent(t) => composite_over(t, behind),
                };
                let ratio = contrast_ratio(rgb, behind);
                if best.is_none_or(|(r, _)| ratio > r) {
                    best = Some((ratio, rgb));
                }
            }
        }
    }
    match best {
        Some((r, c)) if r >= THRESHOLD => Edge::Ratio(r, c),
        _ => match unresolved {
            Some(reason) => Edge::Unresolved(reason),
            None => best.map_or(Edge::Absent, |(r, c)| Edge::Ratio(r, c)),
        },
    }
}

fn fill_edge(v: &CaptureView<'_>, el: usize, behind: Srgb) -> Edge {
    if let Some(image) = v.style(el, "background-image") {
        if image.trim() != "none" && !image.trim().is_empty() {
            return Edge::Unresolved("background-image on the control".to_string());
        }
    }
    let Some(raw) = v.style(el, "background-color") else {
        return Edge::Unresolved("no background-color".to_string());
    };
    match parse_css_color(raw) {
        None => Edge::Unresolved(format!("unparseable background-color `{raw}`")),
        Some(color) => {
            let rgb = match color {
                Color::Opaque(c) => c,
                Color::Translucent(t) => composite_over(t, behind),
            };
            Edge::Ratio(contrast_ratio(rgb, behind), rgb)
        }
    }
}

fn describe(edge: &Edge) -> serde_json::Value {
    match edge {
        Edge::Ratio(r, c) => json!({ "ratio": round2(*r), "colour": hex(*c) }),
        Edge::Absent => json!({ "ratio": null, "reason": "no border" }),
        Edge::Unresolved(reason) => json!({ "ratio": null, "reason": reason }),
    }
}

fn evaluate(v: &CaptureView<'_>, el: usize) -> Finding {
    let expected = "the control's border or fill has at least 3:1 contrast with its surroundings";
    let make = |state: State, observed: serde_json::Value| {
        finding(&INFO, v, v.selector(el), state, expected, observed).limit(LIMIT)
    };
    if v.is_disabled(el) {
        return make(State::NotTested, json!({ "reason": "disabled control" }));
    }
    if let Some(reason) = v.opacity_issue(el) {
        return make(State::NeedsReview, json!({ "reason": reason }));
    }
    let behind = match v.parent_element(el).map(|p| v.effective_background(p)) {
        Some(Ok(c)) => c,
        Some(Err(reason)) => {
            return make(
                State::NeedsReview,
                json!({ "reason": format!("background behind the control not resolvable: {reason}") }),
            )
        }
        None => Srgb {
            r: 255,
            g: 255,
            b: 255,
        },
    };
    let border = border_edge(v, el, behind);
    let fill = fill_edge(v, el, behind);
    let observed = json!({
        "behind": hex(behind),
        "border": describe(&border),
        "fill": describe(&fill),
    });
    let passes = |e: &Edge| matches!(e, Edge::Ratio(r, _) if *r >= THRESHOLD);
    if passes(&border) || passes(&fill) {
        return make(State::Pass, observed);
    }
    let unresolved = matches!(border, Edge::Unresolved(_)) || matches!(fill, Edge::Unresolved(_));
    let shadow = v
        .style(el, "box-shadow")
        .is_some_and(|s| s.trim() != "none" && !s.trim().is_empty());
    let (state, note) = if unresolved {
        (State::NeedsReview, "a colour could not be resolved")
    } else if shadow {
        (State::NeedsReview, "box-shadow present; not evaluated")
    } else if ua_drawn(v, el) {
        (
            State::NeedsReview,
            "the browser draws this control; its appearance is not captured",
        )
    } else {
        (State::Fail, "")
    };
    let mut observed = observed;
    if !note.is_empty() {
        observed["note"] = json!(note);
    }
    make(state, observed)
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    v.elements()
        .filter(|el| is_control(v, *el) && v.visible(*el))
        .map(|el| evaluate(v, el))
        .collect()
}
