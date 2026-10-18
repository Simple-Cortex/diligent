//! Optional corroboration of the deterministic checks by an in-page axe-core run.
//!
//! The Rust checks stay authoritative. When a capture directory carries an
//! `axe_result` capture, [`apply`] compares every finding with what axe-core
//! reported for the same element. Agreement changes nothing. Any disagreement
//! moves the finding to `needs_review`, never to `pass` or `fail`, and keeps
//! the original state in the finding's corroboration record. Without an
//! `axe_result` capture the step does nothing and adds no extension.
//!
//! * [`map`]: which axe-core rules stand for which check.
//! * [`apply`](fn@apply): the comparison, the axe-only leads and the summary counts.

pub mod map;

mod apply;

/// Key under `extensions` that holds the corroboration record.
pub const EXTENSION_KEY: &str = "diligent.corroboration";

pub use apply::{
    apply, apply_with, AxeAt, CorroborationSummary, DISAGREE_LIMITATION, ENGINE, LEAD_LIMITATION,
};
