//! Capture data model and loader.
//!
//! The types here are what `diligent-rs collect` writes to disk: DOM snapshots,
//! accessibility trees, focus walks and network logs, plus the
//! `manifest.jsonl` that indexes them. `diligent-runtime` produces them and
//! re-exports them at their historical paths. [`CaptureSet`] reads a capture
//! directory back and verifies every file against the manifest hash.
//!
//! This crate has no browser dependency, so checks can run over recorded
//! captures without Chrome.

mod model;
mod set;

pub use model::{
    AxNode, AxTree, AxeError, AxeNode, AxeOutcome, AxeResult, AxeRule, BoundingBox, ColorScheme,
    DocumentMetrics, DomNode, DomSnapshot, FocusStop, FocusWalk, FocusWalkEnd, ManifestEntry,
    Media, NavigationInfo, NetworkEntry, PerfMetrics, Viewport, COMPUTED_STYLE_ALLOWLIST,
};
pub use set::{CaptureError, CaptureSet, MAX_FILE_BYTES, MAX_MANIFEST_LINES};
