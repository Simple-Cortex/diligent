//! `BRW-FED-SEARCH` (FED-17, binding; authored fixtures, no ACT rule).
//!
//! Decides whether the rendered page offers a site search control a person
//! can find and name:
//!
//! * a visible node the accessibility tree calls `search` or `searchbox`, or
//!   a visible `input[type=search]`, or a visible `form` that holds a text
//!   input whose name, placeholder or `aria-label` mentions "search";
//! * with a non-empty accessible name on the input.
//!
//! * `pass` when such a control has an accessible name.
//! * `needs_review` when a control is present but unnamed, when a search
//!   control is in the markup but not visible (a closed dialog or menu), and
//!   when nothing is found on a page other than the home page.
//! * `fail` only when nothing is found and the page is the home page.
//!
//! Does not decide: whether the search works, whether results are usable
//! (FED-19), or whether a search offered only on other pages satisfies the
//! rule. Controls revealed by interaction are not reached.

use std::collections::BTreeSet;

use serde_json::json;

use super::fed_common::{is_home, with_ax};
use super::{clip, finding, Ctx};
use crate::capture_view::{normalize_space, CaptureView};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FED-SEARCH",
    act: &[],
    informs: &["FED-17"],
    summary: "The home page offers a site search control with an accessible name",
    channel: "browser",
    never_fails: false,
    absent_reason: "The capture has no baseline snapshot.",
};

const LIMIT: &str = "Looks for a search control and its name. Whether search works, and whether a control revealed by interaction exists, is not decided.";

fn role_token(v: &CaptureView<'_>, el: usize) -> Option<String> {
    v.attr(el, "role")
        .and_then(|r| r.split_ascii_whitespace().next())
        .map(str::to_ascii_lowercase)
}

fn is_text_input(v: &CaptureView<'_>, el: usize) -> bool {
    match v.tag(el) {
        "input" => v
            .attr(el, "type")
            .map(|t| t.trim().to_ascii_lowercase())
            .is_none_or(|t| t.is_empty() || t == "text" || t == "search"),
        "textarea" => false,
        _ => matches!(
            role_token(v, el).as_deref(),
            Some("searchbox" | "textbox" | "combobox")
        ),
    }
}

/// Accessible name of an input: the accessibility tree's, else `aria-label`, a `label`, or `title`.
fn input_name(v: &CaptureView<'_>, el: usize) -> String {
    if v.ax.is_some() {
        return v.ax_name(el).map(normalize_space).unwrap_or_default();
    }
    for attr in ["aria-label", "title"] {
        if let Some(n) = v
            .attr(el, attr)
            .map(normalize_space)
            .filter(|n| !n.is_empty())
        {
            return n;
        }
    }
    if let Some(id) = v.attr(el, "id").filter(|i| !i.is_empty()) {
        for l in v
            .elements()
            .filter(|e| v.tag(*e) == "label" && v.attr(*e, "for") == Some(id))
        {
            let t = v.visible_text(l);
            if !t.is_empty() {
                return t;
            }
        }
    }
    String::new()
}

fn mentions_search(v: &CaptureView<'_>, el: usize) -> bool {
    let low = |s: Option<&str>| s.unwrap_or("").to_lowercase();
    low(v.attr(el, "placeholder")).contains("search")
        || low(v.attr(el, "aria-label")).contains("search")
        || low(v.ax_name(el)).contains("search")
        || low(v.attr(el, "name")).contains("search")
}

fn usable(v: &CaptureView<'_>, el: usize) -> bool {
    v.visible(el) && !v.in_aria_hidden(el)
}

/// Records a search landmark or searchbox: its inputs, or that it is bare or hidden.
fn landmark(
    v: &CaptureView<'_>,
    el: usize,
    role: &str,
    not_usable: bool,
    inputs: &mut BTreeSet<usize>,
    bare: &mut BTreeSet<usize>,
    hidden: &mut BTreeSet<usize>,
) {
    if not_usable {
        hidden.insert(el);
        return;
    }
    if role == "searchbox" {
        inputs.insert(el);
        return;
    }
    let inside: Vec<usize> = v
        .descendants(el)
        .into_iter()
        .filter(|d| v.is_element(*d) && is_text_input(v, *d) && usable(v, *d))
        .collect();
    if inside.is_empty() {
        bare.insert(el);
    }
    inputs.extend(inside);
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    // Inputs to judge, and whether each came from a visible search landmark with no input.
    let mut inputs: BTreeSet<usize> = BTreeSet::new();
    let mut bare_landmarks: BTreeSet<usize> = BTreeSet::new();
    let mut hidden: BTreeSet<usize> = BTreeSet::new();

    if let Some(tree) = v.ax {
        for n in &tree.nodes {
            if n.ignored {
                continue;
            }
            let Some(role) = n.role.as_deref().map(str::to_ascii_lowercase) else {
                continue;
            };
            if role != "search" && role != "searchbox" {
                continue;
            }
            let Some(el) = n.backend_dom_node_id.and_then(|b| v.dom_index(b)) else {
                continue;
            };
            if v.is_element(el) {
                landmark(
                    v,
                    el,
                    &role,
                    !usable(v, el),
                    &mut inputs,
                    &mut bare_landmarks,
                    &mut hidden,
                );
            }
        }
    }
    for el in v.elements() {
        if let Some(r @ ("search" | "searchbox")) = role_token(v, el).as_deref() {
            landmark(
                v,
                el,
                r,
                !usable(v, el),
                &mut inputs,
                &mut bare_landmarks,
                &mut hidden,
            );
        }
        if v.tag(el) == "input"
            && v.attr(el, "type")
                .is_some_and(|t| t.trim().eq_ignore_ascii_case("search"))
        {
            if usable(v, el) {
                inputs.insert(el);
            } else {
                hidden.insert(el);
            }
        }
        if v.tag(el) == "form" {
            let matching: Vec<usize> = v
                .descendants(el)
                .into_iter()
                .filter(|d| v.is_element(*d) && is_text_input(v, *d) && mentions_search(v, *d))
                .collect();
            if usable(v, el) {
                inputs.extend(matching.into_iter().filter(|d| usable(v, *d)));
            } else if !matching.is_empty() {
                hidden.insert(el);
            }
        }
    }

    let expected = "a visible search control with an accessible name";
    let make = |state: crate::model::State, instance: String, observed: serde_json::Value| {
        with_ax(finding(&INFO, v, instance, state, expected, observed), v).limit(LIMIT)
    };
    let named: Vec<(usize, String)> = inputs
        .iter()
        .map(|e| (*e, input_name(v, *e)))
        .filter(|(_, n)| !n.is_empty())
        .collect();
    if let Some((el, name)) = named.first() {
        return vec![make(
            State::Pass,
            v.selector(*el),
            json!({ "control": v.tag(*el), "name": clip(name, 120), "named_controls": named.len() }),
        )];
    }
    if let Some(el) = inputs.iter().next() {
        return vec![make(
            State::NeedsReview,
            v.selector(*el),
            json!({
                "control": v.tag(*el),
                "name": null,
                "reason": "a search control is present but has no accessible name",
            }),
        )];
    }
    if let Some(el) = bare_landmarks.iter().next() {
        return vec![make(
            State::NeedsReview,
            v.selector(*el),
            json!({
                "control": "search landmark",
                "name": null,
                "reason": "a search landmark is present but holds no visible text input",
            }),
        )];
    }
    if let Some(el) = hidden.iter().next() {
        return vec![make(
            State::NeedsReview,
            v.selector(*el),
            json!({
                "control": null,
                "hidden_candidates": hidden.len(),
                "reason": "a search control is in the markup but is not visible; interaction may reveal it",
            }),
        )];
    }
    if is_home(v) {
        return vec![make(
            State::Fail,
            "html".to_string(),
            json!({ "control": null, "home_page": true, "reason": "no search control rendered on the home page" }),
        )];
    }
    vec![make(
        State::NeedsReview,
        "html".to_string(),
        json!({
            "control": null,
            "home_page": false,
            "reason": "no search control on this page; the site may offer it elsewhere",
        }),
    )]
}
