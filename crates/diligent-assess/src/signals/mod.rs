//! Content, craft, form and lab performance signals.
//!
//! Signals are advisory readings of a capture directory. They are not check
//! results: they never enter `RESULTS.jsonl`, the applicability result or the
//! gate, they say `lead` rather than `fail`, and none of them says "required".
//! `assess` writes them to `SIGNALS.jsonl` unless `--no-signals` is given.
//!
//! * [`content`]: readability, passive voice, acronyms and link text of the main text.
//! * [`craft`]: type size, line length, fonts, spacing grid, radii, headings, contrast margin and motion.
//! * [`form`]: placeholder-only labels, autocomplete, required marking, error summary, steps, dates, uploads.
//! * [`perf`]: lab Core Web Vitals and load numbers from a `perf_metrics` capture.

use std::collections::BTreeMap;

use diligent_capture::CaptureSet;
use serde::Serialize;

use crate::capture_view::CaptureView;
use crate::checks::fed_common::is_block;
use crate::checks::Ctx;

pub mod content;
pub mod craft;
pub mod emit;
pub mod form;
pub mod model;
pub mod perf;

pub use emit::SignalRow;
pub use model::{info, Confidence, Family, Signal, SignalInfo, SignalState, Threshold, ALL};

/// Elements whose text is never part of the page's own prose.
const NEVER_PROSE: [&str; 4] = ["script", "style", "noscript", "template"];

/// The part of the page whose text counts as the main text.
///
/// The AX `main` landmark, else a `main` element or `role="main"`, else
/// `body` without `nav`, `header`, `footer`, `aside` and the matching
/// landmark roles.
pub struct Scope<'a> {
    v: &'a CaptureView<'a>,
    main: Option<usize>,
}

impl<'a> Scope<'a> {
    /// Finds the scope of `v`.
    pub fn new(v: &'a CaptureView<'a>) -> Scope<'a> {
        let mut main = v.ax.and_then(|tree| {
            tree.nodes
                .iter()
                .filter(|n| {
                    !n.ignored
                        && n.role
                            .as_deref()
                            .is_some_and(|r| r.eq_ignore_ascii_case("main"))
                })
                .filter_map(|n| n.backend_dom_node_id.and_then(|b| v.dom_index(b)))
                .find(|i| v.is_element(*i))
        });
        if main.is_none() {
            main = v.elements().find(|e| {
                v.tag(*e) == "main"
                    || v.attr(*e, "role").is_some_and(|r| {
                        r.split_ascii_whitespace()
                            .next()
                            .is_some_and(|t| t.eq_ignore_ascii_case("main"))
                    })
            });
        }
        Scope { v, main }
    }

    /// True when the page has a main landmark.
    pub fn has_main(&self) -> bool {
        self.main.is_some()
    }

    /// True when node `n` is inside the scope.
    pub fn contains(&self, n: usize) -> bool {
        let v = self.v;
        if let Some(m) = self.main {
            if n != m && !v.is_ancestor(m, n) {
                return false;
            }
        }
        !v.self_and_ancestors(n).any(|a| {
            if !v.is_element(a) {
                return false;
            }
            if NEVER_PROSE.contains(&v.tag(a)) {
                return true;
            }
            if self.main.is_some() {
                return false;
            }
            matches!(v.tag(a), "nav" | "header" | "footer" | "aside")
                || v.attr(a, "role").is_some_and(|r| {
                    matches!(
                        r.split_ascii_whitespace().next(),
                        Some("navigation" | "banner" | "contentinfo" | "complementary")
                    )
                })
        })
    }

    /// Visible text nodes in scope, in document order.
    pub fn text_nodes(&self) -> Vec<usize> {
        let v = self.v;
        (0..v.len())
            .filter(|t| {
                v.is_text(*t)
                    && v.node(*t)
                        .text
                        .as_deref()
                        .is_some_and(|s| !s.trim().is_empty())
                    && v.text_visible(*t)
                    && v.parent_element(*t).is_some_and(|p| !v.in_aria_hidden(p))
                    && self.contains(*t)
            })
            .collect()
    }

    /// Text of each block in scope, whitespace collapsed, in document order.
    pub fn blocks(&self) -> Vec<String> {
        let v = self.v;
        let mut out: Vec<(usize, String)> = Vec::new();
        for t in self.text_nodes() {
            let key = v
                .ancestors(t)
                .find(|a| v.is_element(*a) && is_block(v, *a))
                .or_else(|| v.parent(t))
                .unwrap_or(0);
            let text = v.node(t).text.as_deref().unwrap_or("");
            match out.last_mut() {
                Some((k, s)) if *k == key => s.push_str(text),
                _ => out.push((key, text.to_string())),
            }
        }
        out.into_iter()
            .map(|(_, s)| crate::capture_view::normalize_space(&s))
            .filter(|s| !s.is_empty())
            .collect()
    }

    /// The main text with a full stop closing every block that lacks one, for sentence counting.
    pub fn prose(&self) -> String {
        self.blocks()
            .iter()
            .map(|b| {
                if b.ends_with(['.', '!', '?', ':', ';', '…']) {
                    b.clone()
                } else {
                    format!("{b}.")
                }
            })
            .collect::<Vec<_>>()
            .join(" ")
    }

    /// The main text with one block per line, for acronym detection.
    pub fn lines(&self) -> String {
        self.blocks().join("\n")
    }
}

/// Rounds to `places` decimals, for values in rows.
pub fn round_to(x: f64, places: i32) -> f64 {
    let f = 10f64.powi(places);
    (x * f).round() / f
}

/// Runs every signal over `set`. Each key speaks at least once, in table order.
pub fn run(set: &CaptureSet) -> Vec<Signal> {
    let ctx = Ctx::new(set);
    let mut found: Vec<Signal> = Vec::new();
    found.extend(content::run(&ctx));
    found.extend(craft::run(&ctx));
    found.extend(form::run(&ctx));
    found.extend(perf::run(&ctx));
    let mut out: Vec<Signal> = Vec::with_capacity(found.len());
    for info in ALL.iter() {
        let mut rows: Vec<Signal> = found
            .iter()
            .filter(|s| s.key == info.key)
            .cloned()
            .collect();
        if rows.is_empty() {
            let mut s = Signal::new(
                info.key,
                SignalState::NotTested,
                serde_json::Value::Null,
                info.absent_reason,
            )
            .limit(info.absent_reason);
            if let Some(v) = ctx.primary.as_ref() {
                s = s.on(v);
            }
            rows.push(s);
        }
        out.extend(rows);
    }
    out
}

/// Counts of signals by state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct StateCounts {
    /// Leads.
    pub leads: usize,
    /// Within practice.
    pub within_practice: usize,
    /// Informational.
    pub informational: usize,
    /// Not tested.
    pub not_tested: usize,
}

impl StateCounts {
    fn add(&mut self, s: SignalState) {
        match s {
            SignalState::Lead => self.leads += 1,
            SignalState::WithinPractice => self.within_practice += 1,
            SignalState::Informational => self.informational += 1,
            SignalState::NotTested => self.not_tested += 1,
        }
    }
}

/// The `signals` block of the assess summary.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct SignalSummary {
    /// Leads.
    pub leads: usize,
    /// Within practice.
    pub within_practice: usize,
    /// Informational.
    pub informational: usize,
    /// Not tested.
    pub not_tested: usize,
    /// The same counts per family.
    pub by_family: BTreeMap<String, StateCounts>,
}

/// Counts `signals`.
pub fn summarize(signals: &[Signal]) -> SignalSummary {
    let mut total = StateCounts::default();
    let mut by_family: BTreeMap<String, StateCounts> = BTreeMap::new();
    for s in signals {
        total.add(s.state);
        if let Some(info) = info(s.key) {
            by_family
                .entry(info.family.as_str().to_string())
                .or_default()
                .add(s.state);
        }
    }
    SignalSummary {
        leads: total.leads,
        within_practice: total.within_practice,
        informational: total.informational,
        not_tested: total.not_tested,
        by_family,
    }
}
