//! `BRW-ARIA-HIDDEN-FOCUSABLE` (WCAG 4.1.2; ACT 6cfa84).
//!
//! Decides, for the outermost `aria-hidden="true"` elements and the Tab walk:
//!
//! * `fail` for each Tab stop that is, or sits inside, an `aria-hidden="true"` element;
//! * `pass` for each outermost `aria-hidden="true"` element with no Tab stop inside it.
//!
//! `not_tested` when no element carries `aria-hidden="true"`. Does not decide:
//! elements with `tabindex="-1"`, which never appear in the Tab walk, or
//! content that becomes focusable only after interaction.

use serde_json::json;

use super::{finding, not_tested, Ctx};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-ARIA-HIDDEN-FOCUSABLE",
    act: &["6cfa84"],
    informs: &["A11Y-4.1.2"],
    summary: "No element hidden with aria-hidden=\"true\" can receive keyboard focus",
    channel: "browser",
    never_fails: false,
    absent_reason: "No element has aria-hidden=\"true\".",
};

const LIMIT: &str =
    "Elements with tabindex=\"-1\" do not appear in the Tab walk, so they are not flagged.";

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let hidden = |i: usize| {
        v.attr(i, "aria-hidden")
            .is_some_and(|x| x.trim().eq_ignore_ascii_case("true"))
    };
    let roots: Vec<usize> = v
        .elements()
        .filter(|i| hidden(*i) && !v.ancestors(*i).any(|a| v.is_element(a) && hidden(a)))
        .collect();
    if roots.is_empty() {
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
    let mut occupied: Vec<usize> = Vec::new();
    for (k, stop) in walk.stops.iter().enumerate() {
        let Some(el) = v.dom_index(stop.backend_node_id) else {
            continue;
        };
        if !v.in_aria_hidden(el) {
            continue;
        }
        let root = v
            .self_and_ancestors(el)
            .filter(|n| v.is_element(*n) && hidden(*n))
            .last();
        if let Some(r) = root {
            occupied.push(r);
        }
        out.push(
            finding(
                &INFO,
                v,
                v.selector(el),
                State::Fail,
                "no focusable element inside aria-hidden=\"true\"",
                json!({
                    "stop_index": k,
                    "tag": stop.tag,
                    "aria_hidden_root": root.map(|r| v.selector(r)),
                }),
            )
            .with(v.key("focus_order"))
            .limit(LIMIT)
            .scoped_to_opt(root.map(|r| v.selector(r))),
        );
    }
    for r in roots {
        if occupied.contains(&r) {
            continue;
        }
        out.push(
            finding(
                &INFO,
                v,
                v.selector(r),
                State::Pass,
                "no focusable element inside aria-hidden=\"true\"",
                json!({ "focusable_descendants_in_tab_order": 0 }),
            )
            .with(v.key("focus_order"))
            .limit(LIMIT),
        );
    }
    out
}
