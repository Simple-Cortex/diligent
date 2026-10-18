//! `SRC-DUP-ID` (ACT 3ea0c8, informs A11Y-4.1.2).
//!
//! Decides: no two elements in the document share a non-empty `id`. Every
//! element after the first with a given id is a `fail`, reported with the
//! selector of the first holder, so each duplicate pair is visible. When ids
//! exist and none repeat, one document-level `pass` is recorded.
//!
//! Does not decide: ids inside `iframe srcdoc`, shadow trees or `template`
//! content, or ids created by script. `xml:id` is not an `id`.

use serde_json::json;

use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-DUP-ID",
    act: &["3ea0c8"],
    informs: &["A11Y-4.1.2"],
    summary: "id attribute values are unique in the document",
    absent_reason: "No element in the document has a non-empty id.",
};

const EXPECTED: &str = "every id value appears on exactly one element";

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    if ctx.distinct_id_count() == 0 {
        return;
    }
    let mut duplicates = 0usize;
    for el in ctx.elements.iter().copied() {
        let Some(id) = el.value().id().filter(|i| !i.is_empty()) else {
            continue;
        };
        let group = ctx.id_group(id);
        if group.len() < 2 || group[0] == el {
            continue;
        }
        duplicates += 1;
        let observed = json!({
            "id": id,
            "first_holder": ctx.selector(group[0]),
            "holders": group.len(),
        });
        em.emit(&INFO, el, Outcome::new(State::Fail, EXPECTED, observed));
    }
    if duplicates == 0 {
        em.emit_root(
            &INFO,
            Outcome::new(
                State::Pass,
                EXPECTED,
                json!({ "distinct_ids": ctx.distinct_id_count(), "duplicates": 0 }),
            ),
        );
    }
}
