//! SSRF hardening: redirect hops and declared sitemaps are vetted before any request.

use std::cell::RefCell;
use std::net::IpAddr;

use diligent_infra::fake::{FakeFetcher, FakeResolver};
use diligent_infra::observe::{capture, ChainEnd, OriginObservation};
use diligent_infra::{
    run, FetchError, FetchResponse, Fetcher, Options, ResolveError, Resolver, Status,
};
use serde_json::{json, Value};
use url::Url;

/// Records every URL it is asked to fetch, then delegates to the fake.
struct Recording {
    inner: FakeFetcher,
    calls: RefCell<Vec<String>>,
}

impl Fetcher for Recording {
    fn get(&self, url: &Url) -> Result<FetchResponse, FetchError> {
        self.calls.borrow_mut().push(url.to_string());
        self.inner.get(url)
    }
}

/// Resolver where chosen hosts resolve to chosen addresses; others are public.
struct Addrs {
    inner: FakeResolver,
    map: Vec<(&'static str, IpAddr)>,
}

impl Resolver for Addrs {
    fn txt(&self, name: &str) -> Result<Vec<String>, ResolveError> {
        self.inner.txt(name)
    }
    fn addrs(&self, host: &str) -> Result<Vec<IpAddr>, ResolveError> {
        match self.map.iter().find(|(h, _)| *h == host) {
            Some((_, ip)) => Ok(vec![*ip]),
            None => self.inner.addrs(host),
        }
    }
}

fn redirect(to: &str) -> Value {
    json!({ "status": 302, "headers": { "Location": to } })
}

fn observation(responses: Value) -> Value {
    json!({
        "origin": "https://www.example.gov/",
        "captured_at": "2026-01-01T00:00:00Z",
        "run_id": "ssrf",
        "responses": responses,
        "default": { "status": 404 }
    })
}

fn harness(v: &Value, map: Vec<(&'static str, IpAddr)>) -> (Recording, Addrs) {
    (
        Recording {
            inner: FakeFetcher::from_observation(v),
            calls: RefCell::new(Vec::new()),
        },
        Addrs {
            inner: FakeResolver::from_observation(v),
            map,
        },
    )
}

fn captured(v: &Value, map: Vec<(&'static str, IpAddr)>) -> (OriginObservation, Vec<String>) {
    let (f, r) = harness(v, map);
    let origin = Url::parse("https://www.example.gov/").unwrap();
    let obs = capture(&origin, &f, &r, &Options::default()).unwrap();
    let calls = f.calls.borrow().clone();
    (obs, calls)
}

fn assert_refused_not_fetched(target: &str, map: Vec<(&'static str, IpAddr)>) {
    let v = observation(json!({ "https://www.example.gov/": redirect(target) }));
    let (obs, calls) = captured(&v, map.clone());
    assert!(
        matches!(
            obs.https_home.end,
            ChainEnd::BadLocation(_) | ChainEnd::OffDomain(_)
        ),
        "{target}: {:?}",
        obs.https_home.end
    );
    assert!(
        calls.iter().all(|c| c != target),
        "{target} was fetched: {calls:?}"
    );
    // Not a redirect pass either.
    let (f, r) = harness(&v, map);
    let origin = Url::parse("https://www.example.gov/").unwrap();
    let report = run(&origin, &f, &r, &Options::default());
    for row in report
        .rows
        .iter()
        .filter(|row| row.check_id.starts_with("INFRA-01"))
    {
        assert_ne!(row.status, Status::Pass, "{} passed", row.check_id);
    }
}

#[test]
fn redirect_to_loopback_is_refused() {
    assert_refused_not_fetched("http://127.0.0.1/", vec![]);
}

#[test]
fn redirect_to_metadata_address_is_refused() {
    assert_refused_not_fetched("http://169.254.169.254/latest/meta-data/", vec![]);
}

#[test]
fn redirect_to_ipv6_loopback_is_refused() {
    assert_refused_not_fetched("http://[::1]/", vec![]);
}

#[test]
fn redirect_to_rfc1918_is_refused() {
    assert_refused_not_fetched("http://10.0.0.5/", vec![]);
}

#[test]
fn redirect_to_ipv4_mapped_private_is_refused() {
    assert_refused_not_fetched("http://[::ffff:10.0.0.5]/", vec![]);
}

#[test]
fn same_domain_host_resolving_privately_is_refused() {
    // On the registrable domain, so only address vetting can stop it.
    assert_refused_not_fetched(
        "https://internal.example.gov/admin",
        vec![("internal.example.gov", "192.168.1.10".parse().unwrap())],
    );
}

#[test]
fn redirect_to_other_registrable_domain_is_needs_review_and_not_fetched() {
    let target = "https://elsewhere.example.org/";
    let v = observation(json!({ "https://www.example.gov/": redirect(target) }));
    let (obs, calls) = captured(&v, vec![]);
    assert_eq!(obs.https_home.end, ChainEnd::OffDomain(target.to_string()));
    assert!(calls.iter().all(|c| c != target));
}

#[test]
fn declared_sitemap_on_another_registrable_domain_is_not_fetched() {
    let off = "https://evil.example.org/sitemap.xml";
    let v = observation(json!({
        "https://www.example.gov/robots.txt": {
            "status": 200,
            "headers": { "Content-Type": "text/plain" },
            "body": format!("User-agent: *\nSitemap: {off}\n")
        }
    }));
    let (obs, calls) = captured(&v, vec![]);
    assert_eq!(obs.declared_sitemaps.len(), 1);
    assert_eq!(
        obs.declared_sitemaps[0].end,
        ChainEnd::OffDomain(off.to_string())
    );
    assert!(calls.iter().all(|c| c != off), "{calls:?}");
    let (f, r) = harness(&v, vec![]);
    let origin = Url::parse("https://www.example.gov/").unwrap();
    let report = run(&origin, &f, &r, &Options::default());
    let row = report
        .rows
        .iter()
        .find(|row| row.check_id == "FED-18-sitemap")
        .unwrap();
    assert_ne!(row.status, Status::Pass);
}

#[test]
fn declared_sitemap_resolving_privately_is_not_fetched() {
    let url = "https://intranet.example.gov/sitemap.xml";
    let v = observation(json!({
        "https://www.example.gov/robots.txt": {
            "status": 200,
            "body": format!("Sitemap: {url}\n")
        }
    }));
    let (obs, calls) = captured(
        &v,
        vec![("intranet.example.gov", "10.1.2.3".parse().unwrap())],
    );
    assert!(matches!(
        obs.declared_sitemaps[0].end,
        ChainEnd::BadLocation(_)
    ));
    assert!(calls.iter().all(|c| c != url));
}

#[test]
fn private_start_url_needs_opt_in_and_only_exempts_its_own_host() {
    let meta = "http://169.254.169.254/latest/meta-data/";
    let v = json!({
        "origin": "https://127.0.0.1/",
        "captured_at": "2026-01-01T00:00:00Z",
        "run_id": "ssrf",
        "responses": { "https://127.0.0.1/": redirect(meta) },
        "default": { "status": 404 }
    });
    let origin = Url::parse("https://127.0.0.1/").unwrap();

    // Default: refused before any request.
    let (f, r) = harness(&v, vec![]);
    let obs = capture(&origin, &f, &r, &Options::default()).unwrap();
    assert!(matches!(obs.https_home.end, ChainEnd::BadLocation(_)));
    assert!(f.calls.borrow().is_empty());

    // Opted in: the start host is fetched, the metadata hop is still refused.
    let opts = Options {
        allow_private_origin: true,
        ..Options::default()
    };
    let (f, r) = harness(&v, vec![]);
    let obs = capture(&origin, &f, &r, &opts).unwrap();
    assert!(f.calls.borrow().iter().any(|c| c == "https://127.0.0.1/"));
    assert!(f.calls.borrow().iter().all(|c| c != meta));
    assert!(matches!(
        obs.https_home.end,
        ChainEnd::BadLocation(_) | ChainEnd::OffDomain(_)
    ));
    let report = run(&origin, &f, &r, &opts);
    assert!(report.rows.iter().all(|row| row
        .limitations
        .iter()
        .any(|l| l.contains("private origin allowed by operator"))));
}
