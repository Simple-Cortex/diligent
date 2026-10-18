//! Craft signals: type size, measure, fonts, spacing, radii, headings, colour margin and motion.
//!
//! These read computed styles and layout boxes of one rendered page. They are
//! practice readings against cited guidance, not rules. User-agent default
//! margins count like authored ones, so an unstyled page reads off the grid.

use std::collections::{BTreeSet, HashSet};

use serde_json::json;

use super::model::{Signal, SignalState};
use super::{round_to, Scope};
use crate::capture_view::{parse_px, CaptureView, BASELINE};
use crate::checks::fed_common::is_block;
use crate::checks::{clip, Ctx};
use crate::color::{composite_over, parse_css_color, Color};
use crate::contrast::{classify_text, contrast_ratio, required_ratio, ContrastLevel};

const REDUCED_MOTION: &str = "reduced-motion";

/// Runs the craft signals.
pub fn run(ctx: &Ctx<'_>) -> Vec<Signal> {
    let Some(v) = ctx.primary.as_ref() else {
        return Vec::new();
    };
    let scope = Scope::new(v);
    let mut out: Vec<Signal> = Vec::new();
    out.extend(body_font_size(v, &scope));
    out.extend(line_length(v, &scope));
    out.extend(font_families(v));
    out.extend(spacing_grid(v));
    out.extend(radii(v));
    out.extend(heading_skips(v));
    out.extend(contrast_margin(v));
    out.extend(reduced_motion(ctx, v));
    out.extend(svg_icons(v));
    out
}

fn font_px(v: &CaptureView<'_>, el: usize) -> Option<f64> {
    v.inherited_style(el, "font-size").and_then(parse_px)
}

fn body_font_size(v: &CaptureView<'_>, scope: &Scope<'_>) -> Option<Signal> {
    let mut floor: Option<(f64, usize)> = None;
    for t in scope.text_nodes() {
        if v.node(t)
            .text
            .as_deref()
            .is_none_or(|s| s.trim().chars().count() < 20)
        {
            continue;
        }
        let Some(p) = v.parent_element(t) else {
            continue;
        };
        let Some(px) = font_px(v, p) else { continue };
        if floor.is_none_or(|(f, _)| px < f) {
            floor = Some((px, p));
        }
    }
    let (px, el) = floor?;
    Some(
        Signal::new(
            "craft.body_font_size_floor",
            if px < 16.0 { SignalState::Lead } else { SignalState::WithinPractice },
            json!(round_to(px, 2)),
            format!("smallest body text is {px:.1} px"),
        )
        .unit("px")
        .at_instance(v.selector(el))
        .limit("Only text nodes of 20 characters or more in the main text count, so small print and labels are not read.")
        .on(v),
    )
}

fn line_length(v: &CaptureView<'_>, scope: &Scope<'_>) -> Option<Signal> {
    let mut lengths: Vec<f64> = Vec::new();
    for el in v
        .elements()
        .filter(|e| matches!(v.tag(*e), "p" | "li") && v.visible(*e))
    {
        if !v
            .descendants(el)
            .into_iter()
            .chain(std::iter::once(el))
            .any(|n| scope.contains(n))
        {
            continue;
        }
        if v.visible_text(el).chars().count() < 80 {
            continue;
        }
        let (Some(b), Some(px)) = (v.bbox(el), font_px(v, el)) else {
            continue;
        };
        if px > 0.0 {
            lengths.push(b.width / (0.5 * px));
        }
    }
    if lengths.is_empty() {
        return None;
    }
    lengths.sort_by(f64::total_cmp);
    let idx = ((lengths.len() as f64 * 0.9).ceil() as usize).clamp(1, lengths.len()) - 1;
    let p90 = lengths[idx];
    Some(
        Signal::new(
            "craft.line_length_ch",
            if p90 > 80.0 { SignalState::Lead } else { SignalState::WithinPractice },
            json!(round_to(p90, 1)),
            format!("ninetieth percentile line length is about {p90:.0} ch over {} blocks", lengths.len()),
        )
        .unit("ch")
        .limit("Approximation: box width divided by half the font size, since the width of the 0 glyph is not captured. Padding is included in the width.")
        .limit("Measured at the widest recorded viewport on paragraphs and list items as they render. A page that sets no max-width reads long by construction, so a lead is a prompt to set a measure, not a defect count.")
        .on(v),
    )
}

fn first_family(value: &str) -> String {
    value
        .split(',')
        .next()
        .unwrap_or("")
        .trim()
        .trim_matches(|c| c == '"' || c == '\'')
        .to_lowercase()
}

fn font_families(v: &CaptureView<'_>) -> Option<Signal> {
    let mut families: BTreeSet<String> = BTreeSet::new();
    for t in 0..v.len() {
        if !v.is_text(t)
            || v.node(t)
                .text
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
            || !v.text_visible(t)
        {
            continue;
        }
        if let Some(f) = v.inherited_style(t, "font-family") {
            let name = first_family(f);
            if !name.is_empty() {
                families.insert(name);
            }
        }
    }
    if families.is_empty() {
        return None;
    }
    Some(
        Signal::new(
            "craft.font_family_count",
            if families.len() > 3 { SignalState::Lead } else { SignalState::WithinPractice },
            json!({ "count": families.len(), "families": families }),
            format!("{} distinct first font families", families.len()),
        )
        .unit("families")
        .limit("Counts the first family of each declaration; fallback fonts and icon fonts are not told apart.")
        .on(v),
    )
}

const SPACING: [&str; 8] = [
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
];

fn spacing_grid(v: &CaptureView<'_>) -> Option<Signal> {
    let (mut total, mut on_grid) = (0usize, 0usize);
    for el in v.elements().filter(|e| v.visible(*e) && is_block(v, *e)) {
        for prop in SPACING {
            let Some(px) = v.style(el, prop).and_then(parse_px) else {
                continue;
            };
            let px = px.abs();
            if px < 0.01 {
                continue;
            }
            total += 1;
            let q = px / 4.0;
            if (q - q.round()).abs() < 0.01 {
                on_grid += 1;
            }
        }
    }
    if total == 0 {
        return None;
    }
    let share = on_grid as f64 / total as f64;
    Some(
        Signal::new(
            "craft.spacing_grid_share",
            if share < 0.80 { SignalState::Lead } else { SignalState::WithinPractice },
            json!(round_to(share, 3)),
            format!("{on_grid} of {total} non-zero margins and paddings are multiples of 4 px"),
        )
        .unit("share")
        .limit("User-agent default margins, such as heading margins in em, count like authored values. Computed px values are read, so calc() is already resolved.")
        .on(v),
    )
}

fn radii(v: &CaptureView<'_>) -> Option<Signal> {
    let props = [
        "border-top-left-radius",
        "border-top-right-radius",
        "border-bottom-right-radius",
        "border-bottom-left-radius",
    ];
    let mut distinct: BTreeSet<String> = BTreeSet::new();
    let mut seen_any = false;
    for el in v.elements().filter(|e| v.visible(*e)) {
        for p in props {
            let Some(value) = v.style(el, p).map(str::trim) else {
                continue;
            };
            seen_any = true;
            let zero = value
                .split_whitespace()
                .all(|part| parse_px(part).is_some_and(|x| x.abs() < 0.01) || part == "0");
            if !value.is_empty() && !zero {
                distinct.insert(value.to_string());
            }
        }
    }
    if !seen_any {
        return None;
    }
    Some(
        Signal::new(
            "craft.border_radius_distinct",
            if distinct.len() > 4 {
                SignalState::Lead
            } else {
                SignalState::WithinPractice
            },
            json!({ "count": distinct.len(), "radii": distinct }),
            format!("{} distinct non-zero border radii", distinct.len()),
        )
        .unit("radii")
        .limit(
            "Radii are compared as computed strings, so 50% and 9999px count as different values.",
        )
        .on(v),
    )
}

fn heading_level(v: &CaptureView<'_>, el: usize) -> Option<u8> {
    match v.tag(el) {
        "h1" => return Some(1),
        "h2" => return Some(2),
        "h3" => return Some(3),
        "h4" => return Some(4),
        "h5" => return Some(5),
        "h6" => return Some(6),
        _ => {}
    }
    if v.attr(el, "role")
        .is_some_and(|r| r.split_ascii_whitespace().next() == Some("heading"))
    {
        return v
            .attr(el, "aria-level")
            .and_then(|l| l.trim().parse::<u8>().ok())
            .or(Some(2));
    }
    None
}

fn heading_skips(v: &CaptureView<'_>) -> Option<Signal> {
    let levels: Vec<(usize, u8)> = v
        .elements()
        .filter(|e| v.visible(*e))
        .filter_map(|e| heading_level(v, e).map(|l| (e, l)))
        .collect();
    if levels.is_empty() {
        return None;
    }
    let mut skips: Vec<serde_json::Value> = Vec::new();
    let mut first_at: Option<usize> = None;
    for pair in levels.windows(2) {
        if pair[1].1 > pair[0].1 + 1 {
            first_at.get_or_insert(pair[1].0);
            skips.push(
                json!({ "from": pair[0].1, "to": pair[1].1, "selector": v.selector(pair[1].0) }),
            );
        }
    }
    let mut s = Signal::new(
        "craft.heading_hierarchy_skips",
        if skips.is_empty() {
            SignalState::WithinPractice
        } else {
            SignalState::Lead
        },
        json!({ "count": skips.len(), "skips": skips.iter().take(10).collect::<Vec<_>>() }),
        format!(
            "{} heading level jumps greater than one over {} headings",
            skips.len(),
            levels.len()
        ),
    )
    .unit("skips")
    .limit("Document order only; a page may start below h1 without this being counted.")
    .on(v);
    if let Some(at) = first_at {
        s = s.at_instance(v.selector(at));
    }
    Some(s)
}

fn font_weight(value: &str) -> Option<f64> {
    match value.trim() {
        "normal" => Some(400.0),
        "bold" | "bolder" => Some(700.0),
        "lighter" => Some(300.0),
        other => other.parse::<f64>().ok(),
    }
}

fn contrast_margin(v: &CaptureView<'_>) -> Option<Signal> {
    let mut seen: HashSet<usize> = HashSet::new();
    let mut lowest: Option<(f64, f64, usize)> = None;
    let mut fragile = 0usize;
    let mut measured = 0usize;
    for t in 0..v.len() {
        if !v.is_text(t)
            || v.node(t)
                .text
                .as_deref()
                .is_none_or(|s| s.trim().is_empty())
            || !v.text_visible(t)
        {
            continue;
        }
        let Some(p) = v.parent_element(t) else {
            continue;
        };
        if v.in_aria_hidden(p) || v.is_disabled(p) || !seen.insert(p) {
            continue;
        }
        let Some(fg) = v.inherited_style(p, "color").and_then(parse_css_color) else {
            continue;
        };
        let Ok(bg) = v.effective_background(p) else {
            continue;
        };
        let fg = match fg {
            Color::Opaque(c) => c,
            Color::Translucent(a) => composite_over(a, bg),
        };
        let Some(px) = font_px(v, p) else { continue };
        let Some(weight) = v.inherited_style(p, "font-weight").and_then(font_weight) else {
            continue;
        };
        let required = required_ratio(classify_text(px, weight >= 700.0), ContrastLevel::Aa);
        let margin = contrast_ratio(fg, bg) / required;
        if margin < 1.0 {
            continue;
        }
        measured += 1;
        if margin <= 1.10 {
            fragile += 1;
        }
        if lowest.is_none_or(|(m, _, _)| margin < m) {
            lowest = Some((margin, contrast_ratio(fg, bg), p));
        }
    }
    let (margin, ratio, el) = lowest?;
    Some(
        Signal::new(
            "craft.contrast_margin",
            if fragile > 0 { SignalState::Lead } else { SignalState::WithinPractice },
            json!({ "min_margin": round_to(margin, 3), "ratio": round_to(ratio, 2), "fragile_pairs": fragile, "pairs": measured }),
            format!("smallest margin above the AA ratio is {margin:.3} ({ratio:.2}:1); {fragile} of {measured} pairs within 10%"),
        )
        .unit("ratio of required")
        .at_instance(v.selector(el))
        .limit("Pairs below the AA ratio are failures of BRW-TEXT-CONTRAST and are not repeated here. Text over images and gradients is not resolved.")
        .on(v),
    )
}

fn seconds(list: &str) -> Vec<f64> {
    list.split(',')
        .filter_map(|part| {
            let p = part.trim();
            if let Some(ms) = p.strip_suffix("ms") {
                ms.trim().parse::<f64>().ok().map(|x| x / 1000.0)
            } else {
                p.strip_suffix('s')
                    .and_then(|s| s.trim().parse::<f64>().ok())
            }
        })
        .collect()
}

fn reduced_motion(ctx: &Ctx<'_>, base: &CaptureView<'_>) -> Option<Signal> {
    let rm = CaptureView::new(ctx.set, base.viewport, REDUCED_MOTION)?;
    if base.media != BASELINE {
        return None;
    }
    let mut moving: Vec<usize> = Vec::new();
    for el in rm.elements().filter(|e| rm.visible(*e)) {
        let animated = rm
            .style(el, "animation-name")
            .map(str::trim)
            .is_some_and(|n| !n.is_empty() && n != "none");
        let transitions = rm
            .style(el, "transition-duration")
            .is_some_and(|d| seconds(d).iter().any(|s| *s > 0.0));
        if animated || transitions {
            moving.push(el);
        }
    }
    let mut s = Signal::new(
        "craft.reduced_motion_persists",
        if moving.is_empty() { SignalState::WithinPractice } else { SignalState::Lead },
        json!({
            "count": moving.len(),
            "elements": moving.iter().take(10).map(|e| clip(&rm.selector(*e), 120)).collect::<Vec<_>>(),
        }),
        format!("{} elements still animate or transition with prefers-reduced-motion: reduce", moving.len()),
    )
    .unit("elements")
    .limit("Reads computed animation and transition properties of one snapshot; it does not say whether the motion is essential or large.")
    .on(base)
    .with(rm.key("dom_snapshot"));
    if let Some(first) = moving.first() {
        s = s.at_instance(rm.selector(*first));
    }
    Some(s)
}

fn svg_named(v: &CaptureView<'_>, el: usize) -> bool {
    v.ax_name(el).is_some_and(|n| !n.trim().is_empty())
        || v.attr(el, "aria-label")
            .is_some_and(|n| !n.trim().is_empty())
        || v.attr(el, "aria-labelledby")
            .is_some_and(|n| !n.trim().is_empty())
        || v.children(el).iter().any(|c| {
            v.is_element(*c)
                && v.tag(*c) == "title"
                && v.children(*c).iter().any(|t| {
                    v.node(*t)
                        .text
                        .as_deref()
                        .is_some_and(|s| !s.trim().is_empty())
                })
        })
}

fn svg_icons(v: &CaptureView<'_>) -> Option<Signal> {
    let svgs: Vec<usize> = v
        .elements()
        .filter(|e| v.tag(*e) == "svg" && v.visible(*e))
        .collect();
    if svgs.is_empty() {
        return None;
    }
    let unnamed: Vec<usize> = svgs
        .iter()
        .copied()
        .filter(|e| !v.in_aria_hidden(*e) && !svg_named(v, *e))
        .collect();
    Some(
        Signal::new(
            "craft.svg_icons_unnamed",
            SignalState::Informational,
            json!({
                "count": unnamed.len(),
                "svgs": svgs.len(),
                "instances": unnamed.iter().take(10).map(|e| v.selector(*e)).collect::<Vec<_>>(),
            }),
            format!("{} of {} visible inline svg have no name and are not hidden from assistive technology", unnamed.len(), svgs.len()),
        )
        .unit("svg")
        .limit("An unnamed svg is fine when it is decorative and its container is named; this counts, it does not judge.")
        .on(v),
    )
}
