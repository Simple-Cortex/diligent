//! `BRW-TEXT-CONTRAST` (WCAG 1.4.3; ACT afw4f7).
//!
//! Decides, for each element that directly holds visible text, whether the
//! text colour reaches the AA ratio against the opaque background behind it:
//!
//! * `pass` or `fail` when the foreground and the background resolve;
//! * `needs_review` when the foreground is not a colour this crate parses, or
//!   the background cannot be resolved (a `background-image`, an opacity below
//!   1 on the chain, or an unparseable colour);
//! * `needs_review` for text with no letters or digits (it may be a symbol, not
//!   text) and for text under a `text-shadow`, which can lift contrast and is not
//!   captured;
//! * `not_tested` for text in disabled controls, in the labels of disabled
//!   controls, and in `<option>` elements.
//!
//! Text inside an `aria-hidden="true"` subtree is not examined and produces no
//! row. Does not decide: text over images, gradients, `mix-blend-mode`,
//! `text-shadow` or `-webkit-text-fill-color`; overlap by positioned siblings;
//! text rendered in a canvas or an image.

use std::collections::HashSet;

use serde_json::json;

use super::{clip, finding, hex, round2, Ctx};
use crate::capture_view::{parse_px, CaptureView};
use crate::color::{composite_over, parse_css_color, Color};
use crate::contrast::{classify_text, contrast_ratio, required_ratio, ContrastLevel, TextSize};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-TEXT-CONTRAST",
    act: &["afw4f7"],
    informs: &["A11Y-1.4.3"],
    summary: "Visible text has at least the AA contrast ratio against its background",
    channel: "browser",
    never_fails: false,
    absent_reason: "No visible text in the baseline DOM snapshot.",
};

const LIMIT: &str = "Computed from the colours in the DOM snapshot only: images, gradients, blend modes, text shadows and overlapping elements are not modelled, and the canvas is assumed white when no ancestor sets an opaque background.";

fn font_weight(value: &str) -> Option<f64> {
    match value.trim() {
        "normal" => Some(400.0),
        "bold" | "bolder" => Some(700.0),
        "lighter" => Some(300.0),
        other => other.parse::<f64>().ok(),
    }
}

/// Labels that belong to a disabled control, found once per document.
struct DisabledLabels {
    /// Label elements wrapping, or pointing with `for` at, a disabled control.
    nodes: HashSet<usize>,
    /// Ids named by the `aria-labelledby` of a disabled element.
    ids: HashSet<String>,
}

impl DisabledLabels {
    fn new(v: &CaptureView<'_>) -> Self {
        let mut nodes = HashSet::new();
        let mut ids = HashSet::new();
        let control = |e: usize| {
            v.is_element(e) && matches!(v.tag(e), "input" | "select" | "textarea" | "button")
        };
        for el in v.elements() {
            if v.tag(el) == "label" {
                let by_for = v
                    .attr(el, "for")
                    .is_some_and(|id| v.by_id(id).iter().any(|t| v.is_disabled(*t)));
                let wraps = v
                    .descendants(el)
                    .into_iter()
                    .any(|d| control(d) && v.is_disabled(d));
                if by_for || wraps {
                    nodes.insert(el);
                }
            }
            if let Some(list) = v.attr(el, "aria-labelledby") {
                if v.is_disabled(el) {
                    ids.extend(list.split_ascii_whitespace().map(str::to_string));
                }
            }
        }
        DisabledLabels { nodes, ids }
    }

    fn covers(&self, v: &CaptureView<'_>, p: usize) -> bool {
        v.self_and_ancestors(p).any(|n| {
            self.nodes.contains(&n) || v.attr(n, "id").is_some_and(|id| self.ids.contains(id))
        })
    }
}

fn has_text_shadow(v: &CaptureView<'_>, p: usize) -> bool {
    v.self_and_ancestors(p).any(|n| {
        v.is_element(n)
            && (v
                .attr(n, "style")
                .is_some_and(|s| s.to_ascii_lowercase().contains("text-shadow"))
                || v.style(n, "text-shadow")
                    .is_some_and(|t| t.trim() != "none"))
    })
}

fn evaluate(v: &CaptureView<'_>, p: usize, labels: &DisabledLabels) -> Finding {
    let sample = clip(
        &v.children(p)
            .iter()
            .filter(|c| v.is_text(**c))
            .filter_map(|c| v.node(*c).text.as_deref())
            .collect::<Vec<_>>()
            .join(" ")
            .split_whitespace()
            .collect::<Vec<_>>()
            .join(" "),
        60,
    );
    let make = |state: State, observed: serde_json::Value, expected: &str| {
        finding(&INFO, v, v.selector(p), state, expected, observed).limit(LIMIT)
    };
    let excluded = v
        .self_and_ancestors(p)
        .any(|n| v.is_element(n) && matches!(v.tag(n), "option" | "optgroup"));
    let disabled_label = labels.covers(v, p);
    if v.is_disabled(p) || excluded || disabled_label {
        let reason = if excluded {
            "text in an option element"
        } else if disabled_label {
            "label of a disabled control"
        } else {
            "text in a disabled control"
        };
        return make(
            State::NotTested,
            json!({ "reason": reason, "text": sample }),
            "text contrast does not apply to disabled controls or option text",
        );
    }
    let review = |reason: String| {
        make(
            State::NeedsReview,
            json!({ "reason": reason, "text": sample }),
            "foreground and background colours resolve to opaque values",
        )
    };
    let own_text: String = v
        .children(p)
        .iter()
        .filter(|c| v.is_text(**c))
        .filter_map(|c| v.node(*c).text.as_deref())
        .collect();
    if !own_text.chars().any(char::is_alphanumeric) {
        return review(
            "text has no letters or digits; it may be a symbol rather than text".to_string(),
        );
    }
    if has_text_shadow(v, p) {
        return review("text-shadow present; its effect on contrast is not captured".to_string());
    }
    let Some(fg_text) = v.inherited_style(p, "color") else {
        return review("no computed colour".to_string());
    };
    let Some(fg) = parse_css_color(fg_text) else {
        return review(format!("unparseable colour `{fg_text}`"));
    };
    let bg = match v.effective_background(p) {
        Ok(bg) => bg,
        Err(reason) => return review(format!("background not resolvable: {reason}")),
    };
    let fg_rgb = match fg {
        Color::Opaque(c) => c,
        Color::Translucent(t) => composite_over(t, bg),
    };
    let Some(size_px) = v.inherited_style(p, "font-size").and_then(parse_px) else {
        return review("unparseable font-size".to_string());
    };
    let Some(weight) = v.inherited_style(p, "font-weight").and_then(font_weight) else {
        return review("unparseable font-weight".to_string());
    };
    let size = classify_text(size_px, weight >= 700.0);
    let required = required_ratio(size, ContrastLevel::Aa);
    let ratio = contrast_ratio(fg_rgb, bg);
    let state = if ratio >= required {
        State::Pass
    } else {
        State::Fail
    };
    make(
        state,
        json!({
            "foreground": hex(fg_rgb),
            "background": hex(bg),
            "ratio": round2(ratio),
            "required": required,
            "font_size_px": size_px,
            "font_weight": weight,
            "text_size": if size == TextSize::Large { "large" } else { "normal" },
            "text": sample,
        }),
        &format!("contrast ratio of at least {required}:1"),
    )
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let mut holders: Vec<usize> = Vec::new();
    let mut seen: HashSet<usize> = HashSet::new();
    for t in 0..v.len() {
        if !v.is_text(t)
            || v.node(t)
                .text
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
        {
            continue;
        }
        let Some(p) = v.parent_element(t) else {
            continue;
        };
        if !v.text_visible(t) || v.in_aria_hidden(p) {
            continue;
        }
        if seen.insert(p) {
            holders.push(p);
        }
    }
    let labels = DisabledLabels::new(v);
    holders
        .into_iter()
        .map(|p| evaluate(v, p, &labels))
        .collect()
}
