//! Scanning entry points: one document, a path, and JSONL output.

use std::collections::HashSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use scraper::Html;
use walkdir::WalkDir;

use crate::checks;
use crate::dom::Ctx;
use crate::emit::Emitter;
use crate::model::{Finding, SourceRef, State};
use crate::sha256::sha256_hex;
use crate::time;

/// Default cap on a single file, in bytes.
pub const DEFAULT_MAX_FILE_BYTES: u64 = 8 * 1024 * 1024;

/// Errors that stop a scan before any file is read.
#[derive(Debug, thiserror::Error)]
pub enum ScanError {
    /// The path does not exist.
    #[error("path not found: {0}")]
    NotFound(String),
    /// A directory walk or read failed.
    #[error("i/o error at {path}: {source}")]
    Io {
        /// Path involved.
        path: String,
        /// Underlying error.
        #[source]
        source: io::Error,
    },
}

/// Options for [`scan_path`].
#[derive(Debug, Clone)]
pub struct ScanOptions {
    /// Run identifier; generated when `None`.
    pub run_id: Option<String>,
    /// RFC 3339 capture time; the current time when `None`.
    pub captured_at: Option<String>,
    /// Files larger than this are skipped.
    pub max_file_bytes: u64,
}

impl Default for ScanOptions {
    fn default() -> Self {
        ScanOptions {
            run_id: None,
            captured_at: None,
            max_file_bytes: DEFAULT_MAX_FILE_BYTES,
        }
    }
}

/// A file the scan did not read, with the reason.
#[derive(Debug, Clone)]
pub struct SkippedFile {
    /// Path as recorded in findings.
    pub path: String,
    /// Why it was skipped.
    pub reason: String,
}

/// Result of [`scan_path`].
#[derive(Debug, Clone)]
pub struct ScanReport {
    /// The path that was scanned, as given.
    pub root: String,
    /// Run identifier shared by every finding.
    pub run_id: String,
    /// Every finding, grouped by file in path order.
    pub findings: Vec<Finding>,
    /// Number of HTML files scanned.
    pub files_scanned: usize,
    /// Number of HTML files skipped.
    pub files_skipped: usize,
    /// Detail for each skipped file.
    pub skipped: Vec<SkippedFile>,
    /// Identifiers of the checks that ran.
    pub checks_run: Vec<String>,
    /// `(check_id, path)` for each document where a check found nothing to test.
    pub not_tested: Vec<(String, String)>,
}

impl ScanReport {
    /// Count of findings per state: pass, fail, needs_review, not_tested.
    pub fn counts(&self) -> [(State, usize); 4] {
        let mut out = [
            (State::Pass, 0),
            (State::Fail, 0),
            (State::NeedsReview, 0),
            (State::NotTested, 0),
        ];
        for f in &self.findings {
            for slot in out.iter_mut() {
                if slot.0 == f.result.status {
                    slot.1 += 1;
                }
            }
        }
        out
    }
}

/// Scans one HTML document held in memory.
///
/// Every check records at least one finding: results for what it found, or a
/// single `not_tested` when its preconditions were absent.
pub fn scan_html(html: &str, source: &SourceRef) -> Vec<Finding> {
    let doc = Html::parse_document(html);
    let ctx = Ctx::new(&doc);
    let mut em = Emitter::new(&ctx, source, sha256_hex(html.as_bytes()));
    for check in checks::all() {
        (check.run)(&mut em);
        em.ensure_recorded(check.info);
    }
    em.finish()
}

fn is_html_file(path: &Path) -> bool {
    path.extension()
        .and_then(|e| e.to_str())
        .is_some_and(|e| e.eq_ignore_ascii_case("html") || e.eq_ignore_ascii_case("htm"))
}

fn skip_dir(name: &str) -> bool {
    matches!(name, ".git" | "node_modules")
}

fn display_path(root: &Path, file: &Path) -> String {
    let rel = file.strip_prefix(root).unwrap_or(file);
    let rel = if rel.as_os_str().is_empty() {
        file.file_name().map(PathBuf::from).unwrap_or_default()
    } else {
        rel.to_path_buf()
    };
    rel.to_string_lossy().replace('\\', "/")
}

/// Scans a single HTML file, or every `.html` and `.htm` file under a directory.
///
/// Paths in findings are relative to `path` (or the file name when `path` is a
/// file). A file passed directly is scanned whatever its extension.
pub fn scan_path(path: &Path, opts: &ScanOptions) -> Result<ScanReport, ScanError> {
    if !path.exists() {
        return Err(ScanError::NotFound(path.display().to_string()));
    }
    let run_id = opts
        .run_id
        .clone()
        .unwrap_or_else(|| time::new_run_id(&path.display().to_string()));
    let captured_at = opts.captured_at.clone().unwrap_or_else(time::now_rfc3339);

    let mut files: Vec<PathBuf> = Vec::new();
    let base: PathBuf;
    if path.is_file() {
        base = path.parent().map(Path::to_path_buf).unwrap_or_default();
        files.push(path.to_path_buf());
    } else {
        base = path.to_path_buf();
        let walker = WalkDir::new(path)
            .follow_links(false)
            .sort_by_file_name()
            .into_iter()
            .filter_entry(|e| {
                !(e.file_type().is_dir()
                    && e.depth() > 0
                    && e.file_name().to_str().is_some_and(skip_dir))
            });
        for entry in walker {
            let entry = entry.map_err(|e| ScanError::Io {
                path: e
                    .path()
                    .map(|p| p.display().to_string())
                    .unwrap_or_else(|| path.display().to_string()),
                source: e.into(),
            })?;
            if entry.file_type().is_file() && is_html_file(entry.path()) {
                files.push(entry.into_path());
            }
        }
    }

    let mut report = ScanReport {
        root: path.display().to_string(),
        run_id: run_id.clone(),
        findings: Vec::new(),
        files_scanned: 0,
        files_skipped: 0,
        skipped: Vec::new(),
        checks_run: checks::all()
            .iter()
            .map(|c| c.info.id.to_string())
            .collect(),
        not_tested: Vec::new(),
    };

    for file in files {
        let shown = display_path(&base, &file);
        let skip = |report: &mut ScanReport, reason: String| {
            report.files_skipped += 1;
            report.skipped.push(SkippedFile {
                path: shown.clone(),
                reason,
            });
        };
        let size = match std::fs::metadata(&file) {
            Ok(m) => m.len(),
            Err(e) => {
                skip(&mut report, format!("cannot stat: {e}"));
                continue;
            }
        };
        if size > opts.max_file_bytes {
            skip(
                &mut report,
                format!("larger than {} bytes", opts.max_file_bytes),
            );
            continue;
        }
        let bytes = match std::fs::read(&file) {
            Ok(b) => b,
            Err(e) => {
                skip(&mut report, format!("cannot read: {e}"));
                continue;
            }
        };
        let html = String::from_utf8_lossy(&bytes);
        let source = SourceRef {
            path: shown.clone(),
            run_id: run_id.clone(),
            captured_at: captured_at.clone(),
        };
        let findings = scan_html(&html, &source);
        report.files_scanned += 1;
        for f in &findings {
            if f.result.status == State::NotTested {
                report
                    .not_tested
                    .push((f.result.check_id.clone(), shown.clone()));
            }
        }
        report.findings.extend(findings);
    }
    Ok(report)
}

/// Writes `EVIDENCE.jsonl` and `RESULTS.jsonl` rows.
///
/// Evidence records are deduplicated by `evidence_id`, because several results
/// can rest on the same element.
pub fn write_jsonl(
    report: &ScanReport,
    mut evidence_out: impl Write,
    mut results_out: impl Write,
) -> io::Result<()> {
    let mut seen: HashSet<&str> = HashSet::new();
    for f in &report.findings {
        if seen.insert(f.evidence.evidence_id.as_str()) {
            serde_json::to_writer(&mut evidence_out, &f.evidence).map_err(io::Error::from)?;
            evidence_out.write_all(b"\n")?;
        }
        serde_json::to_writer(&mut results_out, &f.result).map_err(io::Error::from)?;
        results_out.write_all(b"\n")?;
    }
    evidence_out.flush()?;
    results_out.flush()
}
