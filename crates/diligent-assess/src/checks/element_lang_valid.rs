//! `BRW-ELEMENT-LANG-VALID` (WCAG 3.1.2; ACT de46e4).
//!
//! Decides, for every element other than `html` that has a non-empty `lang`
//! or `xml:lang` attribute and visible text of its own (text that is not
//! inside a nested element with its own language attribute):
//!
//! * `pass` when the value is a well-formed BCP 47 tag whose primary language
//!   subtag is in the embedded ISO 639-1 or 639-2 list, in its shortest form;
//! * `fail` when the value is malformed, the primary subtag cannot be a
//!   language, or it is a three-letter code that has a two-letter form;
//! * `needs_review` when the value is well formed and its primary subtag is
//!   three letters outside the embedded list, which may be an ISO 639-3 language.
//!
//! An empty value means "unspecified" and is not examined. Does not decide:
//! whether the language matches the text, or later subtags (script, region,
//! variant) against the IANA registry.

use serde_json::json;

use super::{finding, Ctx};
use crate::capture_view::CaptureView;
use crate::lang_tags::{check_lang, LangCheck};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-ELEMENT-LANG-VALID",
    act: &["de46e4"],
    informs: &["A11Y-3.1.2"],
    summary: "lang attributes on elements are valid language tags",
    channel: "browser",
    never_fails: false,
    absent_reason: "No element other than html has a non-empty lang or xml:lang attribute with visible text of its own.",
};

const LIMIT: &str = "Checks tag syntax and the primary language subtag against an embedded ISO 639-1 and 639-2 list. It does not check that the language matches the text, or the script, region and variant subtags.";

fn declares_language(v: &CaptureView<'_>, el: usize) -> bool {
    v.has_attr(el, "lang") || v.has_attr(el, "xml:lang")
}

/// True when `el` has visible text outside any nested element that declares its own language.
fn has_own_text(v: &CaptureView<'_>, el: usize) -> bool {
    let mut stack: Vec<usize> = v.children(el).to_vec();
    while let Some(n) = stack.pop() {
        if v.is_text(n) {
            if v.node(n)
                .text
                .as_deref()
                .is_some_and(|t| !t.trim().is_empty())
                && v.text_visible(n)
            {
                return true;
            }
        } else if v.is_element(n)
            && v.tag(n) == "img"
            && v.visible(n)
            && v.attr(n, "alt").is_some_and(|a| !a.trim().is_empty())
        {
            return true;
        } else if v.is_element(n) && !declares_language(v, n) {
            stack.extend_from_slice(v.children(n));
        }
    }
    false
}

fn evaluate(v: &CaptureView<'_>, el: usize) -> Option<Finding> {
    if v.tag(el) == "html" {
        return None;
    }
    // Collect (attr_name, raw_value) for attributes that are present.
    let present: Vec<(&str, &str)> = [
        ("lang", v.attr(el, "lang")),
        ("xml:lang", v.attr(el, "xml:lang")),
    ]
    .into_iter()
    .filter_map(|(name, value)| value.map(|x| (name, x)))
    .collect();
    // No lang/xml:lang attribute at all → nothing to check.
    if present.is_empty() || !has_own_text(v, el) {
        return None;
    }
    // Separate: empty means "unspecified" (not examined); whitespace-only is malformed (fail).
    let values: Vec<(&str, &str)> = present.into_iter().filter(|(_, x)| !x.is_empty()).collect();
    if values.is_empty() {
        return None;
    }
    let mut problems: Vec<String> = Vec::new();
    let mut review: Vec<String> = Vec::new();
    for (name, value) in &values {
        match check_lang(value) {
            LangCheck::Valid => {}
            LangCheck::Unlisted(code) => review.push(format!(
                "`{name}` primary subtag `{code}` is not in ISO 639-1 or 639-2; it may be an ISO 639-3 language"
            )),
            LangCheck::Invalid(why) => problems.push(format!("`{name}`: {why}")),
        }
    }
    let state = if !problems.is_empty() {
        State::Fail
    } else if !review.is_empty() {
        State::NeedsReview
    } else {
        State::Pass
    };
    Some(
        finding(
            &INFO,
            v,
            v.selector(el),
            state,
            "a well-formed BCP 47 language tag with a known primary language",
            json!({
                "lang": v.attr(el, "lang"),
                "xml_lang": v.attr(el, "xml:lang"),
                "problems": problems,
                "review": review,
            }),
        )
        .limit(LIMIT),
    )
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    v.elements().filter_map(|el| evaluate(v, el)).collect()
}
