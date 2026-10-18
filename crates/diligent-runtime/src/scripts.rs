//! Reviewed constant scripts.
//!
//! These are the only strings the Chrome adapter ever passes to
//! `Runtime.evaluate`. They take no input, build no code from data, and each
//! one states below exactly what it reads or does. Adding a script requires a
//! code review; no model-provided JavaScript ever runs.

/// Reads: `document.title` and the `lang` attribute of `<html>`.
/// Returns a JSON string `{"title": string, "lang": string}`. Mutates nothing.
pub const DOC_META: &str = "JSON.stringify({title: document.title || '', lang: (document.documentElement && document.documentElement.getAttribute('lang')) || ''})";

/// Mutates only focus: blurs the active element and scrolls to the top so the
/// next Tab starts at the beginning of the document. Returns `true`.
pub const FOCUS_RESET: &str = "(function(){ if (document.activeElement && document.activeElement !== document.body) { document.activeElement.blur(); } window.scrollTo(0, 0); return true; })()";

/// Returns the element that currently has focus (a remote object reference,
/// not data). The adapter resolves it to a backend node id with
/// `DOM.describeNode`. Reads `document.activeElement` only.
pub const ACTIVE_ELEMENT: &str = "document.activeElement";

/// Shared by the focus scripts: the focus-indicator signature of an element.
/// Covers outline, box-shadow, border color, background color, text color and
/// text decoration.
macro_rules! focus_signature {
    () => {
        "function sig(el){ var s = getComputedStyle(el); return [s.outlineStyle, s.outlineWidth, s.outlineColor, s.boxShadow, s.borderTopColor, s.borderBottomColor, s.backgroundColor, s.color, s.textDecorationLine].join('|'); } "
    };
}

/// Run once before a focus walk, while nothing has focus. Reads the
/// focus-indicator signature of every candidate focusable element (links with
/// `href`, `area[href]`, `button`, `input`, `select`, `textarea`, `summary`,
/// `[tabindex]`, editable content, controls on `audio` and `video`, `object`,
/// `embed`) plus every element whose content overflows its box (browsers make
/// scrollable regions keyboard focusable), and keeps them in a `WeakMap` held in a non-enumerable, deletable
/// `window.__diligentFocusBaseline`. Returns the number of candidates. Nothing
/// else is mutated. [`FOCUS_FINISH`] removes the property.
pub const FOCUS_BASELINE: &str = concat!(
    "(function(){ ",
    focus_signature!(),
    "var m = new WeakMap(); var list = document.querySelectorAll('a[href],area[href],button,input,select,textarea,summary,[tabindex],[contenteditable]:not([contenteditable=false]),audio[controls],video[controls],object,embed'); for (var i = 0; i < list.length; i++) { m.set(list[i], sig(list[i])); } var all = document.getElementsByTagName('*'); for (var j = 0; j < all.length; j++) { var el = all[j]; if (!m.has(el) && (el.scrollHeight > el.clientHeight || el.scrollWidth > el.clientWidth)) { m.set(el, sig(el)); } } Object.defineProperty(window, '__diligentFocusBaseline', {value: m, configurable: true}); return list.length; })()"
);

/// Reads, for `document.activeElement`: tag name, an approximate accessible
/// name (aria-label, alt, value, title or trimmed text, cut to 120 chars),
/// its `getBoundingClientRect()`, and whether its focus-indicator signature
/// differs from the one [`FOCUS_BASELINE`] recorded while it was unfocused.
/// It does not blur or refocus anything, so the page's own focus and blur
/// handlers see only the Tab key. Returns a JSON string
/// `{"tag": string, "name": string, "is_body": bool, "indicator": bool|null, "rect": {"x","y","width","height"}}`.
/// `indicator` is `null` when the element was not a baseline candidate; `rect`
/// is absent when focus is on the body.
pub const FOCUS_READ: &str = concat!(
    "(function(){ var e = document.activeElement; if (!e || e === document.body || e === document.documentElement) { return JSON.stringify({tag: e ? e.tagName.toLowerCase() : '', name: '', is_body: true, indicator: false}); } ",
    focus_signature!(),
    "var m = window.__diligentFocusBaseline; var base = m ? m.get(e) : undefined; var r = e.getBoundingClientRect(); var focused = sig(e); var name = (e.getAttribute('aria-label') || e.getAttribute('alt') || e.getAttribute('value') || e.getAttribute('title') || e.textContent || '').trim().replace(/\\s+/g, ' ').slice(0, 120); return JSON.stringify({tag: e.tagName.toLowerCase(), name: name, is_body: false, indicator: base === undefined ? null : focused !== base, rect: {x: r.x, y: r.y, width: r.width, height: r.height}}); })()"
);

/// Removes `window.__diligentFocusBaseline`. Returns `true`.
pub const FOCUS_FINISH: &str =
    "(function(){ delete window.__diligentFocusBaseline; return true; })()";

/// Reads `document.scrollingElement` (falling back to `document.documentElement`)
/// for `scrollWidth` and `scrollHeight`, and `document.documentElement` for
/// `clientWidth` and `clientHeight`. Returns a JSON string
/// `{"scroll_width": int, "scroll_height": int, "client_width": int, "client_height": int}`.
/// Mutates nothing.
pub const DOCUMENT_METRICS: &str = "(function(){ var d = document.documentElement; var s = document.scrollingElement || d; return JSON.stringify({scroll_width: s.scrollWidth, scroll_height: s.scrollHeight, client_width: d.clientWidth, client_height: d.clientHeight}); })()";

/// Text-spacing capture mode, step one. Appends one `<style data-diligent="text-spacing">`
/// element to `<head>` (or the root element) holding the WCAG 1.4.12 metrics:
/// `* { line-height: 1.5 !important; letter-spacing: 0.12em !important; word-spacing: 0.16em !important; }`
/// and `p { margin-bottom: 2em !important; }`. Removes any earlier element it
/// added first. Returns `true` when the browser attached a stylesheet to the
/// element and `false` when it did not (for example a Content-Security-Policy
/// forbids inline styles). Mutates only that one element.
pub const TEXT_SPACING_APPLY: &str = "(function(){ var old = document.querySelectorAll('style[data-diligent=\"text-spacing\"]'); for (var i = 0; i < old.length; i++) { old[i].remove(); } var el = document.createElement('style'); el.setAttribute('data-diligent', 'text-spacing'); el.textContent = '* { line-height: 1.5 !important; letter-spacing: 0.12em !important; word-spacing: 0.16em !important; } p { margin-bottom: 2em !important; }'; (document.head || document.documentElement).appendChild(el); return el.sheet !== null; })()";

/// Text-spacing capture mode, step two. Removes every
/// `<style data-diligent="text-spacing">` element and nothing else. Returns the
/// number removed.
pub const TEXT_SPACING_REMOVE: &str = "(function(){ var old = document.querySelectorAll('style[data-diligent=\"text-spacing\"]'); var n = old.length; for (var i = 0; i < n; i++) { old[i].remove(); } return n; })()";

// ---------------------------------------------------------------------------
// axe-core corroborator (optional, off by default). See docs/browser-worker.md,
// "Reviewed scripts", for the policy exception that covers the vendored bundle.
// ---------------------------------------------------------------------------

/// The vendored axe-core bundle, unmodified from the npm tarball. The one
/// third-party script the worker evaluates. It is pinned by [`AXE_VERSION`] and
/// [`AXE_BUNDLE_SHA256`], reviewed as a unit against its upstream release (see
/// `vendor/axe-core/NOTICE.md`), and only ever invoked through [`AXE_RUN`]. It
/// runs inside the audited page in headless Chrome, never in the engine process.
pub const AXE_BUNDLE: &str = include_str!("../vendor/axe-core/axe.min.js");

/// Version of [`AXE_BUNDLE`].
pub const AXE_VERSION: &str = "4.14.0";

/// Lowercase hex SHA-256 of [`AXE_BUNDLE`]. A unit test re-hashes the bundle so
/// a silent edit fails CI.
pub const AXE_BUNDLE_SHA256: &str =
    "20c09fe157a8a34a30e241aaa1fcdade657734f08ab379ecfbeb7d45cc46e878";

/// Reads `typeof window.axe`. Returns `true` when axe is already defined on the
/// page (the bundle is then not evaluated again). Mutates nothing.
pub const AXE_PRESENT: &str = "typeof window.axe !== 'undefined'";

/// The only way [`AXE_BUNDLE`] is invoked. Takes no input and builds no code
/// from data. An async IIFE that calls
/// `axe.run(document, {iframes: false, elementRef: false, selectors: true,
/// ancestry: false, xpath: false, preload: false, resultTypes: [all four],
/// reporter: 'v2', rules: {target-size, identical-links-same-purpose enabled}})`
/// and returns one JSON string shaped like `diligent_capture::AxeResult`
/// (without `error`; `summary` is the first check message of a node, taken from
/// `any`, `all`, `none` in that order, because the `v2` reporter has no
/// `failureSummary`), with rules sorted by id, nodes sorted by selector and
/// capped at 500 per rule. Reads the DOM and computed styles through axe;
/// mutates only the scroll position (saved and restored) and, through axe, its
/// own caches ([`AXE_CLEANUP`] drops them). `preload: false` keeps axe from
/// fetching stylesheets, so the run makes no network requests. The two enabled
/// rules are the ones axe ships disabled that the Rust checks have a
/// counterpart for. Nothing time-dependent, no HTML snippets, no element
/// references are returned.
///
/// COUPLING: the `sel` function below is a copy of `CaptureView::selector` in
/// `crates/diligent-assess/src/capture_view.rs`: `tag#id` when the id is the
/// only element id of that value in the document (`tag[id="..."]` with `\` and
/// `"` escaped when the id is not a simple identifier), otherwise the
/// `html > body > tag:nth-child(n) > ...` path built from the element up. Change
/// both together; the corroboration step matches findings to axe nodes by exact
/// selector equality. The Rust side also stops climbing at 2048 ancestors,
/// which no real page reaches.
pub const AXE_RUN: &str = r#"(async function(){
var MAX_NODES = 500;
var counts = new Map();
var all = document.getElementsByTagName('*');
for (var i = 0; i < all.length; i++) { var v = all[i].getAttribute('id'); if (v) { counts.set(v, (counts.get(v) || 0) + 1); } }
function tag(el) { return el.localName.toLowerCase(); }
function simple(id) { return /^[A-Za-z_][A-Za-z0-9_-]*$/.test(id); }
function sel(el) {
  var id = el.getAttribute('id');
  if (id && counts.get(id) === 1) {
    if (simple(id)) { return tag(el) + '#' + id; }
    return tag(el) + '[id="' + id.replace(/\\/g, '\\\\').replace(/"/g, '\\"') + '"]';
  }
  var parts = [];
  var e = el;
  while (e) {
    var p = e.parentElement;
    if (p) { parts.push(tag(e) + ':nth-child(' + (Array.prototype.indexOf.call(p.children, e) + 1) + ')'); } else { parts.push(tag(e)); }
    e = p;
  }
  parts.reverse();
  return parts.join(' > ');
}
function resolve(node) {
  var t = node && node.target;
  if (!Array.isArray(t) || t.length !== 1 || typeof t[0] !== 'string') { return null; }
  try { var m = document.querySelectorAll(t[0]); return m.length === 1 ? sel(m[0]) : null; } catch (err) { return null; }
}
function summary(node) {
  var groups = [node && node.any, node && node.all, node && node.none];
  for (var g = 0; g < groups.length; g++) {
    var checks = groups[g] || [];
    for (var c = 0; c < checks.length; c++) {
      var m = checks[c] && checks[c].message;
      if (typeof m === 'string' && m.trim().length > 0) { return m.trim().split('\n')[0].slice(0, 200); }
    }
  }
  return null;
}
function cmp(a, b) { return a < b ? -1 : (a > b ? 1 : 0); }
var sx = window.scrollX, sy = window.scrollY;
var res;
try {
  res = await axe.run(document, {
    iframes: false, elementRef: false, selectors: true, ancestry: false, xpath: false, preload: false,
    resultTypes: ['violations', 'passes', 'incomplete', 'inapplicable'], reporter: 'v2',
    rules: {'target-size': {enabled: true}, 'identical-links-same-purpose': {enabled: true}}
  });
} finally { window.scrollTo(sx, sy); }
var meta = new Map();
axe.getRules().forEach(function(r) { meta.set(r.ruleId, {tags: (r.tags || []).slice().sort(), act: (r.actIds || []).slice().sort()}); });
var rank = {violation: 3, incomplete: 2, pass: 1, inapplicable: 0};
var rules = new Map();
function take(list, nodeOutcome, ruleOutcome) {
  (list || []).forEach(function(r) {
    var m = meta.get(r.id) || {tags: (r.tags || []).slice().sort(), act: []};
    var e = rules.get(r.id);
    if (!e) { e = {id: r.id, act_ids: m.act, tags: m.tags, outcome: ruleOutcome, impact: null, nodes: [], truncated: false}; rules.set(r.id, e); }
    if (rank[ruleOutcome] > rank[e.outcome]) { e.outcome = ruleOutcome; }
    if (e.impact === null && typeof r.impact === 'string') { e.impact = r.impact; }
    (r.nodes || []).forEach(function(n) {
      if (nodeOutcome === null) { return; }
      e.nodes.push({selector: resolve(n), outcome: nodeOutcome, summary: nodeOutcome === 'pass' ? null : summary(n)});
    });
  });
}
take(res.violations, 'violation', 'violation');
take(res.incomplete, 'incomplete', 'incomplete');
take(res.passes, 'pass', 'pass');
take(res.inapplicable, null, 'inapplicable');
var out = [];
rules.forEach(function(e) {
  e.nodes.sort(function(a, b) {
    var sa = a.selector === null ? '' : a.selector, sb = b.selector === null ? '' : b.selector;
    return cmp(a.selector === null ? 0 : 1, b.selector === null ? 0 : 1) || cmp(sa, sb) || cmp(a.outcome, b.outcome) || cmp(a.summary || '', b.summary || '');
  });
  if (e.nodes.length > MAX_NODES) { e.nodes = e.nodes.slice(0, MAX_NODES); e.truncated = true; }
  out.push(e);
});
out.sort(function(a, b) { return cmp(a.id, b.id); });
return JSON.stringify({engine: 'axe-core', version: axe.version, url: document.URL, rules: out});
})()"#;

/// Drops axe's internal caches (`axe.teardown()`, errors ignored) and deletes
/// `window.axe`. Returns `true`. Always run after [`AXE_RUN`], even on failure.
pub const AXE_CLEANUP: &str = "(function(){ try { if (window.axe && typeof window.axe.teardown === 'function') { window.axe.teardown(); } } catch (e) {} try { delete window.axe; } catch (e) {} return true; })()";

// ---------------------------------------------------------------------------
// Lab performance metrics (`perf_metrics`, on by default, `collect --no-perf`).
// ---------------------------------------------------------------------------

/// Installed before `navigate` with `Page.addScriptToEvaluateOnNewDocument`, so
/// it runs at the start of every document the tab loads. Does nothing in
/// subframes. Creates the reserved, non-enumerable, deletable global
/// `window.__diligentPerf` and starts four buffered `PerformanceObserver`s:
/// `largest-contentful-paint` (keeps the last entry's `startTime` and element),
/// `layout-shift` (keeps `[startTime, value]` of entries without
/// `hadRecentInput`), `paint` (keeps the `first-contentful-paint` start) and
/// `longtask` (sums `duration`). An observer type the browser lacks is listed in
/// `missing`. Takes no input; reads and mutates nothing on the page.
pub const PERF_INSTALL: &str = r#"(function(){
if (window.top !== window || window.__diligentPerf) { return; }
var st = {lcp: null, lcpEl: null, shifts: [], fcp: null, longMs: 0, missing: [], obs: []};
function watch(type, fn) {
  try {
    var o = new PerformanceObserver(function(list) { list.getEntries().forEach(fn); });
    o.observe({type: type, buffered: true});
    st.obs.push(o);
  } catch (e) { st.missing.push(type); }
}
watch('largest-contentful-paint', function(e) { st.lcp = e.startTime; st.lcpEl = e.element || null; });
watch('layout-shift', function(e) { if (!e.hadRecentInput) { st.shifts.push([e.startTime, e.value]); } });
watch('paint', function(e) { if (e.name === 'first-contentful-paint') { st.fcp = e.startTime; } });
watch('longtask', function(e) { st.longMs += e.duration; });
Object.defineProperty(window, '__diligentPerf', {value: st, configurable: true});
})();"#;

/// Reads what [`PERF_INSTALL`] collected plus `performance.getEntriesByType`
/// for `navigation` and `resource`, and returns one JSON string shaped like
/// `diligent_capture::PerfMetrics`. Mutates nothing.
///
/// - LCP is the last entry's `startTime`; `lcp_element` is its selector.
/// - CLS is the largest session window of the recorded shifts: a window starts
///   at a shift, takes following shifts less than 1000 ms after the previous one
///   and less than 5000 ms after the window's first, and its value is the sum.
///   `0` when the observer ran and saw no shift; `null` when it is unavailable.
/// - TTFB is the navigation `responseStart`, DCL `domContentLoadedEventEnd`,
///   load `loadEventEnd` (`null` while it is 0).
/// - Transfer bytes sum `encodedBodySize` over the navigation entry and every
///   resource entry; the resource count excludes the navigation entry.
/// - Times are rounded to one decimal, CLS to four.
///
/// COUPLING: `sel` is the same selector algorithm as in [`AXE_RUN`] and
/// `CaptureView::selector` in `crates/diligent-assess/src/capture_view.rs`.
/// Change all three together.
pub const PERF_READ: &str = r#"(function(){
var st = window.__diligentPerf;
function r1(x) { return (typeof x === 'number' && isFinite(x)) ? Math.round(x * 10) / 10 : null; }
function r4(x) { return (typeof x === 'number' && isFinite(x)) ? Math.round(x * 10000) / 10000 : null; }
if (!st) {
  return JSON.stringify({label: 'lab', lcp_ms: null, lcp_element: null, cls: null, fcp_ms: null, ttfb_ms: null, dom_content_loaded_ms: null, load_ms: null, transfer_bytes: null, resource_count: null, long_tasks_ms: null, notes: ['error: the performance observers were not installed']});
}
var counts = new Map();
var all = document.getElementsByTagName('*');
for (var i = 0; i < all.length; i++) { var v = all[i].getAttribute('id'); if (v) { counts.set(v, (counts.get(v) || 0) + 1); } }
function tag(el) { return el.localName.toLowerCase(); }
function sel(el) {
  var id = el.getAttribute('id');
  if (id && counts.get(id) === 1) {
    if (/^[A-Za-z_][A-Za-z0-9_-]*$/.test(id)) { return tag(el) + '#' + id; }
    return tag(el) + '[id="' + id.replace(/\\/g, '\\\\').replace(/"/g, '\\"') + '"]';
  }
  var parts = [];
  var e = el;
  while (e) {
    var p = e.parentElement;
    if (p) { parts.push(tag(e) + ':nth-child(' + (Array.prototype.indexOf.call(p.children, e) + 1) + ')'); } else { parts.push(tag(e)); }
    e = p;
  }
  parts.reverse();
  return parts.join(' > ');
}
var cls = null;
if (st.missing.indexOf('layout-shift') === -1) {
  cls = 0;
  var shifts = st.shifts.slice().sort(function(a, b) { return a[0] - b[0]; });
  var sum = 0, first = 0, last = 0;
  for (var k = 0; k < shifts.length; k++) {
    var t = shifts[k][0];
    if (k === 0 || t - last >= 1000 || t - first >= 5000) { sum = 0; first = t; }
    sum += shifts[k][1];
    last = t;
    if (sum > cls) { cls = sum; }
  }
}
var nav = performance.getEntriesByType('navigation')[0];
var res = performance.getEntriesByType('resource');
var bytes = nav ? nav.encodedBodySize || 0 : 0;
for (var j = 0; j < res.length; j++) { bytes += res[j].encodedBodySize || 0; }
var el = (st.lcpEl && st.lcpEl.isConnected) ? sel(st.lcpEl) : null;
var notes = ['lab value, no throttling, headless'];
st.missing.forEach(function(t) { notes.push('observer unavailable: ' + t); });
return JSON.stringify({
  label: 'lab',
  lcp_ms: r1(st.lcp), lcp_element: el, cls: r4(cls), fcp_ms: r1(st.fcp),
  ttfb_ms: nav ? r1(nav.responseStart) : null,
  dom_content_loaded_ms: nav ? r1(nav.domContentLoadedEventEnd) : null,
  load_ms: (nav && nav.loadEventEnd > 0) ? r1(nav.loadEventEnd) : null,
  transfer_bytes: nav ? Math.round(bytes) : null,
  resource_count: res.length,
  long_tasks_ms: st.missing.indexOf('longtask') === -1 ? r1(st.longMs) : null,
  notes: notes
});
})()"#;

/// Disconnects the observers and deletes `window.__diligentPerf`. Returns `true`.
pub const PERF_CLEANUP: &str = "(function(){ var st = window.__diligentPerf; if (st) { st.obs.forEach(function(o){ try { o.disconnect(); } catch (e) {} }); } try { delete window.__diligentPerf; } catch (e) {} return true; })()";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn axe_bundle_matches_pinned_digest() {
        assert_eq!(
            crate::util::sha256_hex(AXE_BUNDLE.as_bytes()),
            AXE_BUNDLE_SHA256
        );
    }

    #[test]
    fn axe_bundle_declares_pinned_version() {
        assert!(AXE_BUNDLE.starts_with(&format!("/*! axe v{AXE_VERSION}")));
    }
}
