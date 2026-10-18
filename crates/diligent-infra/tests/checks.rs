//! Runs every `fixtures/infra/<case>` through `run` with the fakes, compares the rows with
//! `expected-results.jsonl`, and validates rows and evidence against the schemas.

use std::fs;
use std::path::{Path, PathBuf};

use diligent_infra::checks::robots_disallows_all;
use diligent_infra::fake::run_observation;
use diligent_infra::observe::registrable_domain;
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixture_dirs() -> Vec<PathBuf> {
    let mut dirs: Vec<PathBuf> = fs::read_dir(repo_root().join("fixtures/infra"))
        .unwrap()
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("observation.json").is_file())
        .collect();
    dirs.sort();
    dirs
}

fn read_json(path: &Path) -> Value {
    serde_json::from_str(&fs::read_to_string(path).unwrap()).unwrap()
}

fn validator(name: &str) -> jsonschema::Validator {
    let schema = read_json(&repo_root().join("schemas").join(name));
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .should_validate_formats(true)
        .build(&schema)
        .unwrap()
}

#[test]
fn there_are_the_ten_fixtures() {
    let names: Vec<String> = fixture_dirs()
        .iter()
        .map(|d| d.file_name().unwrap().to_string_lossy().into_owned())
        .collect();
    for want in [
        "federal-good",
        "http-only",
        "hsts-short",
        "no-dmarc",
        "spf-multiple-records",
        "vdp-security-txt-only",
        "vdp-page-only",
        "robots-disallow-all",
        "non-gov-host",
        "timeout-everywhere",
    ] {
        assert!(names.iter().any(|n| n == want), "missing fixture {want}");
    }
}

#[test]
fn fixtures_match_expected_results() {
    for dir in fixture_dirs() {
        let observation = read_json(&dir.join("observation.json"));
        let report = run_observation(&observation).expect("valid origin");
        let actual: Vec<Value> = report
            .rows
            .iter()
            .map(|r| serde_json::to_value(r).unwrap())
            .collect();
        let expected: Vec<Value> = fs::read_to_string(dir.join("expected-results.jsonl"))
            .unwrap()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(actual.len(), expected.len(), "{}: row count", dir.display());
        for (i, (a, e)) in actual.iter().zip(&expected).enumerate() {
            assert_eq!(a, e, "{}: row {i} ({})", dir.display(), a["check_id"]);
        }
    }
}

#[test]
fn fixture_output_validates_against_both_schemas() {
    let check = validator("check-result.schema.json");
    let evidence = validator("evidence-record.schema.json");
    for dir in fixture_dirs() {
        let report = run_observation(&read_json(&dir.join("observation.json"))).unwrap();
        assert!(!report.rows.is_empty(), "{}", dir.display());
        let ids: Vec<&str> = report
            .evidence
            .iter()
            .map(|e| e.evidence_id.as_str())
            .collect();
        for row in &report.rows {
            let v = serde_json::to_value(row).unwrap();
            let errors: Vec<String> = check.iter_errors(&v).map(|e| e.to_string()).collect();
            assert!(errors.is_empty(), "{}: {errors:?}", dir.display());
            for r in &row.evidence_refs {
                assert!(
                    ids.contains(&r.as_str()),
                    "{}: dangling evidence ref {r}",
                    dir.display()
                );
            }
        }
        for rec in &report.evidence {
            let v = serde_json::to_value(rec).unwrap();
            let errors: Vec<String> = evidence.iter_errors(&v).map(|e| e.to_string()).collect();
            assert!(errors.is_empty(), "{}: {errors:?}", dir.display());
            let text = v.to_string().to_ascii_lowercase();
            assert!(
                !text.contains("session=abc123"),
                "set-cookie value leaked into evidence"
            );
        }
    }
}

#[test]
fn robots_parser_reads_the_star_group_only() {
    assert!(robots_disallows_all("User-agent: *\nDisallow: /\n"));
    assert!(!robots_disallows_all(
        "User-agent: *\nDisallow: /private/\n"
    ));
    assert!(!robots_disallows_all("User-agent: badbot\nDisallow: /\n"));
    assert!(robots_disallows_all(
        "User-agent: a\nUser-agent: *\nDisallow: / # all\n"
    ));
    assert!(!robots_disallows_all(
        "User-agent: *\nAllow: /\nUser-agent: other\nDisallow: /\n"
    ));
}

#[test]
fn registrable_domain_heuristic() {
    assert_eq!(registrable_domain("www.usa.gov"), "usa.gov");
    assert_eq!(registrable_domain("usa.gov"), "usa.gov");
    assert_eq!(registrable_domain("a.b.example.co.uk"), "example.co.uk");
    assert_eq!(registrable_domain("127.0.0.1"), "127.0.0.1");
}
