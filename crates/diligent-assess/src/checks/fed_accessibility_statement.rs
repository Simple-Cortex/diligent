//! `BRW-FED-ACCESSIBILITY-STATEMENT` (FED-05, binding; authored fixtures, no ACT rule).
//!
//! The same decision as `BRW-FED-PRIVACY-LINK`, for a link whose accessible
//! name or text mentions "accessibility". Links whose address is only a
//! fragment of the same page do not count, because they cannot reach a
//! statement.
//!
//! * `pass` when a visible link with an absolute `http(s)` address matches;
//!   `location` is `footer` or `body`.
//! * `needs_review` for buttons, anchors without a working `href` and
//!   controls that are not visible, and on pages other than the home page
//!   when nothing matches.
//! * `fail` only when nothing matches and the page is the home page.
//!
//! Does not decide: where the link leads, whether the statement is accessible,
//! or whether it offers a feedback path.

use super::fed_common::{required_link, LinkSpec};
use super::Ctx;
use crate::model::{CheckInfo, Finding};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FED-ACCESSIBILITY-STATEMENT",
    act: &[],
    informs: &["FED-05"],
    summary: "The rendered page has a visible accessibility statement link with a working address",
    channel: "browser",
    never_fails: false,
    absent_reason: "The capture has no baseline snapshot.",
};

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    required_link(
        &LinkSpec {
            info: &INFO,
            word: "accessibility",
            label: "accessibility",
            skip_fragment_only: true,
        },
        v,
    )
}
