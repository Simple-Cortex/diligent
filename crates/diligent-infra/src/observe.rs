//! Capturing an [`OriginObservation`]: every network fact the checks read, gathered once.

use diligent_source::time;
use url::Url;

use crate::fetch::{
    vet_target, FetchError, FetchResponse, Fetcher, Options, ResolveError, Resolver, MAX_REDIRECTS,
};
use crate::preload::{lookup_preload, PreloadStatus};

/// Why a redirect chain stopped.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChainEnd {
    /// The last hop is not a followable redirect.
    Final,
    /// More than [`MAX_REDIRECTS`] redirects.
    TooManyRedirects,
    /// A redirect pointed at a scheme other than http or https (not followed).
    BadScheme(String),
    /// A redirect `Location` could not be resolved to a URL.
    BadLocation(String),
    /// A redirect pointed at another registrable domain. It was not fetched.
    OffDomain(String),
    /// A request in the chain failed; earlier hops are still recorded.
    Error(FetchError),
}

/// The responses seen while following redirects from one start URL.
#[derive(Debug, Clone)]
pub struct Chain {
    /// Every response received, in order. The first is the start URL.
    pub hops: Vec<FetchResponse>,
    /// Why the chain ended.
    pub end: ChainEnd,
}

impl Chain {
    /// The last response when the chain ended normally.
    pub fn final_response(&self) -> Option<&FetchResponse> {
        if self.end == ChainEnd::Final {
            self.hops.last()
        } else {
            None
        }
    }

    /// The request failure that ended the chain, if any.
    pub fn error(&self) -> Option<&FetchError> {
        match &self.end {
            ChainEnd::Error(e) => Some(e),
            _ => None,
        }
    }
}

/// True for the status codes that carry a redirect.
pub fn is_redirect(status: u16) -> bool {
    matches!(status, 301 | 302 | 303 | 307 | 308)
}

/// Follows redirects from `start`, up to `max_redirects`, never leaving http and https.
///
/// Every request, including the first and each redirect hop, is vetted with
/// [`vet_target`] first and the connection is pinned to the vetted addresses. A
/// refused target ends the chain as [`ChainEnd::BadLocation`] and is never fetched.
/// A redirect to a registrable domain other than `apex` ends the chain as
/// [`ChainEnd::OffDomain`] and is not fetched.
pub fn follow(
    fetcher: &dyn Fetcher,
    resolver: &dyn Resolver,
    start: &Url,
    max_redirects: usize,
    apex: &str,
    exempt_host: Option<&str>,
) -> Chain {
    let mut hops: Vec<FetchResponse> = Vec::new();
    let mut url = start.clone();
    loop {
        if exempt_host.is_some_and(|h| host_is(&url, h)) {
            // Operator opted in to this exact host; no vetting or pinning.
            let resp = match fetcher.get(&url) {
                Ok(r) => r,
                Err(e) => {
                    return Chain {
                        hops,
                        end: ChainEnd::Error(e),
                    }
                }
            };
            let location = resp.header("location");
            let redirect = is_redirect(resp.status);
            hops.push(resp);
            let Some(location) = location.filter(|_| redirect) else {
                return Chain {
                    hops,
                    end: ChainEnd::Final,
                };
            };
            if hops.len() > max_redirects {
                return Chain {
                    hops,
                    end: ChainEnd::TooManyRedirects,
                };
            }
            match url.join(location.trim()) {
                Ok(next) if matches!(next.scheme(), "http" | "https") => {
                    if !on_apex(&next, apex) && !exempt_host.is_some_and(|h| host_is(&next, h)) {
                        return Chain {
                            hops,
                            end: ChainEnd::OffDomain(next.to_string()),
                        };
                    }
                    url = next;
                    continue;
                }
                Ok(next) => {
                    return Chain {
                        hops,
                        end: ChainEnd::BadScheme(next.scheme().to_string()),
                    }
                }
                Err(e) => {
                    return Chain {
                        hops,
                        end: ChainEnd::BadLocation(format!("{location}: {e}")),
                    }
                }
            }
        }
        let addrs = match vet_target(resolver, &url) {
            Ok(addrs) => addrs,
            Err(FetchError::Refused(why)) => {
                return Chain {
                    hops,
                    end: ChainEnd::BadLocation(format!("refused {url}: {why}")),
                }
            }
            Err(e) => {
                return Chain {
                    hops,
                    end: ChainEnd::Error(e),
                }
            }
        };
        let resp = match fetcher.get_pinned(&url, &addrs) {
            Ok(r) => r,
            Err(e) => {
                return Chain {
                    hops,
                    end: ChainEnd::Error(e),
                }
            }
        };
        let location = resp.header("location");
        let redirect = is_redirect(resp.status);
        hops.push(resp);
        let Some(location) = location.filter(|_| redirect) else {
            return Chain {
                hops,
                end: ChainEnd::Final,
            };
        };
        if hops.len() > max_redirects {
            return Chain {
                hops,
                end: ChainEnd::TooManyRedirects,
            };
        }
        match url.join(location.trim()) {
            Ok(next) if matches!(next.scheme(), "http" | "https") => {
                if !on_apex(&next, apex) && !exempt_host.is_some_and(|h| host_is(&next, h)) {
                    return Chain {
                        hops,
                        end: ChainEnd::OffDomain(next.to_string()),
                    };
                }
                url = next;
            }
            Ok(next) => {
                return Chain {
                    hops,
                    end: ChainEnd::BadScheme(next.scheme().to_string()),
                }
            }
            Err(e) => {
                return Chain {
                    hops,
                    end: ChainEnd::BadLocation(format!("{location}: {e}")),
                }
            }
        }
    }
}

/// True when `url`'s host is exactly `host` (case-insensitive, IPv6 without brackets).
fn host_is(url: &Url, host: &str) -> bool {
    match url.host() {
        Some(url::Host::Domain(d)) => d.eq_ignore_ascii_case(host),
        Some(url::Host::Ipv4(ip)) => ip.to_string() == host,
        Some(url::Host::Ipv6(ip)) => ip.to_string() == host,
        None => false,
    }
}

/// True when `url`'s host is on registrable domain `apex`. IP hosts never are.
fn on_apex(url: &Url, apex: &str) -> bool {
    match url.host() {
        Some(url::Host::Domain(d)) => registrable_domain(d) == apex,
        _ => false,
    }
}

/// A TXT lookup result.
#[derive(Debug, Clone)]
pub enum DnsObs {
    /// DNS checks were skipped by configuration.
    Skipped,
    /// The lookup answered; `txt` may be empty.
    Answer {
        /// The name queried.
        name: String,
        /// One string per TXT record.
        txt: Vec<String>,
    },
    /// The lookup failed.
    Failed {
        /// The name queried.
        name: String,
        /// The failure.
        error: ResolveError,
    },
}

/// Everything captured about one origin. Checks are pure functions of this value.
#[derive(Debug, Clone)]
pub struct OriginObservation {
    /// The origin as given (https).
    pub origin: Url,
    /// Host name of the origin, lowercase.
    pub host: String,
    /// Registrable domain of the host (see [`registrable_domain`]).
    pub apex: String,
    /// RFC 3339 capture time.
    pub captured_at: String,
    /// Run identifier.
    pub run_id: String,
    /// Chain from `http://host/`.
    pub http_redirect: Chain,
    /// Chain from the origin URL over https.
    pub https_home: Chain,
    /// `/vulnerability-disclosure-policy` on the final https host.
    pub vdp_page: Chain,
    /// `/.well-known/security.txt`.
    pub security_txt: Chain,
    /// `/robots.txt`.
    pub robots: Chain,
    /// `/sitemap.xml`.
    pub sitemap: Chain,
    /// Sitemaps declared by `Sitemap:` lines in robots.txt (first three, other than the default).
    pub declared_sitemaps: Vec<Chain>,
    /// TXT at the apex.
    pub spf: DnsObs,
    /// TXT at `_dmarc.<apex>`.
    pub dmarc: DnsObs,
    /// HSTS preload status from hstspreload.org (None if lookup not performed).
    pub preload_status: Option<PreloadStatus>,
    /// HSTS preload API response (None if lookup not performed or failed before response).
    pub preload_response: Option<FetchResponse>,
}

impl OriginObservation {
    /// The https response headers and body the header and HTML checks read, if one exists.
    pub fn https_response(&self) -> Option<&FetchResponse> {
        self.https_home
            .final_response()
            .filter(|r| r.url.scheme() == "https")
    }

    /// The host of the final https response, else the origin host.
    pub fn final_host(&self) -> String {
        self.https_response()
            .and_then(|r| r.url.host_str().map(str::to_ascii_lowercase))
            .unwrap_or_else(|| self.host.clone())
    }
}

/// Registrable domain by a small heuristic: the last two labels, or three when the TLD is a
/// two-letter country code under `co`, `com`, `gov`, `ac`, `org`, `net`, or `edu`. This is not
/// the Public Suffix List.
pub fn registrable_domain(host: &str) -> String {
    let host = host.trim_end_matches('.').to_ascii_lowercase();
    let labels: Vec<&str> = host.split('.').collect();
    if labels.len() <= 2 || host.parse::<std::net::IpAddr>().is_ok() {
        return host;
    }
    let n = labels.len();
    let tld = labels[n - 1];
    let sld = labels[n - 2];
    let take =
        if tld.len() == 2 && matches!(sld, "co" | "com" | "gov" | "ac" | "org" | "net" | "edu") {
            3
        } else {
            2
        };
    labels[n.saturating_sub(take)..].join(".")
}

fn with_path(base: &Url, path: &str) -> Url {
    let mut u = base.clone();
    u.set_path(path);
    u.set_query(None);
    u.set_fragment(None);
    u
}

/// Sitemap URLs declared by `Sitemap:` lines in a robots.txt body.
pub fn declared_sitemap_urls(robots_text: &str) -> Vec<Url> {
    let mut out = Vec::new();
    for line in robots_text.lines() {
        let line = line.split('#').next().unwrap_or("").trim();
        if let Some((field, value)) = line.split_once(':') {
            if field.trim().eq_ignore_ascii_case("sitemap") {
                if let Ok(u) = Url::parse(value.trim()) {
                    if matches!(u.scheme(), "http" | "https") {
                        out.push(u);
                    }
                }
            }
        }
    }
    out
}

fn lookup(resolver: &dyn Resolver, name: String, opts: &Options) -> DnsObs {
    if opts.skip_dns {
        return DnsObs::Skipped;
    }
    match resolver.txt(&name) {
        Ok(txt) => DnsObs::Answer { name, txt },
        Err(error) => DnsObs::Failed { name, error },
    }
}

/// Captures every observation for `origin`. Returns `None` when `origin` has no host.
pub fn capture(
    origin: &Url,
    fetcher: &dyn Fetcher,
    resolver: &dyn Resolver,
    opts: &Options,
) -> Option<OriginObservation> {
    let host = origin.host_str()?.to_ascii_lowercase();
    let apex = registrable_domain(&host);
    let exempt: Option<String> = opts.allow_private_origin.then(|| match origin.host() {
        Some(url::Host::Ipv6(ip)) => ip.to_string(),
        _ => host.clone(),
    });
    let captured_at = opts.captured_at.clone().unwrap_or_else(time::now_rfc3339);
    let run_id = opts
        .run_id
        .clone()
        .unwrap_or_else(|| time::new_run_id(origin.as_str()));

    let http_start = Url::parse(&format!("http://{host}/")).ok()?;
    let http_redirect = follow(
        fetcher,
        resolver,
        &http_start,
        MAX_REDIRECTS,
        &apex,
        exempt.as_deref(),
    );
    let https_home = follow(
        fetcher,
        resolver,
        origin,
        MAX_REDIRECTS,
        &apex,
        exempt.as_deref(),
    );

    let base = https_home
        .final_response()
        .filter(|r| r.url.scheme() == "https")
        .map(|r| r.url.clone())
        .unwrap_or_else(|| origin.clone());
    let vdp_page = follow(
        fetcher,
        resolver,
        &with_path(&base, "/vulnerability-disclosure-policy"),
        MAX_REDIRECTS,
        &apex,
        exempt.as_deref(),
    );
    let security_txt = follow(
        fetcher,
        resolver,
        &with_path(&base, "/.well-known/security.txt"),
        MAX_REDIRECTS,
        &apex,
        exempt.as_deref(),
    );
    let robots = follow(
        fetcher,
        resolver,
        &with_path(&base, "/robots.txt"),
        MAX_REDIRECTS,
        &apex,
        exempt.as_deref(),
    );
    let default_sitemap = with_path(&base, "/sitemap.xml");
    let sitemap = follow(
        fetcher,
        resolver,
        &default_sitemap,
        MAX_REDIRECTS,
        &apex,
        exempt.as_deref(),
    );

    let mut declared_sitemaps = Vec::new();
    if let Some(r) = robots.final_response().filter(|r| r.status == 200) {
        for u in declared_sitemap_urls(&r.text())
            .into_iter()
            .filter(|u| u != &default_sitemap)
            .take(3)
        {
            if on_apex(&u, &apex) || exempt.as_deref().is_some_and(|h| host_is(&u, h)) {
                declared_sitemaps.push(follow(
                    fetcher,
                    resolver,
                    &u,
                    MAX_REDIRECTS,
                    &apex,
                    exempt.as_deref(),
                ));
            } else {
                declared_sitemaps.push(Chain {
                    hops: Vec::new(),
                    end: ChainEnd::OffDomain(u.to_string()),
                });
            }
        }
    }

    let spf = lookup(resolver, apex.clone(), opts);
    let dmarc = lookup(resolver, format!("_dmarc.{apex}"), opts);

    let (preload_status, preload_response) = if opts.preload_lookup {
        let result = lookup_preload(fetcher, &apex);
        (Some(result.status), result.response)
    } else {
        (None, None)
    };

    Some(OriginObservation {
        origin: origin.clone(),
        host,
        apex,
        captured_at,
        run_id,
        http_redirect,
        https_home,
        vdp_page,
        security_txt,
        robots,
        sitemap,
        declared_sitemaps,
        spf,
        dmarc,
        preload_status,
        preload_response,
    })
}
