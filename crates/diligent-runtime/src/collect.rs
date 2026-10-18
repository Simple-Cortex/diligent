//! Evidence collector: drives a [`Browser`] across viewports and media modes
//! and writes content-addressed captures plus a `manifest.jsonl`.

use crate::browser::{AxeError, AxeResult, Browser, DomSnapshot, PerfMetrics};
use crate::canonical::CanonicalIds;
use crate::protocol::{
    ColorScheme, ErrorKind, Limits, Media, Viewport, WorkerError, PROTOCOL_VERSION,
};
use crate::util::{now_rfc3339, sha256_hex};
pub use diligent_capture::ManifestEntry;
use serde::{Deserialize, Serialize};
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

/// Default viewports: 320x568, 768x1024, 1024x768, 1280x800 at dpr 1.
pub const DEFAULT_VIEWPORTS: [(u32, u32); 4] = [(320, 568), (768, 1024), (1024, 768), (1280, 800)];

/// Maximum focus stops walked per page.
pub const MAX_FOCUS_STOPS: usize = 200;

/// A named media-emulation mode.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum MediaMode {
    /// No emulated preferences.
    Baseline,
    /// `prefers-reduced-motion: reduce`.
    ReducedMotion,
    /// `forced-colors: active`.
    ForcedColors,
    /// `prefers-color-scheme: dark`.
    Dark,
    /// Capture mode, not a media feature: the WCAG 1.4.12 text-spacing
    /// stylesheet is injected for the DOM snapshot only. Media emulation stays
    /// at the baseline. Not in [`MediaMode::ALL`]; enabled with `--media text-spacing`.
    TextSpacing,
}

impl MediaMode {
    /// The default capture modes, in capture order. Excludes [`MediaMode::TextSpacing`].
    pub const ALL: [MediaMode; 4] = [
        MediaMode::Baseline,
        MediaMode::ReducedMotion,
        MediaMode::ForcedColors,
        MediaMode::Dark,
    ];

    /// Stable name used in the manifest and on the command line.
    pub fn name(self) -> &'static str {
        match self {
            MediaMode::Baseline => "baseline",
            MediaMode::ReducedMotion => "reduced-motion",
            MediaMode::ForcedColors => "forced-colors",
            MediaMode::Dark => "dark",
            MediaMode::TextSpacing => "text-spacing",
        }
    }

    /// Parse a command-line name.
    pub fn parse(name: &str) -> Option<MediaMode> {
        MediaMode::ALL
            .into_iter()
            .chain([MediaMode::TextSpacing])
            .find(|m| m.name() == name)
    }

    /// The emulation this mode applies.
    pub fn media(self) -> Media {
        let mut media = Media::BASELINE;
        match self {
            MediaMode::Baseline | MediaMode::TextSpacing => {}
            MediaMode::ReducedMotion => media.reduced_motion = true,
            MediaMode::ForcedColors => media.forced_colors = true,
            MediaMode::Dark => media.color_scheme = ColorScheme::Dark,
        }
        media
    }
}

/// What to collect.
#[derive(Debug, Clone)]
pub struct CollectPlan {
    /// Page to open.
    pub url: String,
    /// Viewports, one navigation each.
    pub viewports: Vec<Viewport>,
    /// Media modes captured at each viewport.
    pub media_modes: Vec<MediaMode>,
    /// Capture screenshots.
    pub screenshots: bool,
    /// Run the axe-core corroborator once per viewport, in baseline media only,
    /// and record an `axe_result` capture. Off by default.
    pub corroborate_axe: bool,
    /// Record lab performance metrics (`perf_metrics`) per viewport, in baseline
    /// media. On by default; `collect --no-perf` turns it off.
    pub perf: bool,
    /// Per-navigation timeout.
    pub timeout: Duration,
    /// Size limits.
    pub limits: Limits,
}

impl CollectPlan {
    /// Plan with the default viewports and media modes.
    pub fn new(url: impl Into<String>) -> Self {
        CollectPlan {
            url: url.into(),
            viewports: default_viewports(),
            media_modes: MediaMode::ALL.to_vec(),
            screenshots: true,
            corroborate_axe: false,
            perf: true,
            timeout: Duration::from_millis(30_000),
            limits: Limits {
                max_bytes: 52_428_800,
                max_nodes: 50_000,
            },
        }
    }
}

/// The default viewport list at dpr 1.
pub fn default_viewports() -> Vec<Viewport> {
    DEFAULT_VIEWPORTS
        .iter()
        .map(|&(width, height)| Viewport {
            width,
            height,
            dpr: 1.0,
        })
        .collect()
}

/// Summary of one collection run.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct CollectReport {
    /// Requested URL.
    pub url: String,
    /// Final URL of the first navigation.
    pub final_url: String,
    /// Document title of the first navigation.
    pub title: String,
    /// `lang` of the first navigation.
    pub lang: Option<String>,
    /// Path of the manifest file.
    pub manifest_path: String,
    /// Every capture written.
    pub evidence: Vec<ManifestEntry>,
    /// Total bytes of evidence files.
    pub total_bytes: u64,
    /// Start time.
    pub started_at: String,
    /// Finish time.
    pub finished_at: String,
}

struct Sink<'a> {
    out: &'a Path,
    manifest: fs::File,
    limits: Limits,
    total_bytes: u64,
    entries: Vec<ManifestEntry>,
    /// Manifest lines already on disk when the run started, plus the lines
    /// written since. A capture whose `(kind, viewport, media, sha256)` is in
    /// here, and whose file still re-hashes, is not written again.
    existing: Vec<ManifestEntry>,
}

fn io_failure(context: &str, error: std::io::Error) -> WorkerError {
    WorkerError::idempotent(ErrorKind::CollectorFailure, format!("{context}: {error}"))
}

static TEMP_COUNTER: AtomicU64 = AtomicU64::new(0);

/// Writes `bytes` to `path` through a temp file in the same directory, flushes
/// it and renames it into place, so a crash never leaves a half-written file
/// under the final name.
fn atomic_write(path: &Path, bytes: &[u8]) -> std::io::Result<()> {
    let name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    let tmp: PathBuf = path.with_file_name(format!(
        ".{name}.tmp-{}-{}-{}",
        std::process::id(),
        crate::util::now_millis(),
        TEMP_COUNTER.fetch_add(1, Ordering::Relaxed)
    ));
    let written = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&tmp)?;
        file.write_all(bytes)?;
        file.sync_all()?;
        fs::rename(&tmp, path)
    })();
    if written.is_err() {
        let _ = fs::remove_file(&tmp);
    }
    written
}

/// True when `path` is a regular file whose bytes hash to `sha`.
fn file_hashes_to(path: &Path, sha: &str) -> bool {
    match fs::symlink_metadata(path) {
        Ok(meta) if meta.is_file() => fs::read(path)
            .map(|bytes| sha256_hex(&bytes).eq_ignore_ascii_case(sha))
            .unwrap_or(false),
        _ => false,
    }
}

fn same_viewport(a: Viewport, b: Viewport) -> bool {
    a.width == b.width && a.height == b.height && (a.dpr - b.dpr).abs() < 1e-6
}

/// Opens `manifest.jsonl` for appending and returns the entries already in it.
///
/// An earlier run that died mid-line leaves a final line without a newline.
/// That torn tail is cut off before anything is appended, so the next line
/// cannot be glued onto it. Complete lines are never touched. Lines that do
/// not parse are left in place and ignored for resume purposes.
fn open_manifest(path: &Path) -> Result<(fs::File, Vec<ManifestEntry>), WorkerError> {
    let file = fs::OpenOptions::new()
        .read(true)
        .append(true)
        .create(true)
        .open(path)
        .map_err(|e| io_failure("opening manifest", e))?;
    let existing_bytes = fs::read(path).map_err(|e| io_failure("reading manifest", e))?;
    let keep = match existing_bytes.iter().rposition(|&b| b == b'\n') {
        Some(last) => last + 1,
        None => 0,
    };
    if keep < existing_bytes.len() {
        file.set_len(keep as u64)
            .map_err(|e| io_failure("repairing manifest", e))?;
        file.sync_all()
            .map_err(|e| io_failure("repairing manifest", e))?;
    }
    let existing = String::from_utf8_lossy(&existing_bytes[..keep])
        .lines()
        .filter_map(|line| serde_json::from_str::<ManifestEntry>(line).ok())
        .collect();
    Ok((file, existing))
}

impl Sink<'_> {
    fn write(
        &mut self,
        kind: &str,
        extension: &str,
        bytes: &[u8],
        viewport: Viewport,
        media: MediaMode,
        final_url: &str,
    ) -> Result<(), WorkerError> {
        let size = bytes.len() as u64;
        if self.total_bytes + size > self.limits.max_bytes {
            return Err(WorkerError::idempotent(
                ErrorKind::CollectorFailure,
                format!(
                    "limit max_bytes ({}) exceeded while writing {kind}",
                    self.limits.max_bytes
                ),
            ));
        }
        let sha = sha256_hex(bytes);
        let file_name = format!("{sha}.{extension}");
        let path = self.out.join(&file_name);
        self.total_bytes += size;
        let seen = self.existing.iter().position(|e| {
            e.kind == kind
                && e.media == media.name()
                && e.sha256 == sha
                && same_viewport(e.viewport, viewport)
        });
        if let Some(index) = seen {
            if file_hashes_to(&path, &sha) {
                // Resume: this capture is already in the manifest and intact.
                self.entries.push(self.existing[index].clone());
                return Ok(());
            }
        }
        if !file_hashes_to(&path, &sha) {
            atomic_write(&path, bytes).map_err(|e| io_failure("writing evidence file", e))?;
        }
        let entry = ManifestEntry {
            evidence_id: sha.clone(),
            kind: kind.to_string(),
            viewport,
            media: media.name().to_string(),
            path: file_name,
            sha256: sha,
            bytes: size,
            captured_at: now_rfc3339(),
            final_url: final_url.to_string(),
            collector_version: env!("CARGO_PKG_VERSION").to_string(),
            protocol_version: PROTOCOL_VERSION.to_string(),
        };
        let line = serde_json::to_string(&entry).map_err(|e| {
            WorkerError::idempotent(ErrorKind::MalformedResponse, format!("manifest entry: {e}"))
        })?;
        // One write of one complete line, then flush, so a crash leaves whole
        // lines or a torn tail that `open_manifest` removes on the next run.
        let mut buffer = line.into_bytes();
        buffer.push(b'\n');
        self.manifest
            .write_all(&buffer)
            .map_err(|e| io_failure("writing manifest", e))?;
        self.manifest
            .sync_data()
            .map_err(|e| io_failure("writing manifest", e))?;
        self.existing.push(entry.clone());
        self.entries.push(entry);
        Ok(())
    }

    fn write_json<T: Serialize>(
        &mut self,
        kind: &str,
        value: &T,
        viewport: Viewport,
        media: MediaMode,
        final_url: &str,
    ) -> Result<(), WorkerError> {
        let bytes = serde_json::to_vec(value).map_err(|e| {
            WorkerError::idempotent(
                ErrorKind::MalformedResponse,
                format!("serializing {kind}: {e}"),
            )
        })?;
        self.write(kind, "json", &bytes, viewport, media, final_url)
    }
}

fn check_node_limit(dom: &DomSnapshot, limits: Limits) -> Result<(), WorkerError> {
    if dom.nodes.len() as u64 > u64::from(limits.max_nodes) {
        return Err(WorkerError::idempotent(
            ErrorKind::CollectorFailure,
            format!(
                "limit max_nodes ({}) exceeded: snapshot has {} nodes",
                limits.max_nodes,
                dom.nodes.len()
            ),
        ));
    }
    Ok(())
}

/// Collect evidence for `plan` into `out`.
///
/// Per viewport the page is navigated once. At that viewport each requested
/// media mode gets a DOM snapshot, an accessibility tree and (unless disabled)
/// a screenshot. With `corroborate_axe`, the baseline mode also gets an
/// `axe_result` (or one that carries the error). The `text-spacing` mode is the exception: it gets a DOM
/// snapshot only, taken while the text-spacing stylesheet is injected, and the
/// stylesheet is removed before the next mode. The focus order is captured in
/// the baseline mode only. The
/// network log is captured at the first viewport only. A limit overrun or any
/// browser failure aborts with the error; partial evidence stays on disk and
/// in the manifest.
///
/// Payload files are written through a temp file and a rename, and
/// `manifest.jsonl` is opened for append and never truncated. Running again
/// into the same directory skips every capture whose `(kind, viewport, media,
/// sha256)` is already in the manifest and whose file still re-hashes.
pub async fn collect_page(
    browser: &mut dyn Browser,
    plan: &CollectPlan,
    out: &Path,
) -> Result<CollectReport, WorkerError> {
    let started_at = now_rfc3339();
    if plan.viewports.is_empty() || plan.media_modes.is_empty() {
        return Err(WorkerError::new(
            ErrorKind::CollectorFailure,
            "plan needs at least one viewport and one media mode",
        ));
    }
    fs::create_dir_all(out).map_err(|e| io_failure("creating output directory", e))?;
    let manifest_path = out.join("manifest.jsonl");
    // Append, never truncate: a second run into the same directory resumes.
    let (manifest, existing) = open_manifest(&manifest_path)?;
    let mut sink = Sink {
        out,
        manifest,
        limits: plan.limits,
        total_bytes: 0,
        entries: Vec::new(),
        existing,
    };

    if plan.perf {
        browser.install_perf().await?;
    } else {
        browser.remove_perf().await?;
    }
    let mut first: Option<(String, String, Option<String>)> = None;
    for (viewport_index, &viewport) in plan.viewports.iter().enumerate() {
        browser.emulate_media(Media::BASELINE).await?;
        browser.set_viewport(viewport).await?;
        let info = browser.navigate(&plan.url, plan.timeout).await?;
        if first.is_none() {
            first = Some((
                info.final_url.clone(),
                info.title.clone(),
                info.lang.clone(),
            ));
        }
        let final_url = info.final_url.clone();

        if plan.perf {
            // Read first, right after load and before anything else touches the
            // page, so the capture steps cannot move the measurement.
            let perf = match browser.read_perf().await {
                Ok(metrics) => metrics,
                Err(e) => PerfMetrics {
                    label: "lab".to_string(),
                    lcp_ms: None,
                    lcp_element: None,
                    cls: None,
                    fcp_ms: None,
                    ttfb_ms: None,
                    dom_content_loaded_ms: None,
                    load_ms: None,
                    transfer_bytes: None,
                    resource_count: None,
                    long_tasks_ms: None,
                    notes: vec![format!(
                        "error: {}: {}",
                        e.kind.as_str(),
                        e.message.chars().take(200).collect::<String>()
                    )],
                },
            };
            sink.write_json(
                "perf_metrics",
                &perf,
                viewport,
                MediaMode::Baseline,
                &final_url,
            )?;
        }

        // Canonical ids are fixed by the first DOM snapshot of this page load.
        let mut ids: Option<CanonicalIds> = None;
        for &mode in &plan.media_modes {
            browser.emulate_media(mode.media()).await?;
            if mode == MediaMode::TextSpacing {
                // Capture mode: DOM snapshot only, under the injected stylesheet.
                browser.apply_text_spacing().await?;
                let dom = browser.dom_snapshot().await;
                let removed = browser.remove_text_spacing().await;
                let mut dom = dom?;
                removed?;
                check_node_limit(&dom, plan.limits)?;
                ids.get_or_insert_with(|| CanonicalIds::from_dom(&dom))
                    .dom(&mut dom);
                sink.write_json("dom_snapshot", &dom, viewport, mode, &final_url)?;
                continue;
            }
            let mut dom = browser.dom_snapshot().await?;
            check_node_limit(&dom, plan.limits)?;
            let canonical = ids.get_or_insert_with(|| CanonicalIds::from_dom(&dom));
            canonical.dom(&mut dom);
            sink.write_json("dom_snapshot", &dom, viewport, mode, &final_url)?;
            let mut ax = browser.ax_tree().await?;
            canonical.ax(&mut ax);
            sink.write_json("ax_tree", &ax, viewport, mode, &final_url)?;
            if mode == MediaMode::Baseline {
                let mut walk = browser.focus_order(MAX_FOCUS_STOPS).await?;
                canonical.focus(&mut walk);
                sink.write_json("focus_order", &walk, viewport, mode, &final_url)?;
            }
            if plan.screenshots {
                let png = browser.screenshot().await?;
                sink.write("screenshot", "png", &png, viewport, mode, &final_url)?;
            }
            if plan.corroborate_axe && mode == MediaMode::Baseline {
                // Last step of the baseline block, so axe's scrolling and caches
                // cannot disturb the snapshot, tree, focus walk or screenshot.
                let axe = match browser.run_corroborator().await {
                    Ok(result) => result,
                    Err(e) => AxeResult {
                        engine: "axe-core".to_string(),
                        version: None,
                        url: final_url.clone(),
                        rules: Vec::new(),
                        error: Some(AxeError {
                            kind: e.kind.as_str().to_string(),
                            message: e.message.chars().take(200).collect(),
                        }),
                    },
                };
                sink.write_json("axe_result", &axe, viewport, mode, &final_url)?;
            }
        }
        browser.emulate_media(Media::BASELINE).await?;

        if viewport_index == 0 {
            let log = browser.network_log().await?;
            sink.write_json(
                "network_log",
                &log,
                viewport,
                MediaMode::Baseline,
                &final_url,
            )?;
        }
    }

    let (final_url, title, lang) = first.unwrap_or_default();
    Ok(CollectReport {
        url: plan.url.clone(),
        final_url,
        title,
        lang,
        manifest_path: manifest_path.to_string_lossy().into_owned(),
        total_bytes: sink.total_bytes,
        evidence: sink.entries,
        started_at,
        finished_at: now_rfc3339(),
    })
}
