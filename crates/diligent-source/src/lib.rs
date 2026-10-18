//! Static HTML pass: parser-based checks over saved HTML.
//!
//! Audit specification section 9.5 responsibility: parser-based checks and source leads
//! with resolved file locations and framework adapters.
//!
//! This crate looks at HTML text without a browser. It decides only what the
//! markup settles by itself: attribute presence, structure, token validity and
//! duplicate ids. It never claims that a WCAG success criterion passes. Each
//! finding is one evidence record plus one check result in the shapes of
//! `schemas/evidence-record.schema.json` and `schemas/check-result.schema.json`.
//! Rolling results up to rule level happens elsewhere.
//!
//! Entry points:
//! - [`scan_html`] scans one document held in memory.
//! - [`scan_path`] walks a file or directory of `.html` and `.htm` files.
//! - [`write_jsonl`] writes `EVIDENCE.jsonl` and `RESULTS.jsonl` rows.
//! - [`cli`] runs `source scan <path> [--out <dir>] [--json]`.

pub mod checks;
pub mod cli;
pub mod dom;
pub mod emit;
pub mod model;
pub mod scan;
pub mod sha256;
pub mod time;

pub use cli::cli;
pub use model::{CheckResult, EvidenceRecord, Finding, SourceRef, State};
pub use scan::{
    scan_html, scan_path, write_jsonl, ScanError, ScanOptions, ScanReport, SkippedFile,
};

/// The responsibility this crate owns in the audit specification's module boundaries.
pub const CRATE_ROLE: &str =
    "Parser-based checks and source leads with resolved file locations and framework adapters.";
