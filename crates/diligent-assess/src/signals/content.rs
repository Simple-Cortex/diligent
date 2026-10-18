//! Content signals: plain language readings of the main text and links.
//!
//! Each reads text the browser rendered, in the scope defined by
//! [`super::Scope`]. They reuse the sentence, syllable, passive voice, link
//! text and acronym functions of `crate::content`, with their stated false
//! positives. Readability signals need at least 100 words to say anything.

use serde_json::json;

use super::model::{Signal, SignalState};
use super::{round_to, Scope};
use crate::capture_view::CaptureView;
use crate::checks::{clip, Ctx};
use crate::content::{acronyms_undefined, link_text_quality, readability, LinkTextQuality};

const MIN_WORDS: usize = 100;
const LIST_MAX: usize = 10;

fn state_over(x: f64, limit: f64) -> SignalState {
    if x > limit {
        SignalState::Lead
    } else {
        SignalState::WithinPractice
    }
}

/// Runs the content signals.
pub fn run(ctx: &Ctx<'_>) -> Vec<Signal> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let scope = Scope::new(v);
    let mut out: Vec<Signal> = Vec::new();
    let prose = scope.prose();
    let r = readability(&prose);
    let scope_note = if scope.has_main() {
        "Reads the text of the main landmark."
    } else {
        "No main landmark: reads the body without navigation, header, footer and aside."
    };
    if r.words >= MIN_WORDS {
        out.push(
            Signal::new(
                "content.readability.fk_grade",
                state_over(r.flesch_kincaid_grade, 8.0),
                json!(round_to(r.flesch_kincaid_grade, 1)),
                format!(
                    "Flesch-Kincaid grade {:.1} over {} words and {} sentences",
                    r.flesch_kincaid_grade, r.words, r.sentences
                ),
            )
            .unit("grade")
            .limit("Syllables are counted by rule; short or list-heavy text skews the grade.")
            .limit(scope_note)
            .on(v),
        );
        out.push(
            Signal::new(
                "content.readability.mean_sentence_length",
                state_over(r.mean_sentence_length, 20.0),
                json!(round_to(r.mean_sentence_length, 1)),
                format!(
                    "mean sentence length {:.1} words; {} sentences over 20 words",
                    r.mean_sentence_length, r.long_sentence_count
                ),
            )
            .unit("words")
            .limit("Blocks without closing punctuation count as one sentence each.")
            .limit(scope_note)
            .on(v),
        );
        out.push(
            Signal::new(
                "content.readability.passive_share",
                state_over(r.passive_voice_share, 0.20),
                json!(round_to(r.passive_voice_share, 3)),
                format!("{} of {} sentences use the passive voice", r.passive_voice_count, r.sentences),
            )
            .unit("share")
            .limit("The detector has known false positives: adjectives that look like participles, and -en words not on its stop list.")
            .limit(scope_note)
            .on(v),
        );
    }
    let lines = scope.lines();
    if r.words > 0 {
        let acronyms = acronyms_undefined(&lines);
        out.push(
            Signal::new(
                "content.acronyms_undefined",
                if acronyms.is_empty() { SignalState::WithinPractice } else { SignalState::Lead },
                json!(acronyms),
                format!("{} acronyms used without an expansion", acronyms.len()),
            )
            .unit("acronyms")
            .limit("The expansion is not checked against the letters; roman numerals and all-caps words in running text are reported.")
            .limit(scope_note)
            .on(v),
        );
    }
    out.extend(link_text(v));
    out.extend(heading_density(v, &scope, r.words));
    out
}

fn link_text(v: &CaptureView<'_>) -> Option<Signal> {
    let mut total = 0usize;
    let mut bad: Vec<serde_json::Value> = Vec::new();
    let mut count = 0usize;
    for el in v
        .elements()
        .filter(|e| v.tag(*e) == "a" && v.has_attr(*e, "href") && v.visible(*e))
    {
        total += 1;
        let text = v
            .ax_name(el)
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string)
            .unwrap_or_else(|| v.visible_text(el));
        let kind = match link_text_quality(&text) {
            LinkTextQuality::Ok => continue,
            LinkTextQuality::Generic(p) => format!("generic: {p}"),
            LinkTextQuality::BareUrl => "bare url".to_string(),
            LinkTextQuality::Empty => "empty".to_string(),
        };
        count += 1;
        if bad.len() < LIST_MAX {
            bad.push(json!({ "selector": v.selector(el), "text": clip(&text, 80), "kind": kind }));
        }
    }
    if total == 0 {
        return None;
    }
    Some(
        Signal::new(
            "content.link_text_generic",
            if count >= 1 { SignalState::Lead } else { SignalState::WithinPractice },
            json!({ "count": count, "links": total, "instances": bad }),
            format!("{count} of {total} visible links have generic, bare URL or empty text"),
        )
        .unit("links")
        .limit("Judged from the link text alone. WCAG 2.4.4 allows generic text when the surrounding context names the purpose.")
        .on(v),
    )
}

fn heading_density(v: &CaptureView<'_>, scope: &Scope<'_>, words: usize) -> Option<Signal> {
    let headings = v
        .elements()
        .filter(|e| {
            let h = matches!(v.tag(*e), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
                || v.attr(*e, "role")
                    .is_some_and(|r| r.split_ascii_whitespace().next() == Some("heading"));
            h && v.visible(*e) && scope.contains(*e)
        })
        .count();
    if headings == 0 || words == 0 {
        return None;
    }
    let density = words as f64 / headings as f64;
    Some(
        Signal::new(
            "content.heading_density",
            SignalState::Informational,
            json!(round_to(density, 1)),
            format!("{words} words under {headings} headings"),
        )
        .unit("words per heading")
        .limit("A number with no threshold: it describes the page and does not judge it.")
        .on(v),
    )
}
