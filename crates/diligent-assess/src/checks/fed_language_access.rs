//! `BRW-FED-LANGUAGE-ACCESS` (FED-29, human-routed; authored fixtures, no ACT rule).
//!
//! Decides whether the rendered page has an entry point to language
//! assistance: a visible link or button that
//!
//! * carries an `hreflang` attribute, or
//! * carries a `lang` attribute whose primary subtag differs from the
//!   document's, or
//! * is named for a language or for language help, from the fixed list in
//!   `lang_names.rs` (for example Español, 中文, "Language assistance").
//!
//! * `pass` ("entry point present") when one exists.
//! * `needs_review` when none does: no language access entry point is
//!   rendered and the documentary route applies.
//!
//! Never `fail`. Does not decide: whether the target is translated, which
//! languages are required, or whether language help exists elsewhere.

use serde_json::json;

use super::fed_common::{controls, with_ax};
use super::{clip, finding, Ctx};
use crate::capture_view::CaptureView;
use crate::lang_names;
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FED-LANGUAGE-ACCESS",
    act: &[],
    informs: &["FED-29"],
    summary: "The rendered page has a language access entry point",
    channel: "browser",
    never_fails: true,
    absent_reason: "The capture has no baseline snapshot.",
};

fn primary_subtag(tag: &str) -> String {
    tag.trim()
        .split(['-', '_'])
        .next()
        .unwrap_or("")
        .to_ascii_lowercase()
}

fn document_language(v: &CaptureView<'_>) -> String {
    v.elements()
        .find(|e| v.tag(*e) == "html")
        .and_then(|e| v.attr(e, "lang"))
        .map(primary_subtag)
        .unwrap_or_default()
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let expected = "a visible link or button that offers another language or language help";
    let doc_lang = document_language(v);
    let mut found: Option<(usize, &'static str, String, String)> = None;
    for c in controls(v).into_iter().filter(|c| c.visible) {
        let how = if v
            .attr(c.el, "hreflang")
            .is_some_and(|h| !h.trim().is_empty())
        {
            Some((
                "hreflang",
                v.attr(c.el, "hreflang").unwrap_or("").trim().to_string(),
            ))
        } else if v
            .attr(c.el, "lang")
            .is_some_and(|l| !l.trim().is_empty() && primary_subtag(l) != doc_lang)
        {
            Some((
                "lang",
                v.attr(c.el, "lang").unwrap_or("").trim().to_string(),
            ))
        } else {
            [c.name.as_str(), c.text.as_str()]
                .iter()
                .find_map(|t| lang_names::find(t))
                .map(|name| ("name", name.to_string()))
        };
        if let Some((kind, detail)) = how {
            found = Some((c.el, kind, detail, clip(&c.name, 120)));
            break;
        }
    }
    let limit = "Presence of an entry point by attribute or name only; whether the target is translated, and which languages are required, is not decided.";
    let f = match found {
        Some((el, kind, matched, name)) => finding(
            &INFO,
            v,
            v.selector(el),
            State::Pass,
            expected,
            json!({ "entry_point": "present", "matched_by": kind, "matched": matched, "name": name, "document_lang": doc_lang }),
        ),
        None => finding(
            &INFO,
            v,
            "html".to_string(),
            State::NeedsReview,
            expected,
            json!({
                "entry_point": "absent",
                "document_lang": doc_lang,
                "reason": "no language access entry point rendered; documentary route applies",
            }),
        ),
    };
    vec![with_ax(f, v).limit(limit)]
}
