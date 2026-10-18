//! `BRW-TEXT-SPACING-STYLE` (WCAG 1.4.12; ACT 24afc2, 78fd32, 9e45ec).
//!
//! Two parts.
//!
//! Style attributes, in the baseline capture. For each element with visible
//! text and a `style` attribute declaring `letter-spacing`, `word-spacing` or
//! `line-height`:
//!
//! * `pass` when no declaration is `!important`, or each `!important` one
//!   already meets the WCAG 1.4.12 metric (line height 1.5, letter spacing
//!   0.12, word spacing 0.16, all times the font size);
//! * `pass` when an `!important` value is `inherit` or `unset`, which does not
//!   lock a value;
//! * `fail` when an `!important` value is below the metric, or is `normal`,
//!   `initial`, `revert` or an expression this check cannot evaluate: the author locked the property
//!   and nothing proves the metric is met.
//!
//! Elements that are not rendered, sit entirely above or left of the page
//! origin, or sit inside a container with `overflow: scroll` are not examined.
//!
//! Text-spacing capture, when the collector ran `--media text-spacing`. For
//! each element with visible text whose overflow is not `visible`:
//!
//! * `needs_review` when the element clips (`hidden` or `clip`) on an axis and
//!   text extends past its box on that axis, so text may be clipped;
//! * `pass` otherwise, including scrollable (`auto`, `scroll`) overflow.
//!
//! Does not decide: spacing set in stylesheets, text clipped by something
//! other than the element's own overflow, or reflow of text in other elements.

use serde_json::json;

use super::{finding, Ctx};
use crate::capture_view::{parse_px, CaptureView};
use crate::model::{CheckInfo, Finding, State};

/// Check description.
pub const INFO: CheckInfo = CheckInfo {
    id: "BRW-TEXT-SPACING-STYLE",
    act: &["24afc2", "78fd32", "9e45ec"],
    informs: &["A11Y-1.4.12"],
    summary: "Authors do not lock text spacing below the WCAG 1.4.12 metrics, and text is not clipped when spacing grows",
    channel: "browser",
    never_fails: false,
    absent_reason: "No style attribute sets letter-spacing, word-spacing or line-height, and no text-spacing capture exists.",
};

struct Declaration {
    property: String,
    value: String,
    important: bool,
}

/// Splits a `style` attribute into declarations, respecting quotes and parentheses.
fn declarations(style: &str) -> Vec<Declaration> {
    let mut pieces: Vec<String> = Vec::new();
    let mut current = String::new();
    let mut depth = 0usize;
    let mut quote: Option<char> = None;
    for c in style.chars() {
        match quote {
            Some(q) => {
                current.push(c);
                if c == q {
                    quote = None;
                }
            }
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    current.push(c);
                }
                '(' => {
                    depth += 1;
                    current.push(c);
                }
                ')' => {
                    depth = depth.saturating_sub(1);
                    current.push(c);
                }
                ';' if depth == 0 => pieces.push(std::mem::take(&mut current)),
                _ => current.push(c),
            },
        }
    }
    pieces.push(current);
    pieces
        .into_iter()
        .filter_map(|piece| {
            let (name, value) = piece.split_once(':')?;
            let mut value = value.trim().to_string();
            let mut important = false;
            let lower = value.to_ascii_lowercase();
            if let Some(pos) = lower.rfind('!') {
                if lower[pos + 1..].trim() == "important" {
                    important = true;
                    value.truncate(pos);
                    value = value.trim().to_string();
                }
            }
            Some(Declaration {
                property: name.trim().to_ascii_lowercase(),
                value,
                important,
            })
        })
        .collect()
}

/// Converts a declared value to pixels, or `None` when it cannot be evaluated.
fn to_px(property: &str, value: &str, font_px: f64, root_px: f64) -> Option<f64> {
    let v = value.trim().to_ascii_lowercase();
    let number = |suffix: &str| {
        v.strip_suffix(suffix)
            .and_then(|n| n.trim().parse::<f64>().ok())
    };
    let px = if let Some(n) = number("px") {
        n
    } else if let Some(n) = number("rem") {
        n * root_px
    } else if let Some(n) = number("em") {
        n * font_px
    } else if let Some(n) = number("pt") {
        n * 96.0 / 72.0
    } else if let Some(n) = number("%") {
        if property == "line-height" {
            n / 100.0 * font_px
        } else {
            return None;
        }
    } else if let Ok(n) = v.parse::<f64>() {
        if property == "line-height" {
            n * font_px
        } else if n == 0.0 {
            0.0
        } else {
            return None;
        }
    } else {
        return None;
    };
    px.is_finite().then_some(px)
}

fn metric(property: &str) -> Option<f64> {
    match property {
        "line-height" => Some(1.5),
        "letter-spacing" => Some(0.12),
        "word-spacing" => Some(0.16),
        _ => None,
    }
}

fn style_attributes(v: &CaptureView<'_>, out: &mut Vec<Finding>) {
    let root_px = v
        .elements()
        .find(|e| v.tag(*e) == "html")
        .and_then(|e| v.style(e, "font-size"))
        .and_then(parse_px)
        .unwrap_or(16.0);
    for el in v.elements() {
        let Some(style) = v.attr(el, "style") else {
            continue;
        };
        // The last declaration of a property wins, so earlier duplicates are dropped.
        let mut decls: Vec<Declaration> = Vec::new();
        for d in declarations(style)
            .into_iter()
            .filter(|d| metric(&d.property).is_some())
        {
            decls.retain(|e| e.property != d.property);
            decls.push(d);
        }
        if decls.is_empty() || !v.visible(el) || v.visible_text_nodes(el).is_empty() {
            continue;
        }
        if v.bbox(el)
            .is_some_and(|b| b.x + b.width <= 0.0 || b.y + b.height <= 0.0)
        {
            continue;
        }
        let scrolls = |a: usize| {
            v.is_element(a)
                && ["overflow-x", "overflow-y"]
                    .iter()
                    .any(|p| v.style(a, p).map(str::trim) == Some("scroll"))
        };
        if v.ancestors(el).any(scrolls) {
            continue;
        }
        let font_px = v.style(el, "font-size").and_then(parse_px).unwrap_or(16.0);
        let mut state = State::Pass;
        let mut rows = Vec::new();
        for d in &decls {
            let ratio = metric(&d.property).unwrap_or(0.0);
            let needed = ratio * font_px;
            let inherits = matches!(
                d.value.trim().to_ascii_lowercase().as_str(),
                "inherit" | "unset"
            );
            let (s, note) = if !d.important {
                (State::Pass, None)
            } else if inherits {
                (
                    State::Pass,
                    Some("inherit or unset; it does not lock a value"),
                )
            } else {
                match to_px(&d.property, &d.value, font_px, root_px) {
                    Some(px) if px + 0.005 >= needed => (State::Pass, None),
                    Some(_) => (State::Fail, Some("below the 1.4.12 metric")),
                    None => (
                        State::Fail,
                        Some("locks the property to a value that is not provably at the metric"),
                    ),
                }
            };
            state = state.worst(s);
            rows.push(json!({
                "property": d.property,
                "value": d.value,
                "important": d.important,
                "required_px": (needed * 100.0).round() / 100.0,
                "state": s.as_str(),
                "note": note,
            }));
        }
        out.push(
            finding(
                &INFO,
                v,
                v.selector(el),
                state,
                "an !important spacing declaration is at least the 1.4.12 metric, or absent",
                json!({ "font_size_px": font_px, "declarations": rows }),
            )
            .limit("Only style attributes are read. Spacing set in stylesheets is not examined, and the metric uses the element's own font size."),
        );
    }
}

fn exceeds(inner: (f64, f64), outer: (f64, f64)) -> bool {
    inner.0 < outer.0 - 1.0 || inner.1 > outer.1 + 1.0
}

fn clipping(s: &CaptureView<'_>, out: &mut Vec<Finding>) {
    for el in s.elements() {
        if matches!(s.tag(el), "html" | "body") || !s.rendered(el) {
            continue;
        }
        let axis = |prop: &str| {
            s.style(el, prop)
                .map(|x| x.trim().to_string())
                .unwrap_or_default()
        };
        let (ox, oy) = (axis("overflow-x"), axis("overflow-y"));
        let non_visible = |o: &str| matches!(o, "hidden" | "clip" | "auto" | "scroll");
        if !non_visible(&ox) && !non_visible(&oy) {
            continue;
        }
        let Some(b) = s.bbox(el) else { continue };
        let texts = s.visible_text_nodes(el);
        if texts.is_empty() {
            continue;
        }
        let clips = |o: &str| matches!(o, "hidden" | "clip");
        let mut over_x = false;
        let mut over_y = false;
        for t in texts {
            let boxed = s.bbox(t).or_else(|| {
                s.parent_element(t)
                    .filter(|p| *p != el)
                    .and_then(|p| s.bbox(p))
            });
            let Some(tb) = boxed else { continue };
            over_x |= exceeds((tb.x, tb.x + tb.width), (b.x, b.x + b.width));
            over_y |= exceeds((tb.y, tb.y + tb.height), (b.y, b.y + b.height));
        }
        let clipped_x = clips(&ox) && over_x;
        let clipped_y = clips(&oy) && over_y;
        let (state, note) = if clipped_x || clipped_y {
            (
                State::NeedsReview,
                Some("text may be clipped under spacing overrides"),
            )
        } else {
            (State::Pass, None)
        };
        out.push(
            finding(
                &INFO,
                s,
                s.selector(el),
                state,
                "text stays inside the element when WCAG 1.4.12 spacing is applied",
                json!({
                    "overflow_x": ox,
                    "overflow_y": oy,
                    "text_extends_past_box_x": over_x,
                    "text_extends_past_box_y": over_y,
                    "note": note,
                }),
            )
            .limit("Text extent comes from layout boxes in the text-spacing capture. Clipping by ancestors or by the viewport is not examined."),
        );
    }
}

/// Runs the check.
pub fn run(ctx: &Ctx<'_>) -> Vec<Finding> {
    let mut out = Vec::new();
    if let Some(v) = ctx.primary.as_ref() {
        style_attributes(v, &mut out);
    }
    if let Some(s) = ctx.spacing.as_ref() {
        clipping(s, &mut out);
    }
    out
}
