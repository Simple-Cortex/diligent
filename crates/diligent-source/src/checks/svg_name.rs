//! `SRC-SVG-NAME` (ACT 7d6734, informs A11Y-1.1.1).
//!
//! Decides: an SVG element with an explicit role of `img`, `graphics-document`
//! or `graphics-symbol` has a name from `aria-label`, `aria-labelledby`, or a
//! direct child `<title>` with text.
//!
//! Does not decide: whether the name describes the graphic, inline SVG added by
//! script, SVG referenced from `img` or CSS, or implicit graphics roles.

use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-SVG-NAME",
    act: &["7d6734"],
    informs: &["A11Y-1.1.1"],
    summary: "SVG element with an explicit graphics role has a name",
    absent_reason: "No visible SVG element with role img, graphics-document or graphics-symbol.",
};

const EXPECTED: &str =
    "SVG with a graphics role has aria-label, aria-labelledby or a <title> child";

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    for el in ctx.elements.iter().copied() {
        if !dom::is_svg(el) {
            continue;
        }
        let role = dom::explicit_role(el);
        if !matches!(
            role.as_deref(),
            Some("img" | "graphics-document" | "graphics-symbol")
        ) {
            continue;
        }
        if dom::is_hidden(el) {
            continue;
        }
        let labelledby = dom::labelledby_text(ctx, el).filter(|t| !t.is_empty());
        let aria_label = dom::attr_trim(el, "aria-label");
        let title_child = el
            .child_elements()
            .find(|c| dom::is_svg(*c) && dom::tag(*c) == "title")
            .map(|t| dom::content_text(ctx, t, false))
            .filter(|t| !t.is_empty());
        let source = if labelledby.is_some() {
            Some("aria-labelledby")
        } else if aria_label.is_some() {
            Some("aria-label")
        } else if title_child.is_some() {
            Some("title")
        } else {
            None
        };
        let observed = json!({
            "tag": dom::tag(el),
            "role": role,
            "name_source": source,
            "aria_label": aria_label,
            "aria_labelledby_text": labelledby,
            "title": title_child,
        });
        let state = if source.is_some() {
            State::Pass
        } else {
            State::Fail
        };
        em.emit(&INFO, el, Outcome::new(state, EXPECTED, observed));
    }
}
