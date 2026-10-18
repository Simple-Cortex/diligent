//! Helpers shared by the `BRW-FED-*` checks. Not a check itself.
//!
//! The federal checks all read the rendered page the same way: which controls
//! are links, which are visible, where the identifier and the footer are, and
//! whether the page is the home page. Keeping those answers in one place keeps
//! the checks consistent with each other.

use std::collections::HashSet;

use diligent_capture::CaptureSet;
use serde_json::{json, Value};
use url::Url;

use super::{clip, finding};
use crate::capture_view::{normalize_space, CaptureView};
use crate::model::{CheckInfo, EvidenceKey, Finding, State};

/// True when the view's document path is `/` or `/index.html`.
pub fn is_home(v: &CaptureView<'_>) -> bool {
    match Url::parse(&v.dom.document_url) {
        Ok(u) => {
            let path = u.path();
            path.is_empty() || path == "/" || path.eq_ignore_ascii_case("/index.html")
        }
        Err(_) => false,
    }
}

/// Adds the accessibility tree to the evidence when the view has one.
pub fn with_ax(f: Finding, v: &CaptureView<'_>) -> Finding {
    if v.ax.is_some() {
        f.with(v.key("ax_tree"))
    } else {
        f
    }
}

/// Evidence key of the network log, when the capture has one.
pub fn network_key(set: &CaptureSet) -> Option<EvidenceKey> {
    set.entries()
        .iter()
        .find(|e| e.kind == "network_log")
        .map(|e| EvidenceKey {
            kind: "network_log",
            viewport: e.viewport,
            media: None,
        })
}

/// Text of all text nodes under `el`, rendered or not, whitespace collapsed.
/// Script and style text is left out.
pub fn raw_text(v: &CaptureView<'_>, el: usize) -> String {
    let mut out = String::new();
    for d in v.descendants(el) {
        if !v.is_text(d) {
            continue;
        }
        if v.parent_element(d)
            .is_some_and(|p| matches!(v.tag(p), "script" | "style"))
        {
            continue;
        }
        if let Some(t) = v.node(d).text.as_deref() {
            if !t.trim().is_empty() {
                out.push_str(t);
                out.push(' ');
            }
        }
    }
    normalize_space(&out)
}

/// True when the element's `class` attribute contains `needle`, ignoring case.
pub fn class_contains(v: &CaptureView<'_>, el: usize, needle: &str) -> bool {
    v.attr(el, "class")
        .is_some_and(|c| c.to_lowercase().contains(needle))
}

/// Elements whose class contains `needle` and that have no such ancestor, in document order.
pub fn outermost_by_class(v: &CaptureView<'_>, needle: &str) -> Vec<usize> {
    let all: Vec<usize> = v
        .elements()
        .filter(|e| class_contains(v, *e, needle))
        .collect();
    let set: HashSet<usize> = all.iter().copied().collect();
    all.into_iter()
        .filter(|e| !v.ancestors(*e).any(|a| set.contains(&a)))
        .collect()
}

/// True for elements that start a new block: laid out and not inline.
pub fn is_block(v: &CaptureView<'_>, el: usize) -> bool {
    match v.style(el, "display").map(str::trim) {
        None => false,
        Some(d) => {
            !(d.starts_with("inline") || d.starts_with("ruby") || d == "contents" || d == "none")
        }
    }
}

fn squash(s: &str) -> String {
    s.chars()
        .filter(|c| !c.is_whitespace())
        .flat_map(char::to_lowercase)
        .collect()
}

/// True when `text` contains `phrase`, ignoring case and all whitespace.
///
/// Whitespace is ignored because the visible text of two inline elements
/// separated only by a space node is joined without one.
pub fn contains_phrase(text: &str, phrase: &str) -> bool {
    let want = squash(phrase);
    !want.is_empty() && squash(text).contains(&want)
}

/// The text after the first occurrence of `phrase`, ignoring case and whitespace in the match.
pub fn text_after_phrase<'t>(text: &'t str, phrase: &str) -> Option<&'t str> {
    let want: Vec<char> = squash(phrase).chars().collect();
    if want.is_empty() {
        return None;
    }
    let mut hay: Vec<(char, usize)> = Vec::new();
    for (i, c) in text.char_indices() {
        if c.is_whitespace() {
            continue;
        }
        let end = i + c.len_utf8();
        for lc in c.to_lowercase() {
            hay.push((lc, end));
        }
    }
    if hay.len() < want.len() {
        return None;
    }
    for s in 0..=hay.len() - want.len() {
        if hay[s..s + want.len()]
            .iter()
            .map(|(c, _)| *c)
            .eq(want.iter().copied())
        {
            let end = hay[s + want.len() - 1].1;
            return text.get(end..).map(str::trim);
        }
    }
    None
}

/// Most text nodes taken as starting points, and most ancestors climbed from each.
const MAX_SEEDS: usize = 64;
const MAX_HOPS: usize = 6;

/// The deepest block element that contains `phrase` in its visible text.
///
/// Finds text nodes that hold one of the rarer words of the phrase, then climbs
/// at most a few ancestors from each to the first block whose whole visible text
/// holds the phrase. This finds a phrase split across inline elements without
/// computing the text of every block on the page. With `scope`, only that
/// element and its descendants are searched.
pub fn block_with_phrase(v: &CaptureView<'_>, scope: Option<usize>, phrase: &str) -> Option<usize> {
    let words: Vec<String> = phrase
        .split_whitespace()
        .filter(|w| w.chars().count() >= 6)
        .map(str::to_lowercase)
        .collect();
    if words.is_empty() {
        return None;
    }
    let pool: Vec<usize> = match scope {
        Some(s) => v.descendants(s),
        None => (0..v.len()).collect(),
    };
    let seeds: Vec<usize> = pool
        .into_iter()
        .filter(|t| {
            v.is_text(*t)
                && v.node(*t).text.as_deref().is_some_and(|x| {
                    let low = x.to_lowercase();
                    words.iter().any(|w| low.contains(w.as_str()))
                })
                && v.text_visible(*t)
        })
        .take(MAX_SEEDS)
        .collect();
    let mut best: Option<usize> = None;
    for seed in seeds {
        for (hops, anc) in v.ancestors(seed).enumerate() {
            if hops >= MAX_HOPS || !v.is_element(anc) {
                break;
            }
            if matches!(v.tag(anc), "html" | "body" | "main") {
                break;
            }
            if let Some(s) = scope {
                if anc != s && !v.is_ancestor(s, anc) {
                    break;
                }
            }
            if is_block(v, anc) && contains_phrase(&v.visible_text(anc), phrase) {
                best = Some(best.map_or(anc, |b| b.min(anc)));
                break;
            }
        }
    }
    best
}

/// How a control's `href` reads.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Href {
    /// An absolute `http` or `https` URL after resolving against the document.
    Usable(String),
    /// Empty, `#`, `javascript:` or another scheme.
    Placeholder(String),
    /// No `href` attribute.
    Missing,
}

/// Classifies the `href` of `el`.
pub fn href_state(v: &CaptureView<'_>, el: usize) -> Href {
    let Some(raw) = v.attr(el, "href") else {
        return Href::Missing;
    };
    let t = raw.trim();
    if t.is_empty() || t == "#" || t.to_ascii_lowercase().starts_with("javascript:") {
        return Href::Placeholder(raw.to_string());
    }
    let resolved = v.resolve_url(t);
    match Url::parse(&resolved) {
        Ok(u) if matches!(u.scheme(), "http" | "https") => Href::Usable(resolved),
        _ => Href::Placeholder(raw.to_string()),
    }
}

/// What a control is, as far as navigation goes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    /// A link: an anchor with `href`, or `role="link"`.
    Link,
    /// A button, or an anchor without `href`, or `role="button"`.
    Other,
}

/// A link or button on the page.
#[derive(Debug, Clone)]
pub struct Control {
    /// DOM index.
    pub el: usize,
    /// Link or other control.
    pub kind: Kind,
    /// Accessible name, else `aria-label`, `title`, `value`, else the text.
    pub name: String,
    /// Visible text, or all text when the control is not visible.
    pub text: String,
    /// Laid out and visible.
    pub visible: bool,
}

fn name_of(v: &CaptureView<'_>, el: usize, text: &str) -> String {
    if let Some(n) = v.ax_name(el).map(str::trim).filter(|n| !n.is_empty()) {
        return n.to_string();
    }
    for attr in ["aria-label", "title", "value"] {
        if let Some(n) = v.attr(el, attr).map(str::trim).filter(|n| !n.is_empty()) {
            return normalize_space(n);
        }
    }
    text.to_string()
}

/// Every link and button of the page, rendered or not, in document order.
pub fn controls(v: &CaptureView<'_>) -> Vec<Control> {
    let mut out = Vec::new();
    for el in v.elements() {
        let tag = v.tag(el);
        let role = v
            .attr(el, "role")
            .and_then(|r| r.split_ascii_whitespace().next())
            .map(str::to_ascii_lowercase);
        let role = role.as_deref();
        let input_button = tag == "input"
            && v.attr(el, "type").is_some_and(|t| {
                matches!(
                    t.trim().to_ascii_lowercase().as_str(),
                    "button" | "submit" | "image" | "reset"
                )
            });
        let is_link = role == Some("link")
            || (matches!(tag, "a" | "area") && v.has_attr(el, "href") && role != Some("button"));
        let is_other =
            role == Some("button") || matches!(tag, "a" | "button" | "summary") || input_button;
        if !is_link && !is_other {
            continue;
        }
        let visible = v.visible(el);
        let text = if visible {
            v.visible_text(el)
        } else {
            raw_text(v, el)
        };
        out.push(Control {
            el,
            kind: if is_link { Kind::Link } else { Kind::Other },
            name: name_of(v, el, &text),
            text,
            visible,
        });
    }
    out
}

impl Control {
    /// True when `pred` holds for the accessible name or the text.
    pub fn matches(&self, pred: impl Fn(&str) -> bool) -> bool {
        pred(&self.name) || pred(&self.text)
    }
}

/// Elements that count as the footer for the privacy and accessibility links.
pub fn contentinfo(v: &CaptureView<'_>) -> Vec<usize> {
    let mut out: Vec<usize> = Vec::new();
    if let Some(tree) = v.ax {
        for n in &tree.nodes {
            if n.ignored
                || !n
                    .role
                    .as_deref()
                    .is_some_and(|r| r.eq_ignore_ascii_case("contentinfo"))
            {
                continue;
            }
            if let Some(i) = n.backend_dom_node_id.and_then(|b| v.dom_index(b)) {
                if v.is_element(i) && !out.contains(&i) {
                    out.push(i);
                }
            }
        }
    }
    for el in v.elements() {
        let landmark = v.attr(el, "role").is_some_and(|r| {
            r.split_ascii_whitespace()
                .next()
                .is_some_and(|t| t.eq_ignore_ascii_case("contentinfo"))
        });
        if landmark && !out.contains(&el) {
            out.push(el);
        }
    }
    out.sort_unstable();
    out
}

/// The identifier: `usa-identifier` elements, else the `contentinfo` landmark, else the last `footer`.
pub struct Region {
    /// Elements that make up the region.
    pub els: Vec<usize>,
    /// How it was found: `usa-identifier`, `contentinfo` or `footer`.
    pub how: &'static str,
}

/// Finds the identifier region.
pub fn identifier_region(v: &CaptureView<'_>) -> Option<Region> {
    let ids = outermost_by_class(v, "usa-identifier");
    if !ids.is_empty() {
        return Some(Region {
            els: ids,
            how: "usa-identifier",
        });
    }
    let info = contentinfo(v);
    if !info.is_empty() {
        return Some(Region {
            els: info,
            how: "contentinfo",
        });
    }
    v.elements()
        .filter(|e| v.tag(*e) == "footer")
        .last()
        .map(|e| Region {
            els: vec![e],
            how: "footer",
        })
}

/// Every element that counts as identifier, footer or `contentinfo`.
pub fn footer_regions(v: &CaptureView<'_>) -> Vec<usize> {
    let mut out = outermost_by_class(v, "usa-identifier");
    for e in contentinfo(v) {
        if !out.contains(&e) {
            out.push(e);
        }
    }
    for e in v.elements().filter(|e| v.tag(*e) == "footer") {
        if !out.contains(&e) {
            out.push(e);
        }
    }
    out
}

/// True when `el` is one of `regions` or inside one.
pub fn within(v: &CaptureView<'_>, el: usize, regions: &[usize]) -> bool {
    regions.iter().any(|r| *r == el || v.is_ancestor(*r, el))
}

/// Which link a [`required_link`] check looks for.
pub struct LinkSpec<'a> {
    /// The check.
    pub info: &'a CheckInfo,
    /// Lowercase word the name or text must contain.
    pub word: &'a str,
    /// Human label for expected and observed text, such as `privacy`.
    pub label: &'a str,
    /// Links whose `href` is only a fragment do not count.
    pub skip_fragment_only: bool,
}

/// The shared body of `BRW-FED-PRIVACY-LINK` and `BRW-FED-ACCESSIBILITY-STATEMENT`.
///
/// * `pass` when a visible link with an absolute `http(s)` target matches; its
///   location is `footer` when it sits in the identifier, a footer or the
///   `contentinfo` landmark, else `body`.
/// * `needs_review` when the only matches are placeholders, buttons, anchors
///   without `href`, or are not visible; and on pages other than the home page
///   when nothing matches.
/// * `fail` only when nothing matches at all and the page is the home page.
pub fn required_link(spec: &LinkSpec<'_>, v: &CaptureView<'_>) -> Vec<Finding> {
    let word = spec.word;
    let pred = |s: &str| s.to_lowercase().contains(word);
    let regions = footer_regions(v);
    let mut usable: Vec<(Control, String)> = Vec::new();
    let mut unusable: Vec<Control> = Vec::new();
    let mut hidden: Vec<Control> = Vec::new();
    for c in controls(v) {
        if !c.matches(pred) {
            continue;
        }
        if spec.skip_fragment_only
            && v.attr(c.el, "href")
                .is_some_and(|h| h.trim().starts_with('#'))
        {
            continue;
        }
        if !c.visible {
            hidden.push(c);
            continue;
        }
        match (c.kind, href_state(v, c.el)) {
            (Kind::Link, Href::Usable(url)) => usable.push((c, url)),
            _ => unusable.push(c),
        }
    }
    let expected = format!(
        "a visible link named for {} with a working address",
        spec.label
    );
    let make = |state: State, instance: String, observed: Value| {
        with_ax(
            finding(spec.info, v, instance, state, expected.clone(), observed),
            v,
        )
    };
    let limit = "Looks for a link by its name or text. Where the link leads, and whether the page it reaches is the right one, is not decided.";
    if !usable.is_empty() {
        let pick = usable
            .iter()
            .find(|(c, _)| within(v, c.el, &regions))
            .or_else(|| usable.first())
            .map(|(c, url)| (c.el, c.name.clone(), url.clone()));
        let (el, name, url) = pick.unwrap_or((0, String::new(), String::new()));
        let location = if within(v, el, &regions) {
            "footer"
        } else {
            "body"
        };
        return vec![make(
            State::Pass,
            v.selector(el),
            json!({
                "location": location,
                "name": clip(&name, 120),
                "href": clip(&url, 300),
                "matching_links": usable.len(),
            }),
        )
        .limit(limit)];
    }
    let described = |list: &[Control]| -> Vec<Value> {
        list.iter()
            .take(5)
            .map(|c| {
                json!({
                    "selector": v.selector(c.el),
                    "tag": v.tag(c.el),
                    "name": clip(&c.name, 120),
                    "href": v.attr(c.el, "href").map(|h| clip(h, 200)),
                })
            })
            .collect()
    };
    if !unusable.is_empty() {
        return vec![make(
            State::NeedsReview,
            v.selector(unusable[0].el),
            json!({
                "location": null,
                "reason": "a control with the name is present but it is not a link with a working address",
                "controls": described(&unusable),
            }),
        )
        .limit(limit)];
    }
    if !hidden.is_empty() {
        return vec![make(
            State::NeedsReview,
            v.selector(hidden[0].el),
            json!({
                "location": null,
                "reason": "a control with the name is in the markup but is not visible; it may be revealed by interaction",
                "controls": described(&hidden),
            }),
        )
        .limit(limit)];
    }
    if is_home(v) {
        return vec![make(
            State::Fail,
            "html".to_string(),
            json!({ "location": null, "reason": format!("no visible link names {} on the home page", spec.label) }),
        )
        .limit(limit)];
    }
    vec![make(
        State::NeedsReview,
        "html".to_string(),
        json!({
            "location": null,
            "reason": format!("no link names {} on this page; the site may offer it from the home page only", spec.label),
        }),
    )
    .limit(limit)]
}
