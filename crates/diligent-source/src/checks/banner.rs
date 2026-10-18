//! `SRC-BANNER-PRESENT` (no ACT rule, informs the pending FED-32 federal banner standard).
//!
//! Records: whether the document contains the USWDS banner (an element with
//! class `usa-banner`) or an equivalent official-site statement in its text.
//!
//! Always `needs_review`: the standard is pending and the wording, position
//! and behaviour need a person or a rendered check.
//!
//! Does not decide: banner text accuracy, placement at the top of the page,
//! the expand control, or banners injected by script.

use scraper::{ElementRef, Node};
use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-BANNER-PRESENT",
    act: &[],
    informs: &["FED-32"],
    summary: "Records presence of the USWDS government banner or an equivalent statement",
    absent_reason: "The document has no html element.",
};

const EXPECTED: &str =
    "document contains the .usa-banner structure or an equivalent official-site statement";
const OFFICIAL_PHRASE: &str = "official website of the united states government";

fn direct_text(el: ElementRef<'_>) -> String {
    let mut s = String::new();
    for child in el.children() {
        if let Node::Text(t) = child.value() {
            s.push_str(t);
            s.push(' ');
        }
    }
    dom::normalize_ws(&s).to_lowercase()
}

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    let class_hit = ctx
        .elements
        .iter()
        .copied()
        .find(|e| dom::has_class(*e, "usa-banner"));
    let text_hit = ctx.elements.iter().copied().find(|e| {
        !matches!(dom::tag(*e), "script" | "style") && direct_text(*e).contains(OFFICIAL_PHRASE)
    });
    let observed = json!({
        "usa_banner_class": class_hit.is_some(),
        "official_site_text": text_hit.is_some(),
        "present": class_hit.is_some() || text_hit.is_some(),
    });
    let outcome = Outcome::new(State::NeedsReview, EXPECTED, observed).limit(
        "Pending standard: presence is recorded; wording, placement and behaviour need review.",
    );
    match class_hit.or(text_hit) {
        Some(el) => em.emit(&INFO, el, outcome),
        None => em.emit_root(&INFO, outcome),
    }
}
