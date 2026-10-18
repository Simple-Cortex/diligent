//! `BRW-FED-META-DESCRIPTION-CRITERIA` (FED-34, pending GSA standard; authored fixtures, no ACT rule).
//!
//! Measures `meta[name=description]` in the rendered page against the pending
//! acceptance criterion of at least 50 characters.
//!
//! * `pass` when exactly one such element exists and its trimmed `content` has
//!   50 characters or more.
//! * `needs_review` when it is shorter, when it is absent, and when more than
//!   one is present.
//!
//! Never `fail`: the standard is pending. Does not decide: whether the
//! description is unique across pages (the inventory does not carry
//! descriptions), accurate, or useful in search results.

use serde_json::json;

use super::fed_common::with_ax;
use super::{clip, finding, Ctx};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FED-META-DESCRIPTION-CRITERIA",
    act: &[],
    informs: &["FED-34"],
    summary: "The page has one meta description of at least 50 characters",
    channel: "browser",
    never_fails: true,
    absent_reason: "The capture has no baseline snapshot.",
};

const MIN_CHARS: usize = 50;

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let expected = "exactly one meta description of at least 50 characters";
    let metas: Vec<usize> = v
        .elements()
        .filter(|e| {
            v.tag(*e) == "meta"
                && v.attr(*e, "name")
                    .is_some_and(|n| n.trim().eq_ignore_ascii_case("description"))
        })
        .collect();
    let content = |e: usize| {
        v.attr(e, "content")
            .map(str::trim)
            .unwrap_or("")
            .to_string()
    };
    let (state, instance, observed) = match metas.as_slice() {
        [] => (
            State::NeedsReview,
            "html".to_string(),
            json!({ "count": 0, "reason": "no meta description" }),
        ),
        [one] => {
            let text = content(*one);
            let length = text.chars().count();
            let state = if length >= MIN_CHARS {
                State::Pass
            } else {
                State::NeedsReview
            };
            let reason = (length < MIN_CHARS)
                .then(|| format!("below the pending {MIN_CHARS}-character minimum"));
            (
                state,
                v.selector(*one),
                json!({ "count": 1, "length": length, "content": clip(&text, 200), "reason": reason }),
            )
        }
        many => (
            State::NeedsReview,
            v.selector(many[0]),
            json!({
                "count": many.len(),
                "lengths": many.iter().map(|e| content(*e).chars().count()).collect::<Vec<_>>(),
                "reason": "more than one meta description",
            }),
        ),
    };
    vec![with_ax(finding(&INFO, v, instance, state, expected, observed), v)
        .limit("Pending GSA standard: a pass means the pending acceptance criteria held. Uniqueness across pages is not evaluated: the inventory does not carry descriptions.")]
}
