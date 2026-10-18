//! `BRW-BYPASS-BLOCKS` (WCAG 2.4.1; ACT cf77f2 with 047fe0, b40fd1, ye5d6e).
//!
//! Decides whether the page offers a way past repeated blocks:
//!
//! * `pass` when one of the first three Tab stops is a named skip link to an
//!   element on the page, or the page has a `main` landmark, or a heading with a name;
//! * `not_tested` when none of those exist and the page has no `nav`, `header`
//!   or `banner` and fewer than three links before its first heading, so there
//!   is too little repeated content to judge;
//! * `fail` otherwise.
//!
//! Does not decide: whether the skip link target is the right place, whether
//! the landmark or heading structure is sensible, or whether a skip link that
//! appears only on focus works for pointer users.

use serde_json::json;

use super::{finding, Ctx};
use crate::capture_view::CaptureView;
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-BYPASS-BLOCKS",
    act: &["cf77f2", "047fe0", "b40fd1", "ye5d6e"],
    informs: &["A11Y-2.4.1"],
    summary: "The page has a skip link, a main landmark or headings to bypass repeated blocks",
    channel: "browser",
    never_fails: false,
    absent_reason: "The page has too little repeated content to judge.",
};

const LIMIT: &str =
    "Looks for the presence of a mechanism, not whether it lands in the right place.";

fn is_heading(v: &CaptureView<'_>, el: usize) -> bool {
    matches!(v.tag(el), "h1" | "h2" | "h3" | "h4" | "h5" | "h6")
        || v.attr(el, "role")
            .is_some_and(|r| r.split_ascii_whitespace().next() == Some("heading"))
}

fn skip_link(v: &CaptureView<'_>) -> Option<(usize, String)> {
    let walk = v.focus?;
    walk.stops.iter().take(3).find_map(|stop| {
        let el = v.dom_index(stop.backend_node_id)?;
        if v.tag(el) != "a" {
            return None;
        }
        let href = v.attr(el, "href")?.trim();
        let fragment = href.strip_prefix('#').filter(|f| !f.is_empty())?;
        let decoded = percent_decode(fragment);
        let target = [fragment, decoded.as_str()].iter().any(|f| {
            !v.by_id(f).is_empty()
                || v.elements()
                    .any(|e| v.tag(e) == "a" && v.attr(e, "name") == Some(f))
        });
        if !target {
            return None;
        }
        let name = v
            .ax_name(el)
            .map(str::trim)
            .filter(|n| !n.is_empty())
            .map(str::to_string);
        let name = name.or_else(|| Some(stop.name.trim().to_string()).filter(|n| !n.is_empty()))?;
        Some((el, name))
    })
}

fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hex = std::str::from_utf8(&bytes[i + 1..i + 3])
                .ok()
                .and_then(|h| u8::from_str_radix(h, 16).ok());
            if let Some(b) = hex {
                out.push(b);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

fn has_main(v: &CaptureView<'_>) -> bool {
    match v.ax {
        Some(tree) => tree.nodes.iter().any(|n| {
            !n.ignored
                && n.role
                    .as_deref()
                    .is_some_and(|r| r.eq_ignore_ascii_case("main"))
        }),
        None => v
            .elements()
            .any(|e| v.tag(e) == "main" || v.attr(e, "role").is_some_and(|r| r.trim() == "main")),
    }
}

fn has_named_heading(v: &CaptureView<'_>) -> bool {
    match v.ax {
        Some(tree) => tree.nodes.iter().any(|n| {
            !n.ignored
                && n.role
                    .as_deref()
                    .is_some_and(|r| r.eq_ignore_ascii_case("heading"))
                && n.name.as_deref().is_some_and(|s| !s.trim().is_empty())
        }),
        None => v
            .elements()
            .any(|e| is_heading(v, e) && !v.visible_text(e).is_empty()),
    }
}

fn has_block_landmark(v: &CaptureView<'_>) -> bool {
    let by_dom = v.elements().any(|e| {
        matches!(v.tag(e), "nav" | "header")
            || v.attr(e, "role").is_some_and(|r| {
                matches!(
                    r.split_ascii_whitespace().next(),
                    Some("navigation" | "banner")
                )
            })
    });
    let by_ax = v.ax.is_some_and(|tree| {
        tree.nodes.iter().any(|n| {
            !n.ignored
                && n.role.as_deref().is_some_and(|r| {
                    r.eq_ignore_ascii_case("navigation") || r.eq_ignore_ascii_case("banner")
                })
        })
    });
    by_dom || by_ax
}

fn links_before_first_heading(v: &CaptureView<'_>) -> usize {
    let mut count = 0;
    for e in v.elements() {
        if is_heading(v, e) {
            break;
        }
        if v.tag(e) == "a" && v.has_attr(e, "href") {
            count += 1;
        }
    }
    count
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let mut evidence = vec![v.key("dom_snapshot")];
    if v.ax.is_some() {
        evidence.push(v.key("ax_tree"));
    }
    if v.focus.is_some() {
        evidence.push(v.key("focus_order"));
    }
    let make = |state: State, observed: serde_json::Value| {
        let mut f = finding(
            &INFO,
            v,
            "html".to_string(),
            state,
            "a skip link, a main landmark or a heading with a name",
            observed,
        )
        .limit(LIMIT);
        for key in &evidence {
            f = f.with(key.clone());
        }
        f
    };
    if let Some((el, name)) = skip_link(v) {
        return vec![make(
            State::Pass,
            json!({ "mechanism": "skip_link", "link": v.selector(el), "name": name }),
        )];
    }
    if has_main(v) {
        return vec![make(State::Pass, json!({ "mechanism": "main_landmark" }))];
    }
    if has_named_heading(v) {
        return vec![make(State::Pass, json!({ "mechanism": "heading" }))];
    }
    let links = links_before_first_heading(v);
    if !has_block_landmark(v) && links < 3 {
        return Vec::new();
    }
    vec![make(
        State::Fail,
        json!({
            "mechanism": null,
            "links_before_first_heading": links,
            "has_nav_header_or_banner": has_block_landmark(v),
        }),
    )]
}
