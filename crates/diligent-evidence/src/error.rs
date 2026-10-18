//! Errors for the evidence store.

use diligent_capture::CaptureError;
use std::path::{Path, PathBuf};

/// Why a store operation failed.
#[derive(Debug, thiserror::Error)]
pub enum EvidenceError {
    /// A file or directory could not be read or written.
    #[error("{}: {message}", path.display())]
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Operating-system message.
        message: String,
    },
    /// The capture directory failed to load or verify.
    #[error("capture set: {0}")]
    Capture(#[from] CaptureError),
    /// `STORE.json` names a store version this build does not understand.
    #[error("{}: store_version `{found}` is not supported, this build writes `{expected}`", path.display())]
    StoreVersion {
        /// The `STORE.json` path.
        path: PathBuf,
        /// Version found on disk.
        found: String,
        /// Version this build understands.
        expected: String,
    },
    /// The directory is not an initialised store.
    #[error("{}: not an evidence store (no STORE.json)", path.display())]
    NotAStore {
        /// The directory given.
        path: PathBuf,
    },
    /// A cache key leaves a dimension empty without saying why.
    #[error(
        "dimension `{0}` is empty and no reason was given; pass a non-empty missing reason for it"
    )]
    MissingReason(String),
    /// An argument or input record is not acceptable.
    #[error("{0}")]
    Invalid(String),
    /// A record the caller named does not exist.
    #[error("{0}")]
    NotFound(String),
}

impl EvidenceError {
    pub(crate) fn io(path: &Path, error: &std::io::Error) -> Self {
        EvidenceError::Io {
            path: path.to_path_buf(),
            message: error.to_string(),
        }
    }
}

/// Result alias for store operations.
pub type Result<T> = std::result::Result<T, EvidenceError>;
