//! `diligent-rs collect`: command-line front end for the collector.

use crate::browser::Browser;
use crate::chrome::{ChromeBrowser, ChromeConfig};
use crate::collect::{collect_page, default_viewports, CollectPlan, MediaMode};
use crate::protocol::{ErrorKind, Viewport, WorkerError};
use crate::util::origin_of;
use std::path::PathBuf;
use std::process::ExitCode;
use std::time::Duration;

const HELP: &str = "Usage: diligent-rs collect --url <url> --out <dir> [options]\n\
\n\
Options:\n  --url <url>              page to collect (required)\n  --out <dir>              evidence directory (required)\n  --allow-origin <origin>  extra allowed origin, repeatable (default: the URL's own origin)\n  --viewports <list>       comma list of widths or WxH, default 320,768,1024,1280\n  --media <list>           baseline,reduced-motion,forced-colors,dark (default) or text-spacing (capture mode, opt-in)\n  --no-screenshots         skip PNG screenshots\n  --no-perf                skip the lab performance metrics (perf_metrics, baseline media, one per viewport)\n  --strict-origins         block every request outside the allowed origins, subresources included (fixtures, air-gapped runs). Off by default\n  --corroborate <list>     optional second engine, repeatable; only `axe` (axe-core, baseline media, top frame). Off by default\n  --chrome-path <path>     Chrome binary (else DILIGENT_CHROME, else common paths)\n  --timeout-ms <n>         per-operation timeout, default 30000\n  --max-nodes <n>          DOM node limit per snapshot, default 50000\n  --max-bytes <n>          total evidence byte limit, default 52428800\n  --json                   print the CollectReport as JSON\n\
\n\
Exit codes: 0 success, 2 usage error, timeout or blocked origin, 3 configuration or collector failure.";

#[derive(Debug)]
struct Options {
    url: String,
    out: PathBuf,
    allow_origins: Vec<String>,
    viewports: Vec<Viewport>,
    media: Vec<MediaMode>,
    screenshots: bool,
    corroborate_axe: bool,
    perf: bool,
    strict_origins: bool,
    chrome_path: Option<PathBuf>,
    timeout_ms: u64,
    max_nodes: u32,
    max_bytes: u64,
    json: bool,
}

fn parse_viewports(list: &str) -> Result<Vec<Viewport>, String> {
    let defaults = default_viewports();
    let mut out = Vec::new();
    for token in list.split(',').filter(|t| !t.is_empty()) {
        if let Some((w, h)) = token.split_once('x') {
            let width: u32 = w.parse().map_err(|_| format!("bad viewport `{token}`"))?;
            let height: u32 = h.parse().map_err(|_| format!("bad viewport `{token}`"))?;
            out.push(Viewport {
                width,
                height,
                dpr: 1.0,
            });
        } else {
            let width: u32 = token
                .parse()
                .map_err(|_| format!("bad viewport `{token}`"))?;
            let height = defaults
                .iter()
                .find(|v| v.width == width)
                .map_or(800, |v| v.height);
            out.push(Viewport {
                width,
                height,
                dpr: 1.0,
            });
        }
    }
    if out.is_empty() {
        return Err("--viewports needs at least one value".to_string());
    }
    Ok(out)
}

fn parse(args: &[String]) -> Result<Options, String> {
    let mut url = None;
    let mut out = None;
    let mut options = Options {
        url: String::new(),
        out: PathBuf::new(),
        allow_origins: Vec::new(),
        viewports: default_viewports(),
        media: MediaMode::ALL.to_vec(),
        screenshots: true,
        corroborate_axe: false,
        perf: true,
        strict_origins: false,
        chrome_path: None,
        timeout_ms: 30_000,
        max_nodes: 50_000,
        max_bytes: 52_428_800,
        json: false,
    };
    let mut i = 0;
    while i < args.len() {
        let token = args[i].as_str();
        let (name, inline) = match token.strip_prefix("--") {
            Some(rest) => match rest.split_once('=') {
                Some((n, v)) => (n.to_string(), Some(v.to_string())),
                None => (rest.to_string(), None),
            },
            None => return Err(format!("unexpected argument `{token}`")),
        };
        match name.as_str() {
            "no-screenshots" => options.screenshots = false,
            "json" => options.json = true,
            "no-perf" => options.perf = false,
            "strict-origins" => options.strict_origins = true,
            "url" | "out" | "allow-origin" | "viewports" | "media" | "corroborate"
            | "chrome-path" | "timeout-ms" | "max-nodes" | "max-bytes" => {
                let value = match inline {
                    Some(v) => v,
                    None => {
                        i += 1;
                        match args.get(i) {
                            Some(v) if !v.starts_with("--") => v.clone(),
                            _ => return Err(format!("option --{name} requires a value")),
                        }
                    }
                };
                match name.as_str() {
                    "url" => url = Some(value),
                    "out" => out = Some(PathBuf::from(value)),
                    "allow-origin" => options.allow_origins.push(value),
                    "viewports" => options.viewports = parse_viewports(&value)?,
                    "media" => {
                        let mut modes = Vec::new();
                        for part in value.split(',').filter(|p| !p.is_empty()) {
                            modes.push(
                                MediaMode::parse(part)
                                    .ok_or_else(|| format!("unknown media mode `{part}`"))?,
                            );
                        }
                        if modes.is_empty() {
                            return Err("--media needs at least one value".to_string());
                        }
                        options.media = modes;
                    }
                    "corroborate" => {
                        let mut any = false;
                        for part in value.split(',').filter(|p| !p.is_empty()) {
                            if part != "axe" {
                                return Err(format!(
                                    "unknown corroborator `{part}` (only `axe` is supported)"
                                ));
                            }
                            options.corroborate_axe = true;
                            any = true;
                        }
                        if !any {
                            return Err("--corroborate needs at least one value".to_string());
                        }
                    }
                    "chrome-path" => options.chrome_path = Some(PathBuf::from(value)),
                    "timeout-ms" => {
                        options.timeout_ms = value
                            .parse()
                            .map_err(|_| format!("bad --timeout-ms `{value}`"))?
                    }
                    "max-nodes" => {
                        options.max_nodes = value
                            .parse()
                            .map_err(|_| format!("bad --max-nodes `{value}`"))?
                    }
                    _ => {
                        options.max_bytes = value
                            .parse()
                            .map_err(|_| format!("bad --max-bytes `{value}`"))?
                    }
                }
            }
            other => return Err(format!("unknown option --{other}")),
        }
        i += 1;
    }
    options.url = url.ok_or("--url is required")?;
    options.out = out.ok_or("--out is required")?;
    Ok(options)
}

fn fail(error: &WorkerError, json: bool) -> ExitCode {
    if json {
        println!("{}", serde_json::json!({"status": "error", "error": error}));
    } else {
        eprintln!("diligent-rs: {error}");
    }
    match error.kind {
        ErrorKind::Timeout | ErrorKind::BlockedOrigin => ExitCode::from(2),
        _ => ExitCode::from(3),
    }
}

/// Run `diligent-rs collect`. `args` are the full argument list including the
/// leading `collect`.
pub fn collect_cli(args: &[String]) -> ExitCode {
    let rest = &args[1.min(args.len())..];
    if rest.iter().any(|a| a == "--help" || a == "-h") {
        println!("{HELP}");
        return ExitCode::SUCCESS;
    }
    let options = match parse(rest) {
        Ok(o) => o,
        Err(message) => {
            eprintln!("diligent-rs: {message}. Run `collect --help`.");
            return ExitCode::from(2);
        }
    };
    let Some(own_origin) = origin_of(&options.url) else {
        eprintln!("diligent-rs: --url must be an absolute http or https URL");
        return ExitCode::from(2);
    };
    let runtime = match tokio::runtime::Builder::new_multi_thread()
        .enable_all()
        .build()
    {
        Ok(r) => r,
        Err(e) => {
            eprintln!("diligent-rs: cannot start async runtime: {e}");
            return ExitCode::from(3);
        }
    };
    runtime.block_on(async move {
        let mut allowed: Vec<String> = options
            .allow_origins
            .iter()
            .map(|o| origin_of(o).unwrap_or_else(|| o.clone()))
            .collect();
        if allowed.is_empty() {
            allowed.push(own_origin);
        }
        let mut config = ChromeConfig::new();
        config.chrome_path = options.chrome_path.clone();
        config.allowed_origins = allowed;
        config.strict_origins = options.strict_origins;
        config.timeout = Duration::from_millis(options.timeout_ms);
        let mut browser = match ChromeBrowser::launch(config).await {
            Ok(b) => b,
            Err(e) => return fail(&e, options.json),
        };
        let mut plan = CollectPlan::new(options.url.clone());
        plan.viewports = options.viewports.clone();
        plan.media_modes = options.media.clone();
        plan.screenshots = options.screenshots;
        plan.corroborate_axe = options.corroborate_axe;
        plan.perf = options.perf;
        plan.timeout = Duration::from_millis(options.timeout_ms);
        plan.limits.max_nodes = options.max_nodes;
        plan.limits.max_bytes = options.max_bytes;
        let result = collect_page(&mut browser, &plan, &options.out).await;
        let _ = browser.close().await;
        match result {
            Ok(report) => {
                if options.json {
                    match serde_json::to_string(&report) {
                        Ok(text) => println!("{text}"),
                        Err(e) => {
                            eprintln!("diligent-rs: cannot serialize report: {e}");
                            return ExitCode::from(3);
                        }
                    }
                } else {
                    println!(
                        "collected {} evidence file(s), {} bytes, from {}\nmanifest: {}",
                        report.evidence.len(),
                        report.total_bytes,
                        report.final_url,
                        report.manifest_path
                    );
                }
                ExitCode::SUCCESS
            }
            Err(e) => fail(&e, options.json),
        }
    })
}
