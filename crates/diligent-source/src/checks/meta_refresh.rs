//! `SRC-META-REFRESH` (ACT bc659a, informs A11Y-2.2.1).
//!
//! Decides: the first `meta[http-equiv=refresh]` whose `content` starts with a
//! valid time has a delay of 0 or more than 72000 seconds (20 hours). Any
//! other delay fails. Elements whose content does not start with a valid time
//! are skipped, as are later refresh elements, matching ACT bc659a.
//!
//! Does not decide: refresh headers sent by the server, script redirects, or
//! whether the page offers a way to extend the timeout.

use scraper::ElementRef;
use serde_json::json;

use crate::dom;
use crate::emit::{Emitter, Outcome};
use crate::model::{CheckInfo, State};

/// Check description.
pub static INFO: CheckInfo = CheckInfo {
    id: "SRC-META-REFRESH",
    act: &["bc659a"],
    informs: &["A11Y-2.2.1"],
    summary: "Meta refresh has no delay, or a delay over 20 hours",
    absent_reason: "No meta http-equiv=refresh element with a valid time.",
};

const EXPECTED: &str = "meta refresh delay is 0 or greater than 72000 seconds";
const LONG_DELAY_SECONDS: u64 = 72_000;

fn is_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

/// Parses the leading time of a refresh `content` value, in whole seconds.
///
/// Returns `None` when the value does not start with a valid time followed by
/// the end of the string, `;`, `,` or whitespace.
pub(crate) fn parse_refresh_delay(content: &str) -> Option<u64> {
    let s = content.trim_start_matches(is_ws);
    let int_end = s.find(|c: char| !c.is_ascii_digit()).unwrap_or(s.len());
    if int_end == 0 {
        return None;
    }
    let int_part: u64 = s[..int_end].parse().unwrap_or(u64::MAX);
    let mut rest = &s[int_end..];
    let mut fraction_nonzero = false;
    if let Some(r) = rest.strip_prefix('.') {
        let frac_end = r.find(|c: char| !c.is_ascii_digit()).unwrap_or(r.len());
        fraction_nonzero = r[..frac_end].chars().any(|c| c != '0');
        rest = &r[frac_end..];
    }
    if let Some(c) = rest.chars().next() {
        if !(c == ';' || c == ',' || is_ws(c)) {
            return None;
        }
    }
    Some(if int_part == 0 && fraction_nonzero {
        1
    } else {
        int_part
    })
}

/// Runs the check.
pub fn run<'a>(em: &mut Emitter<'a>) {
    let ctx = em.ctx;
    let chosen: Option<(ElementRef<'a>, u64)> = ctx.elements.iter().copied().find_map(|e| {
        if !(dom::is_html(e) && dom::tag(e) == "meta") {
            return None;
        }
        let eq = dom::attr(e, "http-equiv")?;
        if !eq.trim().eq_ignore_ascii_case("refresh") {
            return None;
        }
        let delay = parse_refresh_delay(dom::attr(e, "content")?)?;
        Some((e, delay))
    });
    let Some((meta, delay)) = chosen else {
        return;
    };
    let ok = delay == 0 || delay > LONG_DELAY_SECONDS;
    let observed = json!({
        "delay_seconds": delay,
        "content": dom::attr(meta, "content").map(|c| dom::truncate_chars(c, 200)),
    });
    let state = if ok { State::Pass } else { State::Fail };
    em.emit(&INFO, meta, Outcome::new(state, EXPECTED, observed));
}

#[cfg(test)]
mod tests {
    use super::parse_refresh_delay as p;

    #[test]
    fn parses_act_examples() {
        assert_eq!(p("0; URL='https://github.com'"), Some(0));
        assert_eq!(p("30"), Some(30));
        assert_eq!(p("72001; https://w3.org"), Some(72001));
        assert_eq!(p("0: https://w3.org"), None);
        assert_eq!(p("-00.12 foo"), None);
        assert_eq!(p("; 30"), None);
        assert_eq!(p(""), None);
        assert_eq!(p("+5; https://w3.org"), None);
        assert_eq!(p("foo; URL='https://w3.org'"), None);
    }
}
