//! `BRW-FED-DAP` (FED-27, binding; authored fixtures, no ACT rule).
//!
//! Decides, from the network log of the capture, whether the page loaded the
//! Digital Analytics Program script `Universal-Federated-Analytics-Min.js`
//! from `dap.digitalgov.gov`. Host and file name compare without regard to
//! case; query parameters compare by name in any order.
//!
//! * `pass` when a log entry for that script has status 200. The `agency` and
//!   `subagency` query values are recorded when present.
//! * `needs_review` when the script is referenced, by a `script[src]` in the
//!   DOM or by a log entry, but no entry shows it loading with status 200:
//!   blocked by the collector's origin policy, failed, or not requested.
//! * `fail` when neither the DOM nor the log mentions the script.
//! * `not_tested` when the capture has no network log.
//!
//! Does not decide: whether the rule applies to this site (`dap_applies` is
//! the predicate engine's call), whether the parameters name the right agency,
//! or whether the script ran. A page without DAP that is exempt shows `fail`
//! here and `not_applicable` in the applicability result.

use diligent_capture::NetworkEntry;
use serde_json::json;
use url::Url;

use super::fed_common::{network_key, with_ax};
use super::{finding, not_tested, Ctx};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FED-DAP",
    act: &[],
    informs: &["FED-27"],
    summary: "The page loads the Digital Analytics Program script from dap.digitalgov.gov",
    channel: "browser",
    never_fails: false,
    absent_reason: "The capture has no network log.",
};

const HOST: &str = "dap.digitalgov.gov";
const FILE: &str = "universal-federated-analytics-min.js";

const LIMIT: &str = "Reads one page load's network log. Whether DAP applies to this site is decided by the applicability result, not here, and a loaded script is not proof that it reported.";

/// True when `url` is the DAP script: host and file name match, ignoring case.
fn is_dap(url: &str) -> bool {
    let Ok(u) = Url::parse(url) else { return false };
    u.host_str().is_some_and(|h| h.eq_ignore_ascii_case(HOST))
        && u.path().to_ascii_lowercase().ends_with(FILE)
}

fn query_value(url: &str, name: &str) -> Option<String> {
    let u = Url::parse(url).ok()?;
    u.query_pairs()
        .find(|(k, _)| k.eq_ignore_ascii_case(name))
        .map(|(_, v)| v.into_owned())
}

fn loaded(entries: &[NetworkEntry]) -> Option<&NetworkEntry> {
    entries
        .iter()
        .find(|e| is_dap(&e.url) && e.status == Some(200))
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let entries = match ctx.set.network() {
        Some(e) if !e.is_empty() => e,
        _ => {
            return vec![not_tested(
                &INFO,
                Some(v),
                "The capture has no network log.",
            )]
        }
    };
    let expected = "the DAP script loads from dap.digitalgov.gov with status 200";
    let make = |state: State, observed: serde_json::Value| {
        let mut f = with_ax(
            finding(&INFO, v, "html".to_string(), state, expected, observed),
            v,
        )
        .limit(LIMIT);
        if let Some(key) = network_key(ctx.set) {
            f = f.with(key);
        }
        f
    };
    if let Some(e) = loaded(entries) {
        return vec![make(
            State::Pass,
            json!({
                "url": e.url,
                "status": e.status,
                "agency": query_value(&e.url, "agency"),
                "subagency": query_value(&e.url, "subagency"),
            }),
        )];
    }
    let in_log: Vec<&NetworkEntry> = entries.iter().filter(|e| is_dap(&e.url)).collect();
    let in_dom: Vec<String> = v
        .elements()
        .filter(|el| v.tag(*el) == "script")
        .filter_map(|el| v.attr(el, "src").map(|s| v.resolve_url(s)))
        .filter(|u| is_dap(u))
        .collect();
    if !in_log.is_empty() || !in_dom.is_empty() {
        let log_rows: Vec<serde_json::Value> = in_log
            .iter()
            .map(|e| json!({ "status": e.status, "blocked_by_policy": e.blocked_by_policy }))
            .collect();
        let reference = in_dom
            .first()
            .cloned()
            .or_else(|| in_log.first().map(|e| e.url.clone()));
        return vec![make(
            State::NeedsReview,
            json!({
                "reason": "script referenced but not observed loading",
                "referenced_in_dom": !in_dom.is_empty(),
                "log_entries": log_rows,
                "agency": reference.as_deref().and_then(|u| query_value(u, "agency")),
                "subagency": reference.as_deref().and_then(|u| query_value(u, "subagency")),
            }),
        )];
    }
    vec![make(
        State::Fail,
        json!({ "reason": "no reference to the DAP script in the page or the network log", "log_entries": entries.len() }),
    )]
}
