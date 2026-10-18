//! `BRW-FED-IDENTIFIER` (FED-13 primary; also FED-03 to FED-10; authored fixtures, no ACT rule).
//!
//! Finds the identifier in the rendered page: an element whose class contains
//! `usa-identifier`, else the `contentinfo` landmark, else the last `footer`.
//! Then reports one finding per required link, each naming its own pack rule:
//! About (FED-04), Accessibility (FED-05), Privacy (FED-06), FOIA (FED-07),
//! No FEAR Act (FED-08), Performance or budget (FED-09), Inspector General
//! (FED-10) and USA.gov (FED-13), plus one for the parent agency statement
//! (FED-03). The link phrases are the ones `SRC-IDENTIFIER-LINKS` uses.
//!
//! * `pass`: a visible link with the phrase in its accessible name or text and
//!   an `href` that resolves to an absolute `http(s)` URL. For USA.gov the host
//!   must be `usa.gov` or `www.usa.gov`. For FED-03, the identifier says
//!   "official website of the" and then names an agency.
//! * `needs_review`: the link is present but its `href` is empty, `#`,
//!   `javascript:` or another scheme, or it is not a link, or the USA.gov link
//!   points elsewhere; or the link is not in the identifier, where the
//!   obligation may be met elsewhere.
//! * `not_tested`: no identifier or footer at all (for every rule but FED-13,
//!   which records `needs_review`).
//!
//! Never `fail`. Does not decide: where a link leads, the order of links, or
//! whether the agency named is the real parent.

use serde_json::{json, Value};
use url::Url;

use super::fed_common::{
    block_with_phrase, controls, href_state, identifier_region, text_after_phrase, within, Control,
    Href, Kind, Region,
};
use super::{clip, finding, not_tested, Ctx};
use crate::capture_view::CaptureView;
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FED-IDENTIFIER",
    act: &[],
    informs: &[
        "FED-13", "FED-03", "FED-04", "FED-05", "FED-06", "FED-07", "FED-08", "FED-09", "FED-10",
    ],
    summary: "The identifier links About, Accessibility, Privacy, FOIA, No FEAR, performance, Inspector General and USA.gov",
    channel: "browser",
    never_fails: true,
    absent_reason: "The capture has no baseline snapshot.",
};

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

const AGENCY_PHRASE: &str = "official website of the";

const LIMIT: &str = "Looks for link text and an address that resolves; where the link leads and whether it is the right page is not decided. The obligation may be met outside the identifier.";

fn usa_gov_host(url: &str) -> bool {
    Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .is_some_and(|h| h == "usa.gov" || h == "www.usa.gov")
}

fn first_match<'c>(
    v: &CaptureView<'_>,
    list: &'c [Control],
    needles: &[&str],
    label: &str,
) -> Option<&'c Control> {
    let named = |c: &&Control| {
        c.matches(|s| {
            let low = s.to_lowercase();
            needles.iter().any(|n| low.contains(n))
        })
    };
    let by_host = |c: &&Control| {
        label == "USA.gov" && matches!(href_state(v, c.el), Href::Usable(ref u) if usa_gov_host(u))
    };
    let hits: Vec<&Control> = list.iter().filter(|c| named(c) || by_host(c)).collect();
    hits.iter()
        .copied()
        .find(|c| matches!(href_state(v, c.el), Href::Usable(_)))
        .or_else(|| hits.first().copied())
}

fn link_finding(
    v: &CaptureView<'_>,
    region: &Region,
    list: &[Control],
    row: &(&str, &[&str], &'static str),
) -> Finding {
    let (label, needles, rule) = *row;
    let expected = format!("the identifier links {label}");
    let hit = first_match(v, list, needles, label);
    let make = |state: State, instance: String, observed: Value| {
        super::fed_common::with_ax(
            finding(&INFO, v, instance, state, expected.clone(), observed),
            v,
        )
        .rules(&[rule])
        .limit(LIMIT)
    };
    let Some(c) = hit else {
        return make(
            State::NeedsReview,
            v.selector(region.els[0]),
            json!({
                "required_link": label,
                "present": false,
                "identifier_found_by": region.how,
                "reason": "not in the identifier; the obligation may be met elsewhere",
            }),
        );
    };
    let href = href_state(v, c.el);
    let (state, shown_href, why): (State, Option<String>, Option<&str>) = match (&href, c.kind) {
        (Href::Usable(url), Kind::Link) => {
            if label == "USA.gov" && !usa_gov_host(url) {
                (
                    State::NeedsReview,
                    Some(url.clone()),
                    Some("the link is named USA.gov but its address is another host"),
                )
            } else {
                (State::Pass, Some(url.clone()), None)
            }
        }
        (Href::Usable(url), Kind::Other) => (
            State::NeedsReview,
            Some(url.clone()),
            Some("the control is a button, not a link"),
        ),
        (Href::Placeholder(raw), _) => (
            State::NeedsReview,
            Some(raw.clone()),
            Some("the address is empty, #, javascript: or not http(s)"),
        ),
        (Href::Missing, _) => (State::NeedsReview, None, Some("the control has no address")),
    };
    make(
        state,
        v.selector(c.el),
        json!({
            "required_link": label,
            "present": true,
            "matched_text": clip(&c.name, 120),
            "href": shown_href.map(|h| clip(&h, 300)),
            "reason": why,
        }),
    )
}

fn agency_finding(v: &CaptureView<'_>, region: &Region) -> Finding {
    let expected = "the identifier says which parent agency the site belongs to";
    let mut agency: Option<String> = None;
    let mut block_selector: Option<String> = None;
    for scope in &region.els {
        if let Some(b) = block_with_phrase(v, Some(*scope), AGENCY_PHRASE) {
            let text = v.visible_text(b);
            if let Some(rest) = text_after_phrase(&text, AGENCY_PHRASE) {
                let name = rest
                    .trim_matches(|c: char| c.is_whitespace() || c == '.' || c == ':' || c == ',');
                let squashed: String = name
                    .chars()
                    .filter(|c| c.is_alphanumeric())
                    .flat_map(char::to_lowercase)
                    .collect();
                let generic = squashed.starts_with("unitedstatesgovernment")
                    || squashed.starts_with("usgovernment")
                    || squashed.starts_with("ugovernment");
                if squashed.chars().count() >= 2 && !generic {
                    agency = Some(name.to_string());
                    block_selector = Some(v.selector(b));
                    break;
                }
            }
        }
    }
    let (state, observed) = match &agency {
        Some(a) => (
            State::Pass,
            json!({ "agency_statement": clip(a, 160), "identifier_found_by": region.how }),
        ),
        None => (
            State::NeedsReview,
            json!({
                "agency_statement": null,
                "identifier_found_by": region.how,
                "reason": "no \"official website of the <agency>\" statement naming a parent agency in the identifier",
            }),
        ),
    };
    super::fed_common::with_ax(
        finding(
            &INFO,
            v,
            block_selector.unwrap_or_else(|| v.selector(region.els[0])),
            state,
            expected,
            observed,
        ),
        v,
    )
    .rules(&["FED-03"])
    .limit("Looks for the statement and a name after it; whether the agency is the real parent is not decided.")
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let Some(region) = identifier_region(v) else {
        let mut out = vec![super::fed_common::with_ax(
            finding(
                &INFO,
                v,
                "html".to_string(),
                State::NeedsReview,
                "the identifier links the USA.gov",
                json!({ "identifier": null, "reason": "no identifier, contentinfo landmark or footer in the page" }),
            ),
            v,
        )
        .rules(&["FED-13"])
        .limit(LIMIT)];
        for rule in [
            "FED-03", "FED-04", "FED-05", "FED-06", "FED-07", "FED-08", "FED-09", "FED-10",
        ] {
            out.push(
                not_tested(
                    &INFO,
                    Some(v),
                    "The page has no identifier, contentinfo landmark or footer to read.",
                )
                .rules(&[rule]),
            );
        }
        return out;
    };
    let list: Vec<Control> = controls(v)
        .into_iter()
        .filter(|c| c.visible && within(v, c.el, &region.els))
        .collect();
    let mut out: Vec<Finding> = REQUIRED
        .iter()
        .map(|row| link_finding(v, &region, &list, row))
        .collect();
    out.push(agency_finding(v, &region));
    // FED-13 is the primary rule: list its finding first.
    out.sort_by_key(|f| f.wcag.first().copied() != Some("FED-13"));
    out
}
