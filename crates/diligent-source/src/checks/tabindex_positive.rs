//! `SRC-TABINDEX-POSITIVE` (no ACT rule, informs A11Y-2.4.3).
//!
//! Decides: no visible element has a `tabindex` greater than 0. Each such
//! element is a `fail`. When elements carry `tabindex` and none is positive,
//! one document-level `pass` is recorded.
//!
//! Does not decide: whether the resulting focus order is logical, or tabindex
//! values set by script.

use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-TABINDEX-POSITIVE",
    act: &[],
    informs: &["A11Y-2.4.3"],
    summary: "No element uses a tabindex greater than 0",
    absent_reason: "No visible element has a valid tabindex attribute.",
};

const EXPECTED: &str = "tabindex is 0 or negative";

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    let mut seen = 0usize;
    let mut positive = 0usize;
    for el in ctx.elements.iter().copied() {
        let Some(value) = dom::tabindex(el) else {
            continue;
        };
        if dom::is_hidden(el) {
            continue;
        }
        seen += 1;
        if value > 0 {
            positive += 1;
            em.emit(
                &INFO,
                el,
                Outcome::new(
                    State::Fail,
                    EXPECTED,
                    json!({ "tabindex": value, "tag": dom::tag(el) }),
                ),
            );
        }
    }
    if seen > 0 && positive == 0 {
        em.emit_root(
            &INFO,
            Outcome::new(
                State::Pass,
                EXPECTED,
                json!({ "elements_with_tabindex": seen, "positive": 0 }),
            ),
        );
    }
}
