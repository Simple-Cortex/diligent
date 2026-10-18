//! `SRC-CONTROL-NAME` (ACT e086e5, informs A11Y-4.1.2 and A11Y-3.3.2).
//!
//! Decides: a native `input` (other than hidden, submit, button, reset, image),
//! `select` or `textarea` has a programmatic name from `aria-labelledby`
//! (with a target that has text), `aria-label`, a `label[for]` that points at
//! it, a wrapping `label`, or `title`. A control whose only hint is
//! `placeholder` fails, with `placeholder_only: true` in the observation.
//!
//! The placeholder rule is stricter than ACT rule e086e5, which accepts a
//! placeholder as a name. That divergence is deliberate and recorded in the
//! fixture manifest.
//!
//! Does not decide: whether the label text is accurate, custom widgets built
//! from `div role=textbox` and similar, or controls created by script.

use scraper::ElementRef;
use serde_json::json;

use crate::dom::{self, Ctx};
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-CONTROL-NAME",
    act: &["e086e5"],
    informs: &["A11Y-4.1.2", "A11Y-3.3.2"],
    summary: "Native form control has a programmatic name",
    absent_reason: "No visible native input, select or textarea needing a name.",
};

const EXPECTED: &str =
    "control has a label[for], wrapping label, aria-label, aria-labelledby or title name";

fn label_text(ctx: &Ctx<'_>, label: ElementRef<'_>) -> String {
    dom::content_text(ctx, label, true)
}

fn find_name<'a>(
    ctx: &Ctx<'a>,
    labels: &[ElementRef<'a>],
    el: ElementRef<'a>,
) -> Option<(&'static str, String)> {
    if let Some(t) = dom::labelledby_text(ctx, el).filter(|t| !t.is_empty()) {
        return Some(("aria-labelledby", t));
    }
    if let Some(l) = dom::attr_trim(el, "aria-label") {
        return Some(("aria-label", dom::normalize_ws(l)));
    }
    let id = dom::attr_trim(el, "id");
    if let Some(id) = id {
        if ctx.by_id(id) == Some(el) {
            for label in labels {
                if dom::attr(*label, "for").map(str::trim) == Some(id) {
                    let t = label_text(ctx, *label);
                    if !t.is_empty() {
                        return Some(("label[for]", t));
                    }
                }
            }
        }
    }
    for anc in dom::ancestors(el) {
        if dom::tag(anc) == "label" && dom::is_html(anc) {
            let associated = match dom::attr(anc, "for").map(str::trim) {
                None => true,
                Some(f) => Some(f) == id,
            };
            if associated {
                let t = label_text(ctx, anc);
                if !t.is_empty() {
                    return Some(("label-wrapping", t));
                }
            }
            break;
        }
    }
    if let Some(t) = dom::attr_trim(el, "title") {
        return Some(("title", dom::normalize_ws(t)));
    }
    None
}

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    let labels: Vec<ElementRef<'a>> = ctx
        .elements
        .iter()
        .copied()
        .filter(|e| dom::tag(*e) == "label" && dom::is_html(*e))
        .collect();
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
        let applicable = match tag {
            "input" => !matches!(
                ty.as_deref(),
                Some("hidden" | "submit" | "button" | "reset" | "image")
            ),
            "select" | "textarea" => true,
            _ => false,
        };
        if !applicable || dom::is_hidden(el) || dom::presentational_honored(el) {
            continue;
        }
        let placeholder = dom::attr_trim(el, "placeholder");
        match find_name(ctx, &labels, el) {
            Some((source, name)) => {
                let observed = json!({
                    "tag": tag,
                    "type": ty,
                    "name_source": source,
                    "name": dom::truncate_chars(&name, 200),
                    "placeholder_only": false,
                });
                em.emit(&INFO, el, Outcome::new(State::Pass, EXPECTED, observed));
            }
            None => {
                let observed = json!({
                    "tag": tag,
                    "type": ty,
                    "name_source": null,
                    "placeholder": placeholder,
                    "placeholder_only": placeholder.is_some(),
                });
                let mut o = Outcome::new(State::Fail, EXPECTED, observed);
                if placeholder.is_some() {
                    o = o.limit(
                        "Only a placeholder is present. ACT e086e5 accepts this; Diligent does not, because a placeholder disappears on input.",
                    );
                }
                em.emit(&INFO, el, o);
            }
        }
    }
}
