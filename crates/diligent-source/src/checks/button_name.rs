//! `SRC-BUTTON-NAME` (ACT 97a4e1, informs A11Y-4.1.2).
//!
//! Decides: a `button`, an `input` of type submit, button, reset or image, or
//! any element with `role="button"` has a name from text content, `value`
//! (inputs only), `alt` (image inputs only), `aria-label`, `aria-labelledby`
//! or `title`. A submit or reset input without a value has a browser default
//! label and passes.
//!
//! Differs from the work order wording in one respect: on a `button` element,
//! `value` is not a name source, matching ACT cases that fail
//! `<button value="read more"></button>`.
//!
//! Does not decide: whether the name describes the action, or buttons that
//! scripts create or relabel.

use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-BUTTON-NAME",
    act: &["97a4e1"],
    informs: &["A11Y-4.1.2"],
    summary: "Button has a non-empty accessible name",
    absent_reason: "No visible button, button-type input or role=button element.",
};

const EXPECTED: &str =
    "button has text content, value (input), alt (image input), aria-label, aria-labelledby or title";

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    for el in ctx.elements.iter().copied() {
        let tag = dom::tag(el);
        let role = dom::explicit_role(el);
        let html = dom::is_html(el);
        let input_ty = if html && tag == "input" {
            Some(dom::input_type(el))
        } else {
            None
        };
        let native = (html && tag == "button")
            || matches!(
                input_ty.as_deref(),
                Some("submit" | "button" | "reset" | "image")
            );
        let role_ok = match role.as_deref() {
            None | Some("button") => true,
            Some(r) => native && dom::is_presentational_role(r),
        };
        let by_role = role.as_deref() == Some("button");
        if !((native && role_ok) || by_role) {
            continue;
        }
        if dom::is_hidden(el) || dom::presentational_honored(el) {
            continue;
        }

        let mut name: Option<(&'static str, String)> = None;
        if let Some(t) = dom::labelledby_text(ctx, el).filter(|t| !t.is_empty()) {
            name = Some(("aria-labelledby", t));
        } else if let Some(l) = dom::attr_trim(el, "aria-label") {
            name = Some(("aria-label", dom::normalize_ws(l)));
        } else if let Some(ty) = input_ty.as_deref() {
            let value = dom::attr_trim(el, "value");
            match ty {
                "image" => {
                    if let Some(a) = dom::attr_trim(el, "alt") {
                        name = Some(("alt", dom::normalize_ws(a)));
                    }
                }
                "submit" | "reset" => {
                    name = Some(match value {
                        Some(v) => ("value", dom::normalize_ws(v)),
                        None => ("default-label", ty.to_string()),
                    });
                }
                _ => {
                    if let Some(v) = value {
                        name = Some(("value", dom::normalize_ws(v)));
                    }
                }
            }
        } else {
            let text = dom::content_text(ctx, el, false);
            if !text.is_empty() {
                name = Some(("content", text));
            }
        }
        if name.is_none() {
            if let Some(t) = dom::attr_trim(el, "title") {
                name = Some(("title", dom::normalize_ws(t)));
            }
        }

        let observed = json!({
            "tag": tag,
            "type": input_ty,
            "role": role,
            "name_source": name.as_ref().map(|n| n.0),
            "name": name.as_ref().map(|n| dom::truncate_chars(&n.1, 200)),
        });
        let state = if name.is_some() {
            State::Pass
        } else {
            State::Fail
        };
        em.emit(&INFO, el, Outcome::new(state, EXPECTED, observed));
    }
}
