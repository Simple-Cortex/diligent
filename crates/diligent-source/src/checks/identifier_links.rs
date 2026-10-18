//! `SRC-IDENTIFIER-LINKS` (no ACT rule, informs FED-04 to FED-10 and FED-13).
//!
//! Records, when a `.usa-identifier` element exists, which of the required
//! identifier links appear among the link texts inside it: About,
//! Accessibility, FOIA, No FEAR, Inspector General, Performance, Privacy,
//! USA.gov. One result per required link, each naming its own pack rule.
//!
//! Always `needs_review`: presence in link text is a fact for the rule to
//! weigh, and says nothing about where the link points.
//!
//! Does not decide: link targets, link order, the agency parent statement, or
//! identifiers that are not marked up with the USWDS class.

use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-IDENTIFIER-LINKS",
    act: &[],
    informs: &[
        "FED-04", "FED-05", "FED-06", "FED-07", "FED-08", "FED-09", "FED-10", "FED-13",
    ],
    summary: "Records which required identifier links appear in a .usa-identifier",
    absent_reason: "The document has no .usa-identifier element.",
};

const EXPECTED: &str = "the identifier lists the required links";

/// Required link, the substrings that satisfy it, and the pack rule it informs.
const REQUIRED: &[(&str, &[&str], &str)] = &[
    ("About", &["about"], "FED-04"),
    ("Accessibility", &["accessibility"], "FED-05"),
    ("Privacy", &["privacy"], "FED-06"),
    ("FOIA", &["foia", "freedom of information"], "FED-07"),
    ("No FEAR", &["no fear"], "FED-08"),
    ("Performance", &["performance"], "FED-09"),
    ("Inspector General", &["inspector general"], "FED-10"),
    ("USA.gov", &["usa.gov"], "FED-13"),
];

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    let Some(identifier) = ctx
        .elements
        .iter()
        .copied()
        .find(|e| dom::has_class(*e, "usa-identifier"))
    else {
        return;
    };
    let links: Vec<(String, Option<&str>)> = identifier
        .descendent_elements()
        .filter(|e| dom::is_html(*e) && dom::tag(*e) == "a" && dom::has_attr(*e, "href"))
        .map(|e| {
            (
                dom::link_name(ctx, e).0.to_lowercase(),
                dom::attr(e, "href"),
            )
        })
        .collect();
    for (label, needles, rule) in REQUIRED {
        let hit = links
            .iter()
            .find(|(text, _)| needles.iter().any(|n| text.contains(n)));
        let observed = json!({
            "required_link": label,
            "present": hit.is_some(),
            "matched_text": hit.map(|(t, _)| dom::truncate_chars(t, 120)),
            "href": hit.and_then(|(_, h)| h.map(|h| dom::truncate_chars(h, 200))),
        });
        let outcome = Outcome::new(State::NeedsReview, EXPECTED, observed)
            .rule(rule)
            .limit("Pending standard: link text presence only; target and wording need review.");
        em.emit(&INFO, identifier, outcome);
    }
}
