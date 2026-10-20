//! Runs every kept ACT case and every Diligent-authored fixture through
//! `scan_html` and compares the aggregate state per check with the manifests.
//!
//! The aggregate for a check is the worst state it recorded in the document:
//! fail, then needs_review, then pass, then not_tested.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};

use diligent_source::{scan_html, scan_text, Finding, Flavor, SourceRef, State};
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn load(path: &Path) -> Value {
    let text = fs::read_to_string(path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("parse {}: {e}", path.display()))
}

fn aggregate(findings: &[Finding], check_id: &str) -> Option<State> {
    findings
        .iter()
        .filter(|f| f.result.check_id == check_id)
        .map(|f| f.result.status)
        .reduce(State::worst)
}

fn is_subset(expected: &Value, actual: &Value) -> bool {
    match (expected, actual) {
        (Value::Object(e), Value::Object(a)) => e
            .iter()
            .all(|(k, v)| a.get(k).is_some_and(|av| is_subset(v, av))),
        _ => expected == actual,
    }
}

fn html_files(dir: &Path) -> BTreeSet<String> {
    let mut out = BTreeSet::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else if p.extension().is_some_and(|e| e == "html") {
                out.insert(
                    p.strip_prefix(dir)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    out
}

#[test]
fn act_cases_match_manifest() {
    let dir = repo_root().join("fixtures/act");
    let manifest = load(&dir.join("manifest.json"));
    let mut failures = Vec::new();
    let mut kept = 0usize;
    for case in manifest["cases"].as_array().expect("cases array") {
        let path = case["path"].as_str().unwrap();
        if !case["skipped_reason"].is_null() {
            assert!(
                case["diligent_expected"].is_null(),
                "{path}: skipped case must not carry an expected state"
            );
            continue;
        }
        kept += 1;
        let html =
            fs::read_to_string(dir.join(path)).unwrap_or_else(|e| panic!("read {path}: {e}"));
        let check = case["check_id"].as_str().unwrap();
        let want = State::parse(case["diligent_expected"].as_str().unwrap()).expect("known state");
        let findings = scan_html(&html, &SourceRef::new(path));
        let got = aggregate(&findings, check);
        if got != Some(want) {
            failures.push(format!(
                "{path} [{check}] act={} want={} got={:?}",
                case["act_expected"].as_str().unwrap(),
                want.as_str(),
                got.map(State::as_str)
            ));
        }
    }
    assert!(kept > 0, "no kept ACT cases");
    assert!(
        failures.is_empty(),
        "{} of {kept} ACT cases differ:\n{}",
        failures.len(),
        failures.join("\n")
    );
}

#[test]
fn act_manifest_covers_every_fixture_file() {
    let dir = repo_root().join("fixtures/act");
    let manifest = load(&dir.join("manifest.json"));
    let listed: BTreeSet<String> = manifest["cases"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap().to_string())
        .collect();
    assert_eq!(
        listed,
        html_files(&dir),
        "manifest and files on disk differ"
    );
    for case in manifest["cases"].as_array().unwrap() {
        if let Some(reason) = case["skipped_reason"].as_str() {
            assert!(!reason.trim().is_empty());
        }
    }
}

#[test]
fn act_divergences_are_explained() {
    let dir = repo_root().join("fixtures/act");
    let manifest = load(&dir.join("manifest.json"));
    for case in manifest["cases"].as_array().unwrap() {
        let Some(want) = case["diligent_expected"].as_str() else {
            continue;
        };
        let natural = match case["act_expected"].as_str().unwrap() {
            "failed" => "fail",
            "inapplicable" => "not_tested",
            _ => "pass",
        };
        // Passed cases may legitimately become needs_review; anything else that
        // differs from the natural mapping needs a note.
        let explained = want == natural
            || (natural == "pass" && want == "needs_review")
            || case["note"].is_string();
        assert!(
            explained,
            "{} differs from ACT without a note",
            case["path"]
        );
    }
}

#[test]
fn authored_fixtures_match_manifest() {
    let dir = repo_root().join("fixtures/source");
    let manifest = load(&dir.join("manifest.json"));
    let mut failures = Vec::new();
    let entries = manifest["entries"].as_array().expect("entries array");
    assert!(!entries.is_empty());
    for entry in entries {
        let path = entry["path"].as_str().unwrap();
        let check = entry["check_id"].as_str().unwrap();
        let html =
            fs::read_to_string(dir.join(path)).unwrap_or_else(|e| panic!("read {path}: {e}"));
        let flavor = Flavor::from_path(Path::new(path)).unwrap_or(Flavor::Html);
        let findings = scan_text(&html, &SourceRef::new(path), flavor);
        let want = State::parse(entry["expected"].as_str().unwrap()).expect("known state");
        let got = aggregate(&findings, check);
        if got != Some(want) {
            failures.push(format!(
                "{path} [{check}] want={} got={:?}",
                want.as_str(),
                got.map(State::as_str)
            ));
            continue;
        }
        let of_check: Vec<&Finding> = findings
            .iter()
            .filter(|f| f.result.check_id == check)
            .collect();
        if let Some(count) = entry.get("count").and_then(Value::as_u64) {
            if of_check.len() as u64 != count {
                failures.push(format!(
                    "{path} [{check}] want {count} findings, got {}",
                    of_check.len()
                ));
            }
        }
        let selected = match entry.get("select") {
            Some(sel) => of_check.iter().find(|f| is_subset(sel, &f.result.observed)),
            None => of_check.first(),
        };
        if let Some(expected_observed) = entry.get("observed") {
            match selected {
                Some(f) if is_subset(expected_observed, &f.result.observed) => {}
                Some(f) => failures.push(format!(
                    "{path} [{check}] observed {} lacks {expected_observed}",
                    f.result.observed
                )),
                None => failures.push(format!("{path} [{check}] no finding matches select")),
            }
        }
    }
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn authored_manifest_covers_every_fixture_file() {
    let dir = repo_root().join("fixtures/source");
    let manifest = load(&dir.join("manifest.json"));
    let listed: BTreeSet<String> = manifest["entries"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["path"].as_str().unwrap().to_string())
        .collect();
    let on_disk = html_files(&dir);
    // selector/ fixtures are exercised by selectors.rs rather than the manifest.
    let mut on_disk: BTreeSet<String> = on_disk
        .into_iter()
        .filter(|p| !p.starts_with("selector/"))
        .collect();
    // Template flavours have their own extensions; every file there is listed too.
    let templates = dir.join("templates");
    let mut stack = vec![templates.clone()];
    while let Some(d) = stack.pop() {
        for entry in fs::read_dir(&d).unwrap() {
            let p = entry.unwrap().path();
            if p.is_dir() {
                stack.push(p);
            } else {
                on_disk.insert(
                    p.strip_prefix(&dir)
                        .unwrap()
                        .to_string_lossy()
                        .replace('\\', "/"),
                );
            }
        }
    }
    assert_eq!(listed, on_disk);
}

#[test]
fn every_check_records_at_least_one_finding() {
    let html = "<!DOCTYPE html><html lang=\"en\"><head><title>A page title for the static pass</title></head><body><p>Hi</p></body></html>";
    let findings = scan_html(html, &SourceRef::new("t.html"));
    let ids: BTreeSet<&str> = findings
        .iter()
        .map(|f| f.result.check_id.as_str())
        .collect();
    assert_eq!(ids.len(), 20, "{ids:?}");
}
