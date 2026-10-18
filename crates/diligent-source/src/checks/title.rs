//! `SRC-TITLE` (ACT 2779a5, informs A11Y-2.4.2 and the pending FED-33 page title standard).
//!
//! Decides: the document has a first HTML `<title>` with non-blank text.
//! `pass` additionally requires exactly one `<title>`, located in `<head>`.
//! More than one title, or a title outside the head, is `needs_review`.
//! A missing or blank first title is `fail`.
//!
//! Records the trimmed title length so the pending federal minimum (20
//! characters) can be judged by the rule, not here.
//!
//! Does not decide: whether the title describes the page or is unique across
//! the site.

use scraper::ElementRef;
use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-TITLE",
    act: &["2779a5"],
    informs: &["A11Y-2.4.2", "FED-33"],
    summary: "Document has exactly one non-empty title in head",
    absent_reason: "The document has no html element.",
};

const EXPECTED: &str = "exactly one non-empty <title> element inside <head>";
/// Pending federal minimum title length, in characters.
pub const PENDING_MIN_CHARS: usize = 20;

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    let titles: Vec<ElementRef<'a>> = ctx
        .elements
        .iter()
        .copied()
        .filter(|e| dom::is_html(*e) && dom::tag(*e) == "title")
        .collect();
    let in_head =
        |t: ElementRef<'_>| dom::ancestors(t).any(|a| dom::tag(a) == "head" && dom::is_html(a));
    let head_count = titles.iter().filter(|t| in_head(**t)).count();
    let Some(first) = titles.first().copied() else {
        em.emit_root(
            &INFO,
            Outcome::new(
                State::Fail,
                EXPECTED,
                json!({ "title_count": 0, "title_count_in_head": 0, "reason": "missing" }),
            ),
        );
        return;
    };
    let text = dom::normalize_ws(&first.text().collect::<String>());
    let length = text.chars().count();
    let observed = json!({
        "title": dom::truncate_chars(&text, 200),
        "length": length,
        "meets_pending_minimum": length >= PENDING_MIN_CHARS,
        "pending_minimum_chars": PENDING_MIN_CHARS,
        "title_count": titles.len(),
        "title_count_in_head": head_count,
    });
    let outcome = if text.is_empty() {
        let mut o = observed;
        o["reason"] = json!("blank");
        Outcome::new(State::Fail, EXPECTED, o)
    } else if titles.len() == 1 && head_count == 1 {
        Outcome::new(State::Pass, EXPECTED, observed)
    } else {
        Outcome::new(State::NeedsReview, EXPECTED, observed)
            .limit("More than one title element, or the title is outside head.")
    };
    em.emit(&INFO, first, outcome);
}
