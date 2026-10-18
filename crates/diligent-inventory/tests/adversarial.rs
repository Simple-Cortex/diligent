//! Adversarial tests for the inventory crate.

use diligent_inventory::fixture::run_case;
use diligent_inventory::links::{analyze, resolve_links};
use diligent_inventory::prng::{sample_without_replacement, XorShift64Star};
use diligent_inventory::sitemap::{collect, parse, Limits, SitemapFetch};
use diligent_inventory::urls::normalize;
use flate2::write::GzEncoder;
use flate2::Compression;
use serde_json::{json, Value};
use std::io::Write;

use url::Url;

fn gz(bytes: &[u8]) -> Vec<u8> {
    let mut enc = GzEncoder::new(Vec::new(), Compression::default());
    enc.write_all(bytes).unwrap();
    enc.finish().unwrap()
}

fn run_with_site(site: &Value, options: &Value) -> diligent_inventory::fixture::CaseRun {
    let tmp = std::env::temp_dir().join(format!(
        "diligent-inv-adv-{}-{}",
        std::process::id(),
        rand_counter()
    ));
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    std::fs::write(tmp.join("site.json"), serde_json::to_string(site).unwrap()).unwrap();
    std::fs::write(
        tmp.join("options.json"),
        serde_json::to_string(options).unwrap(),
    )
    .unwrap();
    let result = run_case(&tmp, "", "options.json").unwrap();
    let _ = std::fs::remove_dir_all(&tmp);
    result
}

static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
fn rand_counter() -> u64 {
    COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
}

// ---- sitemap <loc> pointing at private IPs are never fetched ----

#[test]
fn sitemap_loc_pointing_at_private_ips_is_never_fetched() {
    let private_urls = vec![
        "http://127.0.0.1/secret",
        "http://10.0.0.5/admin",
        "http://[::1]/internal",
        "http://169.254.169.254/latest/meta-data",
    ];
    let mut sitemap_xml = String::from(
        r#"<?xml version="1.0"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#,
    );
    for u in &private_urls {
        sitemap_xml.push_str(&format!("<url><loc>{u}</loc></url>"));
    }
    sitemap_xml.push_str("</urlset>");
    let site = json!({
        "https://example.gov/": { "status": 200, "headers": {"content-type": "text/html"}, "body": "<html><body>home</body></html>" },
        "https://example.gov/robots.txt": { "status": 200, "body": "User-agent: *\nAllow: /\n" },
        "https://example.gov/sitemap.xml": { "status": 200, "headers": {"content-type": "application/xml"}, "body": sitemap_xml },
    });
    let options = json!({
        "start": "https://example.gov/",
        "run_id": "run-2026-10-18-0a1b2c",
    });
    let result = run_with_site(&site, &options);
    for private_url in &private_urls {
        assert!(
            result.requests.iter().all(|r| r != private_url),
            "private URL {private_url} should not be fetched, but was. Requests: {:?}",
            result.requests
        );
    }
}

// ---- robots Sitemap: to a private host is never fetched ----

#[test]
fn robots_sitemap_directive_to_private_host_is_never_fetched() {
    let site = json!({
        "https://example.gov/": { "status": 200, "headers": {"content-type": "text/html"}, "body": "<html><body>home</body></html>" },
        "https://example.gov/robots.txt": { "status": 200, "body": "User-agent: *\nAllow: /\nSitemap: http://10.0.0.5/sitemap.xml\n" },
        "https://example.gov/sitemap.xml": { "status": 404 },
    });
    let options = json!({
        "start": "https://example.gov/",
        "run_id": "run-2026-10-18-0a1b2c",
    });
    let result = run_with_site(&site, &options);
    assert!(
        result.requests.iter().all(|r| !r.contains("10.0.0.5")),
        "private sitemap URL should not be fetched. Requests: {:?}",
        result.requests
    );
}

// ---- javascript: and data: hrefs ignored ----

#[test]
fn javascript_and_data_hrefs_are_ignored() {
    let html = r#"<html><body>
        <a href="javascript:alert(1)">js</a>
        <a href="data:text/html,hi">data</a>
        <a href="https://example.gov/real">real</a>
    </body></html>"#;
    let facts = analyze(html);
    let base = Url::parse("https://example.gov/").unwrap();
    let links = resolve_links(&facts, &base, &[]);
    let urls: Vec<String> = links.iter().map(|u| u.to_string()).collect();
    assert_eq!(urls, vec!["https://example.gov/real"]);
}

// ---- IDN host link treated as off-origin ----

#[test]
fn idn_host_link_is_treated_as_off_origin() {
    let html = r#"<html><body>
        <a href="https://münchen.example/page">idn</a>
        <a href="https://example.gov/real">real</a>
    </body></html>"#;
    let facts = analyze(html);
    let base = Url::parse("https://example.gov/").unwrap();
    let links = resolve_links(&facts, &base, &[]);
    let urls: Vec<String> = links.iter().map(|u| u.to_string()).collect();
    assert!(urls.contains(&"https://example.gov/real".to_string()));
    assert!(
        urls.iter().all(|u| !u.contains("münchen")),
        "IDN host link should be resolved but is off-origin"
    );
}

// ---- page with 100,000 links finishes under the discovered cap ----

#[test]
fn page_with_many_links_finishes_under_discovered_cap() {
    let mut html = String::from("<html><body>");
    for i in 0..100_000 {
        html.push_str(&format!("<a href=\"/p/{i}\">link {i}</a>"));
    }
    html.push_str("</body></html>");
    let facts = analyze(&html);
    assert!(facts.hrefs_capped || facts.hrefs.len() <= 10_000);
}

// ---- URLs differing only by fragment or by path case dedupe per the documented rule ----

#[test]
fn urls_differing_only_by_fragment_dedupe() {
    let keys: Vec<String> = vec![];
    let a = normalize(
        &Url::parse("https://e.gov/page#section1").unwrap(),
        &keys,
        false,
    );
    let b = normalize(
        &Url::parse("https://e.gov/page#section2").unwrap(),
        &keys,
        false,
    );
    assert_eq!(a.as_str(), b.as_str());
}

#[test]
fn urls_differing_by_path_case_do_not_dedupe() {
    let keys: Vec<String> = vec![];
    let a = normalize(&Url::parse("https://e.gov/Page").unwrap(), &keys, false);
    let b = normalize(&Url::parse("https://e.gov/page").unwrap(), &keys, false);
    assert_ne!(a.as_str(), b.as_str(), "path case is significant");
}

// ---- same seed → identical random set; different seed → different set ----

#[test]
fn same_seed_produces_identical_random_set() {
    let items: Vec<i32> = (0..100).collect();
    let mut rng1 = XorShift64Star::from_seed("test-seed");
    let mut rng2 = XorShift64Star::from_seed("test-seed");
    let a = sample_without_replacement(&items, 10, &mut rng1);
    let b = sample_without_replacement(&items, 10, &mut rng2);
    assert_eq!(a, b);
}

#[test]
fn different_seed_produces_different_set_when_enough_candidates() {
    let items: Vec<i32> = (0..100).collect();
    let mut rng1 = XorShift64Star::from_seed("seed-alpha");
    let mut rng2 = XorShift64Star::from_seed("seed-beta");
    let a = sample_without_replacement(&items, 10, &mut rng1);
    let b = sample_without_replacement(&items, 10, &mut rng2);
    assert_ne!(
        a, b,
        "different seeds should produce different samples with 100 candidates"
    );
}

// ---- gzip sitemap larger than max_body_bytes is truncated and flagged partial ----

#[test]
fn gzip_sitemap_larger_than_max_body_bytes_is_truncated() {
    let mut large_xml = String::from(
        r#"<?xml version="1.0"?><urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#,
    );
    for i in 0..5000 {
        large_xml.push_str(&format!("<url><loc>https://e.gov/p/{i}</loc></url>"));
    }
    large_xml.push_str("</urlset>");
    let body = gz(large_xml.as_bytes());
    let max = 1024;
    let parsed = parse(&body, max, false).unwrap();
    assert!(parsed.truncated, "sitemap should be flagged as truncated");
}

// ---- 1,000-deep sitemap index stops at depth 3 ----

#[test]
fn deep_sitemap_index_stops_at_depth_3() {
    let mut fetched = 0usize;
    let mut fetch = |_url: &Url| -> SitemapFetch {
        fetched += 1;
        if fetched == 1 {
            let xml = r#"<sitemapindex><sitemap><loc>https://e.gov/sm-1.xml</loc></sitemap></sitemapindex>"#;
            SitemapFetch::Body {
                body: xml.as_bytes().to_vec(),
                truncated: false,
            }
        } else {
            let xml = format!(
                r#"<sitemapindex><sitemap><loc>https://e.gov/sm-{}.xml</loc></sitemap></sitemapindex>"#,
                fetched
            );
            SitemapFetch::Body {
                body: xml.as_bytes().to_vec(),
                truncated: false,
            }
        }
    };
    let roots = vec![(Url::parse("https://e.gov/sitemap.xml").unwrap(), "sitemap")];
    let limits = Limits {
        max_depth: 3,
        max_files: 50,
        max_body_bytes: 1 << 20,
        max_entries: 5000,
    };
    let collected = collect(&roots, &limits, &mut fetch);
    assert!(
        fetched <= 5,
        "should stop at depth 3, fetched {fetched} files"
    );
    assert!(
        collected.notes.iter().any(|n| n.contains("depth_limit"))
            || collected
                .remaining
                .iter()
                .any(|r| r.contains("nesting limit")),
        "expected a depth-limit note or remaining entry, got notes={:?} remaining={:?}",
        collected.notes,
        collected.remaining
    );
}

// ---- robots 5xx disallows all ----

#[test]
fn robots_5xx_disallows_all() {
    let site = json!({
        "https://example.gov/": { "status": 200, "headers": {"content-type": "text/html"}, "body": "<html><body><a href=\"/page\">page</a></body></html>" },
        "https://example.gov/robots.txt": { "status": 500 },
        "https://example.gov/sitemap.xml": { "status": 404 },
        "https://example.gov/page": { "status": 200, "headers": {"content-type": "text/html"}, "body": "<html><body>page</body></html>" },
    });
    let options = json!({
        "start": "https://example.gov/",
        "run_id": "run-2026-10-18-0a1b2c",
    });
    let result = run_with_site(&site, &options);
    let page_route = result
        .outcome
        .inventory
        .routes
        .iter()
        .find(|r| r.url.ends_with("/page"));
    if let Some(route) = page_route {
        assert!(
            route.excluded_by_robots || !route.fetched,
            "page should be excluded or not fetched when robots returns 5xx"
        );
    }
}

// ---- https start redirecting to http://www is not trusted ----

#[test]
fn https_start_redirecting_to_http_www_is_not_trusted() {
    let site = json!({
        "https://example.gov/": { "status": 302, "location": "http://www.example.gov/" },
        "http://www.example.gov/": { "status": 200, "headers": {"content-type": "text/html"}, "body": "<html><body>www</body></html>" },
    });
    let options = json!({
        "start": "https://example.gov/",
        "run_id": "run-2026-10-18-0a1b2c",
    });
    let result = run_with_site(&site, &options);
    assert!(
        result.outcome.start_failure.is_some(),
        "https->http downgrade should cause start failure"
    );
}
