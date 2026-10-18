//! Evidence hashing, provenance, redaction and freshness.
//!
//! Audit specification section 9.5 responsibility: Hashing, provenance, redaction, artifact storage, freshness and attestation validation.
//!
//! [`Store`] is an audit-level, content-addressed evidence store. Payload
//! bytes live under `objects/<sha256>.<ext>`; `index.jsonl`, `keys.jsonl` and
//! `reuse.jsonl` are append-only logs that say what each object is, which
//! cache keys select it and when a later run reused it. A resumed run asks
//! [`Store::plan_resume`] what it may skip; only complete, version-compatible
//! work whose objects still re-hash is reused.
//!
//! Attestation validation is deferred; see `docs/evidence-store.md`.
//!
//! The helpers [`atomic_write`], [`append_line`], [`canonical_json`] and
//! [`sha256_hex`] are public so the other crates can adopt them.

mod canonical;
mod cli;
mod error;
mod io;
mod model;
mod store;
mod time;

pub use canonical::canonical_json;
pub use cli::cli;
pub use error::{EvidenceError, Result};
pub use io::{append_line, atomic_write, atomic_write_new, atomic_write_with, sha256_hex};
pub use model::{
    is_fresh, is_run_id, is_sha256_hex, CacheKey, Deployment, EvidenceRecord, IndexEntry,
    IndexRecord, IngestContext, IngestReport, KeyRecord, KeyStatus, Problem, Producer, Redaction,
    ResumePlan, ReuseRecord, Tombstone, VerifyReport, ViewportKey, EVIDENCE_KINDS,
    OPTIONAL_DIMENSIONS, SCHEMA_VERSION, STORE_VERSION,
};
pub use store::Store;
pub use time::{format_unix_seconds, now_rfc3339, parse_rfc3339};

/// The responsibility this crate owns in the audit specification's module boundaries.
pub const CRATE_ROLE: &str =
    "Hashing, provenance, redaction, artifact storage, freshness and attestation validation.";
