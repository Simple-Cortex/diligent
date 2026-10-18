//! The `Browser` trait and the capture data model.
//!
//! The trait is the seam between Diligent's orchestration and a concrete CDP
//! client. The Chrome adapter implements it; tests and future clients can
//! implement it without touching the collector. The capture types live in
//! `diligent-capture` and are re-exported here at their original paths.

use crate::protocol::{ErrorKind, Media, Viewport, WorkerError};
use async_trait::async_trait;
use std::time::Duration;

pub use diligent_capture::{
    AxNode, AxTree, AxeError, AxeNode, AxeOutcome, AxeResult, AxeRule, BoundingBox,
    DocumentMetrics, DomNode, DomSnapshot, FocusStop, FocusWalk, FocusWalkEnd, NavigationInfo,
    NetworkEntry, PerfMetrics, COMPUTED_STYLE_ALLOWLIST,
};

/// Abstraction over a controllable browser page.
#[async_trait]
pub trait Browser: Send {
    /// Navigate to `url` and wait for load. Enforces the origin allowlist.
    async fn navigate(
        &mut self,
        url: &str,
        timeout: Duration,
    ) -> Result<NavigationInfo, WorkerError>;
    /// Apply a viewport.
    async fn set_viewport(&mut self, viewport: Viewport) -> Result<(), WorkerError>;
    /// Apply media-feature emulation.
    async fn emulate_media(&mut self, media: Media) -> Result<(), WorkerError>;
    /// Capture the flattened DOM with layout boxes and computed styles.
    async fn dom_snapshot(&mut self) -> Result<DomSnapshot, WorkerError>;
    /// Capture the full accessibility tree.
    async fn ax_tree(&mut self) -> Result<AxTree, WorkerError>;
    /// Walk the keyboard focus order with synthesized Tab presses.
    async fn focus_order(&mut self, max_tabs: usize) -> Result<FocusWalk, WorkerError>;
    /// Inject the reviewed WCAG 1.4.12 text-spacing stylesheet (`text-spacing` capture mode).
    async fn apply_text_spacing(&mut self) -> Result<(), WorkerError> {
        Err(WorkerError::new(
            ErrorKind::UnsupportedFeature,
            "this browser cannot apply the text-spacing stylesheet",
        ))
    }
    /// Remove the stylesheet injected by [`Browser::apply_text_spacing`].
    async fn remove_text_spacing(&mut self) -> Result<(), WorkerError> {
        Ok(())
    }
    /// Run the optional axe-core corroborator on the current page and return its
    /// normalized result. Rust checks stay authoritative; this is a second opinion.
    async fn run_corroborator(&mut self) -> Result<AxeResult, WorkerError> {
        Err(WorkerError::new(
            ErrorKind::UnsupportedFeature,
            "this browser cannot run the axe-core corroborator",
        ))
    }
    /// Install the lab-performance observers so they run from the start of every
    /// document loaded afterwards. Idempotent. Call before `navigate`.
    async fn install_perf(&mut self) -> Result<(), WorkerError> {
        Ok(())
    }
    /// Remove what [`Browser::install_perf`] installed. Idempotent; no-op when nothing is installed.
    async fn remove_perf(&mut self) -> Result<(), WorkerError> {
        Ok(())
    }
    /// Wait for the page to settle, read the lab performance metrics and drop the
    /// page-side state. Needs [`Browser::install_perf`] before the navigation.
    async fn read_perf(&mut self) -> Result<PerfMetrics, WorkerError> {
        Err(WorkerError::new(
            ErrorKind::UnsupportedFeature,
            "this browser cannot read lab performance metrics",
        ))
    }
    /// Capture a PNG screenshot of the viewport.
    async fn screenshot(&mut self) -> Result<Vec<u8>, WorkerError>;
    /// Network entries recorded since the last navigation.
    async fn network_log(&mut self) -> Result<Vec<NetworkEntry>, WorkerError>;
    /// Focus the first element matching a CSS selector.
    async fn focus(&mut self, selector: &str) -> Result<(), WorkerError>;
    /// Press one key from the safe allowlist.
    async fn press_key(&mut self, key: &str) -> Result<(), WorkerError>;
    /// Focus the element matching `selector` and insert `text`.
    async fn fill_text(&mut self, selector: &str, text: &str) -> Result<(), WorkerError>;
    /// Close the browser and release the process.
    async fn close(&mut self) -> Result<(), WorkerError>;
}
