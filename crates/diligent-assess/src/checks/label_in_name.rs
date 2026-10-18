//! `BRW-LABEL-IN-NAME` (WCAG 2.5.3; ACT 2ee8b8).
//!
//! Decides, for each interactive element whose role takes its name from its
//! content (button, link, checkbox, radio, menu items, switch, tab), whether
//! the visible text appears in the accessible name, using
//! [`crate::label::label_in_name`]:
//!
//! * `pass` for an exact match or a name that starts with the visible text;
//! * `fail` when the visible text is absent from the name;
//! * `needs_review` when the visible text is only contained inside the name,
//!   or the comparison is ambiguous (symbols, mixed scripts);
//! * `not_tested` for disabled controls and controls with an image but no text.
//!
//! Does not decide: text boxes, selects and sliders, whose labels are not their
//! content; text added by CSS generated content; the name of the control as a
//! speech user would say it.

use serde_json::json;

use super::{clip, finding, Ctx};
use crate::label::{label_in_name, LabelMatch};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-LABEL-IN-NAME",
    act: &["2ee8b8"],
    informs: &["A11Y-2.5.3"],
    summary: "The visible label of a control is part of its accessible name",
    channel: "browser",
    never_fails: false,
    absent_reason: "No interactive element with visible text in the baseline capture.",
};

const NAME_FROM_CONTENT: [&str; 9] = [
    "button",
    "link",
    "checkbox",
    "radio",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "switch",
    "tab",
];

const LIMIT: &str = "Compares the visible text nodes with the accessible name from the accessibility tree. Text from CSS generated content and images is not visible to the comparison.";

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    if v.ax.is_none() {
        return Vec::new();
    }
    let mut out = Vec::new();
    for el in v.interactive() {
        let Some(role) = v.ax_role(el) else { continue };
        if !NAME_FROM_CONTENT.contains(&role.as_str()) {
            continue;
        }
        let name = v.ax_name(el).unwrap_or("").to_string();
        let mut visible = v.visible_text(el);
        if visible.is_empty() && v.tag(el) == "input" {
            let kind = v.attr(el, "type").unwrap_or("").to_ascii_lowercase();
            if matches!(kind.as_str(), "button" | "submit" | "reset") {
                visible = v.attr(el, "value").unwrap_or("").trim().to_string();
            }
        }
        let make = |state: State, expected: &str, observed: serde_json::Value| {
            finding(&INFO, v, v.selector(el), state, expected, observed)
                .with(v.key("ax_tree"))
                .limit(LIMIT)
        };
        if v.is_disabled(el) {
            out.push(make(
                State::NotTested,
                "visible label is part of the accessible name",
                json!({ "reason": "disabled control", "role": role }),
            ));
            continue;
        }
        if visible.is_empty() {
            let has_image = v.descendants(el).into_iter().any(|d| {
                v.is_element(d) && matches!(v.tag(d), "img" | "svg" | "canvas") && v.visible(d)
            }) || (v.tag(el) == "input"
                && v.attr(el, "type")
                    .is_some_and(|t| t.eq_ignore_ascii_case("image")));
            if has_image {
                out.push(make(
                    State::NotTested,
                    "visible label is part of the accessible name",
                    json!({ "reason": "image-only control, no visible text", "role": role, "name": clip(&name, 80) }),
                ));
            }
            continue;
        }
        let verdict = if visible.chars().count() == 1 {
            LabelMatch::Ambiguous(
                "single-character label; it may be a symbol such as X for close".to_string(),
            )
        } else {
            label_in_name(&visible, &name)
        };
        let (state, how, why) = match &verdict {
            LabelMatch::Match => (State::Pass, "match", None),
            LabelMatch::Prefix => (State::Pass, "prefix", None),
            LabelMatch::NoMatch => (State::Fail, "no_match", None),
            LabelMatch::Contained => (
                State::NeedsReview,
                "contained",
                Some("the visible text appears inside the name but not at its start".to_string()),
            ),
            LabelMatch::Ambiguous(reason) => {
                (State::NeedsReview, "ambiguous", Some(reason.clone()))
            }
        };
        out.push(make(
            state,
            "the accessible name contains the visible label",
            json!({
                "role": role,
                "visible_text": clip(&visible, 120),
                "accessible_name": clip(&name, 120),
                "comparison": how,
                "note": why,
            }),
        ));
    }
    out
}
