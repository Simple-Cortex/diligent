//! Records browser captures for the accessibility fixtures.
//!
//! Serves `fixtures/a11y/` on a fixed loopback port, then for every case in
//! `act/manifest.json` and `authored/manifest.json` runs the collector into
//! `fixtures/a11y/captures/<case_id>/`:
//!
//! - always: viewport 1280x800, media `baseline`, no screenshots, plus the
//!   axe-core corroborator (`axe_result`, baseline only);
//! - when the case lists `"viewports": ["320"]`: also 320x568 `baseline`;
//! - when the case lists `"modes": ["text-spacing"]`: also 1280x800 `text-spacing`;
//! - when the case lists `"modes": ["reduced-motion"]`: also 1280x800 `reduced-motion`
//!   (DOM snapshot and accessibility tree, as `collect` writes for that mode);
//! - every run also writes one `perf_metrics` per viewport (collector default).
//!
//! An authored case may set `"serve_as": "/"` (or `"/index.html"`, any path
//! starting with `/`). While that case is recorded the server answers that exact
//! path with the case file and the collector navigates to it, so the snapshot's
//! `document_url` is `http://127.0.0.1:<port><serve_as>`. Without the field the
//! case is served and recorded at its own path, as before.
//!
//! Case ids: an authored case uses its `case_id`; an ACT case is
//! `<rule_id>-<case>` (for example `afw4f7-passed-example-1`). ACT cases with a
//! non-null `skipped_reason` are not captured.
//!
//! The collector renumbers Chrome's drifting node ids itself (see "Canonical
//! ids" in `docs/browser-worker.md`). The only post-processing here is the
//! network log: `data:` URLs and the favicon probe are dropped, because Chrome
//! records them at random.
//!
//! `perf_metrics` timings are rounded up to a multiple of 500 ms here (see
//! `round_timings_up`), because the raw values are not reproducible.
//!
//! Recording is hermetic: the browser runs with strict origins, so any request
//! outside the local server is blocked and logged with `blocked_by_policy`.
//!
//! Known gap: ACT pages that reference `/test-assets/shared/act-logo.png` get a
//! 404, since the asset is not vendored. The broken image is stable once ids
//! are canonical, so the captures record it as it renders.
//!
//! Nothing outside `fixtures/a11y/captures/` is written. Re-running is a no-op
//! on disk when the pages and Chrome are unchanged: the port is fixed so the
//! document URL inside each snapshot is stable, only the first run's network
//! log is kept, and `captured_at` is carried over from the previous manifest
//! for every capture whose hash did not change.
//!
//! ```text
//! DILIGENT_CHROME=/path/to/chrome cargo run -p diligent-runtime --example bless_a11y \
//!     [-- --only <case_id>] [--root <fixtures/a11y>]
//! ```
//! `DILIGENT_BLESS_PORT` overrides the port (default 47081).

use diligent_runtime::browser::{Browser, PerfMetrics};
use diligent_runtime::chrome::{ChromeBrowser, ChromeConfig};
use diligent_runtime::collect::{collect_page, CollectPlan, ManifestEntry, MediaMode};
use diligent_runtime::protocol::Viewport;
use std::collections::HashMap;
use std::fs;
use std::io::{Read, Write};
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;

struct Case {
    id: String,
    url_path: String,
    wide_viewports: bool,
    text_spacing: bool,
    reduced_motion: bool,
    /// Path the server answers with this case's file while it is recorded.
    serve_as: Option<String>,
}

type Alias = Arc<Mutex<Option<(String, String)>>>;

fn respond(mut stream: TcpStream, root: &Path, alias: &Alias) {
    let mut buffer = [0u8; 4096];
    let read = stream.read(&mut buffer).unwrap_or(0);
    let request = String::from_utf8_lossy(&buffer[..read]);
    let target = request
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .unwrap_or("/");
    let target = target.split(['?', '#']).next().unwrap_or("/");
    let aliased = alias.lock().ok().and_then(|a| {
        a.as_ref()
            .filter(|(from, _)| from == target)
            .map(|(_, to)| to.clone())
    });
    let target = aliased.as_deref().unwrap_or(target);
    let relative = target.trim_start_matches('/');
    let safe = !relative.is_empty()
        && !relative.contains("..")
        && !relative.contains('\\')
        && !relative.starts_with("captures/");
    let reply = match safe.then(|| fs::read(root.join(relative)).ok()).flatten() {
        Some(body) => {
            let kind = match Path::new(relative).extension().and_then(|e| e.to_str()) {
                Some("html") => "text/html; charset=utf-8",
                Some("css") => "text/css",
                Some("svg") => "image/svg+xml",
                _ => "application/octet-stream",
            };
            let mut out = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: {kind}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
                body.len()
            )
            .into_bytes();
            out.extend_from_slice(&body);
            out
        }
        None => {
            b"HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n".to_vec()
        }
    };
    let _ = stream.write_all(&reply);
}

fn read_json(path: &Path) -> Result<serde_json::Value, String> {
    let text = fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    serde_json::from_str(&text).map_err(|e| format!("{}: {e}", path.display()))
}

fn string_list(value: &serde_json::Value, key: &str) -> Vec<String> {
    value
        .get(key)
        .and_then(|v| v.as_array())
        .map(|a| {
            a.iter()
                .filter_map(|s| s.as_str().map(str::to_string))
                .collect()
        })
        .unwrap_or_default()
}

/// Cases from one manifest. `group` is `act` or `authored`.
fn load_cases(root: &Path, group: &str) -> Result<Vec<Case>, String> {
    let manifest = read_json(&root.join(group).join("manifest.json"))?;
    let mut out = Vec::new();
    for entry in manifest["cases"]
        .as_array()
        .ok_or_else(|| format!("{group}/manifest.json has no cases array"))?
    {
        if !entry
            .get("skipped_reason")
            .is_none_or(serde_json::Value::is_null)
            || entry["path"].is_null()
        {
            continue;
        }
        let path = entry["path"]
            .as_str()
            .ok_or_else(|| format!("{group}: case without path"))?;
        let id = match entry.get("case_id").and_then(|v| v.as_str()) {
            Some(id) => id.to_string(),
            None => {
                let rule = entry["rule_id"]
                    .as_str()
                    .ok_or_else(|| format!("{group}: {path} has no case_id or rule_id"))?;
                let case = entry["case"]
                    .as_str()
                    .ok_or_else(|| format!("{group}: {path} has no case"))?;
                format!("{rule}-{case}")
            }
        };
        out.push(Case {
            id,
            url_path: format!("/{group}/{path}"),
            wide_viewports: string_list(entry, "viewports").iter().any(|v| v == "320"),
            text_spacing: string_list(entry, "modes")
                .iter()
                .any(|m| m == "text-spacing"),
            reduced_motion: string_list(entry, "modes")
                .iter()
                .any(|m| m == "reduced-motion"),
            serve_as: match entry.get("serve_as") {
                None | Some(serde_json::Value::Null) => None,
                Some(v) => {
                    let path = v
                        .as_str()
                        .filter(|p| p.starts_with('/') && !p.contains(['?', '#', '\\']))
                        .ok_or_else(|| {
                            format!(
                                "{group}: {path}: serve_as must be a string path starting with `/`"
                            )
                        })?;
                    Some(path.to_string())
                }
            },
        });
    }
    Ok(out)
}

type StampKey = (String, u32, u32, String, String);

fn stamp_key(entry: &ManifestEntry) -> StampKey {
    (
        entry.kind.clone(),
        entry.viewport.width,
        entry.viewport.height,
        entry.media.clone(),
        entry.sha256.clone(),
    )
}

fn previous_timestamps(dir: &Path) -> HashMap<StampKey, String> {
    let mut map = HashMap::new();
    if let Ok(text) = fs::read_to_string(dir.join("manifest.jsonl")) {
        for line in text.lines() {
            if let Ok(entry) = serde_json::from_str::<ManifestEntry>(line) {
                map.insert(stamp_key(&entry), entry.captured_at);
            }
        }
    }
    map
}

fn viewport(width: u32, height: u32) -> Viewport {
    Viewport {
        width,
        height,
        dpr: 1.0,
    }
}

/// Drops entries that are not requests from a network log: data URLs (logged
/// twice at random) and the favicon probe. The collector's own log stays raw.
fn filter_network_log(value: &mut serde_json::Value) {
    if let Some(entries) = value.as_array_mut() {
        entries.retain(|e| {
            let url = e["url"].as_str().unwrap_or_default();
            !url.starts_with("data:") && !url.ends_with("/favicon.ico")
        });
    }
}

/// Paint and load timings of a local page differ by tens of milliseconds from
/// run to run (frame quantization, scheduling), so no fixed rounding step of 50
/// ms makes them byte-stable. The recorder rounds every timing UP to a multiple
/// of 500 ms, which is stable for the small fixture pages, and says so in the
/// notes. Sizes, counts, CLS and the LCP element are recorded as measured.
fn round_timings_up(m: &mut PerfMetrics) {
    let up = |v: &mut Option<f64>| {
        if let Some(x) = v {
            *x = (*x / 500.0).ceil() * 500.0;
        }
    };
    up(&mut m.lcp_ms);
    up(&mut m.fcp_ms);
    up(&mut m.ttfb_ms);
    up(&mut m.dom_content_loaded_ms);
    up(&mut m.load_ms);
    up(&mut m.long_tasks_ms);
    m.notes.push(
        "fixture recording: timings rounded up to a multiple of 500 ms for reproducibility"
            .to_string(),
    );
}

fn sha256_hex(bytes: &[u8]) -> String {
    use sha2::{Digest, Sha256};
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

async fn bless_case(
    browser: &mut ChromeBrowser,
    case: &Case,
    origin: &str,
    captures: &Path,
    alias: &Alias,
) -> Result<(), String> {
    if let Ok(mut a) = alias.lock() {
        *a = case
            .serve_as
            .clone()
            .map(|from| (from, case.url_path.clone()));
    }
    let result = bless_case_inner(browser, case, origin, captures).await;
    if let Ok(mut a) = alias.lock() {
        *a = None;
    }
    result
}

async fn bless_case_inner(
    browser: &mut ChromeBrowser,
    case: &Case,
    origin: &str,
    captures: &Path,
) -> Result<(), String> {
    let dir = captures.join(&case.id);
    let stamps = previous_timestamps(&dir);
    if dir.exists() {
        fs::remove_dir_all(&dir).map_err(|e| format!("clearing {}: {e}", dir.display()))?;
    }
    fs::create_dir_all(&dir).map_err(|e| format!("creating {}: {e}", dir.display()))?;

    let mut runs: Vec<(Viewport, Vec<MediaMode>)> =
        vec![(viewport(1280, 800), vec![MediaMode::Baseline])];
    if case.reduced_motion {
        runs[0].1.push(MediaMode::ReducedMotion);
    }
    if case.text_spacing {
        runs[0].1.push(MediaMode::TextSpacing);
    }
    if case.wide_viewports {
        runs.push((viewport(320, 568), vec![MediaMode::Baseline]));
    }

    let mut manifest = String::new();
    for (index, (vp, modes)) in runs.iter().enumerate() {
        let stage = dir.join(format!(".stage-{index}"));
        let mut plan = CollectPlan::new(format!(
            "{origin}{}",
            case.serve_as.as_deref().unwrap_or(&case.url_path)
        ));
        plan.viewports = vec![*vp];
        plan.media_modes = modes.clone();
        plan.screenshots = false;
        // axe-core corroboration at 1280 baseline only (the first run); the 320
        // and text-spacing captures are unchanged.
        plan.corroborate_axe = index == 0;
        collect_page(browser, &plan, &stage)
            .await
            .map_err(|e| format!("{}: {e:?}", case.id))?;
        let text = fs::read_to_string(stage.join("manifest.jsonl"))
            .map_err(|e| format!("{}: {e}", case.id))?;
        for line in text.lines() {
            let mut entry: ManifestEntry =
                serde_json::from_str(line).map_err(|e| format!("{}: {e}", case.id))?;
            // One network log per case: the one from the first run.
            if entry.kind == "network_log" && index > 0 {
                continue;
            }
            let mut bytes =
                fs::read(stage.join(&entry.path)).map_err(|e| format!("{}: {e}", case.id))?;
            if entry.kind == "network_log" {
                let mut value: serde_json::Value =
                    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", case.id))?;
                filter_network_log(&mut value);
                bytes = serde_json::to_vec(&value).map_err(|e| format!("{}: {e}", case.id))?;
                let sha = sha256_hex(&bytes);
                entry.evidence_id = sha.clone();
                entry.path = format!("{sha}.json");
                entry.sha256 = sha;
                entry.bytes = bytes.len() as u64;
            }
            if entry.kind == "perf_metrics" {
                let mut metrics: PerfMetrics =
                    serde_json::from_slice(&bytes).map_err(|e| format!("{}: {e}", case.id))?;
                round_timings_up(&mut metrics);
                bytes = serde_json::to_vec(&metrics).map_err(|e| format!("{}: {e}", case.id))?;
                let sha = sha256_hex(&bytes);
                entry.evidence_id = sha.clone();
                entry.path = format!("{sha}.json");
                entry.sha256 = sha;
                entry.bytes = bytes.len() as u64;
            }
            if let Some(stamp) = stamps.get(&stamp_key(&entry)) {
                entry.captured_at = stamp.clone();
            }
            let target = dir.join(&entry.path);
            if !target.exists() {
                fs::write(&target, &bytes).map_err(|e| format!("{}: {e}", case.id))?;
            }
            manifest.push_str(&serde_json::to_string(&entry).map_err(|e| e.to_string())?);
            manifest.push('\n');
        }
        fs::remove_dir_all(&stage).map_err(|e| format!("{}: {e}", case.id))?;
    }
    fs::write(dir.join("manifest.jsonl"), manifest).map_err(|e| format!("{}: {e}", case.id))?;
    Ok(())
}

fn tree_size(dir: &Path) -> (usize, u64) {
    let mut files = 0;
    let mut bytes = 0;
    if let Ok(read) = fs::read_dir(dir) {
        for entry in read.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let (f, b) = tree_size(&path);
                files += f;
                bytes += b;
            } else if let Ok(meta) = entry.metadata() {
                files += 1;
                bytes += meta.len();
            }
        }
    }
    (files, bytes)
}

#[tokio::main]
async fn main() {
    let mut root = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/a11y");
    let mut only: Option<String> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--root" => root = PathBuf::from(args.next().expect("--root needs a value")),
            "--only" => only = Some(args.next().expect("--only needs a value")),
            other => {
                eprintln!("unknown argument `{other}`");
                std::process::exit(2);
            }
        }
    }
    let root = fs::canonicalize(&root).unwrap_or_else(|e| {
        eprintln!("{}: {e}", root.display());
        std::process::exit(2);
    });
    let captures = root.join("captures");

    let mut cases = Vec::new();
    for group in ["act", "authored"] {
        match load_cases(&root, group) {
            Ok(found) => cases.extend(found),
            Err(message) => {
                eprintln!("{message}");
                std::process::exit(2);
            }
        }
    }
    if let Some(id) = &only {
        cases.retain(|c| &c.id == id);
    }
    let mut seen = std::collections::HashSet::new();
    for case in &cases {
        if !seen.insert(case.id.clone()) {
            eprintln!("duplicate case id `{}`", case.id);
            std::process::exit(2);
        }
    }

    let port: u16 = std::env::var("DILIGENT_BLESS_PORT")
        .ok()
        .and_then(|p| p.parse().ok())
        .unwrap_or(47081);
    let listener = TcpListener::bind(("127.0.0.1", port)).unwrap_or_else(|e| {
        eprintln!("cannot bind 127.0.0.1:{port} ({e}); set DILIGENT_BLESS_PORT");
        std::process::exit(2);
    });
    listener
        .set_nonblocking(true)
        .expect("nonblocking listener");
    let stop = Arc::new(AtomicBool::new(false));
    let flag = Arc::clone(&stop);
    let serve_root = root.clone();
    let alias: Alias = Arc::new(Mutex::new(None));
    let serve_alias = Arc::clone(&alias);
    let server = std::thread::spawn(move || {
        while !flag.load(Ordering::Relaxed) {
            match listener.accept() {
                Ok((stream, _)) => {
                    let _ = stream.set_nonblocking(false);
                    let root = serve_root.clone();
                    let alias = Arc::clone(&serve_alias);
                    std::thread::spawn(move || respond(stream, &root, &alias));
                }
                Err(_) => std::thread::sleep(Duration::from_millis(5)),
            }
        }
    });
    let origin = format!("http://127.0.0.1:{port}");

    let mut config = ChromeConfig::new();
    config.allowed_origins = vec![origin.clone()];
    // Hermetic recording: nothing outside the local server may load.
    config.strict_origins = true;
    config.timeout = Duration::from_secs(30);
    let mut browser = match ChromeBrowser::launch(config).await {
        Ok(b) => b,
        Err(e) => {
            eprintln!("cannot launch Chrome: {e:?}");
            std::process::exit(3);
        }
    };

    let mut failures = Vec::new();
    for case in &cases {
        if let Err(message) = bless_case(&mut browser, case, &origin, &captures, &alias).await {
            eprintln!("FAILED {message}");
            failures.push(case.id.clone());
        }
    }
    let _ = browser.close().await;
    stop.store(true, Ordering::Relaxed);
    let _ = server.join();

    let (files, bytes) = tree_size(&captures);
    println!(
        "blessed {} of {} case(s); {files} file(s), {bytes} bytes under {}",
        cases.len() - failures.len(),
        cases.len(),
        captures.display()
    );
    if !failures.is_empty() {
        eprintln!("failed cases: {}", failures.join(", "));
        std::process::exit(1);
    }
}
