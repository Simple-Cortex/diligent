//! `SRC-IFRAME-NAME` (ACT cae760, informs A11Y-4.1.2).
//!
//! Decides: an `iframe` has a name from `aria-labelledby`, `aria-label` or
//! `title`. The `name` attribute is not an accessible name. Frames with a
//! negative `tabindex` are skipped, as in ACT cae760.
//!
//! Does not decide: whether the name describes the frame content, or the
//! content of the framed document, which is not loaded.

use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-IFRAME-NAME",
    act: &["cae760"],
    informs: &["A11Y-4.1.2"],
    summary: "iframe has a non-empty accessible name",
    absent_reason: "No visible iframe in the tab order.",
};

const EXPECTED: &str = "iframe has a title, aria-label or aria-labelledby name";

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    for el in ctx.elements.iter().copied() {
        if !(dom::is_html(el) && dom::tag(el) == "iframe") {
            continue;
        }
        if dom::is_hidden(el) || dom::presentational_honored(el) {
            continue;
        }
        if dom::tabindex(el).is_some_and(|t| t < 0) {
            continue;
        }
        let labelledby = dom::labelledby_text(ctx, el).filter(|t| !t.is_empty());
        let aria_label = dom::attr_trim(el, "aria-label");
        let title = dom::attr_trim(el, "title");
        let source = if labelledby.is_some() {
            Some("aria-labelledby")
        } else if aria_label.is_some() {
            Some("aria-label")
        } else if title.is_some() {
            Some("title")
        } else {
            None
        };
        let observed = json!({
            "name_source": source,
            "title": title,
            "aria_label": aria_label,
            "aria_labelledby_text": labelledby,
            "name_attribute": dom::attr(el, "name"),
        });
        let state = if source.is_some() {
            State::Pass
        } else {
            State::Fail
        };
        em.emit(&INFO, el, Outcome::new(state, EXPECTED, observed));
    }
}
