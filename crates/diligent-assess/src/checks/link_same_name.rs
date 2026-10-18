//! `BRW-LINK-SAME-NAME` (WCAG 2.4.4; ACT fd3a94, b20e66).
//!
//! Groups the page's links by accessible name and decides, per group of two or
//! more:
//!
//! * `pass` when every link in the group resolves to the same URL;
//! * `needs_review` when the group points at different URLs, since only a
//!   person can say whether the purposes are equivalent.
//!
//! Never `fail`. Links with a unique name produce no row. Does not decide
//! whether a link's purpose is clear from its context.

use std::collections::BTreeMap;

use serde_json::json;

use super::{clip, finding, Ctx};
use crate::capture_view::normalize_space;
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-LINK-SAME-NAME",
    act: &["fd3a94", "b20e66"],
    informs: &["A11Y-2.4.4"],
    summary: "Links with the same accessible name lead to the same place",
    channel: "browser",
    never_fails: true,
    absent_reason: "No two links share an accessible name.",
};

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    if v.ax.is_none() {
        return Vec::new();
    }
    let mut groups: BTreeMap<String, Vec<usize>> = BTreeMap::new();
    for el in v.interactive() {
        if v.ax_role(el).as_deref() != Some("link") {
            continue;
        }
        let name = normalize_space(v.ax_name(el).unwrap_or("")).to_lowercase();
        if name.is_empty() {
            continue;
        }
        groups.entry(name).or_default().push(el);
    }
    let mut out = Vec::new();
    for (name, links) in groups {
        if links.len() < 2 {
            continue;
        }
        let mut targets: Vec<String> = links
            .iter()
            .map(|l| {
                v.attr(*l, "href")
                    .map(|h| v.resolve_url(h))
                    .unwrap_or_default()
            })
            .collect();
        targets.sort();
        targets.dedup();
        let same = targets.len() == 1;
        let instances: Vec<String> = links.iter().map(|l| v.selector(*l)).collect();
        out.push(
            finding(
                &INFO,
                v,
                instances[0].clone(),
                if same { State::Pass } else { State::NeedsReview },
                "links with the same name lead to the same URL",
                json!({
                    "name": clip(&name, 80),
                    "count": links.len(),
                    "targets": targets.iter().map(|t| clip(t, 200)).collect::<Vec<_>>(),
                    "links": instances.iter().take(10).collect::<Vec<_>>(),
                    "note": if same { json!(null) } else { json!("same name, different targets; confirm equivalent purpose") },
                }),
            )
            .with(v.key("ax_tree"))
            .limit("Names are compared after lowercasing and collapsing whitespace. Whether differing targets serve an equivalent purpose needs a person."),
        );
    }
    out
}
