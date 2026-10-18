//! `SRC-META-DESCRIPTION` (no ACT rule, informs the pending FED-34 meta description standard).
//!
//! Records: whether `meta[name=description]` exists and how long its content
//! is, with the pending federal minimum (50 characters) beside it.
//!
//! Always `needs_review`, never `fail`: the standard is pending, so this is a
//! readiness observation, not a verdict.
//!
//! Does not decide: whether the description is accurate or unique, or tags
//! added by script or server-side after the saved HTML was captured.

use scraper::ElementRef;
use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-META-DESCRIPTION",
    act: &[],
    informs: &["FED-34"],
    summary: "Records meta description presence and length against the pending standard",
    absent_reason: "The document has no html element.",
};

const EXPECTED: &str =
    "meta[name=description] with content of at least 50 characters (pending standard)";
/// Pending federal minimum description length, in characters.
pub const PENDING_MIN_CHARS: usize = 50;

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    let metas: Vec<ElementRef<'a>> = ctx
        .elements
        .iter()
        .copied()
        .filter(|e| {
            dom::is_html(*e)
                && dom::tag(*e) == "meta"
                && dom::attr(*e, "name")
                    .is_some_and(|n| n.trim().eq_ignore_ascii_case("description"))
        })
        .collect();
    match metas.first().copied() {
        None => em.emit_root(
            &INFO,
            Outcome::new(
                State::NeedsReview,
                EXPECTED,
                json!({ "present": false, "length": 0, "count": 0 }),
            )
            .limit("Pending standard: absence is a readiness observation, not a failure."),
        ),
        Some(meta) => {
            let content = dom::attr(meta, "content")
                .map(dom::normalize_ws)
                .unwrap_or_default();
            let length = content.chars().count();
            em.emit(
                &INFO,
                meta,
                Outcome::new(
                    State::NeedsReview,
                    EXPECTED,
                    json!({
                        "present": true,
                        "count": metas.len(),
                        "length": length,
                        "pending_minimum_chars": PENDING_MIN_CHARS,
                        "meets_pending_minimum": length >= PENDING_MIN_CHARS,
                        "content": dom::truncate_chars(&content, 200),
                    }),
                )
                .limit("Pending standard: recorded for the rule to judge."),
            );
        }
    }
}
