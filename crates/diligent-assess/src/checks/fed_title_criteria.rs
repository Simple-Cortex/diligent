//! `BRW-FED-TITLE-CRITERIA` (FED-33, pending GSA standard; authored fixtures, no ACT rule).
//!
//! Measures the rendered `title` against the pending acceptance criteria of
//! at least 20 characters and unique across the site.
//!
//! The title is the first `title` element inside `head`, its text trimmed and
//! its whitespace collapsed. Padding does not count towards the length.
//!
//! * `pass` when the title has 20 characters or more and no duplicate was
//!   found.
//! * `needs_review` when it has 1 to 19 characters ("below the pending
//!   20-character minimum"), when it is empty or absent (the binding failure
//!   for a missing title belongs to `A11Y-2.4.2` through `SRC-TITLE`), and
//!   when `assess --inventory` supplied an inventory in which another fetched
//!   route has the same title. Up to five duplicate URLs are listed.
//!
//! Never `fail`: the standard is pending. Without an inventory, or with one
//! that cannot be read, uniqueness is not evaluated and the row says so.
//! Titles compare case-insensitively after whitespace collapsing, cut to the
//! 200 characters the inventory keeps.

use serde_json::{json, Value};
use url::Url;

use super::fed_common::{raw_text, with_ax};
use super::{clip, finding, Ctx};
use crate::capture_view::CaptureView;
use crate::inventory::{Inventory, InventoryState};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FED-TITLE-CRITERIA",
    act: &[],
    informs: &["FED-33"],
    summary: "The rendered title has at least 20 characters and no other page shares it",
    channel: "browser",
    never_fails: true,
    absent_reason: "The capture has no baseline snapshot.",
};

const MIN_CHARS: usize = 20;
const KEPT_BY_INVENTORY: usize = 200;
const MAX_LISTED: usize = 5;

fn title_text(v: &CaptureView<'_>) -> Option<(usize, String)> {
    let el = v.elements().find(|e| {
        v.tag(*e) == "title"
            && v.ancestors(*e)
                .any(|a| v.is_element(a) && v.tag(a) == "head")
    })?;
    Some((el, raw_text(v, el)))
}

/// A URL with no fragment and no trailing slash on the path, for comparing routes.
fn route_key(url: &str) -> String {
    match Url::parse(url) {
        Ok(mut u) => {
            u.set_fragment(None);
            let mut s = u.to_string();
            if s.ends_with('/') && u.path() != "/" {
                s.pop();
            }
            s
        }
        Err(_) => url.trim().to_string(),
    }
}

fn comparable(title: &str) -> String {
    title
        .split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .to_lowercase()
        .chars()
        .take(KEPT_BY_INVENTORY)
        .collect()
}

fn duplicates(inv: &Inventory, own_url: &str, title: &str) -> Vec<String> {
    let own = route_key(own_url);
    let want = comparable(title);
    inv.routes
        .iter()
        .filter(|r| r.fetched)
        .filter_map(|r| {
            let url = r.final_url.as_deref().or(r.url.as_deref())?;
            let t = r.title.as_deref()?;
            (route_key(url) != own && comparable(t) == want).then(|| url.to_string())
        })
        .collect()
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let expected = "a title of at least 20 characters that no other page shares";
    let (instance, title) = match title_text(v) {
        Some((el, t)) => (v.selector(el), t),
        None => ("html".to_string(), String::new()),
    };
    let length = title.chars().count();
    let own_url = ctx
        .set
        .entries()
        .first()
        .map(|e| e.final_url.clone())
        .filter(|u| !u.is_empty())
        .unwrap_or_else(|| v.dom.document_url.clone());

    let mut limits: Vec<String> = vec![
        "Pending GSA standard: a pass means the pending acceptance criteria held, not that a binding rule is met.".to_string(),
    ];
    let mut dup_urls: Vec<String> = Vec::new();
    let mut uniqueness: Value = json!("not evaluated");
    match ctx.inventory {
        InventoryState::NotSupplied => {
            limits.push("uniqueness not evaluated: no inventory supplied".to_string());
        }
        InventoryState::Unreadable(why) => {
            limits.push(format!(
                "uniqueness not evaluated: inventory unreadable ({})",
                clip(why, 160)
            ));
            uniqueness = json!("inventory unreadable");
        }
        InventoryState::Loaded(inv) => {
            if length == 0 {
                limits.push("uniqueness not evaluated: the page has no title text".to_string());
            } else {
                dup_urls = duplicates(inv, &own_url, &title);
                uniqueness = json!(if dup_urls.is_empty() {
                    "unique among fetched routes"
                } else {
                    "duplicate"
                });
            }
        }
    }

    let mut reasons: Vec<String> = Vec::new();
    if length == 0 {
        reasons.push("the page has no title text; the binding failure belongs to A11Y-2.4.2 through SRC-TITLE".to_string());
    } else if length < MIN_CHARS {
        reasons.push(format!("below the pending {MIN_CHARS}-character minimum"));
    }
    if !dup_urls.is_empty() {
        reasons.push("another fetched route has the same title".to_string());
    }
    let state = if reasons.is_empty() {
        State::Pass
    } else {
        State::NeedsReview
    };
    let observed = json!({
        "title": clip(&title, 200),
        "length": length,
        "uniqueness": uniqueness,
        "duplicate_urls": dup_urls.iter().take(MAX_LISTED).collect::<Vec<_>>(),
        "duplicate_count": dup_urls.len(),
        "reasons": reasons,
    });
    let mut f = with_ax(finding(&INFO, v, instance, state, expected, observed), v);
    for l in limits {
        f = f.limit(l);
    }
    vec![f]
}
