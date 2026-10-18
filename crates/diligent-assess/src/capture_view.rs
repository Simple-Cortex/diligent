//! A read-only view over one DOM snapshot, joined with the accessibility tree
//! and the focus walk of the same viewport.
//!
//! Everything here tolerates hostile captures: parent links that point out of
//! range or form a cycle are cut when the view is built, ancestor walks are
//! depth-capped, and no method indexes without checking.

use std::collections::HashMap;

use diligent_capture::{
    AxNode, AxTree, BoundingBox, CaptureSet, DomNode, DomSnapshot, FocusWalk, Viewport,
};
use url::Url;

use crate::color::{composite_over, parse_css_color, Color, Rgba};
use crate::contrast::Srgb;
use crate::model::EvidenceKey;

/// Media mode name of the default capture.
pub const BASELINE: &str = "baseline";

/// Ancestor walks stop after this many steps.
const MAX_DEPTH: usize = 2048;

/// Tags whose text is never rendered.
const NON_RENDERED_TAGS: [&str; 6] = ["script", "style", "noscript", "template", "head", "title"];

/// Roles an AX node must have to count as an interactive element.
pub const INTERACTIVE_ROLES: [&str; 15] = [
    "button",
    "link",
    "checkbox",
    "radio",
    "textbox",
    "combobox",
    "listbox",
    "menuitem",
    "menuitemcheckbox",
    "menuitemradio",
    "slider",
    "spinbutton",
    "switch",
    "tab",
    "searchbox",
];

/// A DOM snapshot with the joins and indexes the checks need.
pub struct CaptureView<'a> {
    /// Viewport of the snapshot.
    pub viewport: Viewport,
    /// Media mode name of the snapshot.
    pub media: String,
    /// The DOM snapshot.
    pub dom: &'a DomSnapshot,
    /// Accessibility tree of the same viewport and media, when captured.
    pub ax: Option<&'a AxTree>,
    /// Focus walk of the same viewport, when captured.
    pub focus: Option<&'a FocusWalk>,
    parent: Vec<Option<usize>>,
    children: Vec<Vec<usize>>,
    by_backend: HashMap<i64, usize>,
    ax_by_backend: HashMap<i64, usize>,
    ids: HashMap<&'a str, Vec<usize>>,
    has_text_boxes: bool,
}

impl<'a> CaptureView<'a> {
    /// Builds the view for `viewport` and `media`. `None` when the set has no DOM snapshot for them.
    pub fn new(set: &'a CaptureSet, viewport: Viewport, media: &str) -> Option<CaptureView<'a>> {
        let dom = set.dom(viewport, media)?;
        let ax = set.ax(viewport, media);
        let focus = set.focus(viewport);
        Some(Self::from_parts(viewport, media, dom, ax, focus))
    }

    /// Builds a view from already-loaded parts.
    pub fn from_parts(
        viewport: Viewport,
        media: &str,
        dom: &'a DomSnapshot,
        ax: Option<&'a AxTree>,
        focus: Option<&'a FocusWalk>,
    ) -> CaptureView<'a> {
        let n = dom.nodes.len();
        let mut parent: Vec<Option<usize>> = dom
            .nodes
            .iter()
            .enumerate()
            .map(|(i, node)| node.parent.filter(|p| *p < n && *p != i))
            .collect();
        cut_cycles(&mut parent);
        let mut children: Vec<Vec<usize>> = vec![Vec::new(); n];
        for (i, p) in parent.iter().enumerate() {
            if let Some(p) = p {
                children[*p].push(i);
            }
        }
        let mut by_backend = HashMap::new();
        let mut ids: HashMap<&'a str, Vec<usize>> = HashMap::new();
        let mut has_text_boxes = false;
        for (i, node) in dom.nodes.iter().enumerate() {
            if let Some(b) = node.backend_node_id {
                by_backend.entry(b).or_insert(i);
            }
            if node.node_type == 1 {
                if let Some((_, v)) = node.attributes.iter().find(|(k, _)| k == "id") {
                    if !v.is_empty() {
                        ids.entry(v.as_str()).or_default().push(i);
                    }
                }
            } else if node.node_type == 3 && node.bounding_box.is_some() {
                has_text_boxes = true;
            }
        }
        let mut ax_by_backend: HashMap<i64, usize> = HashMap::new();
        if let Some(tree) = ax {
            for (k, node) in tree.nodes.iter().enumerate() {
                let Some(b) = node.backend_dom_node_id else {
                    continue;
                };
                match ax_by_backend.get(&b) {
                    None => {
                        ax_by_backend.insert(b, k);
                    }
                    Some(&old) if tree.nodes[old].ignored && !node.ignored => {
                        ax_by_backend.insert(b, k);
                    }
                    _ => {}
                }
            }
        }
        CaptureView {
            viewport,
            media: media.to_string(),
            dom,
            ax,
            focus,
            parent,
            children,
            by_backend,
            ax_by_backend,
            ids,
            has_text_boxes,
        }
    }

    /// Number of DOM nodes.
    pub fn len(&self) -> usize {
        self.dom.nodes.len()
    }

    /// True when the snapshot has no nodes.
    pub fn is_empty(&self) -> bool {
        self.dom.nodes.is_empty()
    }

    /// The DOM node at `i`.
    pub fn node(&self, i: usize) -> &DomNode {
        &self.dom.nodes[i]
    }

    /// True for element nodes.
    pub fn is_element(&self, i: usize) -> bool {
        self.dom.nodes.get(i).is_some_and(|n| n.node_type == 1)
    }

    /// True for text nodes.
    pub fn is_text(&self, i: usize) -> bool {
        self.dom.nodes.get(i).is_some_and(|n| n.node_type == 3)
    }

    /// Indexes of every element node, in document order.
    pub fn elements(&self) -> impl Iterator<Item = usize> + '_ {
        self.dom
            .nodes
            .iter()
            .enumerate()
            .filter(|(_, n)| n.node_type == 1)
            .map(|(i, _)| i)
    }

    /// Lowercase tag of an element.
    pub fn tag(&self, i: usize) -> &str {
        &self.dom.nodes[i].tag
    }

    /// Value of attribute `name` (attribute names compare ASCII case-insensitively).
    pub fn attr(&self, i: usize, name: &str) -> Option<&str> {
        self.dom.nodes[i]
            .attributes
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    /// True when the element carries attribute `name`, even with an empty value.
    pub fn has_attr(&self, i: usize, name: &str) -> bool {
        self.attr(i, name).is_some()
    }

    /// Parent node index after cycle cutting.
    pub fn parent(&self, i: usize) -> Option<usize> {
        self.parent.get(i).copied().flatten()
    }

    /// Parent that is an element.
    pub fn parent_element(&self, i: usize) -> Option<usize> {
        self.parent(i).filter(|p| self.is_element(*p))
    }

    /// Child node indexes in document order.
    pub fn children(&self, i: usize) -> &[usize] {
        self.children.get(i).map_or(&[], Vec::as_slice)
    }

    /// Ancestor chain from the parent up, capped at a fixed depth.
    pub fn ancestors(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::successors(self.parent(i), move |&p| self.parent(p)).take(MAX_DEPTH)
    }

    /// The element itself followed by its ancestors.
    pub fn self_and_ancestors(&self, i: usize) -> impl Iterator<Item = usize> + '_ {
        std::iter::once(i).chain(self.ancestors(i))
    }

    /// True when `ancestor` is a proper ancestor of `i`.
    pub fn is_ancestor(&self, ancestor: usize, i: usize) -> bool {
        self.ancestors(i).any(|a| a == ancestor)
    }

    /// Descendant nodes of `i` in document order, `i` excluded.
    pub fn descendants(&self, i: usize) -> Vec<usize> {
        let mut out = Vec::new();
        let mut stack: Vec<usize> = self.children(i).iter().rev().copied().collect();
        while let Some(next) = stack.pop() {
            out.push(next);
            if out.len() >= self.len() {
                break;
            }
            stack.extend(self.children(next).iter().rev().copied());
        }
        out
    }

    /// Computed style value of `prop` on `i`, when the node was laid out.
    pub fn style(&self, i: usize, prop: &str) -> Option<&str> {
        self.dom.nodes.get(i)?.styles.get(prop).map(String::as_str)
    }

    /// True when the node has computed styles, meaning the browser laid it out.
    pub fn rendered(&self, i: usize) -> bool {
        self.dom.nodes.get(i).is_some_and(|n| !n.styles.is_empty())
    }

    /// Style of `prop` on node `i`, or on the nearest ancestor that was laid out.
    pub fn inherited_style(&self, i: usize, prop: &str) -> Option<&str> {
        self.self_and_ancestors(i)
            .find(|n| self.rendered(*n))
            .and_then(|n| self.style(n, prop))
    }

    /// Layout box of `i`.
    pub fn bbox(&self, i: usize) -> Option<BoundingBox> {
        self.dom.nodes.get(i)?.bounding_box
    }

    /// DOM node index for a backend node id.
    pub fn dom_index(&self, backend_id: i64) -> Option<usize> {
        self.by_backend.get(&backend_id).copied()
    }

    /// Accessibility node for DOM node `i`, preferring a non-ignored one.
    pub fn ax_node(&self, i: usize) -> Option<&AxNode> {
        let backend = self.dom.nodes.get(i)?.backend_node_id?;
        let k = *self.ax_by_backend.get(&backend)?;
        self.ax?.nodes.get(k)
    }

    /// Accessibility role of DOM node `i`, lowercase.
    pub fn ax_role(&self, i: usize) -> Option<String> {
        self.ax_node(i)?
            .role
            .as_ref()
            .map(|r| r.to_ascii_lowercase())
    }

    /// Accessible name of DOM node `i`.
    pub fn ax_name(&self, i: usize) -> Option<&str> {
        self.ax_node(i)?.name.as_deref()
    }

    /// Elements with exactly the given id.
    pub fn by_id(&self, id: &str) -> &[usize] {
        self.ids.get(id).map_or(&[], Vec::as_slice)
    }

    /// Interactive elements: non-ignored AX nodes with an interactive role that map to a DOM element.
    pub fn interactive(&self) -> Vec<usize> {
        let mut out: Vec<usize> = Vec::new();
        let mut seen: std::collections::HashSet<usize> = std::collections::HashSet::new();
        let Some(tree) = self.ax else { return out };
        for node in &tree.nodes {
            if node.ignored {
                continue;
            }
            let Some(role) = node.role.as_deref() else {
                continue;
            };
            if !INTERACTIVE_ROLES.contains(&role.to_ascii_lowercase().as_str()) {
                continue;
            }
            let Some(dom_i) = node.backend_dom_node_id.and_then(|b| self.dom_index(b)) else {
                continue;
            };
            if self.is_element(dom_i) && seen.insert(dom_i) {
                out.push(dom_i);
            }
        }
        out.sort_unstable();
        out
    }

    /// Resolves `href` against the document URL. Falls back to the trimmed text.
    pub fn resolve_url(&self, href: &str) -> String {
        let href = href.trim();
        match Url::parse(&self.dom.document_url).and_then(|base| base.join(href)) {
            Ok(url) => url.to_string(),
            Err(_) => href.to_string(),
        }
    }

    /// An opacity problem on `i` or an ancestor, as a reason string.
    pub fn opacity_issue(&self, i: usize) -> Option<String> {
        for n in self.self_and_ancestors(i) {
            if !self.is_element(n) {
                continue;
            }
            if let Some(value) = self.style(n, "opacity") {
                match value.trim().parse::<f64>() {
                    Ok(v) if v < 1.0 => {
                        return Some(format!("opacity {} on <{}>", value.trim(), self.tag(n)))
                    }
                    Ok(_) => {}
                    Err(_) => {
                        return Some(format!(
                            "unparseable opacity `{value}` on <{}>",
                            self.tag(n)
                        ))
                    }
                }
            }
        }
        None
    }

    fn opacity_is_zero(&self, i: usize) -> bool {
        self.self_and_ancestors(i).any(|n| {
            self.is_element(n)
                && self
                    .style(n, "opacity")
                    .and_then(|v| v.trim().parse::<f64>().ok())
                    .is_some_and(|v| v <= 0.0)
        })
    }

    /// Visibility predicate for an element: laid out, `display` not `none`,
    /// `visibility` visible, a box with positive width and height, and no
    /// zero opacity along the chain.
    pub fn visible(&self, i: usize) -> bool {
        if !self.is_element(i) {
            return false;
        }
        let Some(b) = self.bbox(i) else { return false };
        if !(b.width > 0.0 && b.height > 0.0) {
            return false;
        }
        if self.style(i, "display").is_none_or(|d| d.trim() == "none") {
            return false;
        }
        if self
            .style(i, "visibility")
            .is_none_or(|v| v.trim() != "visible")
        {
            return false;
        }
        !self.opacity_is_zero(i)
    }

    /// True when text node `t` is rendered and visible.
    ///
    /// Uses the text node's own box when the snapshot records boxes for text
    /// nodes, otherwise the box of the nearest laid-out ancestor.
    pub fn text_visible(&self, t: usize) -> bool {
        if !self.is_text(t) {
            return false;
        }
        let Some(p) = self.parent_element(t) else {
            return false;
        };
        if NON_RENDERED_TAGS.contains(&self.tag(p)) {
            return false;
        }
        let boxed = if self.has_text_boxes {
            self.bbox(t)
        } else {
            self.self_and_ancestors(t).find_map(|n| self.bbox(n))
        };
        let Some(b) = boxed else { return false };
        if !(b.width > 0.0 && b.height > 0.0) {
            return false;
        }
        if self
            .inherited_style(t, "display")
            .is_none_or(|d| d.trim() == "none")
        {
            return false;
        }
        if self
            .inherited_style(t, "visibility")
            .is_none_or(|v| v.trim() != "visible")
        {
            return false;
        }
        !self.opacity_is_zero(p)
    }

    /// Visible, non-whitespace text nodes under `i`, in document order.
    pub fn visible_text_nodes(&self, i: usize) -> Vec<usize> {
        self.descendants(i)
            .into_iter()
            .filter(|n| {
                self.is_text(*n)
                    && self
                        .node(*n)
                        .text
                        .as_deref()
                        .is_some_and(|t| !t.trim().is_empty())
                    && self.text_visible(*n)
            })
            .collect()
    }

    fn display_is_inline(&self, e: usize) -> bool {
        match self.style(e, "display").map(str::trim) {
            None => true,
            Some(d) => matches!(
                d,
                "inline" | "inline-block" | "contents" | "ruby" | "ruby-text"
            ),
        }
    }

    /// True when a block boundary or a `br` separates text nodes `a` and `b` (`a` before `b`).
    fn boundary_between(&self, a: usize, b: usize) -> bool {
        if (a + 1..b).any(|k| self.is_element(k) && self.tag(k) == "br") {
            return true;
        }
        let chain_a: Vec<usize> = self.ancestors(a).collect();
        let mut common: Option<usize> = None;
        for anc in self.ancestors(b) {
            if chain_a.contains(&anc) {
                common = Some(anc);
                break;
            }
            if self.is_element(anc) && !self.display_is_inline(anc) {
                return true;
            }
        }
        for anc in chain_a {
            if Some(anc) == common {
                break;
            }
            if self.is_element(anc) && !self.display_is_inline(anc) {
                return true;
            }
        }
        false
    }

    /// Whitespace-normalised text of the visible text nodes under `i`.
    ///
    /// A space goes between two text nodes only when a block boundary or a
    /// line break separates them, so `Pre<b>fix</b>` reads `Prefix`.
    pub fn visible_text(&self, i: usize) -> String {
        let nodes = self.visible_text_nodes(i);
        let mut out = String::new();
        let mut previous: Option<usize> = None;
        for t in nodes {
            if let Some(prev) = previous {
                if self.boundary_between(prev, t) {
                    out.push(' ');
                }
            }
            out.push_str(self.node(t).text.as_deref().unwrap_or(""));
            previous = Some(t);
        }
        normalize_space(&out)
    }

    /// True when `i` or an ancestor is hidden from assistive technology with `aria-hidden="true"`.
    pub fn in_aria_hidden(&self, i: usize) -> bool {
        self.self_and_ancestors(i).any(|n| {
            self.is_element(n)
                && self
                    .attr(n, "aria-hidden")
                    .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"))
        })
    }

    /// True when `i` is a disabled form control, sits inside one, or has `aria-disabled="true"` on it or an ancestor.
    pub fn is_disabled(&self, i: usize) -> bool {
        self.self_and_ancestors(i).any(|n| {
            if !self.is_element(n) {
                return false;
            }
            let native = matches!(
                self.tag(n),
                "button" | "input" | "select" | "textarea" | "fieldset" | "optgroup" | "option"
            ) && self.has_attr(n, "disabled");
            let aria = self
                .attr(n, "aria-disabled")
                .is_some_and(|v| v.trim().eq_ignore_ascii_case("true"));
            native || aria
        })
    }

    /// The opaque colour behind `start`.
    ///
    /// Walks from `start` up through its ancestors, collecting translucent
    /// `background-color` layers until the first opaque one, then composites
    /// them. With no opaque colour the canvas is white. Any
    /// `background-image` on a visited element, any opacity below 1 on the
    /// chain, or an unparseable colour makes the result unresolvable, and the
    /// reason says which.
    pub fn effective_background(&self, start: usize) -> Result<Srgb, String> {
        if let Some(reason) = self.opacity_issue(start) {
            return Err(reason);
        }
        let mut layers: Vec<Rgba> = Vec::new();
        let mut base: Option<Srgb> = None;
        for e in self.self_and_ancestors(start) {
            if !self.is_element(e) {
                continue;
            }
            if let Some(image) = self.style(e, "background-image") {
                if image.trim() != "none" && !image.trim().is_empty() {
                    return Err(format!("background-image on <{}>", self.tag(e)));
                }
            }
            let Some(value) = self.style(e, "background-color") else {
                continue;
            };
            match parse_css_color(value) {
                None => {
                    return Err(format!(
                        "unparseable background-color `{value}` on <{}>",
                        self.tag(e)
                    ))
                }
                Some(Color::Opaque(c)) => {
                    base = Some(c);
                    break;
                }
                Some(Color::Translucent(t)) => {
                    if t.a > 0.0 {
                        layers.push(t);
                    }
                }
            }
        }
        let mut color = base.unwrap_or(Srgb {
            r: 255,
            g: 255,
            b: 255,
        });
        for layer in layers.into_iter().rev() {
            color = composite_over(layer, color);
        }
        Ok(color)
    }

    /// CSS selector that identifies element `i` in this snapshot.
    ///
    /// `tag#id` when the id is unique in the snapshot, otherwise a
    /// `tag:nth-child(n)` path from `html`.
    pub fn selector(&self, i: usize) -> String {
        if !self.is_element(i) {
            return "html".to_string();
        }
        if let Some(id) = self.attr(i, "id") {
            if !id.is_empty() && self.by_id(id).len() == 1 {
                if is_simple_ident(id) {
                    return format!("{}#{id}", self.tag(i));
                }
                let escaped = id.replace('\\', "\\\\").replace('"', "\\\"");
                return format!("{}[id=\"{escaped}\"]", self.tag(i));
            }
        }
        let mut parts: Vec<String> = Vec::new();
        for e in self.self_and_ancestors(i) {
            if !self.is_element(e) {
                break;
            }
            let part = match self.parent_element(e) {
                Some(p) => {
                    let nth = self
                        .children(p)
                        .iter()
                        .filter(|c| self.is_element(**c))
                        .position(|c| *c == e)
                        .map_or(1, |k| k + 1);
                    format!("{}:nth-child({nth})", self.tag(e))
                }
                None => self.tag(e).to_string(),
            };
            parts.push(part);
        }
        parts.reverse();
        parts.join(" > ")
    }

    /// Evidence key for this view's snapshot or accessibility tree.
    pub fn key(&self, kind: &'static str) -> EvidenceKey {
        let media = if kind == "focus_order" {
            None
        } else {
            Some(self.media.clone())
        };
        EvidenceKey {
            kind,
            viewport: self.viewport,
            media,
        }
    }
}

/// Collapses runs of whitespace to one space and trims.
pub fn normalize_space(s: &str) -> String {
    s.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parses a computed pixel length such as `16px` or `16.5px`.
pub fn parse_px(value: &str) -> Option<f64> {
    let v = value.trim();
    let number = v.strip_suffix("px")?;
    let parsed: f64 = number.trim().parse().ok()?;
    parsed.is_finite().then_some(parsed)
}

fn is_simple_ident(id: &str) -> bool {
    let mut chars = id.chars();
    matches!(chars.next(), Some(c) if c.is_ascii_alphabetic() || c == '_')
        && chars.all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
}

/// Cuts parent links so following them always ends. For each cycle, the link
/// that closes it is removed.
fn cut_cycles(parent: &mut [Option<usize>]) {
    let n = parent.len();
    // 0 unvisited, 1 on the current path, 2 finished.
    let mut state = vec![0u8; n];
    for start in 0..n {
        if state[start] != 0 {
            continue;
        }
        let mut path: Vec<usize> = Vec::new();
        let mut cur = start;
        loop {
            match state[cur] {
                2 => break,
                1 => {
                    if let Some(&last) = path.last() {
                        parent[last] = None;
                    }
                    break;
                }
                _ => {}
            }
            state[cur] = 1;
            path.push(cur);
            match parent[cur] {
                Some(p) => cur = p,
                None => break,
            }
        }
        for node in path {
            state[node] = 2;
        }
    }
}
