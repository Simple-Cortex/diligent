//! Offline fixtures: a site served from `site.json`, and a runner for `fixtures/inventory/<case>/`.
//!
//! `site.json` maps a URL to a response:
//!
//! ```json
//! {
//!   "https://example.gov/": { "status": 200, "headers": {"content-type": "text/html"}, "body": "<html>..." },
//!   "https://example.gov/sitemap.xml.gz": { "body_file": "sitemap.xml", "gzip": true },
//!   "https://example.gov/old": { "location": "https://other.example/new" },
//!   "https://example.gov/down": { "error": "timeout" }
//! }
//! ```
//!
//! `location` implies status 302 unless `status` is given. An error string is `timeout`,
//! `tls:<message>` or `connect:<message>`. A URL that is not listed answers 404. The body is cut
//! at `max_body_bytes` the way the real fetcher cuts it.

use std::cell::RefCell;
use std::collections::HashMap;
use std::io::Write;
use std::path::Path;

use diligent_infra::fake::FakeResolver;
use diligent_infra::fetch::{FetchError, FetchResponse, Fetcher};
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::{json, Value};
use url::Url;

use crate::clock::ManualClock;
use crate::crawl::run;
use crate::model::{parse_processes, InventoryOptions, Outcome};

struct Spec {
    status: u16,
    headers: Vec<(String, String)>,
    body: Vec<u8>,
    error: Option<FetchError>,
}

/// A fetcher that answers from a table and records every URL it was asked for.
pub struct SiteFetcher {
    responses: HashMap<String, Spec>,
    max_body_bytes: usize,
    log: RefCell<Vec<String>>,
}

fn parse_error(s: &str) -> FetchError {
    if s == "timeout" {
        FetchError::Timeout
    } else if let Some(m) = s.strip_prefix("tls:") {
        FetchError::Tls(m.to_string())
    } else if let Some(m) = s.strip_prefix("connect:") {
        FetchError::Connect(m.to_string())
    } else {
        FetchError::Other(s.to_string())
    }
}

fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    let _ = enc.write_all(bytes);
    enc.finish().unwrap_or_default()
}

impl SiteFetcher {
    /// Builds the fetcher. `dir` resolves `body_file` paths.
    pub fn from_site(
        site: &Value,
        dir: Option<&Path>,
        max_body_bytes: usize,
    ) -> Result<Self, String> {
        let map = site
            .as_object()
            .ok_or("site.json must be an object of URL -> response")?;
        let mut responses = HashMap::new();
        for (url, spec) in map {
            let location = spec.get("location").and_then(Value::as_str);
            let status = spec
                .get("status")
                .and_then(Value::as_u64)
                .map(|s| s as u16)
                .unwrap_or(if location.is_some() { 302 } else { 200 });
            let mut headers: Vec<(String, String)> = spec
                .get("headers")
                .and_then(Value::as_object)
                .map(|m| {
                    m.iter()
                        .map(|(k, v)| {
                            (
                                k.to_ascii_lowercase(),
                                v.as_str().unwrap_or_default().to_string(),
                            )
                        })
                        .collect()
                })
                .unwrap_or_default();
            if let Some(l) = location {
                headers.push(("location".to_string(), l.to_string()));
            }
            let mut body = if let Some(text) = spec.get("body").and_then(Value::as_str) {
                text.as_bytes().to_vec()
            } else if let Some(file) = spec.get("body_file").and_then(Value::as_str) {
                let dir = dir.ok_or("body_file needs a case directory")?;
                std::fs::read(dir.join(file))
                    .map_err(|e| format!("{url}: cannot read {file}: {e}"))?
            } else {
                Vec::new()
            };
            if spec.get("gzip").and_then(Value::as_bool).unwrap_or(false) {
                body = gzip(&body);
            }
            let error = spec.get("error").and_then(Value::as_str).map(parse_error);
            responses.insert(
                url.clone(),
                Spec {
                    status,
                    headers,
                    body,
                    error,
                },
            );
        }
        Ok(SiteFetcher {
            responses,
            max_body_bytes,
            log: RefCell::new(Vec::new()),
        })
    }

    /// Every URL requested so far, in order.
    pub fn requests(&self) -> Vec<String> {
        self.log.borrow().clone()
    }
}

impl Fetcher for SiteFetcher {
    fn get(&self, url: &Url) -> Result<FetchResponse, FetchError> {
        self.log.borrow_mut().push(url.as_str().to_string());
        let Some(spec) = self.responses.get(url.as_str()) else {
            return Ok(FetchResponse {
                url: url.clone(),
                status: 404,
                headers: vec![("content-type".to_string(), "text/html".to_string())],
                body: Vec::new(),
                truncated: false,
            });
        };
        if let Some(e) = &spec.error {
            return Err(e.clone());
        }
        let truncated = spec.body.len() > self.max_body_bytes;
        let body = if truncated {
            spec.body[..self.max_body_bytes].to_vec()
        } else {
            spec.body.clone()
        };
        Ok(FetchResponse {
            url: url.clone(),
            status: spec.status,
            headers: spec.headers.clone(),
            body,
            truncated,
        })
    }
}

/// Reads `options.json` content: the start URL and the run options.
pub fn options_from_json(v: &Value) -> Result<(Url, InventoryOptions), String> {
    let start = v
        .get("start")
        .and_then(Value::as_str)
        .ok_or("options.json needs `start`")?;
    let start = Url::parse(start).map_err(|e| format!("start: {e}"))?;
    let text = |k: &str| v.get(k).and_then(Value::as_str).map(str::to_string);
    let mut o = InventoryOptions::new(
        text("run_id").unwrap_or_else(|| "run-2026-10-18-0a1b2c".to_string()),
        text("audit_id").unwrap_or_else(|| "fixture".to_string()),
    );
    o.seed = text("seed");
    let list = |k: &str| -> Vec<String> {
        v.get(k)
            .and_then(Value::as_array)
            .map(|a| {
                a.iter()
                    .filter_map(|x| x.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default()
    };
    o.allowed_origins = list("allowed_origins");
    o.query_keys = list("query_keys");
    if let Some(n) = v.get("max_fetch").and_then(Value::as_u64) {
        o.max_fetch = n as usize;
    }
    if let Some(n) = v.get("max_discovered").and_then(Value::as_u64) {
        o.max_discovered = n as usize;
    }
    if let Some(n) = v.get("max_documents").and_then(Value::as_u64) {
        o.max_documents = n as usize;
    }
    if let Some(n) = v.get("max_time_ms").and_then(Value::as_u64) {
        o.max_time_ms = n;
    }
    if let Some(n) = v.get("max_body_bytes").and_then(Value::as_u64) {
        o.net.max_body_bytes = n as usize;
    }
    if let Some(n) = v.get("requests_per_second").and_then(Value::as_f64) {
        o.requests_per_second = n;
    }
    o.ignore_robots = v
        .get("ignore_robots")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    o.net.allow_private_origin = v
        .get("allow_private")
        .and_then(Value::as_bool)
        .unwrap_or(false);
    Ok((start, o))
}

/// The result of running one options file of a fixture case.
pub struct CaseRun {
    /// `""` for `options.json`, `-b` for `options-b.json`.
    pub suffix: String,
    /// Inventory and sample.
    pub outcome: Outcome,
    /// Every URL the fetcher was asked for.
    pub requests: Vec<String>,
    /// Total time the rate limiter slept on the manual clock, in milliseconds.
    pub slept_ms: u64,
}

/// `(suffix, file name)` for `options.json` and every `options-<suffix>.json` in `dir`, sorted.
pub fn option_files(dir: &Path) -> Vec<(String, String)> {
    let mut out = Vec::new();
    if let Ok(rd) = std::fs::read_dir(dir) {
        for e in rd.flatten() {
            let name = e.file_name().to_string_lossy().into_owned();
            if name == "options.json" {
                out.push((String::new(), name));
            } else if let Some(s) = name
                .strip_prefix("options")
                .and_then(|r| r.strip_suffix(".json"))
            {
                if s.starts_with('-') {
                    out.push((s.to_string(), name));
                }
            }
        }
    }
    out.sort();
    out
}

/// Runs `options_file` of the case in `dir` against its `site.json` with a manual clock.
pub fn run_case(dir: &Path, suffix: &str, options_file: &str) -> Result<CaseRun, String> {
    let read = |name: &str| -> Result<Value, String> {
        let text = std::fs::read_to_string(dir.join(name))
            .map_err(|e| format!("{}: {e}", dir.join(name).display()))?;
        serde_json::from_str(&text).map_err(|e| format!("{name}: {e}"))
    };
    let site = read("site.json")?;
    let options = read(options_file)?;
    let (start, mut opts) = options_from_json(&options)?;
    if dir.join("processes.json").exists() {
        opts.processes = parse_processes(&read("processes.json")?)?;
    }
    let fetcher = SiteFetcher::from_site(&site, Some(dir), opts.net.max_body_bytes)?;
    let resolver = FakeResolver::from_observation(
        &json!({ "addrs": options.get("addrs").cloned().unwrap_or(json!({})) }),
    );
    let clock = ManualClock::new();
    let outcome = run(&start, &opts, &fetcher, &resolver, &clock);
    Ok(CaseRun {
        suffix: suffix.to_string(),
        outcome,
        requests: fetcher.requests(),
        slept_ms: clock.total_slept_ms(),
    })
}
