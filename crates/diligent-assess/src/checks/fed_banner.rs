//! `BRW-FED-BANNER` (FED-32, pending GSA standard; authored fixtures, no ACT rule).
//!
//! Finds the official-website banner in the rendered page: an element whose
//! class contains `usa-banner`, else the deepest block that holds the text
//! "an official website of the United States government", even when the words
//! are split across inline elements. Then checks the pending acceptance
//! criteria that a layout box and the DOM can show:
//!
//! * the banner is visible;
//! * it is the first visible block: it starts within 120 CSS px of the top and
//!   no visible content, apart from skip links, sits above it;
//! * it spans at least 80% of the viewport width;
//! * a visible `img` with a non-empty `alt`, or a visible `svg` or `role="img"`
//!   with a name, stands for the flag;
//! * a visible control with `aria-expanded` and `aria-controls` points to an
//!   element whose text, hidden or not, mentions both ".gov" and "https".
//!
//! `pass` when every criterion holds. `needs_review` when the banner is
//! present and some criterion is missing, naming it, and when no banner is
//! rendered. Never `fail`: the standard is pending and this reports readiness.
//!
//! Does not decide: what the expanded panel looks like or whether it opens
//! (expansion is not tested), the wording beyond the two substrings, colours,
//! or whether the flag image shows a flag.

use serde_json::{json, Value};

use super::fed_common::{block_with_phrase, outermost_by_class, raw_text, with_ax};
use super::{clip, finding, Ctx};
use crate::capture_view::CaptureView;
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-FED-BANNER",
    act: &[],
    informs: &["FED-32"],
    summary: "The rendered page shows the official-website banner as its first block",
    channel: "browser",
    never_fails: true,
    absent_reason: "The capture has no baseline snapshot.",
};

const PHRASE: &str = "an official website of the united states government";
const MAX_TOP: f64 = 120.0;
const MIN_WIDTH_SHARE: f64 = 0.8;

fn find_banner(v: &CaptureView<'_>) -> Option<(usize, &'static str)> {
    let by_class = outermost_by_class(v, "usa-banner");
    if let Some(first) = by_class
        .iter()
        .copied()
        .find(|e| v.visible(*e))
        .or_else(|| by_class.first().copied())
    {
        return Some((first, "class"));
    }
    block_with_phrase(v, None, PHRASE).map(|b| (b, "text"))
}

fn has_name(v: &CaptureView<'_>, el: usize) -> bool {
    if v.ax_name(el).is_some_and(|n| !n.trim().is_empty()) {
        return true;
    }
    if v.attr(el, "aria-label")
        .is_some_and(|n| !n.trim().is_empty())
        || v.attr(el, "aria-labelledby")
            .is_some_and(|n| !n.trim().is_empty())
    {
        return true;
    }
    // An inline svg may name itself with a title child.
    v.children(el)
        .iter()
        .any(|c| v.is_element(*c) && v.tag(*c) == "title" && !raw_text(v, *c).is_empty())
}

fn flag_image(v: &CaptureView<'_>, banner: usize) -> bool {
    v.descendants(banner).into_iter().any(|d| {
        if !v.is_element(d) || !v.visible(d) {
            return false;
        }
        let role_img = v.attr(d, "role").is_some_and(|r| {
            r.split_ascii_whitespace()
                .next()
                .is_some_and(|t| t.eq_ignore_ascii_case("img"))
        });
        match v.tag(d) {
            "img" => v.attr(d, "alt").is_some_and(|a| !a.trim().is_empty()),
            "svg" => has_name(v, d),
            _ => role_img && has_name(v, d),
        }
    })
}

fn toggle_with_gov_https(v: &CaptureView<'_>, banner: usize) -> bool {
    v.descendants(banner).into_iter().any(|d| {
        if !v.is_element(d) || !v.visible(d) || !v.has_attr(d, "aria-expanded") {
            return false;
        }
        let Some(controls) = v.attr(d, "aria-controls") else {
            return false;
        };
        controls.split_ascii_whitespace().any(|id| {
            v.by_id(id).iter().any(|target| {
                let text = raw_text(v, *target).to_lowercase();
                text.contains(".gov") && text.contains("https")
            })
        })
    })
}

/// A visible, non-skip-link element above `y` that carries content, if any.
fn content_above(v: &CaptureView<'_>, banner: usize, y: f64) -> Option<usize> {
    v.elements().find(|e| {
        let e = *e;
        if e == banner || v.is_ancestor(e, banner) || v.is_ancestor(banner, e) || !v.visible(e) {
            return false;
        }
        if matches!(v.tag(e), "html" | "body") {
            return false;
        }
        let Some(b) = v.bbox(e) else { return false };
        // Entirely off the page, such as a skip link parked above the viewport.
        if b.y + b.height <= 0.0 || b.x + b.width <= 0.0 || b.y + 1.0 >= y {
            return false;
        }
        let skip_link = v.self_and_ancestors(e).any(|n| {
            v.is_element(n)
                && v.tag(n) == "a"
                && v.attr(n, "href").is_some_and(|h| h.trim().starts_with('#'))
        });
        if skip_link {
            return false;
        }
        let replaced = matches!(
            v.tag(e),
            "img" | "svg" | "video" | "canvas" | "input" | "button" | "select" | "textarea"
        );
        let own_text = v.children(e).iter().any(|c| {
            v.is_text(*c)
                && v.node(*c)
                    .text
                    .as_deref()
                    .is_some_and(|t| !t.trim().is_empty())
                && v.text_visible(*c)
        });
        replaced || own_text
    })
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let expected = "the banner is visible, first, full width, has a named flag image and an expandable .gov and https explanation";
    let limit = "Pending GSA standard: a pass means the acceptance criteria that markup and layout can show held. Whether the panel expands, and how it looks, is not tested.";
    let Some((banner, how)) = find_banner(v) else {
        return vec![with_ax(
            finding(
                &INFO,
                v,
                "html".to_string(),
                State::NeedsReview,
                expected,
                json!({ "banner": null, "reason": "no banner rendered" }),
            ),
            v,
        )
        .limit(limit)];
    };
    let bbox = v.bbox(banner);
    let visible = v.visible(banner);
    let top = bbox.map(|b| b.y);
    let width = bbox.map(|b| b.width);
    let viewport_width = f64::from(v.viewport.width);
    let near_top = top.is_some_and(|y| y < MAX_TOP);
    let above = if near_top {
        content_above(v, banner, top.unwrap_or(0.0))
    } else {
        None
    };
    let first_block = visible && near_top && above.is_none();
    let full_width = width.is_some_and(|w| w >= MIN_WIDTH_SHARE * viewport_width);
    let flag = flag_image(v, banner);
    let toggle = toggle_with_gov_https(v, banner);
    let criteria: [(&str, bool); 5] = [
        ("visible", visible),
        ("first_visible_block", first_block),
        ("full_width", full_width),
        ("flag_image_with_name", flag),
        ("toggle_with_gov_and_https", toggle),
    ];
    let missing: Vec<&str> = criteria
        .iter()
        .filter(|(_, ok)| !ok)
        .map(|(n, _)| *n)
        .collect();
    let criteria_map: serde_json::Map<String, Value> = criteria
        .iter()
        .map(|(n, ok)| ((*n).to_string(), json!(ok)))
        .collect();
    let mut observed: Value = json!({
        "found_by": how,
        "y": top,
        "width": width,
        "viewport_width": viewport_width,
        "content_above": above.map(|e| v.selector(e)),
        "criteria": criteria_map,
        "missing": missing,
    });
    if let Some(text) = block_text(v, banner) {
        observed["text"] = json!(clip(&text, 160));
    }
    let state = if missing.is_empty() {
        State::Pass
    } else {
        State::NeedsReview
    };
    vec![with_ax(
        finding(&INFO, v, v.selector(banner), state, expected, observed),
        v,
    )
    .limit(limit)]
}

fn block_text(v: &CaptureView<'_>, banner: usize) -> Option<String> {
    let t = v.visible_text(banner);
    (!t.is_empty()).then_some(t)
}
