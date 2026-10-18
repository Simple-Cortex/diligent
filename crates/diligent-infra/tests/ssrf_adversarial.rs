//! Adversarial SSRF tests: odd IP forms, protocol-relative redirects, dual-stack
//! resolver responses, allow_private_origin boundary, and schema validation.

use std::cell::RefCell;
use std::net::IpAddr;

use diligent_infra::fake::{FakeFetcher, FakeResolver};
use diligent_infra::observe::{capture, ChainEnd, OriginObservation};
use diligent_infra::{run, FetchError, FetchResponse, Fetcher, Options, ResolveError, Resolver};
use serde_json::{json, Value};
use url::Url;

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn check_result_validator() -> jsonschema::Validator {
    let schema_text =
        std::fs::read_to_string(repo_root().join("schemas/check-result.schema.json")).unwrap();
    let schema: Value = serde_json::from_str(&schema_text).unwrap();
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .should_validate_formats(true)
        .build(&schema)
        .unwrap()
}

fn assert_valid(validator: &jsonschema::Validator, instance: &Value, what: &str) {
    let errors: Vec<String> = validator
        .iter_errors(instance)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(errors.is_empty(), "{what} is invalid: {errors:?}");
}

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

struct Addrs {
    inner: FakeResolver,
    map: Vec<(&'static str, Vec<IpAddr>)>,
}

impl Resolver for Addrs {
    fn txt(&self, name: &str) -> Result<Vec<String>, ResolveError> {
        self.inner.txt(name)
    }
    fn addrs(&self, host: &str) -> Result<Vec<IpAddr>, ResolveError> {
        let key = host.trim_end_matches('.').to_ascii_lowercase();
        match self.map.iter().find(|(h, _)| *h == key) {
            Some((_, ips)) => Ok(ips.clone()),
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
        "run_id": "ssrf-adv",
        "responses": responses,
        "default": { "status": 404 }
    })
}

fn captured_with(
    v: &Value,
    map: Vec<(&'static str, Vec<IpAddr>)>,
) -> (OriginObservation, Vec<String>) {
    let fetcher = Recording {
        inner: FakeFetcher::from_observation(v),
        calls: RefCell::new(Vec::new()),
    };
    let resolver = Addrs {
        inner: FakeResolver::from_observation(v),
        map,
    };
    let origin = Url::parse("https://www.example.gov/").unwrap();
    let obs = capture(&origin, &fetcher, &resolver, &Options::default()).unwrap();
    let calls = fetcher.calls.borrow().clone();
    (obs, calls)
}

fn assert_refused(target: &str, map: Vec<(&'static str, Vec<IpAddr>)>) {
    let v = observation(json!({ "https://www.example.gov/": redirect(target) }));
    let (obs, calls) = captured_with(&v, map);
    assert!(
        matches!(
            obs.https_home.end,
            ChainEnd::BadLocation(_) | ChainEnd::OffDomain(_)
        ),
        "{target}: expected BadLocation or OffDomain, got {:?}",
        obs.https_home.end
    );
    assert!(
        calls.iter().all(|c| c != target),
        "{target} was fetched: {calls:?}"
    );
}

// ---- Redirects using non-standard IP forms ----

#[test]
fn redirect_to_short_loopback_127_1() {
    // 127.1 is a short form of 127.0.0.1. url::Url normalises it to 127.0.0.1.
    // The redirect target is an IP host, which is never on the same registrable
    // domain as example.gov, so the chain ends as OffDomain. The target is not
    // fetched. Security property holds via the off-domain check.
    let target = "http://127.1/";
    let v = observation(json!({ "https://www.example.gov/": redirect(target) }));
    let (obs, calls) = captured_with(&v, vec![]);
    assert!(
        matches!(obs.https_home.end, ChainEnd::OffDomain(_)),
        "127.1 normalised to 127.0.0.1 by url::Url; IP host is off-domain: {:?}",
        obs.https_home.end
    );
    assert!(
        calls.iter().all(|c| !c.contains("127.")),
        "loopback was fetched: {calls:?}"
    );
}

#[test]
fn redirect_to_decimal_loopback() {
    // 2130706433 is decimal for 127.0.0.1. url::Url normalises it.
    let target = "http://2130706433/";
    let v = observation(json!({ "https://www.example.gov/": redirect(target) }));
    let (obs, calls) = captured_with(&v, vec![]);
    assert!(
        matches!(obs.https_home.end, ChainEnd::OffDomain(_)),
        "2130706433 normalised to 127.0.0.1 by url::Url; IP host is off-domain: {:?}",
        obs.https_home.end
    );
    assert!(
        calls.iter().all(|c| !c.contains("127.")),
        "loopback was fetched: {calls:?}"
    );
}

#[test]
fn redirect_to_hex_loopback() {
    // 0x7f000001 is hex for 127.0.0.1. url::Url normalises it.
    let target = "http://0x7f000001/";
    let v = observation(json!({ "https://www.example.gov/": redirect(target) }));
    let (obs, calls) = captured_with(&v, vec![]);
    assert!(
        matches!(obs.https_home.end, ChainEnd::OffDomain(_)),
        "0x7f000001 normalised to 127.0.0.1 by url::Url; IP host is off-domain: {:?}",
        obs.https_home.end
    );
    assert!(
        calls.iter().all(|c| !c.contains("127.")),
        "loopback was fetched: {calls:?}"
    );
}

#[test]
fn redirect_to_ipv4_mapped_loopback() {
    assert_refused("http://[::ffff:127.0.0.1]/", vec![]);
}

#[test]
fn redirect_to_localhost() {
    assert_refused("http://localhost/", vec![]);
}

// ---- Off-domain tricks ----

#[test]
fn redirect_to_subdomain_of_evil() {
    // example.gov.evil.com is NOT on example.gov's registrable domain.
    assert_refused("https://example.gov.evil.com/", vec![]);
}

#[test]
fn declared_sitemap_on_evil_domain() {
    let off = "https://evil.com/sitemap.xml";
    let v = observation(json!({
        "https://www.example.gov/robots.txt": {
            "status": 200,
            "headers": { "Content-Type": "text/plain" },
            "body": format!("User-agent: *\nSitemap: {off}\n")
        }
    }));
    let (obs, calls) = captured_with(&v, vec![]);
    assert_eq!(obs.declared_sitemaps.len(), 1);
    assert_eq!(
        obs.declared_sitemaps[0].end,
        ChainEnd::OffDomain(off.to_string())
    );
    assert!(calls.iter().all(|c| c != off), "{calls:?}");
}

#[test]
fn protocol_relative_redirect() {
    // A 302 with Location: //evil.com/x — url::Url resolves this relative
    // to the current scheme, yielding https://evil.com/x.
    let v = observation(json!({ "https://www.example.gov/": redirect("//evil.com/x") }));
    let (obs, calls) = captured_with(&v, vec![]);
    assert!(
        matches!(obs.https_home.end, ChainEnd::OffDomain(_)),
        "protocol-relative redirect to evil.com should be OffDomain, got {:?}",
        obs.https_home.end
    );
    assert!(calls.iter().all(|c| !c.contains("evil.com")));
}

// ---- Dual-stack resolver ----

#[test]
fn host_resolving_to_public_and_private_is_refused() {
    // A host whose resolver returns one public and one private address must
    // be refused (any forbidden address taints the host).
    let target = "https://dual.example.gov/";
    let _v = observation(json!({ "https://www.example.gov/": redirect(target) }));
    let map = vec![(
        "dual.example.gov",
        vec![
            "93.184.216.34".parse().unwrap(), // public
            "10.0.0.5".parse().unwrap(),      // private
        ],
    )];
    assert_refused(target, map);
}

// ---- allow_private_origin boundary ----

#[test]
fn allow_private_origin_start_redirect_to_different_loopback_refused() {
    // Start on 127.0.0.1 with allow_private_origin: the start host is fetched,
    // but a redirect to 127.0.0.2 (different loopback address) must be refused.
    let meta = "http://127.0.0.2/";
    let v = json!({
        "origin": "https://127.0.0.1/",
        "captured_at": "2026-01-01T00:00:00Z",
        "run_id": "ssrf-adv-private",
        "responses": { "https://127.0.0.1/": redirect(meta) },
        "default": { "status": 404 }
    });
    let origin = Url::parse("https://127.0.0.1/").unwrap();
    let opts = Options {
        allow_private_origin: true,
        ..Options::default()
    };
    let fetcher = Recording {
        inner: FakeFetcher::from_observation(&v),
        calls: RefCell::new(Vec::new()),
    };
    let resolver = Addrs {
        inner: FakeResolver::from_observation(&v),
        map: vec![],
    };
    let obs = capture(&origin, &fetcher, &resolver, &opts).unwrap();
    // The start host (127.0.0.1) is exempt, but 127.0.0.2 is a different host.
    assert!(fetcher
        .calls
        .borrow()
        .iter()
        .any(|c| c == "https://127.0.0.1/"));
    assert!(
        fetcher.calls.borrow().iter().all(|c| c != meta),
        "127.0.0.2 must not be fetched: {:?}",
        fetcher.calls.borrow()
    );
    assert!(
        matches!(
            obs.https_home.end,
            ChainEnd::BadLocation(_) | ChainEnd::OffDomain(_)
        ),
        "redirect to 127.0.0.2 should be refused, got {:?}",
        obs.https_home.end
    );
}

// ---- Schema validation ----

#[test]
fn every_row_validates_against_check_result_schema() {
    let v = observation(json!({ "https://www.example.gov/": redirect("http://127.0.0.1/") }));
    let fetcher = FakeFetcher::from_observation(&v);
    let resolver = FakeResolver::from_observation(&v);
    let origin = Url::parse("https://www.example.gov/").unwrap();
    let report = run(&origin, &fetcher, &resolver, &Options::default());
    let validator = check_result_validator();
    for row in &report.rows {
        let instance = serde_json::to_value(row).unwrap();
        assert_valid(&validator, &instance, &format!("row {}", row.check_id));
    }
}

#[test]
fn every_row_validates_for_off_domain_redirect() {
    let v = observation(json!({ "https://www.example.gov/": redirect("https://evil.com/") }));
    let fetcher = FakeFetcher::from_observation(&v);
    let resolver = FakeResolver::from_observation(&v);
    let origin = Url::parse("https://www.example.gov/").unwrap();
    let report = run(&origin, &fetcher, &resolver, &Options::default());
    let validator = check_result_validator();
    for row in &report.rows {
        let instance = serde_json::to_value(row).unwrap();
        assert_valid(&validator, &instance, &format!("row {}", row.check_id));
    }
}

#[test]
fn every_row_validates_for_dual_stack_refusal() {
    let v = observation(json!({
        "https://www.example.gov/": redirect("https://dual.example.gov/")
    }));
    let fetcher = FakeFetcher::from_observation(&v);
    let map = vec![(
        "dual.example.gov",
        vec![
            "93.184.216.34".parse().unwrap(),
            "10.0.0.5".parse().unwrap(),
        ],
    )];
    let resolver = Addrs {
        inner: FakeResolver::from_observation(&v),
        map,
    };
    let origin = Url::parse("https://www.example.gov/").unwrap();
    let report = run(&origin, &fetcher, &resolver, &Options::default());
    let validator = check_result_validator();
    for row in &report.rows {
        let instance = serde_json::to_value(row).unwrap();
        assert_valid(&validator, &instance, &format!("row {}", row.check_id));
    }
}

#[test]
fn every_row_validates_for_allow_private_origin() {
    let v = json!({
        "origin": "https://127.0.0.1/",
        "captured_at": "2026-01-01T00:00:00Z",
        "run_id": "ssrf-adv-schema",
        "responses": {
            "https://127.0.0.1/": { "status": 200, "body": "<html></html>" }
        },
        "default": { "status": 404 }
    });
    let origin = Url::parse("https://127.0.0.1/").unwrap();
    let opts = Options {
        allow_private_origin: true,
        ..Options::default()
    };
    let fetcher = FakeFetcher::from_observation(&v);
    let resolver = FakeResolver::from_observation(&v);
    let report = run(&origin, &fetcher, &resolver, &opts);
    let validator = check_result_validator();
    for row in &report.rows {
        let instance = serde_json::to_value(row).unwrap();
        assert_valid(&validator, &instance, &format!("row {}", row.check_id));
    }
}
