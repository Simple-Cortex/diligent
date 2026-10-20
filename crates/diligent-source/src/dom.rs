//! DOM helpers shared by every check.
//!
//! Everything here works on the parsed tree only. Nothing computes styles.
//! Visibility is limited to what the markup states directly: `aria-hidden`,
//! the `hidden` attribute, and inline `display:none` or `visibility:hidden`.

use std::collections::HashMap;

use scraper::{ElementRef, Html, Node};
use serde_json::{Map, Value};

use crate::template::{COMPONENT_TAG, MARK, SPREAD_ATTR};

/// SVG element namespace.
pub const SVG_NS: &str = "http://www.w3.org/2000/svg";
/// HTML element namespace.
pub const HTML_NS: &str = "http://www.w3.org/1999/xhtml";

const MAX_DEPTH: usize = 128;

/// Where an element starts in the source text.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pos {
    /// Byte offset in the source text.
    pub offset: u32,
    /// 1-based line.
    pub line: u32,
    /// 1-based column, counted in characters.
    pub column: u32,
}

/// What the template pre-pass learned about a document.
#[derive(Debug, Clone, Copy, Default)]
pub struct TemplateFacts {
    /// The text came from a template flavour, not plain HTML.
    pub templated: bool,
    /// The template holds a conditional or a loop.
    pub has_conditionals: bool,
    /// The template has no `html` or `body` tag, so it is a fragment.
    pub fragment: bool,
}

/// Parsed document plus lookup tables built once per scan.
pub struct Ctx<'a> {
    /// Every element in document order. The first is the `html` element.
    pub elements: Vec<ElementRef<'a>>,
    ids: HashMap<&'a str, Vec<ElementRef<'a>>>,
    positions: HashMap<usize, Pos>,
    /// What the template pre-pass learned. All false for plain HTML.
    pub template: TemplateFacts,
    /// A `label` has an expression for its `for` attribute.
    pub label_for_expr: bool,
}

fn node_key(el: ElementRef<'_>) -> usize {
    std::ptr::from_ref(el.value()) as usize
}

impl<'a> Ctx<'a> {
    /// Builds the element list and id table for a parsed document.
    pub fn new(doc: &'a Html) -> Self {
        Self::with_positions(doc, &[], TemplateFacts::default())
    }

    /// Like [`Ctx::new`], with a position for each element (one entry per
    /// element in document order, `None` when unknown) and template facts.
    pub fn with_positions(
        doc: &'a Html,
        positions: &[Option<Pos>],
        template: TemplateFacts,
    ) -> Self {
        let elements: Vec<ElementRef<'a>> = doc
            .tree
            .root()
            .descendants()
            .filter_map(ElementRef::wrap)
            .collect();
        let mut ids: HashMap<&'a str, Vec<ElementRef<'a>>> = HashMap::new();
        let mut label_for_expr = false;
        for el in &elements {
            if let Some(id) = el.value().id() {
                if !id.is_empty() && !id.contains(MARK) {
                    ids.entry(id).or_default().push(*el);
                }
            }
            if tag(*el) == "label" && attr(*el, "for").is_some_and(has_mark) {
                label_for_expr = true;
            }
        }
        let positions = elements
            .iter()
            .zip(positions)
            .filter_map(|(el, p)| p.map(|p| (node_key(*el), p)))
            .collect();
        Ctx {
            elements,
            ids,
            positions,
            template,
            label_for_expr,
        }
    }

    /// Where `el` starts in the source, when the source text contains it.
    pub fn position(&self, el: ElementRef<'a>) -> Option<Pos> {
        self.positions.get(&node_key(el)).copied()
    }

    /// The `html` element. The HTML parser always creates one.
    pub fn root(&self) -> ElementRef<'a> {
        self.elements[0]
    }

    /// First element with this id, in document order.
    pub fn by_id(&self, id: &str) -> Option<ElementRef<'a>> {
        self.ids.get(id).and_then(|v| v.first().copied())
    }

    /// All elements sharing this id, in document order.
    pub fn id_group(&self, id: &str) -> &[ElementRef<'a>] {
        self.ids.get(id).map(Vec::as_slice).unwrap_or(&[])
    }

    /// Number of ids that appear at least once.
    pub fn distinct_id_count(&self) -> usize {
        self.ids.len()
    }

    /// Stable CSS path to `el`, preferring `#id` when the id is unique.
    pub fn selector(&self, el: ElementRef<'a>) -> String {
        let mut segs: Vec<String> = Vec::new();
        let mut cur = Some(el);
        while let Some(e) = cur {
            if let Some(id) = e.value().id() {
                if is_simple_ident(id) && self.id_group(id).len() == 1 {
                    segs.push(format!("#{id}"));
                    break;
                }
            }
            let name = tag(e);
            if is_html(e) && matches!(name, "html" | "head" | "body") {
                segs.push(name.to_string());
            } else {
                segs.push(format!("{name}:nth-of-type({})", nth_of_type(e)));
            }
            cur = parent_el(e);
        }
        segs.reverse();
        segs.join(" > ")
    }
}

fn is_simple_ident(id: &str) -> bool {
    let mut chars = id.chars();
    match chars.next() {
        Some(c) if c.is_ascii_alphabetic() || c == '_' => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

fn nth_of_type(el: ElementRef<'_>) -> usize {
    let name = tag(el);
    el.prev_siblings()
        .filter_map(ElementRef::wrap)
        .filter(|s| tag(*s) == name)
        .count()
        + 1
}

/// What an attribute holds, as far as the markup says.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AttrValue<'a> {
    /// The attribute is not there.
    Absent,
    /// The attribute is there with only whitespace.
    Blank,
    /// A literal value.
    Text(&'a str),
    /// A template expression stands in for all or part of the value.
    Expression,
}

/// Classifies an attribute. An expression wins over literal text.
pub fn attr_value<'a>(el: ElementRef<'a>, name: &str) -> AttrValue<'a> {
    match attr(el, name) {
        None => AttrValue::Absent,
        Some(v) if has_mark(v) => AttrValue::Expression,
        Some(v) if v.trim_matches(is_ascii_ws).is_empty() => AttrValue::Blank,
        Some(v) => AttrValue::Text(v),
    }
}

/// True when `s` holds a template expression marker.
pub fn has_mark(s: &str) -> bool {
    s.contains(MARK)
}

/// True for a component element left opaque by the template pre-pass.
pub fn is_component(el: ElementRef<'_>) -> bool {
    tag(el) == COMPONENT_TAG
}

/// True when the element carries an attribute spread.
pub fn has_spread(el: ElementRef<'_>) -> bool {
    has_attr(el, SPREAD_ATTR)
}

/// Local tag name, lowercase for HTML elements.
pub fn tag<'a>(el: ElementRef<'a>) -> &'a str {
    el.value().name()
}

/// Namespace URI of the element.
pub fn namespace<'a>(el: ElementRef<'a>) -> &'a str {
    &el.value().name.ns
}

/// True for elements in the HTML namespace.
pub fn is_html(el: ElementRef<'_>) -> bool {
    namespace(el) == HTML_NS
}

/// True for elements in the SVG namespace.
pub fn is_svg(el: ElementRef<'_>) -> bool {
    namespace(el) == SVG_NS
}

/// Parent element, if the parent is an element.
pub fn parent_el<'a>(el: ElementRef<'a>) -> Option<ElementRef<'a>> {
    el.parent().and_then(ElementRef::wrap)
}

/// Ancestor elements from the nearest to the root.
pub fn ancestors<'a>(el: ElementRef<'a>) -> impl Iterator<Item = ElementRef<'a>> {
    std::iter::successors(parent_el(el), |p| parent_el(*p))
}

/// Attribute value, untrimmed.
pub fn attr<'a>(el: ElementRef<'a>, name: &str) -> Option<&'a str> {
    el.value().attr(name)
}

/// Attribute value trimmed of ASCII whitespace; `None` when absent or blank.
pub fn attr_trim<'a>(el: ElementRef<'a>, name: &str) -> Option<&'a str> {
    attr(el, name)
        .map(|v| v.trim_matches(is_ascii_ws))
        .filter(|v| !v.is_empty())
}

/// True when the attribute exists, whatever its value.
pub fn has_attr(el: ElementRef<'_>, name: &str) -> bool {
    attr(el, name).is_some()
}

/// True when the element has the class token.
pub fn has_class(el: ElementRef<'_>, class: &str) -> bool {
    el.value().classes().any(|c| c == class)
}

fn is_ascii_ws(c: char) -> bool {
    matches!(c, ' ' | '\t' | '\n' | '\r' | '\u{c}')
}

/// Collapses runs of whitespace and trims the ends.
pub fn normalize_ws(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// First token of the `role` attribute, lowercased.
pub fn explicit_role(el: ElementRef<'_>) -> Option<String> {
    attr(el, "role")
        .and_then(|r| r.split_ascii_whitespace().next())
        .map(|r| r.to_ascii_lowercase())
}

/// True for `none` and `presentation`.
pub fn is_presentational_role(role: &str) -> bool {
    matches!(role, "none" | "presentation")
}

/// Parses an integer the way the HTML spec does for `tabindex`.
pub fn parse_int(s: &str) -> Option<i64> {
    let s = s.trim_start_matches(is_ascii_ws);
    let (neg, rest) = match s.as_bytes().first() {
        Some(b'-') => (true, &s[1..]),
        Some(b'+') => (false, &s[1..]),
        _ => (false, s),
    };
    let end = rest
        .find(|c: char| !c.is_ascii_digit())
        .unwrap_or(rest.len());
    if end == 0 {
        return None;
    }
    let n: i64 = rest[..end].parse().unwrap_or(i64::MAX);
    Some(if neg { -n } else { n })
}

/// Parsed `tabindex`, if present and valid.
pub fn tabindex(el: ElementRef<'_>) -> Option<i64> {
    attr(el, "tabindex").and_then(parse_int)
}

/// True when the element carries the `disabled` attribute.
pub fn is_disabled(el: ElementRef<'_>) -> bool {
    has_attr(el, "disabled")
}

/// Lowercased `type` of an `input`, defaulting to `text`.
pub fn input_type(el: ElementRef<'_>) -> String {
    attr(el, "type")
        .map(|t| t.trim().to_ascii_lowercase())
        .filter(|t| !t.is_empty())
        .unwrap_or_else(|| "text".to_string())
}

fn native_focusable(el: ElementRef<'_>) -> bool {
    if !is_html(el) {
        return false;
    }
    match tag(el) {
        "a" | "area" => has_attr(el, "href"),
        "button" | "select" | "textarea" => !is_disabled(el),
        "input" => input_type(el) != "hidden" && !is_disabled(el),
        "iframe" | "summary" => true,
        "audio" | "video" => has_attr(el, "controls"),
        _ => matches!(
            attr(el, "contenteditable").map(str::trim),
            Some("" | "true")
        ),
    }
}

/// True when the element can take focus: a valid `tabindex` or native focusability.
pub fn is_focusable(el: ElementRef<'_>) -> bool {
    tabindex(el).is_some() || native_focusable(el)
}

const GLOBAL_ARIA: &[&str] = &[
    "aria-atomic",
    "aria-busy",
    "aria-controls",
    "aria-current",
    "aria-describedby",
    "aria-details",
    "aria-disabled",
    "aria-dropeffect",
    "aria-errormessage",
    "aria-flowto",
    "aria-grabbed",
    "aria-haspopup",
    "aria-invalid",
    "aria-keyshortcuts",
    "aria-label",
    "aria-labelledby",
    "aria-live",
    "aria-owns",
    "aria-relevant",
    "aria-roledescription",
];

fn has_global_aria(el: ElementRef<'_>) -> bool {
    GLOBAL_ARIA.iter().any(|a| has_attr(el, a))
}

/// True when a `none`/`presentation` role takes effect.
///
/// Conflict resolution: a presentational role is ignored when the element is
/// focusable or carries a global ARIA attribute other than `aria-hidden`.
pub fn presentational_honored(el: ElementRef<'_>) -> bool {
    match explicit_role(el) {
        Some(r) if is_presentational_role(&r) => !is_focusable(el) && !has_global_aria(el),
        _ => false,
    }
}

fn inline_style_hides(style: &str) -> bool {
    style.split(';').any(|decl| {
        let Some((prop, val)) = decl.split_once(':') else {
            return false;
        };
        let prop = prop.trim().to_ascii_lowercase();
        let val = val.trim().to_ascii_lowercase();
        let val = val.trim_end_matches("!important").trim();
        (prop == "display" && val == "none")
            || (prop == "visibility" && (val == "hidden" || val == "collapse"))
    })
}

/// Hidden by the element's own attributes, ignoring ancestors.
pub fn hidden_local(el: ElementRef<'_>) -> bool {
    attr(el, "aria-hidden").is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
        || has_attr(el, "hidden")
        || attr(el, "style").is_some_and(inline_style_hides)
}

/// Hidden by the element or any ancestor, per [`hidden_local`].
pub fn is_hidden(el: ElementRef<'_>) -> bool {
    hidden_local(el) || ancestors(el).any(hidden_local)
}

const WALK_BUDGET: usize = 20_000;

/// Shared state for one name computation. `in_ref` is true while following an
/// `aria-labelledby` reference, during which further references are ignored
/// (accname 1.2). `visiting` and `budget` bound cycles and total work.
struct Walk<'a> {
    budget: usize,
    in_ref: bool,
    visiting: Vec<ElementRef<'a>>,
}

impl<'a> Walk<'a> {
    fn new() -> Self {
        Walk {
            budget: WALK_BUDGET,
            in_ref: false,
            visiting: Vec::new(),
        }
    }
}

/// Text of an element as it contributes to a name: text nodes, image names,
/// and the `aria-label` of descendants. Scripts, styles and hidden
/// descendants contribute nothing. With `skip_controls`, form controls
/// are left out so a wrapping label reads as the label text only.
/// Work is bounded by a depth limit and a total node budget.
pub fn content_text<'a>(ctx: &Ctx<'a>, el: ElementRef<'a>, skip_controls: bool) -> String {
    let mut out = String::new();
    let mut walk = Walk::new();
    collect_text(ctx, el, skip_controls, 0, &mut out, &mut walk);
    normalize_ws(&out)
}

fn collect_text<'a>(
    ctx: &Ctx<'a>,
    el: ElementRef<'a>,
    skip_controls: bool,
    depth: usize,
    out: &mut String,
    walk: &mut Walk<'a>,
) {
    if depth > MAX_DEPTH {
        return;
    }
    for child in el.children() {
        if walk.budget == 0 {
            return;
        }
        walk.budget -= 1;
        match child.value() {
            Node::Text(t) => out.push_str(t),
            Node::Element(_) => {
                let Some(c) = ElementRef::wrap(child) else {
                    continue;
                };
                let name = tag(c);
                if matches!(name, "script" | "style" | "template" | "noscript") {
                    continue;
                }
                if hidden_local(c) {
                    continue;
                }
                if skip_controls && matches!(name, "select" | "textarea" | "input") {
                    continue;
                }
                out.push(' ');
                if name == COMPONENT_TAG {
                    out.push(MARK);
                }
                if name == "img" {
                    out.push_str(&image_name_inner(ctx, c, walk));
                } else if let Some(label) = attr_trim(c, "aria-label") {
                    out.push_str(label);
                } else {
                    collect_text(ctx, c, skip_controls, depth + 1, out, walk);
                }
                out.push(' ');
            }
            _ => {}
        }
    }
}

/// Text from `aria-labelledby` targets. `None` when the attribute is absent or
/// blank; `Some("")` when it is present but resolves to nothing. Cycles and
/// nested references resolve to nothing instead of recursing.
pub fn labelledby_text<'a>(ctx: &Ctx<'a>, el: ElementRef<'a>) -> Option<String> {
    labelledby_inner(ctx, el, &mut Walk::new())
}

fn labelledby_inner<'a>(ctx: &Ctx<'a>, el: ElementRef<'a>, walk: &mut Walk<'a>) -> Option<String> {
    if walk.in_ref {
        return None;
    }
    let raw = attr_trim(el, "aria-labelledby")?;
    walk.in_ref = true;
    walk.visiting.push(el);
    let mut parts = Vec::new();
    for token in raw.split_ascii_whitespace() {
        if walk.budget == 0 {
            break;
        }
        walk.budget -= 1;
        let Some(target) = ctx.by_id(token) else {
            continue;
        };
        if walk.visiting.contains(&target) {
            continue;
        }
        walk.visiting.push(target);
        let text = match attr_trim(target, "aria-label") {
            Some(l) => normalize_ws(l),
            None => {
                let mut out = String::new();
                collect_text(ctx, target, false, 0, &mut out, walk);
                normalize_ws(&out)
            }
        };
        walk.visiting.pop();
        if !text.is_empty() {
            parts.push(text);
        }
    }
    walk.visiting.pop();
    walk.in_ref = false;
    Some(parts.join(" "))
}

/// Name an `img` contributes: labelledby, aria-label, alt, then title.
pub fn image_name<'a>(ctx: &Ctx<'a>, img: ElementRef<'a>) -> String {
    image_name_inner(ctx, img, &mut Walk::new())
}

fn image_name_inner<'a>(ctx: &Ctx<'a>, img: ElementRef<'a>, walk: &mut Walk<'a>) -> String {
    if let Some(t) = labelledby_inner(ctx, img, walk).filter(|t| !t.is_empty()) {
        return t;
    }
    for name in ["aria-label", "alt", "title"] {
        if let Some(v) = attr_trim(img, name) {
            return normalize_ws(v);
        }
    }
    String::new()
}

/// Accessible name of a link and the source it came from.
pub fn link_name<'a>(ctx: &Ctx<'a>, link: ElementRef<'a>) -> (String, &'static str) {
    if let Some(t) = labelledby_text(ctx, link).filter(|t| !t.is_empty()) {
        return (t, "aria-labelledby");
    }
    if let Some(l) = attr_trim(link, "aria-label") {
        return (normalize_ws(l), "aria-label");
    }
    let content = content_text(ctx, link, false);
    if !content.is_empty() {
        return (content, "content");
    }
    if let Some(t) = attr_trim(link, "title") {
        return (normalize_ws(t), "title");
    }
    (String::new(), "none")
}

/// Truncates to at most `max` characters.
pub fn truncate_chars(s: &str, max: usize) -> String {
    s.chars().take(max).collect()
}

/// All attributes as a JSON object, each value cut to 200 characters.
pub fn attributes_json(el: ElementRef<'_>) -> Value {
    let mut map = Map::new();
    for (name, value) in el.value().attrs() {
        map.insert(name.to_string(), Value::String(truncate_chars(value, 200)));
    }
    Value::Object(map)
}

const VOID: &[&str] = &[
    "area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source",
    "track", "wbr",
];

fn push_escaped(out: &mut String, s: &str, in_attr: bool) {
    for c in s.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' if !in_attr => out.push_str("&lt;"),
            '>' if !in_attr => out.push_str("&gt;"),
            '"' if in_attr => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
}

type NodeOf<'a> = <ElementRef<'a> as std::ops::Deref>::Target;

fn write_node(node: NodeOf<'_>, out: &mut String, budget: usize) {
    if out.len() > budget {
        return;
    }
    match node.value() {
        Node::Element(e) => {
            let name = e.name();
            out.push('<');
            out.push_str(name);
            for (k, v) in e.attrs() {
                out.push(' ');
                out.push_str(k);
                out.push_str("=\"");
                push_escaped(out, v, true);
                out.push('"');
            }
            out.push('>');
            if VOID.contains(&name) {
                return;
            }
            for child in node.children() {
                write_node(child, out, budget);
                if out.len() > budget {
                    return;
                }
            }
            out.push_str("</");
            out.push_str(name);
            out.push('>');
        }
        Node::Text(t) => push_escaped(out, t, false),
        _ => {}
    }
}

/// Outer HTML of `el`, cut to `max` characters. The flag is true when cut.
pub fn outer_html_snippet(el: ElementRef<'_>, max: usize) -> (String, bool) {
    let mut out = String::new();
    // Any string longer than 4*max bytes already holds more than max characters.
    write_node(*el, &mut out, max.saturating_mul(4));
    if out.chars().count() > max {
        (truncate_chars(&out, max), true)
    } else {
        (out, false)
    }
}
