//! Serde types for captures. Field names and serde attributes of types that
//! predate this crate are unchanged from `diligent-runtime` 1.0.0.

use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

/// Computed-style properties captured for every laid-out DOM node.
///
/// The first 15 entries are the original 1.0.0 list and keep their order;
/// entries 16 to 35 were added in 1.1.0 and entries 36 to 54 in 1.3.0.
pub const COMPUTED_STYLE_ALLOWLIST: [&str; 54] = [
    "color",
    "background-color",
    "font-size",
    "font-weight",
    "line-height",
    "letter-spacing",
    "outline",
    "outline-offset",
    "visibility",
    "display",
    "opacity",
    "position",
    "z-index",
    "cursor",
    "text-decoration-line",
    "border-top-color",
    "border-right-color",
    "border-bottom-color",
    "border-left-color",
    "border-top-width",
    "border-right-width",
    "border-bottom-width",
    "border-left-width",
    "border-top-style",
    "border-bottom-style",
    "box-shadow",
    "background-image",
    "overflow-x",
    "overflow-y",
    "font-family",
    "text-decoration-color",
    "word-spacing",
    "width",
    "height",
    "text-shadow",
    "margin-top",
    "margin-right",
    "margin-bottom",
    "margin-left",
    "padding-top",
    "padding-right",
    "padding-bottom",
    "padding-left",
    "border-top-left-radius",
    "border-top-right-radius",
    "border-bottom-right-radius",
    "border-bottom-left-radius",
    "max-width",
    "text-align",
    "font-style",
    "animation-name",
    "animation-duration",
    "transition-duration",
    "transition-property",
];

/// Viewport dimensions and device pixel ratio.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Viewport {
    /// CSS pixel width.
    pub width: u32,
    /// CSS pixel height.
    pub height: u32,
    /// Device pixel ratio.
    pub dpr: f64,
}

/// Preferred color scheme to emulate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ColorScheme {
    /// `prefers-color-scheme: light`.
    Light,
    /// `prefers-color-scheme: dark`.
    Dark,
}

/// User-preference media features to emulate.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Media {
    /// `prefers-reduced-motion: reduce` when true.
    pub reduced_motion: bool,
    /// `forced-colors: active` when true.
    pub forced_colors: bool,
    /// `prefers-color-scheme`.
    pub color_scheme: ColorScheme,
}

impl Media {
    /// The baseline media state: no reduced motion, no forced colors, light.
    pub const BASELINE: Media = Media {
        reduced_motion: false,
        forced_colors: false,
        color_scheme: ColorScheme::Light,
    };
}

/// Result of a navigation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NavigationInfo {
    /// URL after redirects.
    pub final_url: String,
    /// HTTP status of the main document, when observed.
    pub status: Option<u16>,
    /// URLs of redirect hops that were followed, in order, excluding the final URL.
    pub redirect_chain: Vec<String>,
    /// Document title.
    pub title: String,
    /// Value of the `lang` attribute on `<html>`, when present and non-empty.
    pub lang: Option<String>,
    /// Transfer size of the main document in bytes (0 when unknown).
    pub document_bytes: u64,
}

/// Layout box in CSS pixels.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub struct BoundingBox {
    /// Left edge.
    pub x: f64,
    /// Top edge.
    pub y: f64,
    /// Width.
    pub width: f64,
    /// Height.
    pub height: f64,
}

/// One node of the flattened DOM snapshot.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomNode {
    /// Position in `DomSnapshot::nodes`.
    pub index: usize,
    /// DOM node type (1 element, 3 text, 9 document, ...).
    pub node_type: i64,
    /// Lowercase tag for elements, node name otherwise.
    pub tag: String,
    /// Attributes in source order.
    pub attributes: Vec<(String, String)>,
    /// Text for text nodes.
    pub text: Option<String>,
    /// Index of the parent node.
    pub parent: Option<usize>,
    /// Backend DOM node id; joins with `AxNode::backend_dom_node_id`.
    pub backend_node_id: Option<i64>,
    /// Layout box when the node is rendered.
    pub bounding_box: Option<BoundingBox>,
    /// Computed styles restricted to [`COMPUTED_STYLE_ALLOWLIST`].
    pub styles: BTreeMap<String, String>,
}

/// Scroll and client dimensions of the document, in CSS pixels.
///
/// `scroll_width` and `scroll_height` come from `document.scrollingElement`
/// (falling back to `document.documentElement`); `client_width` and
/// `client_height` come from `document.documentElement`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct DocumentMetrics {
    /// Width of the scrollable content.
    pub scroll_width: u32,
    /// Height of the scrollable content.
    pub scroll_height: u32,
    /// Width of the viewport excluding scrollbars.
    pub client_width: u32,
    /// Height of the viewport excluding scrollbars.
    pub client_height: u32,
}

/// Flattened DOM snapshot of the main document.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DomSnapshot {
    /// Document URL.
    pub document_url: String,
    /// Nodes in document order.
    pub nodes: Vec<DomNode>,
    /// Document scroll and client sizes. Absent in captures older than protocol 1.1.0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub document_metrics: Option<DocumentMetrics>,
}

/// One node of the accessibility tree.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxNode {
    /// Accessibility node id.
    pub node_id: String,
    /// Role, when computed.
    pub role: Option<String>,
    /// Accessible name.
    pub name: Option<String>,
    /// Where the name came from (`attribute`, `contents`, `implicit`, ...).
    pub name_source: Option<String>,
    /// Accessible description.
    pub description: Option<String>,
    /// Current value.
    pub value: Option<String>,
    /// Properties as `(name, value)` pairs.
    pub properties: Vec<(String, serde_json::Value)>,
    /// True when the node is excluded from the accessibility tree.
    pub ignored: bool,
    /// Backend DOM node id.
    pub backend_dom_node_id: Option<i64>,
    /// Child node ids.
    pub children: Vec<String>,
}

/// Accessibility tree of the main frame.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxTree {
    /// Nodes as returned by the browser.
    pub nodes: Vec<AxNode>,
}

/// One stop in the sequential keyboard focus order.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FocusStop {
    /// Backend DOM node id of the focused element.
    pub backend_node_id: i64,
    /// Lowercase tag name.
    pub tag: String,
    /// Approximate name (see `scripts::FOCUS_READ`).
    pub name: String,
    /// Whether focusing changed outline, shadow, border, background, color or
    /// text decoration. `None` when it could not be measured.
    pub had_visible_indicator: Option<bool>,
    /// Box of the focused element read at focus time with
    /// `getBoundingClientRect()`, so relative to the viewport at the scroll
    /// position the focus left it in. Absent in captures older than protocol 1.1.0.
    #[serde(default)]
    pub bounding_box: Option<BoundingBox>,
}

/// Why a focus walk stopped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum FocusWalkEnd {
    /// A Tab landed on the first stop again.
    ReturnedToFirst,
    /// A Tab left the page content and focus is on `<body>` (or unreadable).
    ReturnedToBody,
    /// A Tab landed on an earlier stop other than the first; carries that stop's index.
    RepeatOf(usize),
    /// The stop limit was reached without any of the above.
    StopLimit,
    /// The first Tab produced no focusable element.
    NoFocusable,
}

/// The keyboard focus walk: stops in Tab order plus how the walk ended.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct FocusWalk {
    /// Stops in Tab order.
    pub stops: Vec<FocusStop>,
    /// Why the walk ended.
    pub terminated_by: FocusWalkEnd,
}

/// One network request seen during page load.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct NetworkEntry {
    /// Request URL.
    pub url: String,
    /// HTTP method.
    pub method: String,
    /// CDP resource type, for example `Document`.
    pub resource_type: String,
    /// HTTP status when a response arrived.
    pub status: Option<u16>,
    /// MIME type of the response.
    pub mime: Option<String>,
    /// True when served from a cache.
    pub from_cache: bool,
    /// Earlier hops of a redirect that led to this entry, in order.
    pub redirect_chain: Vec<String>,
    /// True when the origin differs from the page's origin.
    pub third_party: bool,
    /// True when the request was failed by the network policy (loopback, private,
    /// link-local or metadata host) before it was sent. Absent in older logs.
    #[serde(default)]
    pub blocked_by_policy: bool,
    /// Non-sensitive request headers (`accept`, `accept-language`, `user-agent`, `referer`).
    pub request_headers: BTreeMap<String, String>,
    /// Selected response headers (HSTS, CSP, X-Frame-Options, Referrer-Policy, X-Content-Type-Options).
    pub response_headers: BTreeMap<String, String>,
}

/// One line of `manifest.jsonl`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ManifestEntry {
    /// Equal to `sha256`.
    pub evidence_id: String,
    /// `dom_snapshot`, `ax_tree`, `focus_order`, `screenshot`, `network_log`, `axe_result` or `perf_metrics`.
    pub kind: String,
    /// Viewport the capture was taken at.
    pub viewport: Viewport,
    /// Media mode name (`baseline`, `reduced-motion`, `forced-colors`, `dark`, `text-spacing`).
    pub media: String,
    /// File name relative to the output directory.
    pub path: String,
    /// Lowercase hex SHA-256 of the file.
    pub sha256: String,
    /// File size in bytes.
    pub bytes: u64,
    /// RFC 3339 UTC capture time.
    pub captured_at: String,
    /// URL of the page after redirects.
    pub final_url: String,
    /// Version of the collector crate that wrote the capture. Empty in older manifests.
    #[serde(default)]
    pub collector_version: String,
    /// Browser-worker protocol version in force when the capture was written. Empty in older manifests.
    #[serde(default)]
    pub protocol_version: String,
}

/// Outcome of an axe-core rule or of one node under a rule.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum AxeOutcome {
    /// The rule failed.
    Violation,
    /// The rule passed.
    Pass,
    /// axe could not decide; a person has to look.
    Incomplete,
    /// The rule did not apply to the page.
    Inapplicable,
}

/// One node axe-core reported under a rule.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxeNode {
    /// Selector built by the same algorithm as `CaptureView::selector` in
    /// `diligent-assess` (see `scripts::AXE_RUN`), not axe's own target.
    /// `None` when the node's target does not resolve to exactly one element.
    pub selector: Option<String>,
    /// Outcome for this node: `violation`, `pass` or `incomplete`.
    pub outcome: AxeOutcome,
    /// First line of axe's failure summary, at most 200 characters.
    pub summary: Option<String>,
}

/// One axe-core rule with its outcome and nodes.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxeRule {
    /// axe rule id.
    pub id: String,
    /// ACT rule ids from the rule metadata, sorted; empty when none.
    pub act_ids: Vec<String>,
    /// axe tags, sorted.
    pub tags: Vec<String>,
    /// Rule-level outcome.
    pub outcome: AxeOutcome,
    /// axe impact, when it reported one.
    pub impact: Option<String>,
    /// Nodes sorted by selector, capped at 500 per rule.
    pub nodes: Vec<AxeNode>,
    /// True when `nodes` was cut at the cap.
    pub truncated: bool,
}

/// Why the corroborator produced no rules.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AxeError {
    /// Worker error kind wire name, for example `timeout`.
    pub kind: String,
    /// Message, at most 200 characters.
    pub message: String,
}

/// Normalized axe-core result for one page load (kind `axe_result`).
///
/// Optional corroboration only: the deterministic checks stay authoritative.
/// Top frame only; iframes are not scanned.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct AxeResult {
    /// Always `axe-core`.
    pub engine: String,
    /// `axe.version`; `None` when axe did not run.
    pub version: Option<String>,
    /// `document.URL` when axe ran, the page URL otherwise.
    pub url: String,
    /// Rules sorted by id.
    pub rules: Vec<AxeRule>,
    /// Set when axe could not run; `rules` is then empty.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub error: Option<AxeError>,
}

/// Lab performance measurements for one page load (kind `perf_metrics`).
///
/// Lab values only: unthrottled, headless, one load. They are not field data
/// and say nothing about real users. Every value is optional; `None` means the
/// browser did not report it. When the measurement failed, `notes` starts with
/// `error:` and every value is `None`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct PerfMetrics {
    /// Always `lab`.
    pub label: String,
    /// Largest Contentful Paint `startTime` of the last entry, in ms (one decimal).
    pub lcp_ms: Option<f64>,
    /// Selector of the LCP element, built like `AxeNode::selector`.
    pub lcp_element: Option<String>,
    /// Cumulative Layout Shift, largest session window, four decimals.
    pub cls: Option<f64>,
    /// First Contentful Paint in ms.
    pub fcp_ms: Option<f64>,
    /// Navigation `responseStart` in ms.
    pub ttfb_ms: Option<f64>,
    /// Navigation `domContentLoadedEventEnd` in ms.
    pub dom_content_loaded_ms: Option<f64>,
    /// Navigation `loadEventEnd` in ms.
    pub load_ms: Option<f64>,
    /// Sum of `encodedBodySize` over the navigation and resource entries.
    pub transfer_bytes: Option<u64>,
    /// Number of resource timing entries.
    pub resource_count: Option<u32>,
    /// Total duration of long tasks in ms.
    pub long_tasks_ms: Option<f64>,
    /// Caveats, for example `lab value, no throttling, headless`.
    pub notes: Vec<String>,
}
