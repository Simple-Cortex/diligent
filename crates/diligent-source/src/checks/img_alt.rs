//! `SRC-IMG-ALT-PRESENT` (ACT 23a2a8, informs A11Y-1.1.1).
//!
//! Decides: an `img`, or a non-SVG element with `role="img"`, carries a text
//! alternative in the markup, or is marked presentational.
//!
//! - `alt=""` or a presentational role that takes effect: `pass` (decorative).
//! - A name from `alt`, `aria-label`, `aria-labelledby` or `title`: `needs_review`,
//!   because whether the text is adequate is a human judgement.
//! - `alt=""` on an image inside a link that has no other text: `fail`.
//! - No alternative, or whitespace-only `alt`: `fail`.
//!
//! Does not decide: whether the text describes the image, whether the image
//! is truly decorative, CSS background images, or images added by script.

use serde_json::json;

use crate::dom::{self, Ctx};
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};
use scraper::ElementRef;

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-IMG-ALT-PRESENT",
    act: &["23a2a8"],
    informs: &["A11Y-1.1.1"],
    summary: "Image has a text alternative or a presentational marking in the markup",
    absent_reason: "The document has no visible img or role=img element.",
};

const EXPECTED: &str =
    "img has alt, a presentational role, or an aria-label/aria-labelledby/title name";

fn enclosing_link<'a>(el: ElementRef<'a>) -> Option<ElementRef<'a>> {
    dom::ancestors(el).find(|a| {
        (dom::tag(*a) == "a" && dom::is_html(*a) && dom::has_attr(*a, "href"))
            || dom::explicit_role(*a).as_deref() == Some("link")
    })
}

fn in_textless_link(ctx: &Ctx<'_>, el: ElementRef<'_>) -> bool {
    enclosing_link(el).is_some_and(|link| dom::link_name(ctx, link).0.is_empty())
}

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    for el in ctx.elements.iter().copied() {
        let role = dom::explicit_role(el);
        let is_img = dom::tag(el) == "img" && dom::is_html(el);
        let is_role_img = !is_img && role.as_deref() == Some("img") && !dom::is_svg(el);
        if !is_img && !is_role_img {
            continue;
        }
        if is_img {
            if let Some(r) = role.as_deref() {
                if r != "img" && !dom::is_presentational_role(r) {
                    continue;
                }
            }
        }
        if dom::is_hidden(el) {
            continue;
        }

        let alt = dom::attr(el, "alt");
        let labelledby = dom::labelledby_text(ctx, el).filter(|t| !t.is_empty());
        let aria_label = dom::attr_trim(el, "aria-label");
        let title = dom::attr_trim(el, "title");
        let observed = json!({
            "tag": dom::tag(el),
            "alt": alt,
            "role": role,
            "aria_label": aria_label,
            "aria_labelledby_text": labelledby,
            "title": title,
            "focusable": dom::is_focusable(el),
        });

        if dom::presentational_honored(el) {
            let o = Outcome::new(State::Pass, EXPECTED, observed)
                .limit("Presentational role accepted as decorative; whether the image is decorative is not decided.");
            em.emit(&INFO, el, o);
            continue;
        }

        let alt_text = if is_img {
            alt.map(str::trim).filter(|a| !a.is_empty())
        } else {
            None
        };
        let name_source = if labelledby.is_some() {
            Some("aria-labelledby")
        } else if aria_label.is_some() {
            Some("aria-label")
        } else if alt_text.is_some() {
            Some("alt")
        } else if title.is_some() {
            Some("title")
        } else {
            None
        };

        if let Some(source) = name_source {
            let mut observed = observed;
            observed["name_source"] = json!(source);
            let o = Outcome::new(State::NeedsReview, EXPECTED, observed)
                .limit("A text alternative is present; its adequacy is not decided.");
            em.emit(&INFO, el, o);
            continue;
        }

        let (state, reason) = match (is_img, alt) {
            (true, Some("")) => {
                if in_textless_link(ctx, el) {
                    (State::Fail, "empty_alt_in_link_without_other_text")
                } else {
                    (State::Pass, "empty_alt_decorative")
                }
            }
            (true, Some(_)) => (State::Fail, "whitespace_only_alt"),
            (true, None) => (State::Fail, "missing_alt"),
            (false, _) => (State::Fail, "role_img_without_name"),
        };
        let mut observed = observed;
        observed["reason"] = json!(reason);
        let mut o = Outcome::new(state, EXPECTED, observed);
        if state == State::Pass {
            o = o.limit(
                "Empty alt accepted as decorative; whether the image is decorative is not decided.",
            );
        }
        em.emit(&INFO, el, o);
    }
}
