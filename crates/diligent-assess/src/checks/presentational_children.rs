//! `BRW-PRESENTATIONAL-CHILDREN` (WCAG 4.1.2; ACT 307n5z).
//!
//! Decides, for roles whose children are presentational (button, checkbox,
//! img, math, menu item checkbox and radio, option, progressbar, radio,
//! scrollbar, separator, slider, switch, tab):
//!
//! * `fail` for each Tab stop that sits inside an element with such a role;
//! * `pass` for each such element (void elements aside) with no Tab stop inside.
//!
//! The role is the first valid `role` token, else the implicit role. Does not
//! decide: descendants that are focusable only with `tabindex="-1"`, which
//! never appear in the Tab walk, or content that appears after interaction.

use std::collections::HashSet;

use serde_json::json;

use super::aria_attr_valid::computed_role;
use super::{finding, Ctx};
use crate::aria12::PRESENTATIONAL_CHILDREN_ROLES;
use crate::capture_view::CaptureView;
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-PRESENTATIONAL-CHILDREN",
    act: &["307n5z"],
    informs: &["A11Y-4.1.2"],
    summary: "Elements whose children are presentational contain no focusable content",
    channel: "browser",
    never_fails: false,
    absent_reason: "No element with a presentational-children role in the baseline capture.",
};

const LIMIT: &str =
    "Descendants with tabindex=\"-1\" do not appear in the Tab walk, so they are not flagged.";

fn has_presentational_children(v: &CaptureView<'_>, el: usize) -> Option<&'static str> {
    computed_role(v, el)
        .map(|r| r.name)
        .filter(|name| PRESENTATIONAL_CHILDREN_ROLES.contains(name))
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let Some(walk) = v.focus else {
        return Vec::new();
    };
    let stops: Vec<(usize, usize)> = walk
        .stops
        .iter()
        .enumerate()
        .filter_map(|(k, s)| v.dom_index(s.backend_node_id).map(|el| (k, el)))
        .collect();
    let mut out = Vec::new();
    let mut flagged: HashSet<usize> = HashSet::new();
    for (k, el) in &stops {
        let owner = v
            .ancestors(*el)
            .filter(|a| v.is_element(*a))
            .find_map(|a| has_presentational_children(v, a).map(|role| (a, role)));
        if let Some((owner, role)) = owner {
            flagged.insert(owner);
            out.push(
                finding(
                    &INFO,
                    v,
                    v.selector(*el),
                    State::Fail,
                    "no focusable element inside an element whose children are presentational",
                    json!({
                        "stop_index": k,
                        "owner": v.selector(owner),
                        "owner_role": role,
                    }),
                )
                .with(v.key("focus_order"))
                .limit(LIMIT)
                .scoped_to(v.selector(owner)),
            );
        }
    }
    for el in v.elements() {
        if flagged.contains(&el) || !v.rendered(el) {
            continue;
        }
        let Some(role) = has_presentational_children(v, el) else {
            continue;
        };
        if matches!(v.tag(el), "img" | "input" | "hr") {
            continue;
        }
        let inner = v.descendants(el);
        if stops.iter().any(|(_, s)| inner.contains(s)) {
            continue;
        }
        out.push(
            finding(
                &INFO,
                v,
                v.selector(el),
                State::Pass,
                "no focusable element inside an element whose children are presentational",
                json!({ "owner_role": role, "focusable_descendants_in_tab_order": 0 }),
            )
            .with(v.key("focus_order"))
            .limit(LIMIT),
        );
    }
    out
}
