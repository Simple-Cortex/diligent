//! `BRW-AX-NAME-REQUIRED` (WCAG 4.1.2 and 1.1.1; ACT ffd0e9, 59796f, m6b1q3, 8fc3b6).
//!
//! Decides, for headings, menu items (all three roles), `input type="image"`
//! and `object` elements that the accessibility tree keeps, whether the
//! accessible name is non-empty:
//!
//! * `fail` when the name is empty or only whitespace;
//! * `pass` otherwise.
//!
//! Does not decide: whether the name is meaningful, or elements the
//! accessibility tree ignores.

use serde_json::json;

use super::{clip, finding, Ctx};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-AX-NAME-REQUIRED",
    act: &["ffd0e9", "59796f", "m6b1q3", "8fc3b6"],
    informs: &["A11Y-4.1.2", "A11Y-1.1.1"],
    summary: "Headings, menu items, image buttons and embedded objects have an accessible name",
    channel: "browser",
    never_fails: false,
    absent_reason:
        "No heading, menu item, image button or object element in the accessibility tree.",
};

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let Some(tree) = v.ax else { return Vec::new() };
    let mut out = Vec::new();
    let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
    for node in &tree.nodes {
        if node.ignored {
            continue;
        }
        let Some(el) = node.backend_dom_node_id.and_then(|b| v.dom_index(b)) else {
            continue;
        };
        if !v.is_element(el) || seen.contains(&el) {
            continue;
        }
        let role = node.role.as_deref().unwrap_or("").to_ascii_lowercase();
        let tag = v.tag(el);
        let image_button = tag == "input"
            && v.attr(el, "type")
                .is_some_and(|t| t.trim().eq_ignore_ascii_case("image"));
        let (kind, rules): (&str, &[&'static str]) = if role == "heading" {
            ("heading", &["A11Y-4.1.2"])
        } else if matches!(
            role.as_str(),
            "menuitem" | "menuitemcheckbox" | "menuitemradio"
        ) {
            ("menu item", &["A11Y-4.1.2"])
        } else if image_button {
            ("image button", &["A11Y-1.1.1", "A11Y-4.1.2"])
        } else if tag == "object" {
            ("object", &["A11Y-1.1.1", "A11Y-4.1.2"])
        } else {
            continue;
        };
        seen.insert(el);
        // Chrome names an image input without an author name "Submit"; that is not an author name.
        let authored = |attr: &str| v.attr(el, attr).is_some_and(|x| !x.trim().is_empty());
        // aria-labelledby must reference an existing element id to count as authored.
        let labelledby_resolves = v
            .attr(el, "aria-labelledby")
            .is_some_and(|val| val.split_whitespace().any(|id| !v.by_id(id).is_empty()));
        let default_named = image_button
            && !(authored("alt")
                || authored("aria-label")
                || labelledby_resolves
                || authored("title"));
        let name = if default_named {
            ""
        } else {
            node.name.as_deref().unwrap_or("")
        };
        let empty = name.trim().is_empty();
        out.push(
            finding(
                &INFO,
                v,
                v.selector(el),
                if empty { State::Fail } else { State::Pass },
                "a non-empty accessible name",
                json!({ "kind": kind, "role": role, "name": clip(name.trim(), 120) }),
            )
            .with(v.key("ax_tree"))
            .rules(rules)
            .limit("Tests that a name exists, not that it is meaningful."),
        );
    }
    out
}
