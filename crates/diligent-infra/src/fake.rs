//! Offline fakes for tests and fixtures: a fetcher and resolver fed from `observation.json`.
//!
//! Format:
//! ```json
//! {
//!   "responses": { "https://host/": { "status": 200, "headers": {"name": "value"}, "body": "text" } },
//!   "errors":    { "https://host/x": "timeout" },
//!   "default":   { "status": 404 },
//!   "default_error": "timeout",
//!   "dns": { "host": ["txt one"] },
//!   "dns_errors": { "_dmarc.host": "timeout" },
//!   "addrs": { "host": ["10.0.0.5"] }
//! }
//! ```
//! An error string is `timeout`, `tls:<message>`, or `connect:<message>`. A URL in none of
//! the maps uses `default` if present, then `default_error`, then a connect error.
//! A DNS name in neither `dns` nor `dns_errors` has no TXT records. A host not in `addrs`
//! resolves to the public address 93.184.216.34.

use std::collections::HashMap;
use std::net::IpAddr;

use serde_json::Value;
use url::Url;

use crate::fetch::{FetchError, FetchResponse, Fetcher, ResolveError, Resolver};

/// Fetcher answering from a fixed table.
#[derive(Debug, Default)]
pub struct FakeFetcher {
    responses: HashMap<String, Value>,
    errors: HashMap<String, FetchError>,
    default: Option<Value>,
    default_error: Option<FetchError>,
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

impl FakeFetcher {
    /// Builds a fetcher from an `observation.json` value.
    pub fn from_observation(v: &Value) -> Self {
        let mut f = FakeFetcher::default();
        if let Some(map) = v.get("responses").and_then(Value::as_object) {
            for (k, r) in map {
                f.responses.insert(k.clone(), r.clone());
            }
        }
        if let Some(map) = v.get("errors").and_then(Value::as_object) {
            for (k, e) in map {
                f.errors
                    .insert(k.clone(), parse_error(e.as_str().unwrap_or("unknown")));
            }
        }
        f.default = v.get("default").cloned();
        f.default_error = v
            .get("default_error")
            .and_then(Value::as_str)
            .map(parse_error);
        f
    }

    fn build(url: &Url, spec: &Value) -> FetchResponse {
        let status = spec.get("status").and_then(Value::as_u64).unwrap_or(200) as u16;
        let headers = spec
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
        let body = spec
            .get("body")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .as_bytes()
            .to_vec();
        FetchResponse {
            url: url.clone(),
            status,
            headers,
            body,
            truncated: false,
        }
    }
}

impl Fetcher for FakeFetcher {
    fn get(&self, url: &Url) -> Result<FetchResponse, FetchError> {
        let key = url.as_str();
        if let Some(e) = self.errors.get(key) {
            return Err(e.clone());
        }
        if let Some(spec) = self.responses.get(key) {
            return Ok(Self::build(url, spec));
        }
        if let Some(spec) = &self.default {
            return Ok(Self::build(url, spec));
        }
        if let Some(e) = &self.default_error {
            return Err(e.clone());
        }
        Err(FetchError::Connect(format!("no fixture for {key}")))
    }
}

/// Resolver answering from a fixed table.
#[derive(Debug, Default)]
pub struct FakeResolver {
    txt: HashMap<String, Vec<String>>,
    errors: HashMap<String, ResolveError>,
    addrs: HashMap<String, Vec<IpAddr>>,
}

impl FakeResolver {
    /// Builds a resolver from an `observation.json` value.
    pub fn from_observation(v: &Value) -> Self {
        let mut r = FakeResolver::default();
        if let Some(map) = v.get("dns").and_then(Value::as_object) {
            for (k, list) in map {
                let items = list
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().map(str::to_string))
                            .collect()
                    })
                    .unwrap_or_default();
                r.txt.insert(k.to_ascii_lowercase(), items);
            }
        }
        if let Some(map) = v.get("dns_errors").and_then(Value::as_object) {
            for (k, e) in map {
                let err = match e.as_str() {
                    Some("timeout") => ResolveError::Timeout,
                    Some(other) => ResolveError::Failed(other.to_string()),
                    None => ResolveError::Failed("unknown".to_string()),
                };
                r.errors.insert(k.to_ascii_lowercase(), err);
            }
        }
        if let Some(map) = v.get("addrs").and_then(Value::as_object) {
            for (k, list) in map {
                let ips = list
                    .as_array()
                    .map(|a| {
                        a.iter()
                            .filter_map(|x| x.as_str().and_then(|t| t.parse().ok()))
                            .collect()
                    })
                    .unwrap_or_default();
                r.addrs.insert(k.to_ascii_lowercase(), ips);
            }
        }
        r
    }
}

impl Resolver for FakeResolver {
    fn addrs(&self, host: &str) -> Result<Vec<IpAddr>, ResolveError> {
        let key = host.trim_end_matches('.').to_ascii_lowercase();
        Ok(self
            .addrs
            .get(&key)
            .cloned()
            .unwrap_or_else(|| vec![IpAddr::from([93, 184, 216, 34])]))
    }

    fn txt(&self, name: &str) -> Result<Vec<String>, ResolveError> {
        let key = name.trim_end_matches('.').to_ascii_lowercase();
        if let Some(e) = self.errors.get(&key) {
            return Err(e.clone());
        }
        Ok(self.txt.get(&key).cloned().unwrap_or_default())
    }
}

/// Runs the checks against an `observation.json` value using the fakes.
///
/// Reads `origin` (required), `captured_at`, and `run_id` from the value. Returns `None`
/// when `origin` is missing or not a URL.
pub fn run_observation(v: &Value) -> Option<crate::InfraReport> {
    let origin = Url::parse(v.get("origin")?.as_str()?).ok()?;
    let opts = crate::Options {
        captured_at: v
            .get("captured_at")
            .and_then(Value::as_str)
            .map(str::to_string),
        run_id: v.get("run_id").and_then(Value::as_str).map(str::to_string),
        ..crate::Options::default()
    };
    let fetcher = FakeFetcher::from_observation(v);
    let resolver = FakeResolver::from_observation(v);
    Some(crate::run(&origin, &fetcher, &resolver, &opts))
}
