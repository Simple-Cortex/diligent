//! The fetch layer: traits the checks depend on, plus the real HTTP and DNS implementations.
//!
//! The traits are synchronous so the checks and `run` stay plain functions. The
//! real implementations own a small tokio runtime and `block_on` inside each call,
//! which means they must not be called from inside another tokio runtime.

use std::net::{IpAddr, SocketAddr};

use diligent_netpolicy::{forbidden_reason, is_forbidden_host_literal};
use std::time::Duration;

use hickory_resolver::net::NetError;
use hickory_resolver::proto::rr::RData;
use hickory_resolver::TokioResolver;
use thiserror::Error;
use url::Url;

/// Largest number of redirects `run` follows for any one request chain.
pub const MAX_REDIRECTS: usize = 5;

/// Run-time settings for a scan.
#[derive(Debug, Clone)]
pub struct Options {
    /// Per-request timeout in milliseconds. Default 15000.
    pub timeout_ms: u64,
    /// Largest response body kept per request, in bytes. Default 2 MiB.
    pub max_body_bytes: usize,
    /// Skip every DNS check and record them as `not_tested`.
    pub skip_dns: bool,
    /// Exempt the start URL's host (exactly that host, not its domain) from the
    /// address policy, for local dev servers. Default false.
    pub allow_private_origin: bool,
    /// Fixed capture time (RFC 3339). `None` uses the clock. Set by fixtures.
    pub captured_at: Option<String>,
    /// Fixed run id. `None` generates one. Set by fixtures.
    pub run_id: Option<String>,
    /// Query hstspreload.org for INFRA-02-hsts-preload. Default false.
    pub preload_lookup: bool,
}

impl Default for Options {
    fn default() -> Self {
        Options {
            timeout_ms: 15_000,
            max_body_bytes: 2 * 1024 * 1024,
            skip_dns: false,
            allow_private_origin: false,
            captured_at: None,
            run_id: None,
            preload_lookup: false,
        }
    }
}

/// One HTTP response, with the body capped at `Options::max_body_bytes`.
#[derive(Debug, Clone)]
pub struct FetchResponse {
    /// The URL that was requested (redirects are not followed by the fetcher).
    pub url: Url,
    /// HTTP status code.
    pub status: u16,
    /// Response headers with lowercase names, in wire order. A name may repeat.
    pub headers: Vec<(String, String)>,
    /// Response body bytes, possibly truncated.
    pub body: Vec<u8>,
    /// True when the body was cut at the size cap.
    pub truncated: bool,
}

impl FetchResponse {
    /// All values of header `name` (case-insensitive) joined with `, `, or `None`.
    pub fn header(&self, name: &str) -> Option<String> {
        let values: Vec<&str> = self
            .headers
            .iter()
            .filter(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
            .collect();
        if values.is_empty() {
            None
        } else {
            Some(values.join(", "))
        }
    }

    /// The body as text, replacing invalid UTF-8.
    pub fn text(&self) -> String {
        String::from_utf8_lossy(&self.body).into_owned()
    }
}

/// Why a request produced no response.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum FetchError {
    /// The request exceeded the timeout.
    #[error("timeout")]
    Timeout,
    /// The TLS handshake failed because of the certificate (untrusted, expired, wrong name).
    #[error("tls certificate error: {0}")]
    Tls(String),
    /// DNS, TCP, or another connection failure.
    #[error("connection failed: {0}")]
    Connect(String),
    /// Any other failure (malformed response, body read error).
    #[error("request failed: {0}")]
    Other(String),
    /// The target was refused before any request was sent (loopback, private,
    /// link-local or metadata address).
    #[error("refused: {0}")]
    Refused(String),
}

/// Performs one GET request. Implementations must not follow redirects.
pub trait Fetcher {
    /// GET `url`. A non-2xx status is a response, not an error.
    fn get(&self, url: &Url) -> Result<FetchResponse, FetchError>;

    /// GET `url`, connecting only to `addrs` (the addresses the caller vetted) so a
    /// second DNS answer cannot redirect the connection. The default ignores
    /// `addrs`; the reqwest fetcher pins the connection to them.
    fn get_pinned(&self, url: &Url, addrs: &[IpAddr]) -> Result<FetchResponse, FetchError> {
        let _ = addrs;
        self.get(url)
    }
}

/// Why a DNS lookup produced no answer set.
#[derive(Debug, Clone, PartialEq, Eq, Error)]
pub enum ResolveError {
    /// The lookup exceeded the timeout.
    #[error("timeout")]
    Timeout,
    /// The resolver failed (SERVFAIL, no network, bad configuration).
    #[error("resolver failed: {0}")]
    Failed(String),
}

/// Looks up TXT records.
pub trait Resolver {
    /// TXT strings at `name`, one string per record (character-strings concatenated).
    /// A name with no TXT records, or that does not exist, is `Ok(vec![])`.
    fn txt(&self, name: &str) -> Result<Vec<String>, ResolveError>;

    /// A and AAAA addresses of `host`. A name that does not exist is `Ok(vec![])`.
    fn addrs(&self, host: &str) -> Result<Vec<IpAddr>, ResolveError>;
}

/// Vets `url` before a request: resolves its host (literal IP hosts are checked
/// directly) and refuses it if any address is forbidden. Returns the vetted
/// addresses so the caller can pin the connection to them. A refusal is
/// [`FetchError::Refused`]; a failed lookup is [`FetchError::Connect`].
pub fn vet_target(resolver: &dyn Resolver, url: &Url) -> Result<Vec<IpAddr>, FetchError> {
    let addrs = match url.host() {
        None => return Err(FetchError::Refused(format!("{url} has no host"))),
        Some(url::Host::Ipv4(ip)) => vec![IpAddr::V4(ip)],
        Some(url::Host::Ipv6(ip)) => vec![IpAddr::V6(ip)],
        Some(url::Host::Domain(d)) if is_forbidden_host_literal(d) => {
            return Err(FetchError::Refused(format!(
                "{url}: {d} is a local host name"
            )))
        }
        Some(url::Host::Domain(d)) => resolver
            .addrs(d.trim_end_matches('.'))
            .map_err(|e| FetchError::Connect(format!("address lookup for {d}: {e}")))?,
    };
    if addrs.is_empty() {
        return Err(FetchError::Connect(format!(
            "{url} did not resolve to any address"
        )));
    }
    if let Some((bad, why)) = addrs
        .iter()
        .find_map(|a| forbidden_reason(*a).map(|w| (a, w)))
    {
        return Err(FetchError::Refused(format!(
            "{url} resolves to {bad}: {why}"
        )));
    }
    Ok(addrs)
}

fn runtime() -> Result<tokio::runtime::Runtime, String> {
    tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|e| format!("cannot start async runtime: {e}"))
}

fn error_chain(e: &(dyn std::error::Error + 'static)) -> String {
    let mut parts = vec![e.to_string()];
    let mut cur = e.source();
    while let Some(s) = cur {
        parts.push(s.to_string());
        cur = s.source();
    }
    parts.join(": ")
}

fn classify(e: reqwest::Error) -> FetchError {
    let chain = error_chain(&e);
    let lower = chain.to_ascii_lowercase();
    if e.is_timeout() {
        FetchError::Timeout
    } else if lower.contains("certificate")
        || lower.contains("unknownissuer")
        || lower.contains("notvalidforname")
        || lower.contains("badsignature")
    {
        FetchError::Tls(chain)
    } else if e.is_connect() || e.is_request() {
        FetchError::Connect(chain)
    } else {
        FetchError::Other(chain)
    }
}

/// HTTP fetcher backed by reqwest with rustls. Redirects are never followed here.
pub struct ReqwestFetcher {
    rt: tokio::runtime::Runtime,
    client: reqwest::Client,
    opts: Options,
}

impl ReqwestFetcher {
    fn base_builder(opts: &Options) -> reqwest::ClientBuilder {
        reqwest::Client::builder()
            .redirect(reqwest::redirect::Policy::none())
            .timeout(Duration::from_millis(opts.timeout_ms))
            .connect_timeout(Duration::from_millis(opts.timeout_ms))
            .user_agent(concat!(
                "diligent-infra/",
                env!("CARGO_PKG_VERSION"),
                " (+https://github.com/Simple-Cortex/diligent)"
            ))
    }

    /// Builds a fetcher using the timeout and body cap in `opts`.
    pub fn new(opts: &Options) -> Result<Self, String> {
        let rt = runtime()?;
        let client = {
            let _guard = rt.enter();
            Self::base_builder(opts)
                .build()
                .map_err(|e| format!("cannot build HTTP client: {e}"))?
        };
        Ok(ReqwestFetcher {
            rt,
            client,
            opts: opts.clone(),
        })
    }

    fn send(&self, client: &reqwest::Client, url: &Url) -> Result<FetchResponse, FetchError> {
        let max_body = self.opts.max_body_bytes;
        self.rt.block_on(async {
            let mut resp = client.get(url.clone()).send().await.map_err(classify)?;
            let status = resp.status().as_u16();
            let headers: Vec<(String, String)> = resp
                .headers()
                .iter()
                .map(|(k, v)| {
                    (
                        k.as_str().to_ascii_lowercase(),
                        String::from_utf8_lossy(v.as_bytes()).into_owned(),
                    )
                })
                .collect();
            let mut body: Vec<u8> = Vec::new();
            let mut truncated = false;
            while let Some(chunk) = resp.chunk().await.map_err(classify)? {
                let room = max_body.saturating_sub(body.len());
                if chunk.len() > room {
                    body.extend_from_slice(&chunk[..room]);
                    truncated = true;
                    break;
                }
                body.extend_from_slice(&chunk);
            }
            Ok(FetchResponse {
                url: url.clone(),
                status,
                headers,
                body,
                truncated,
            })
        })
    }
}

impl Fetcher for ReqwestFetcher {
    fn get(&self, url: &Url) -> Result<FetchResponse, FetchError> {
        self.send(&self.client, url)
    }

    /// Pins the connection with `ClientBuilder::resolve_to_addrs`, so the host is
    /// not resolved a second time. A literal IP host needs no pin. Note that a
    /// proxy configured through the environment resolves the host itself.
    fn get_pinned(&self, url: &Url, addrs: &[IpAddr]) -> Result<FetchResponse, FetchError> {
        let Some(url::Host::Domain(domain)) = url.host() else {
            return self.get(url);
        };
        if addrs.is_empty() {
            return self.get(url);
        }
        let port = url.port_or_known_default().unwrap_or(443);
        let sockets: Vec<SocketAddr> = addrs.iter().map(|ip| SocketAddr::new(*ip, port)).collect();
        let client = {
            let _guard = self.rt.enter();
            Self::base_builder(&self.opts)
                .resolve_to_addrs(domain, &sockets)
                .build()
                .map_err(|e| FetchError::Other(format!("cannot build pinned HTTP client: {e}")))?
        };
        self.send(&client, url)
    }
}

/// DNS resolver backed by hickory using the system configuration.
pub struct DnsResolver {
    rt: tokio::runtime::Runtime,
    inner: Result<TokioResolver, String>,
}

impl DnsResolver {
    /// Builds a resolver with the timeout in `opts` and one attempt per query.
    pub fn new(opts: &Options) -> Result<Self, String> {
        let rt = runtime()?;
        let timeout = Duration::from_millis(opts.timeout_ms);
        let inner = rt.block_on(async {
            let mut builder = TokioResolver::builder_tokio().map_err(|e| e.to_string())?;
            builder.options_mut().timeout = timeout;
            builder.options_mut().attempts = 1;
            builder.build().map_err(|e| e.to_string())
        });
        Ok(DnsResolver { rt, inner })
    }
}

impl Resolver for DnsResolver {
    fn addrs(&self, host: &str) -> Result<Vec<IpAddr>, ResolveError> {
        let resolver = self
            .inner
            .as_ref()
            .map_err(|e| ResolveError::Failed(e.clone()))?;
        let fqdn = if host.ends_with('.') {
            host.to_string()
        } else {
            format!("{host}.")
        };
        match self.rt.block_on(resolver.lookup_ip(fqdn)) {
            Ok(lookup) => Ok(lookup.iter().collect()),
            Err(e) if e.is_no_records_found() => Ok(Vec::new()),
            Err(NetError::Timeout) => Err(ResolveError::Timeout),
            Err(e) => Err(ResolveError::Failed(e.to_string())),
        }
    }

    fn txt(&self, name: &str) -> Result<Vec<String>, ResolveError> {
        let resolver = self
            .inner
            .as_ref()
            .map_err(|e| ResolveError::Failed(e.clone()))?;
        let fqdn = if name.ends_with('.') {
            name.to_string()
        } else {
            format!("{name}.")
        };
        match self.rt.block_on(resolver.txt_lookup(fqdn)) {
            Ok(lookup) => {
                let mut out = Vec::new();
                for record in lookup.answers() {
                    if let RData::TXT(txt) = &record.data {
                        let mut joined = String::new();
                        for part in txt.txt_data.iter() {
                            joined.push_str(&String::from_utf8_lossy(part));
                        }
                        out.push(joined);
                    }
                }
                Ok(out)
            }
            Err(e) if e.is_no_records_found() => Ok(Vec::new()),
            Err(NetError::Timeout) => Err(ResolveError::Timeout),
            Err(e) => Err(ResolveError::Failed(e.to_string())),
        }
    }
}
