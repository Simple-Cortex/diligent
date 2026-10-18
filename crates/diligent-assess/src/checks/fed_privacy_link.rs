//! `BRW-FED-PRIVACY-LINK` (FED-06, binding; authored fixtures, no ACT rule).
//!
//! Decides whether the rendered page has a visible link whose accessible name
//! or text mentions "privacy" and whose address resolves to an absolute
//! `http(s)` URL.
//!
//! * `pass` when one exists. `location` is `footer` when it sits in the
//!   identifier, a `footer` or the `contentinfo` landmark, else `body`. The
//!   resolved address is recorded.
//! * `needs_review` when the only matches are buttons, anchors without a
//!   working `href`, or controls that are in the markup but not visible; and
//!   on pages other than the home page when nothing matches.
//! * `fail` only when nothing matches and the page is the home page.
//!
//! Does not decide: where the link leads, whether the page it reaches is a
//! privacy policy, or collection-context notices. The server-HTML check
//! `FED-06-privacy-link` in `diligent-infra` covers pages as served.

use super::fed_common::{required_link, LinkSpec};
use super::Ctx;
use crate::model::{CheckInfo, Finding};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FED-PRIVACY-LINK",
    act: &[],
    informs: &["FED-06"],
    summary: "The rendered page has a visible privacy link with a working address",
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
            word: "privacy",
            label: "privacy",
            skip_fragment_only: false,
        },
        v,
    )
}
