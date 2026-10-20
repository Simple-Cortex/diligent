//! Tests for HSTS preload lookup integration.
//!
//! Tests the full flow from observation capture through check evaluation,
//! verifying that preload lookup results are correctly reflected in the
//! INFRA-02-hsts-preload row.

use std::path::PathBuf;

use diligent_infra::checks::evaluate;
use diligent_infra::fake::{FakeFetcher, FakeResolver};
use diligent_infra::fetch::Options;
use diligent_infra::observe::capture;
use serde_json::Value;
use url::Url;

fn fixture_dir(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("fixtures")
        .join("infra-preload")
        .join(name)
}

fn load_observation(case: &str) -> Value {
    let path = fixture_dir(case).join("observation.json");
    let text = std::fs::read_to_string(&path).expect("read observation.json");
    serde_json::from_str(&text).expect("parse observation.json")
}

fn load_expected(case: &str) -> Value {
    let path = fixture_dir(case).join("expected-preload-row.json");
    let text = std::fs::read_to_string(&path).expect("read expected-preload-row.json");
    serde_json::from_str(&text).expect("parse expected-preload-row.json")
}

fn row_json(row: &diligent_infra::Outcome) -> Value {
    serde_json::json!({
        "check_id": row.check_id,
        "rule": row.rule,
        "channel": row.channel,
        "status": row.status.as_str(),
        "expected": row.expected,
        "observed": row.observed,
        "limitations": row.limitations,
    })
}

fn run_case(case: &str, preload_lookup: bool) -> diligent_infra::Outcome {
    let obs_json = load_observation(case);
    let fetcher = FakeFetcher::from_observation(&obs_json);
    let resolver = FakeResolver::from_observation(&obs_json);
    let opts = Options {
        preload_lookup,
        ..Default::default()
    };
    let url = Url::parse("https://agency.example.gov/").unwrap();
    let obs = capture(&url, &fetcher, &resolver, &opts).expect("capture");
    let outcomes = evaluate(&obs);
    outcomes
        .into_iter()
        .find(|o| o.check_id == "INFRA-02-hsts-preload")
        .expect("INFRA-02-hsts-preload row")
}

#[test]
fn every_case_matches_its_expected_row() {
    for case in [
        "preloaded",
        "unknown",
        "pending",
        "http-500",
        "timeout",
        "malformed-json",
    ] {
        let row = run_case(case, true);
        assert_eq!(row_json(&row), load_expected(case), "case {case}");
    }
    let row = run_case("flag-off", false);
    assert_eq!(row_json(&row), load_expected("flag-off"), "case flag-off");
}

#[test]
fn preloaded_domain_passes_with_inference_basis() {
    let row = run_case("preloaded", true);
    assert_eq!(row.status, diligent_infra::Status::Pass);
    assert_eq!(row.observed["preload_lookup"], "preloaded");
    assert_eq!(row.observed["basis"], "inference");
}

#[test]
fn unknown_status_needs_review() {
    let row = run_case("unknown", true);
    assert_eq!(row.status, diligent_infra::Status::NeedsReview);
    assert_eq!(row.observed["preload_lookup"], "unknown");
}

#[test]
fn pending_status_needs_review() {
    let row = run_case("pending", true);
    assert_eq!(row.status, diligent_infra::Status::NeedsReview);
    assert_eq!(row.observed["preload_lookup"], "pending");
}

#[test]
fn http_500_falls_back_to_header_check() {
    let row = run_case("http-500", true);
    assert_eq!(row.status, diligent_infra::Status::NeedsReview);
    assert!(row
        .limitations
        .iter()
        .any(|l| l.contains("preload lookup failed: HTTP 500")));
}

#[test]
fn timeout_falls_back_to_header_check() {
    let row = run_case("timeout", true);
    assert_eq!(row.status, diligent_infra::Status::NeedsReview);
    assert!(row
        .limitations
        .iter()
        .any(|l| l.contains("preload lookup failed: timeout")));
}

#[test]
fn malformed_json_falls_back_to_header_check() {
    let row = run_case("malformed-json", true);
    assert_eq!(row.status, diligent_infra::Status::NeedsReview);
    assert!(row
        .limitations
        .iter()
        .any(|l| l.contains("preload lookup failed: unparseable response")));
}

#[test]
fn flag_off_skips_lookup() {
    let row_with = run_case("flag-off", true);
    let row_without = run_case("flag-off", false);
    // When flag is off, the row should be identical to header-only check
    assert_eq!(row_without.status, diligent_infra::Status::NeedsReview);
    assert!(row_without.observed.get("preload_lookup").is_none());
    // When flag is on and response is preloaded, it should pass
    assert_eq!(row_with.status, diligent_infra::Status::Pass);
    assert_eq!(row_with.observed["preload_lookup"], "preloaded");
}

#[test]
fn preload_response_recorded_as_evidence() {
    let obs_json = load_observation("preloaded");
    let fetcher = FakeFetcher::from_observation(&obs_json);
    let resolver = FakeResolver::from_observation(&obs_json);
    let opts = Options {
        preload_lookup: true,
        ..Default::default()
    };
    let url = Url::parse("https://agency.example.gov/").unwrap();
    let obs = capture(&url, &fetcher, &resolver, &opts).expect("capture");
    assert!(
        obs.preload_response.is_some(),
        "preload_response should be stored"
    );
    let resp = obs.preload_response.as_ref().unwrap();
    assert_eq!(resp.status, 200);
    assert!(resp.url.as_str().contains("hstspreload.org"));
}
