//! `SRC-AUTOCOMPLETE` (ACT 73f2c2, informs A11Y-1.3.5).
//!
//! Decides: the `autocomplete` value on an `input`, `select` or `textarea`
//! follows the HTML autofill grammar, with tokens from the vendored list in
//! [`super::autofill`]. Controls that are hidden, disabled, `aria-disabled`,
//! of a type autofill does not apply to, or whose value is blank, `on` or
//! `off` are not applicable.
//!
//! Does not decide: whether the field name matches the purpose of the field,
//! which is the human part of success criterion 1.3.5.

use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

use super::autofill;

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-AUTOCOMPLETE",
    act: &["73f2c2"],
    informs: &["A11Y-1.3.5"],
    summary: "autocomplete values are valid autofill tokens",
    absent_reason: "No enabled, visible form control has an autocomplete value to validate.",
};

const EXPECTED: &str = "autocomplete is a valid HTML autofill token sequence";

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    for el in ctx.elements.iter().copied() {
        if !dom::is_html(el) {
            continue;
        }
        let tag = dom::tag(el);
        let ty = if tag == "input" {
            Some(dom::input_type(el))
        } else {
            None
        };
        let control = match tag {
            "input" => !matches!(
                ty.as_deref(),
                Some(
                    "hidden"
                        | "checkbox"
                        | "radio"
                        | "file"
                        | "submit"
                        | "image"
                        | "reset"
                        | "button"
                )
            ),
            "select" | "textarea" => true,
            _ => false,
        };
        if !control {
            continue;
        }
        let Some(value) = dom::attr_trim(el, "autocomplete") else {
            continue;
        };
        let lowered = value.to_ascii_lowercase();
        if lowered == "on" || lowered == "off" {
            continue;
        }
        if dom::is_hidden(el)
            || dom::is_disabled(el)
            || dom::attr(el, "aria-disabled").is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
            || dom::presentational_honored(el)
        {
            continue;
        }
        let verdict = autofill::validate(value);
        let observed = json!({
            "tag": tag,
            "type": ty,
            "autocomplete": dom::truncate_chars(value, 200),
            "valid": verdict.is_ok(),
        });
        let state = if verdict.is_ok() {
            State::Pass
        } else {
            State::Fail
        };
        em.emit(&INFO, el, Outcome::new(state, EXPECTED, observed));
    }
}
