//! Scanning entry points: one document, a path, and JSONL output.

use std::collections::HashSet;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use scraper::Html;
use walkdir::WalkDir;

use crate::checks;
use crate::dom::{Ctx, Pos, TemplateFacts};
use crate::emit::Emitter;
use crate::model::{Finding, SourceRef, State};
use crate::positions::{self, LineIndex};
use crate::sha256::sha256_hex;
use crate::template::{self, Flavor};
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
    /// Number of files scanned.
    pub files_scanned: usize,
    /// Number of files skipped.
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
    scan_text(html, source, Flavor::Html)
}

/// True when `hay` contains `needle` (lowercase ASCII), ignoring ASCII case.
fn contains_ci(hay: &str, needle: &str) -> bool {
    let n = needle.as_bytes();
    hay.as_bytes()
        .windows(n.len())
        .any(|w| w.eq_ignore_ascii_case(n))
}

/// Scans one document held in memory, read as the given flavour.
///
/// `Flavor::Html` is parsed as is. Every other flavour goes through the
/// template pre-pass first. A template with no `html` or `body` tag is a
/// fragment: document-level checks record `not_tested` for it.
pub fn scan_text(text: &str, source: &SourceRef, flavor: Flavor) -> Vec<Finding> {
    let pre = template::preprocess(text, flavor);
    let doc = Html::parse_document(&pre.html);
    let tags = positions::start_tags(&pre.html);
    let lines = LineIndex::new(text);
    let element_names = doc
        .tree
        .root()
        .descendants()
        .filter_map(scraper::ElementRef::wrap)
        .map(|e| e.value().name().to_string())
        .collect::<Vec<_>>();
    let aligned = positions::align(element_names.iter().map(String::as_str), &tags);
    let found: Vec<Option<Pos>> = aligned
        .iter()
        .map(|slot| {
            let tag = tags.get((*slot)?)?;
            let offset = pre.source_offset(tag.offset, text.len());
            let (line, column) = lines.locate(offset);
            Some(Pos {
                offset: u32::try_from(offset).unwrap_or(u32::MAX),
                line,
                column,
            })
        })
        .collect();
    let templated = flavor != Flavor::Html;
    let fragment =
        templated && !contains_ci(&pre.html, "<html") && !contains_ci(&pre.html, "<body");
    let facts = TemplateFacts {
        templated,
        has_conditionals: pre.has_conditionals,
        fragment,
    };
    let ctx = Ctx::with_positions(&doc, &found, facts);
    let mut em = Emitter::new(&ctx, source, sha256_hex(text.as_bytes()));
    for check in checks::all() {
        if fragment && checks::DOCUMENT_LEVEL.contains(&check.info.id) {
            em.skip_fragment(check.info);
            continue;
        }
        (check.run)(&mut em);
        em.ensure_recorded(check.info);
    }
    em.finish()
}

fn is_scannable_file(path: &Path) -> bool {
    Flavor::from_path(path).is_some()
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

/// Scans a single file, or every `.html`, `.htm`, `.jsx`, `.tsx`, `.vue`,
/// `.svelte`, `.astro`, `.twig` and `.liquid` file under a directory.
///
/// Paths in findings are relative to `path` (or the file name when `path` is a
/// file). A file passed directly is scanned whatever its extension; one with an
/// unknown extension is read as HTML.
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
            if entry.file_type().is_file() && is_scannable_file(entry.path()) {
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
        let flavor = Flavor::from_path(&file).unwrap_or(Flavor::Html);
        let findings = scan_text(&html, &source, flavor);
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
