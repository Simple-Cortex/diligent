//! Loader for a capture directory: `manifest.jsonl` plus content-addressed files.

use crate::model::{
    AxTree, AxeResult, DomSnapshot, FocusWalk, ManifestEntry, NetworkEntry, PerfMetrics, Viewport,
};
use serde::de::DeserializeOwned;
use sha2::{Digest, Sha256};
use std::fmt;
use std::fs;
use std::io::Read;
use std::path::{Path, PathBuf};

/// Longest manifest accepted, in lines.
pub const MAX_MANIFEST_LINES: usize = 10_000;
/// Largest capture file accepted, in bytes (64 MiB).
pub const MAX_FILE_BYTES: u64 = 64 * 1024 * 1024;

/// Why a capture directory could not be loaded.
///
/// Every variant names the offending path; manifest problems also carry the
/// 1-based line number.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CaptureError {
    /// A file or directory could not be read.
    Io {
        /// Path that failed.
        path: PathBuf,
        /// Operating-system message.
        message: String,
    },
    /// The manifest has more than [`MAX_MANIFEST_LINES`] lines.
    ManifestTooLarge {
        /// Manifest path.
        path: PathBuf,
        /// The limit that was exceeded.
        limit: usize,
    },
    /// A capture file is larger than [`MAX_FILE_BYTES`].
    FileTooLarge {
        /// File path.
        path: PathBuf,
        /// Actual size in bytes.
        bytes: u64,
        /// The limit that was exceeded.
        limit: u64,
    },
    /// A file's SHA-256 does not match the manifest.
    HashMismatch {
        /// File path.
        path: PathBuf,
        /// Hash the manifest declares.
        expected: String,
        /// Hash of the bytes on disk.
        actual: String,
    },
    /// A manifest line or a capture file is not valid for its type.
    Malformed {
        /// Manifest or file path.
        path: PathBuf,
        /// 1-based manifest line, when the problem is a manifest line.
        line: Option<usize>,
        /// What is wrong.
        message: String,
    },
}

impl fmt::Display for CaptureError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CaptureError::Io { path, message } => write!(f, "{}: {message}", path.display()),
            CaptureError::ManifestTooLarge { path, limit } => {
                write!(
                    f,
                    "{}: manifest has more than {limit} lines",
                    path.display()
                )
            }
            CaptureError::FileTooLarge { path, bytes, limit } => {
                write!(
                    f,
                    "{}: file is {bytes} bytes, over the {limit} byte limit",
                    path.display()
                )
            }
            CaptureError::HashMismatch {
                path,
                expected,
                actual,
            } => write!(
                f,
                "{}: sha256 mismatch, manifest says {expected}, file is {actual}",
                path.display()
            ),
            CaptureError::Malformed {
                path,
                line: Some(line),
                message,
            } => {
                write!(f, "{}:{line}: {message}", path.display())
            }
            CaptureError::Malformed {
                path,
                line: None,
                message,
            } => {
                write!(f, "{}: {message}", path.display())
            }
        }
    }
}

impl std::error::Error for CaptureError {}

#[derive(Debug)]
enum Payload {
    Dom(DomSnapshot),
    Ax(AxTree),
    Focus(FocusWalk),
    Network(Vec<NetworkEntry>),
    Axe(AxeResult),
    Perf(PerfMetrics),
    /// Verified but not parsed (screenshots and unknown kinds).
    Opaque,
}

/// A loaded, hash-verified capture directory.
#[derive(Debug)]
pub struct CaptureSet {
    dir: PathBuf,
    entries: Vec<ManifestEntry>,
    // Parallel to `entries`.
    payloads: Vec<Payload>,
}

fn sha256_hex(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    let mut out = String::with_capacity(64);
    for byte in digest.iter() {
        out.push_str(&format!("{byte:02x}"));
    }
    out
}

fn io_error(path: &Path, error: &std::io::Error) -> CaptureError {
    CaptureError::Io {
        path: path.to_path_buf(),
        message: error.to_string(),
    }
}

fn same_viewport(a: Viewport, b: Viewport) -> bool {
    a.width == b.width && a.height == b.height && (a.dpr - b.dpr).abs() < 1e-6
}

/// A manifest `path` must be one plain file name inside the capture directory.
fn is_plain_file_name(name: &str) -> bool {
    !name.is_empty()
        && name != "."
        && name != ".."
        && !name.contains(['/', '\\', '\0'])
        && !name.contains(':')
}

fn read_limited(path: &Path) -> Result<Vec<u8>, CaptureError> {
    let file = fs::File::open(path).map_err(|e| io_error(path, &e))?;
    let size = file.metadata().map_err(|e| io_error(path, &e))?.len();
    if size > MAX_FILE_BYTES {
        return Err(CaptureError::FileTooLarge {
            path: path.to_path_buf(),
            bytes: size,
            limit: MAX_FILE_BYTES,
        });
    }
    let mut bytes = Vec::with_capacity(size as usize);
    // `take` keeps a file that grows after the size check from exceeding the limit.
    file.take(MAX_FILE_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| io_error(path, &e))?;
    if bytes.len() as u64 > MAX_FILE_BYTES {
        return Err(CaptureError::FileTooLarge {
            path: path.to_path_buf(),
            bytes: bytes.len() as u64,
            limit: MAX_FILE_BYTES,
        });
    }
    Ok(bytes)
}

fn parse_json<T: DeserializeOwned>(
    path: &Path,
    bytes: &[u8],
    what: &str,
) -> Result<T, CaptureError> {
    serde_json::from_slice(bytes).map_err(|e| CaptureError::Malformed {
        path: path.to_path_buf(),
        line: None,
        message: format!("invalid {what}: {e}"),
    })
}

impl CaptureSet {
    /// Load `dir/manifest.jsonl`, verify every listed file against its
    /// SHA-256 and parse the JSON captures.
    ///
    /// Refuses a manifest over [`MAX_MANIFEST_LINES`] lines and any file over
    /// [`MAX_FILE_BYTES`]. Never panics on malformed input.
    pub fn load(dir: &Path) -> Result<CaptureSet, CaptureError> {
        let manifest_path = dir.join("manifest.jsonl");
        let manifest_bytes = read_limited(&manifest_path)?;
        let manifest_text =
            std::str::from_utf8(&manifest_bytes).map_err(|e| CaptureError::Malformed {
                path: manifest_path.clone(),
                line: None,
                message: format!("manifest is not UTF-8: {e}"),
            })?;

        let mut entries: Vec<ManifestEntry> = Vec::new();
        // Path -> (position in `entries`, 1-based manifest line).
        let mut first_line_of_path: std::collections::HashMap<String, (usize, usize)> =
            std::collections::HashMap::new();
        for (index, text) in manifest_text.lines().enumerate() {
            if index >= MAX_MANIFEST_LINES {
                return Err(CaptureError::ManifestTooLarge {
                    path: manifest_path,
                    limit: MAX_MANIFEST_LINES,
                });
            }
            if text.trim().is_empty() {
                continue;
            }
            let entry: ManifestEntry =
                serde_json::from_str(text).map_err(|e| CaptureError::Malformed {
                    path: manifest_path.clone(),
                    line: Some(index + 1),
                    message: e.to_string(),
                })?;
            if !is_plain_file_name(&entry.path) {
                return Err(CaptureError::Malformed {
                    path: manifest_path.clone(),
                    line: Some(index + 1),
                    message: format!("path `{}` is not a plain file name", entry.path),
                });
            }
            // An append-only manifest can repeat a line after a resumed run.
            // Identical lines collapse. Two lines that name one file but
            // disagree about what it is cannot both be right.
            if let Some(&earlier) = first_line_of_path.get(entry.path.as_str()) {
                let prior: &ManifestEntry = &entries[earlier.0];
                if *prior == entry {
                    continue;
                }
                if prior.sha256 != entry.sha256
                    || prior.bytes != entry.bytes
                    || prior.kind != entry.kind
                {
                    return Err(CaptureError::Malformed {
                        path: manifest_path.clone(),
                        line: Some(index + 1),
                        message: format!(
                            "conflicting manifest line for `{}`: it disagrees with line {} about sha256, bytes or kind",
                            entry.path,
                            earlier.1
                        ),
                    });
                }
            } else {
                first_line_of_path.insert(entry.path.clone(), (entries.len(), index + 1));
            }
            entries.push(entry);
        }

        let mut payloads = Vec::with_capacity(entries.len());
        for (index, entry) in entries.iter().enumerate() {
            let path = dir.join(&entry.path);
            // Entries that share a file reuse the first parse of it.
            if let Some(earlier) = entries[..index].iter().position(|e| {
                e.path == entry.path && e.kind == entry.kind && e.sha256 == entry.sha256
            }) {
                let copy = match &payloads[earlier] {
                    Payload::Dom(v) => Payload::Dom(v.clone()),
                    Payload::Ax(v) => Payload::Ax(v.clone()),
                    Payload::Focus(v) => Payload::Focus(v.clone()),
                    Payload::Network(v) => Payload::Network(v.clone()),
                    Payload::Axe(v) => Payload::Axe(v.clone()),
                    Payload::Perf(v) => Payload::Perf(v.clone()),
                    Payload::Opaque => Payload::Opaque,
                };
                payloads.push(copy);
                continue;
            }
            let bytes = read_limited(&path)?;
            let actual = sha256_hex(&bytes);
            if !actual.eq_ignore_ascii_case(&entry.sha256) {
                return Err(CaptureError::HashMismatch {
                    path,
                    expected: entry.sha256.clone(),
                    actual,
                });
            }
            let payload = match entry.kind.as_str() {
                "dom_snapshot" => Payload::Dom(parse_json(&path, &bytes, "dom_snapshot")?),
                "ax_tree" => Payload::Ax(parse_json(&path, &bytes, "ax_tree")?),
                "focus_order" => {
                    Payload::Focus(parse_json(&path, &bytes, "focus_order (FocusWalk)")?)
                }
                "network_log" => Payload::Network(parse_json(&path, &bytes, "network_log")?),
                "axe_result" => Payload::Axe(parse_json(&path, &bytes, "axe_result")?),
                "perf_metrics" => Payload::Perf(parse_json(&path, &bytes, "perf_metrics")?),
                _ => Payload::Opaque,
            };
            payloads.push(payload);
        }

        Ok(CaptureSet {
            dir: dir.to_path_buf(),
            entries,
            payloads,
        })
    }

    /// The directory this set was loaded from.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    /// Every manifest entry, in manifest order.
    pub fn entries(&self) -> &[ManifestEntry] {
        &self.entries
    }

    /// Distinct viewports present in the manifest, in first-seen order.
    pub fn viewports(&self) -> Vec<Viewport> {
        let mut out: Vec<Viewport> = Vec::new();
        for entry in &self.entries {
            if !out.iter().any(|v| same_viewport(*v, entry.viewport)) {
                out.push(entry.viewport);
            }
        }
        out
    }

    fn find(&self, kind: &str, viewport: Option<Viewport>, media: Option<&str>) -> Option<usize> {
        self.entries.iter().position(|e| {
            e.kind == kind
                && viewport.is_none_or(|v| same_viewport(v, e.viewport))
                && media.is_none_or(|m| e.media == m)
        })
    }

    /// DOM snapshot for a viewport and media mode name (for example `baseline`).
    pub fn dom(&self, viewport: Viewport, media: &str) -> Option<&DomSnapshot> {
        match self
            .payloads
            .get(self.find("dom_snapshot", Some(viewport), Some(media))?)?
        {
            Payload::Dom(dom) => Some(dom),
            _ => None,
        }
    }

    /// Accessibility tree for a viewport and media mode name.
    pub fn ax(&self, viewport: Viewport, media: &str) -> Option<&AxTree> {
        match self
            .payloads
            .get(self.find("ax_tree", Some(viewport), Some(media))?)?
        {
            Payload::Ax(ax) => Some(ax),
            _ => None,
        }
    }

    /// Focus walk for a viewport. The collector records it in baseline media only.
    pub fn focus(&self, viewport: Viewport) -> Option<&FocusWalk> {
        match self
            .payloads
            .get(self.find("focus_order", Some(viewport), None)?)?
        {
            Payload::Focus(walk) => Some(walk),
            _ => None,
        }
    }

    /// Network log. The collector records one, at the first viewport.
    pub fn network(&self) -> Option<&[NetworkEntry]> {
        match self.payloads.get(self.find("network_log", None, None)?)? {
            Payload::Network(log) => Some(log),
            _ => None,
        }
    }

    /// axe-core corroboration result for a viewport and media mode name.
    /// The collector records one, in baseline media only, when run with
    /// `--corroborate axe`.
    pub fn axe(&self, viewport: Viewport, media: &str) -> Option<&AxeResult> {
        match self
            .payloads
            .get(self.find("axe_result", Some(viewport), Some(media))?)?
        {
            Payload::Axe(axe) => Some(axe),
            _ => None,
        }
    }

    /// Lab performance metrics for a viewport. The collector records one, in
    /// baseline media, unless run with `--no-perf`.
    pub fn perf(&self, viewport: Viewport) -> Option<&PerfMetrics> {
        match self
            .payloads
            .get(self.find("perf_metrics", Some(viewport), None)?)?
        {
            Payload::Perf(perf) => Some(perf),
            _ => None,
        }
    }
}
