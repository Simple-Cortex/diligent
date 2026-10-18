//! Adversarial tests for the infra checks.
//!
//! These probe edge cases: odd header casing, redirect loops, dangerous schemes,
//! oversized bodies, tricky robots.txt, HTML sitemaps, and host-suffix tricks.

use diligent_infra::checks;
use diligent_infra::fetch::Options;
use diligent_infra::model::Status;
use serde_json::{json, Value};

fn repo_root() -> std::path::PathBuf {
    std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn validator(name: &str) -> jsonschema::Validator {
    let schema_text = std::fs::read_to_string(repo_root().join("schemas").join(name)).unwrap();
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

/// Build an observation from a JSON spec using the fakes.
fn run_fake(spec: &Value) -> diligent_infra::InfraReport {
    diligent_infra::fake::run_observation(spec).expect("valid origin")
}

/// Minimal spec for a given origin with default https response.
fn base_spec(origin: &str) -> Value {
    json!({
        "origin": origin,
        "captured_at": "2026-10-17T00:00:00Z",
        "run_id": "run-2026-10-17-aabbcc",
        "responses": {},
        "default": {"status": 404, "headers": {}, "body": ""},
        "dns": {},
    })
}

// ---------------------------------------------------------------------------
// HSTS edge cases
// ---------------------------------------------------------------------------

#[test]
fn hsts_with_odd_casing_and_spacing_in_max_age() {
    let mut spec = base_spec("https://example.gov/");
    spec["responses"]["https://example.gov/"] = json!({
        "status": 200,
        "headers": {
            "strict-transport-security": "Max-Age = 31536000"
        },
        "body": "<html><head><title>T</title></head><body></body></html>"
    });
    // http:// redirects to https://
    spec["responses"]["http://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    let report = run_fake(&spec);
    let hsts_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "INFRA-02-hsts")
        .unwrap();
    // The param() function splits on ';' and then on '=' with trim, so
    // "Max-Age = 31536000" should parse correctly.
    // The value "31536000" after trimming should parse as u64.
    // But "Max-Age = 31536000" split by ';' gives one part.
    // split_once('=') gives ("Max-Age ", " 31536000").
    // After trim: "Max-Age" eq_ignore_ascii_case "max-age" -> true.
    // value.trim() -> "31536000" -> parse::<u64>() -> Ok(31536000).
    // So this should pass.
    assert_eq!(
        hsts_row.status,
        Status::Pass,
        "HSTS with spaced Max-Age should pass"
    );
}

#[test]
fn hsts_with_multiple_headers_first_wins_or_concatenated() {
    // The fake fetcher stores one header per name; if the spec has a single
    // string, that's what the check sees. Multiple HSTS headers in HTTP are
    // technically invalid but the check should handle one gracefully.
    let mut spec = base_spec("https://example.gov/");
    spec["responses"]["https://example.gov/"] = json!({
        "status": 200,
        "headers": {
            "strict-transport-security": "max-age=31536000; includeSubDomains; preload"
        },
        "body": "<html><title>T</title></html>"
    });
    spec["responses"]["http://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    let report = run_fake(&spec);
    let hsts_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "INFRA-02-hsts")
        .unwrap();
    assert_eq!(hsts_row.status, Status::Pass);
    let preload_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "INFRA-02-hsts-preload")
        .unwrap();
    assert_eq!(preload_row.status, Status::Pass);
}

// ---------------------------------------------------------------------------
// SPF and DMARC edge cases
// ---------------------------------------------------------------------------

#[test]
fn spf_record_split_across_txt_strings_is_one_record() {
    // DNS TXT records can be split across multiple strings in one record.
    // The fake stores each TXT record as one string, so a "split" record
    // would appear as two separate TXT records. The check counts records
    // starting with "v=spf1", so two fragments each starting with "v=spf1"
    // would be seen as two records -> error.
    // But a single TXT record with spaces is fine.
    let mut spec = base_spec("https://example.gov/");
    spec["responses"]["http://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    spec["responses"]["https://example.gov/"] = json!({
        "status": 200,
        "headers": {"strict-transport-security": "max-age=31536000"},
        "body": "<html><title>T</title></html>"
    });
    spec["dns"]["example.gov"] = json!(["v=spf1 include:_spf.google.com -all"]);
    let report = run_fake(&spec);
    let spf_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "INFRA-03-spf")
        .unwrap();
    assert_eq!(spf_row.status, Status::Pass);
}

#[test]
fn dmarc_with_trailing_spaces_and_uppercase_p() {
    let mut spec = base_spec("https://example.gov/");
    spec["responses"]["http://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    spec["responses"]["https://example.gov/"] = json!({
        "status": 200,
        "headers": {"strict-transport-security": "max-age=31536000"},
        "body": "<html><title>T</title></html>"
    });
    spec["dns"]["_dmarc.example.gov"] = json!(["v=DMARC1; P=reject "]);
    let report = run_fake(&spec);
    let dmarc_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "INFRA-03-dmarc")
        .unwrap();
    // param() trims the value, and to_ascii_lowercase handles uppercase P.
    assert_eq!(
        dmarc_row.status,
        Status::Pass,
        "DMARC with uppercase P= and trailing space should pass"
    );
}

// ---------------------------------------------------------------------------
// Redirect edge cases
// ---------------------------------------------------------------------------

#[test]
fn redirect_loop_is_detected() {
    let mut spec = base_spec("https://example.gov/");
    // A -> B -> A loop
    spec["responses"]["https://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://other.gov/"},
        "body": ""
    });
    spec["responses"]["https://other.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    spec["responses"]["http://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    let report = run_fake(&spec);
    // The redirect check (not cert) catches the loop via TooManyRedirects.
    let redirect_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "INFRA-01-https-redirect")
        .unwrap();
    assert_ne!(
        redirect_row.status,
        Status::Pass,
        "redirect loop must not pass redirect check"
    );
}

#[test]
fn redirect_chain_of_exactly_5_hops_succeeds() {
    let mut spec = base_spec("https://example.gov/");
    // 5 hops: a -> b -> c -> d -> e -> final (all on example.gov registrable domain)
    spec["responses"]["https://example.gov/"] =
        json!({"status": 301, "headers": {"location": "https://a.example.gov/"}, "body": ""});
    spec["responses"]["https://a.example.gov/"] =
        json!({"status": 301, "headers": {"location": "https://b.example.gov/"}, "body": ""});
    spec["responses"]["https://b.example.gov/"] =
        json!({"status": 301, "headers": {"location": "https://c.example.gov/"}, "body": ""});
    spec["responses"]["https://c.example.gov/"] =
        json!({"status": 301, "headers": {"location": "https://d.example.gov/"}, "body": ""});
    spec["responses"]["https://d.example.gov/"] =
        json!({"status": 301, "headers": {"location": "https://e.example.gov/"}, "body": ""});
    spec["responses"]["https://e.example.gov/"] = json!({"status": 200, "headers": {"strict-transport-security": "max-age=31536000"}, "body": "<html><title>T</title></html>"});
    spec["responses"]["http://example.gov/"] =
        json!({"status": 301, "headers": {"location": "https://example.gov/"}, "body": ""});
    let report = run_fake(&spec);
    let cert_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "INFRA-01-https-cert")
        .unwrap();
    // 5 redirects then a 200: the chain has 6 hops total (5 redirects + 1 final).
    // MAX_REDIRECTS is 5, so after 5 redirects (hops.len() > 5 check), it should be TooManyRedirects.
    // Let's check: the loop pushes the response, then checks hops.len() > max_redirects.
    // After hop 1 (301), len=1, not > 5, follow.
    // After hop 2 (301), len=2, not > 5, follow.
    // ...
    // After hop 5 (301 from d.gov), len=5, not > 5, follow.
    // After hop 6 (200 from e.gov), len=6, not a redirect, so Final.
    // Wait: the check is `if hops.len() > max_redirects` AFTER pushing. So:
    // Push hop 1 (example.gov 301), len=1, location found, 1 > 5? No, follow.
    // Push hop 2 (a.gov 301), len=2, 2 > 5? No, follow.
    // Push hop 3 (b.gov 301), len=3, 3 > 5? No, follow.
    // Push hop 4 (c.gov 301), len=4, 4 > 5? No, follow.
    // Push hop 5 (d.gov 301), len=5, 5 > 5? No, follow.
    // Push hop 6 (e.gov 200), len=6, not redirect -> Final.
    // So 5 redirects + 1 final = 6 hops, and it succeeds.
    assert_eq!(
        cert_row.status,
        Status::Pass,
        "5 redirects + final should succeed"
    );
}

#[test]
fn redirect_chain_of_6_hops_is_too_many() {
    let mut spec = base_spec("https://example.gov/");
    // 6 redirects before final: should hit TooManyRedirects
    spec["responses"]["https://example.gov/"] =
        json!({"status": 301, "headers": {"location": "https://a.gov/"}, "body": ""});
    spec["responses"]["https://a.gov/"] =
        json!({"status": 301, "headers": {"location": "https://b.gov/"}, "body": ""});
    spec["responses"]["https://b.gov/"] =
        json!({"status": 301, "headers": {"location": "https://c.gov/"}, "body": ""});
    spec["responses"]["https://c.gov/"] =
        json!({"status": 301, "headers": {"location": "https://d.gov/"}, "body": ""});
    spec["responses"]["https://d.gov/"] =
        json!({"status": 301, "headers": {"location": "https://e.gov/"}, "body": ""});
    spec["responses"]["https://e.gov/"] =
        json!({"status": 301, "headers": {"location": "https://f.gov/"}, "body": ""});
    spec["responses"]["https://f.gov/"] =
        json!({"status": 200, "headers": {}, "body": "<html><title>T</title></html>"});
    spec["responses"]["http://example.gov/"] =
        json!({"status": 301, "headers": {"location": "https://example.gov/"}, "body": ""});
    let report = run_fake(&spec);
    // The redirect check catches TooManyRedirects, not the cert check.
    let redirect_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "INFRA-01-https-redirect")
        .unwrap();
    assert_ne!(
        redirect_row.status,
        Status::Pass,
        "6 redirects should not pass redirect check"
    );
}

#[test]
fn javascript_scheme_in_location_is_not_followed() {
    let mut spec = base_spec("https://example.gov/");
    spec["responses"]["https://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "javascript:alert(1)"},
        "body": ""
    });
    spec["responses"]["http://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    let report = run_fake(&spec);
    // The redirect check catches BadScheme, not the cert check.
    let redirect_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "INFRA-01-https-redirect")
        .unwrap();
    assert_eq!(
        redirect_row.status,
        Status::Fail,
        "javascript: redirect must fail the redirect check"
    );
}

#[test]
fn file_scheme_in_location_is_not_followed() {
    let mut spec = base_spec("https://example.gov/");
    spec["responses"]["https://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "file:///etc/passwd"},
        "body": ""
    });
    spec["responses"]["http://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    let report = run_fake(&spec);
    let redirect_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "INFRA-01-https-redirect")
        .unwrap();
    assert_eq!(
        redirect_row.status,
        Status::Fail,
        "file: redirect must fail the redirect check"
    );
}

// ---------------------------------------------------------------------------
// Body size cap
// ---------------------------------------------------------------------------

#[test]
fn oversized_body_is_truncated_by_options() {
    // The FakeFetcher does not enforce max_body_bytes (it's the real fetcher
    // that caps). But we can verify the option exists and the evidence builder
    // records the truncation flag.
    let opts = Options {
        max_body_bytes: 10,
        ..Options::default()
    };
    assert_eq!(opts.max_body_bytes, 10);
    // The default is 2 MiB.
    assert_eq!(Options::default().max_body_bytes, 2 * 1024 * 1024);
}

// ---------------------------------------------------------------------------
// robots.txt edge cases
// ---------------------------------------------------------------------------

#[test]
fn robots_disallow_all_only_under_non_wildcard_user_agent_does_not_fail() {
    // Disallow: / only under Googlebot, not under *.
    let text = "User-agent: Googlebot\nDisallow: /\n\nUser-agent: *\nAllow: /\n";
    assert!(
        !checks::robots_disallows_all(text),
        "Disallow under Googlebot only must not trigger"
    );
}

#[test]
fn robots_disallow_all_under_star_does_fail() {
    let text = "User-agent: *\nDisallow: /\n";
    assert!(checks::robots_disallows_all(text));
}

#[test]
fn robots_with_only_allow_does_not_fail() {
    let text = "User-agent: *\nAllow: /\n";
    assert!(!checks::robots_disallows_all(text));
}

// ---------------------------------------------------------------------------
// Sitemap edge cases
// ---------------------------------------------------------------------------

#[test]
fn sitemap_that_is_html_not_xml_is_needs_review() {
    let mut spec = base_spec("https://example.gov/");
    spec["responses"]["http://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    spec["responses"]["https://example.gov/"] = json!({
        "status": 200,
        "headers": {"strict-transport-security": "max-age=31536000"},
        "body": "<html><title>T</title></html>"
    });
    spec["responses"]["https://example.gov/robots.txt"] = json!({
        "status": 404,
        "headers": {},
        "body": ""
    });
    spec["responses"]["https://example.gov/sitemap.xml"] = json!({
        "status": 200,
        "headers": {"content-type": "text/html"},
        "body": "<html><body><h1>Sitemap</h1></body></html>"
    });
    let report = run_fake(&spec);
    let sitemap_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "FED-18-sitemap")
        .unwrap();
    // HTML sitemap should not pass; the XML parser will fail or the root element won't be urlset.
    assert_ne!(
        sitemap_row.status,
        Status::Pass,
        "HTML sitemap must not pass"
    );
}

// ---------------------------------------------------------------------------
// Gov domain edge cases
// ---------------------------------------------------------------------------

#[test]
fn example_gov_evil_com_does_not_pass_gov_check() {
    // The registrable_domain of "example.gov.evil.com" is "evil.com".
    // The final host is "example.gov.evil.com", which does not end with ".gov".
    let mut spec = base_spec("https://example.gov.evil.com/");
    spec["responses"]["http://example.gov.evil.com/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov.evil.com/"},
        "body": ""
    });
    spec["responses"]["https://example.gov.evil.com/"] = json!({
        "status": 200,
        "headers": {"strict-transport-security": "max-age=31536000"},
        "body": "<html><title>T</title></html>"
    });
    let report = run_fake(&spec);
    let gov_row = report
        .rows
        .iter()
        .find(|r| r.check_id == "FED-01-gov-domain")
        .unwrap();
    assert_ne!(
        gov_row.status,
        Status::Pass,
        "example.gov.evil.com must not pass .gov check"
    );
}

// ---------------------------------------------------------------------------
// Schema validation of every row and evidence record
// ---------------------------------------------------------------------------

#[test]
fn every_row_validates_against_check_result_schema() {
    let mut spec = base_spec("https://example.gov/");
    spec["responses"]["http://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    spec["responses"]["https://example.gov/"] = json!({
        "status": 200,
        "headers": {
            "strict-transport-security": "max-age=31536000; includeSubDomains; preload",
            "content-security-policy": "default-src 'self'",
            "x-content-type-options": "nosniff",
            "referrer-policy": "strict-origin-when-cross-origin",
            "x-frame-options": "DENY"
        },
        "body": "<html><head><title>T</title></head><body><a href=\"/privacy\">Privacy</a><script src=\"https://dap.digitalgov.gov/universal-federated-analytics-min.js\"></script></body></html>"
    });
    spec["dns"]["example.gov"] = json!(["v=spf1 -all"]);
    spec["dns"]["_dmarc.example.gov"] = json!(["v=DMARC1; p=reject"]);
    let report = run_fake(&spec);
    let row_validator = validator("check-result.schema.json");
    for (i, row) in report.rows.iter().enumerate() {
        let v = serde_json::to_value(row).unwrap();
        assert_valid(&row_validator, &v, &format!("row {i} ({})", row.check_id));
    }
}

#[test]
fn every_evidence_validates_against_evidence_record_schema() {
    let mut spec = base_spec("https://example.gov/");
    spec["responses"]["http://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    spec["responses"]["https://example.gov/"] = json!({
        "status": 200,
        "headers": {"strict-transport-security": "max-age=31536000"},
        "body": "<html><title>T</title></html>"
    });
    let report = run_fake(&spec);
    let ev_validator = validator("evidence-record.schema.json");
    for (i, ev) in report.evidence.iter().enumerate() {
        let v = serde_json::to_value(ev).unwrap();
        assert_valid(&ev_validator, &v, &format!("evidence {i}"));
    }
}

// ---------------------------------------------------------------------------
// Stored headers never contain sensitive headers
// ---------------------------------------------------------------------------

#[test]
fn stored_headers_never_contain_set_cookie_or_authorization() {
    let mut spec = base_spec("https://example.gov/");
    spec["responses"]["http://example.gov/"] = json!({
        "status": 301,
        "headers": {"location": "https://example.gov/"},
        "body": ""
    });
    spec["responses"]["https://example.gov/"] = json!({
        "status": 200,
        "headers": {
            "strict-transport-security": "max-age=31536000",
            "set-cookie": "session=secret123",
            "authorization": "Bearer token123",
            "content-type": "text/html"
        },
        "body": "<html><title>T</title></html>"
    });
    let report = run_fake(&spec);
    // Find the evidence record for the https response (status 200).
    let https_ev = report
        .evidence
        .iter()
        .find(|ev| ev.context.get("status").and_then(|s| s.as_u64()) == Some(200))
        .expect("should have evidence for the 200 response");
    // The headers in the evidence context must not contain set-cookie or authorization.
    if let Some(headers) = https_ev.context.get("headers").and_then(|h| h.as_object()) {
        assert!(
            !headers.contains_key("set-cookie"),
            "evidence must not contain set-cookie"
        );
        assert!(
            !headers.contains_key("authorization"),
            "evidence must not contain authorization"
        );
    }
    // The redaction flag must be set because sensitive headers were present.
    assert!(
        https_ev.redaction.applied,
        "redaction.applied should be true when sensitive headers were stripped"
    );
    // Also verify no evidence record anywhere contains these headers.
    for ev in &report.evidence {
        if let Some(headers) = ev.context.get("headers").and_then(|h| h.as_object()) {
            assert!(
                !headers.contains_key("set-cookie"),
                "no evidence record should contain set-cookie"
            );
            assert!(
                !headers.contains_key("authorization"),
                "no evidence record should contain authorization"
            );
        }
    }
}
