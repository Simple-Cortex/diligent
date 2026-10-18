//! `BRW-FED-CONTACT` (FED-12; authored fixtures, no ACT rule).
//!
//! Decides whether the rendered page offers a way to reach a person: a visible
//! link or button whose accessible name or text holds the whole word
//! "contact", "help" or "ask". "Get help" matches through "help". Words inside
//! other words, such as "helpful" or "asked", do not match.
//!
//! * `pass` when one exists.
//! * `needs_review` when none does.
//!
//! Never `fail`: the rule is judged by presence only. Does not decide: where
//! the control leads, whether it works, or whether it is enough.

use serde_json::json;

use super::fed_common::{controls, with_ax};
use super::{clip, finding, Ctx};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FED-CONTACT",
    act: &[],
    informs: &["FED-12"],
    summary: "The rendered page has a visible contact or help control",
    channel: "browser",
    never_fails: true,
    absent_reason: "The capture has no baseline snapshot.",
};

fn is_contact_word(text: &str) -> bool {
    text.split(|c: char| !c.is_alphanumeric())
        .map(str::to_lowercase)
        .any(|w| matches!(w.as_str(), "contact" | "help" | "ask"))
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let expected = "a visible link or button named contact, help or ask";
    let hits: Vec<_> = controls(v)
        .into_iter()
        .filter(|c| c.visible && c.matches(is_contact_word))
        .collect();
    let limit = "Presence of a control by its name only; where it leads and whether it works is not decided.";
    let f = match hits.first() {
        Some(c) => finding(
            &INFO,
            v,
            v.selector(c.el),
            State::Pass,
            expected,
            json!({ "name": clip(&c.name, 120), "tag": v.tag(c.el), "matching_controls": hits.len() }),
        ),
        None => finding(
            &INFO,
            v,
            "html".to_string(),
            State::NeedsReview,
            expected,
            json!({ "matching_controls": 0, "reason": "no contact or help control rendered" }),
        ),
    };
    vec![with_ax(f, v).limit(limit)]
}
