//! `SRC-LINK-NAME` (ACT c487ae, informs A11Y-2.4.4 and A11Y-4.1.2).
//!
//! Decides: an `a[href]` or an element with `role="link"` has a name from
//! `aria-labelledby`, `aria-label`, its text and image content, or `title`.
//! An empty name is `fail`. A name that is only generic text (click here, read
//! more, here, more, learn more, link) is `needs_review`.
//!
//! An `a[href]` with an explicit role other than link, none, presentation or
//! `doc-*` is left to the check for that role, for example the button check.
//!
//! Does not decide: whether the name conveys the destination in context, image
//! map `area` elements, duplicate link text with different targets, or links
//! that scripts create.

use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-LINK-NAME",
    act: &["c487ae"],
    informs: &["A11Y-2.4.4", "A11Y-4.1.2"],
    summary: "Link has a non-empty accessible name that is not generic text",
    absent_reason: "No visible a[href] or role=link element.",
};

const EXPECTED: &str = "link has a specific, non-empty accessible name";

const GENERIC: &[&str] = &[
    "click here",
    "read more",
    "here",
    "more",
    "learn more",
    "link",
];

fn is_generic(name: &str) -> bool {
    let norm = name
        .trim_matches(|c: char| !c.is_alphanumeric())
        .to_lowercase();
    GENERIC.contains(&norm.as_str())
}

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    for el in ctx.elements.iter().copied() {
        let role = dom::explicit_role(el);
        let anchor = dom::is_html(el) && dom::tag(el) == "a" && dom::has_attr(el, "href");
        let anchor_ok = anchor
            && match role.as_deref() {
                None | Some("link") => true,
                Some(r) => dom::is_presentational_role(r) || r.starts_with("doc-"),
            };
        let by_role = role.as_deref() == Some("link");
        if !(anchor_ok || by_role) {
            continue;
        }
        if dom::is_hidden(el) {
            continue;
        }
        let (name, source) = dom::link_name(ctx, el);
        let generic = !name.is_empty() && is_generic(&name);
        let state = if name.is_empty() {
            State::Fail
        } else if generic {
            State::NeedsReview
        } else {
            State::Pass
        };
        let observed = json!({
            "tag": dom::tag(el),
            "role": role,
            "name_source": source,
            "name": dom::truncate_chars(&name, 200),
            "generic_text": generic,
            "href": dom::attr(el, "href").map(|h| dom::truncate_chars(h, 200)),
        });
        let mut o = Outcome::new(state, EXPECTED, observed);
        if generic {
            o = o.limit("Generic link text; whether the surrounding context makes the purpose clear is not decided.");
        }
        em.emit(&INFO, el, o);
    }
}
