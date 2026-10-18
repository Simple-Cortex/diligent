//! `SRC-HEADING-ORDER` (no ACT rule, informs A11Y-1.3.1 as best practice).
//!
//! Decides: native `h1` to `h6` headings in document order do not skip a
//! level going down (h1 then h3) and the document has at most one `h1`. Each
//! problem is a `needs_review` on the offending heading, never a `fail`. A
//! document with headings and no problem records one `pass`.
//!
//! Does not decide: whether the headings describe the sections, `role=heading`
//! with `aria-level`, or heading levels that depend on included templates.

use scraper::ElementRef;
use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-HEADING-ORDER",
    act: &[],
    informs: &["A11Y-1.3.1"],
    summary: "Heading levels do not skip and the page has at most one h1",
    absent_reason: "The document has no visible h1 to h6 heading.",
};

const EXPECTED: &str = "heading levels descend one at a time and there is at most one h1";

fn level(el: ElementRef<'_>) -> Option<u8> {
    if !dom::is_html(el) {
        return None;
    }
    match dom::tag(el) {
        "h1" => Some(1),
        "h2" => Some(2),
        "h3" => Some(3),
        "h4" => Some(4),
        "h5" => Some(5),
        "h6" => Some(6),
        _ => None,
    }
}

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    let headings: Vec<(ElementRef<'a>, u8)> = ctx
        .elements
        .iter()
        .copied()
        .filter_map(|e| level(e).map(|l| (e, l)))
        .filter(|(e, _)| !dom::is_hidden(*e))
        .collect();
    if headings.is_empty() {
        return;
    }
    let mut problems = 0usize;
    let mut previous: Option<u8> = None;
    let mut h1_seen = 0usize;
    for (el, lvl) in &headings {
        if *lvl == 1 {
            h1_seen += 1;
            if h1_seen > 1 {
                problems += 1;
                em.emit(
                    &INFO,
                    *el,
                    Outcome::new(
                        State::NeedsReview,
                        EXPECTED,
                        json!({ "issue": "multiple_h1", "h1_index": h1_seen, "level": 1 }),
                    )
                    .limit("Best-practice check; not a WCAG failure by itself."),
                );
            }
        }
        if let Some(prev) = previous {
            if *lvl > prev + 1 {
                problems += 1;
                em.emit(
                    &INFO,
                    *el,
                    Outcome::new(
                        State::NeedsReview,
                        EXPECTED,
                        json!({ "issue": "skipped_level", "previous_level": prev, "level": lvl }),
                    )
                    .limit("Best-practice check; not a WCAG failure by itself."),
                );
            }
        }
        previous = Some(*lvl);
    }
    if problems == 0 {
        em.emit_root(
            &INFO,
            Outcome::new(
                State::Pass,
                EXPECTED,
                json!({ "headings": headings.len(), "h1_count": h1_seen }),
            ),
        );
    }
}
