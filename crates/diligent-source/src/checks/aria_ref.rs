//! `SRC-ARIA-REF` (no ACT rule, informs A11Y-4.1.2).
//!
//! Decides: every token in `aria-labelledby`, `aria-describedby`,
//! `aria-controls`, `aria-owns` and `aria-activedescendant` matches an `id`
//! in the same document. An element with a token that matches nothing is a
//! `fail`. When references exist and all resolve, one document-level `pass`
//! is recorded.
//!
//! Does not decide: that the target is the right element, that its text is
//! useful, or references that cross into an iframe or shadow tree.

use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-ARIA-REF",
    act: &[],
    informs: &["A11Y-4.1.2"],
    summary: "ARIA id references point at elements that exist",
    absent_reason: "No element uses an ARIA id reference attribute.",
};

const EXPECTED: &str = "every ARIA id reference resolves to an element in the document";

const REF_ATTRS: &[&str] = &[
    "aria-labelledby",
    "aria-describedby",
    "aria-controls",
    "aria-owns",
    "aria-activedescendant",
];

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    let mut references = 0usize;
    let mut broken = 0usize;
    for el in ctx.elements.iter().copied() {
        for name in REF_ATTRS {
            let Some(value) = dom::attr_trim(el, name) else {
                continue;
            };
            references += 1;
            let missing: Vec<&str> = value
                .split_ascii_whitespace()
                .filter(|t| ctx.by_id(t).is_none())
                .collect();
            if missing.is_empty() {
                continue;
            }
            broken += 1;
            let observed = json!({
                "attribute": name,
                "value": dom::truncate_chars(value, 200),
                "missing_ids": missing,
            });
            em.emit(&INFO, el, Outcome::new(State::Fail, EXPECTED, observed));
        }
    }
    if references > 0 && broken == 0 {
        em.emit_root(
            &INFO,
            Outcome::new(
                State::Pass,
                EXPECTED,
                json!({ "references": references, "broken": 0 }),
            ),
        );
    }
}
