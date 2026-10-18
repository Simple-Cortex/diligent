//! Output contract: serialized findings conform to the schemas in `schemas/`,
//! JSONL writing round-trips, and `scan_path` walks directories.

use std::collections::BTreeSet;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use diligent_source::{
    cli, scan_html, scan_path, write_jsonl, ScanError, ScanOptions, SourceRef, State,
};
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn schema(name: &str) -> Value {
    let text = fs::read_to_string(repo_root().join("schemas").join(name)).unwrap();
    serde_json::from_str(&text).unwrap()
}

fn assert_conforms(row: &Value, schema: &Value) {
    let props = schema["properties"].as_object().unwrap();
    for key in schema["required"].as_array().unwrap() {
        let key = key.as_str().unwrap();
        assert!(row.get(key).is_some(), "missing required key {key}: {row}");
    }
    for key in row.as_object().unwrap().keys() {
        assert!(props.contains_key(key), "key {key} not allowed by schema");
    }
    for (key, def) in props {
        let Some(value) = row.get(key) else { continue };
        if let Some(allowed) = def.get("enum").and_then(Value::as_array) {
            assert!(allowed.contains(value), "{key}={value} not in enum");
        }
        if let Some(c) = def.get("const") {
            assert_eq!(value, c, "{key}");
        }
        if let Some(min) = def.get("minLength").and_then(Value::as_u64) {
            if let Some(s) = value.as_str() {
                assert!(s.len() as u64 >= min, "{key} too short");
            }
        }
    }
}

const SAMPLE: &str = "<!DOCTYPE html><html lang=\"en\"><head><title>Sample page for the output contract</title></head>\
<body><img src=\"a.png\"><a href=\"/x\">Read more</a><input name=\"q\" placeholder=\"Search\"></body></html>";

#[test]
fn findings_conform_to_the_schemas() {
    let evidence_schema = schema("evidence-record.schema.json");
    let result_schema = schema("check-result.schema.json");
    let findings = scan_html(SAMPLE, &SourceRef::new("sample.html"));
    assert!(findings.len() >= 20);
    let evidence_ids: BTreeSet<&str> = findings
        .iter()
        .map(|f| f.evidence.evidence_id.as_str())
        .collect();
    for f in &findings {
        let ev = serde_json::to_value(&f.evidence).unwrap();
        let rs = serde_json::to_value(&f.result).unwrap();
        assert_conforms(&ev, &evidence_schema);
        assert_conforms(&rs, &result_schema);
        let sha = ev["sha256"].as_str().unwrap();
        assert_eq!(sha.len(), 64);
        assert!(sha
            .chars()
            .all(|c| c.is_ascii_hexdigit() && !c.is_ascii_uppercase()));
        let run = ev["run_id"].as_str().unwrap();
        assert!(run.starts_with("run-") && run.len() == 21, "{run}");
        assert!(
            ev["context"]["outer_html"]
                .as_str()
                .unwrap()
                .chars()
                .count()
                <= 400
        );
        assert!(ev["context"]["byte_offset"].is_null());
        for r in rs["evidence_refs"].as_array().unwrap() {
            assert!(evidence_ids.contains(r.as_str().unwrap()));
        }
        let ext = &rs["extensions"]["diligent.source"];
        assert_eq!(ext["scope"], "static");
        assert!(ext["act_rule_ids"].is_array() && ext["rule_ids"].is_array());
        assert!(State::parse(rs["status"].as_str().unwrap()).is_some());
    }
}

#[test]
fn rule_ids_exist_in_the_packs() {
    let mut known = BTreeSet::new();
    for entry in fs::read_dir(repo_root().join("rules/packs")).unwrap() {
        let path = entry.unwrap().path();
        let pack: Value = serde_json::from_str(&fs::read_to_string(&path).unwrap()).unwrap();
        for rule in pack["rules"].as_array().unwrap() {
            known.insert(rule["rule_id"].as_str().unwrap().to_string());
        }
    }
    let findings = scan_html(SAMPLE, &SourceRef::new("sample.html"));
    for f in &findings {
        for id in f.result.extensions["diligent.source"]["rule_ids"]
            .as_array()
            .unwrap()
        {
            assert!(known.contains(id.as_str().unwrap()), "unknown rule id {id}");
        }
    }
}

#[test]
fn long_snippets_are_truncated_to_400_characters() {
    let filler = "x".repeat(2000);
    let html = format!("<!DOCTYPE html><html lang=\"en\"><head><title>Long snippet fixture page</title></head><body><a href=\"/x\">{filler}</a></body></html>");
    let findings = scan_html(&html, &SourceRef::new("long.html"));
    let link = findings
        .iter()
        .find(|f| f.result.check_id == "SRC-LINK-NAME")
        .unwrap();
    assert_eq!(link.evidence.context["outer_html_truncated"], true);
    assert_eq!(
        link.evidence.context["outer_html"]
            .as_str()
            .unwrap()
            .chars()
            .count(),
        400
    );
}

fn temp_dir(name: &str) -> PathBuf {
    let dir = Path::new(env!("CARGO_TARGET_TMPDIR")).join(name);
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir
}

#[test]
fn scan_path_walks_html_files_only() {
    let dir = temp_dir("scan-path-walk");
    fs::create_dir_all(dir.join("sub")).unwrap();
    fs::create_dir_all(dir.join("node_modules")).unwrap();
    fs::write(dir.join("a.html"), SAMPLE).unwrap();
    fs::write(dir.join("sub/b.htm"), SAMPLE).unwrap();
    fs::write(dir.join("notes.txt"), "not html").unwrap();
    fs::write(dir.join("node_modules/skip.html"), SAMPLE).unwrap();
    let report = scan_path(&dir, &ScanOptions::default()).unwrap();
    assert_eq!(report.files_scanned, 2);
    assert_eq!(report.files_skipped, 0);
    assert_eq!(report.checks_run.len(), 20);
    let paths: BTreeSet<&str> = report
        .findings
        .iter()
        .map(|f| f.evidence.payload_ref.as_str())
        .collect();
    assert_eq!(paths, BTreeSet::from(["a.html", "sub/b.htm"]));
    assert!(report
        .not_tested
        .iter()
        .all(|(_, p)| p == "a.html" || p == "sub/b.htm"));
    let run_ids: BTreeSet<&str> = report
        .findings
        .iter()
        .map(|f| f.evidence.run_id.as_str())
        .collect();
    assert_eq!(run_ids.len(), 1);
}

#[test]
fn scan_path_skips_oversized_files_and_reports_missing_paths() {
    let dir = temp_dir("scan-path-skip");
    fs::write(dir.join("big.html"), SAMPLE).unwrap();
    let opts = ScanOptions {
        max_file_bytes: 10,
        ..ScanOptions::default()
    };
    let report = scan_path(&dir, &opts).unwrap();
    assert_eq!((report.files_scanned, report.files_skipped), (0, 1));
    assert_eq!(report.skipped[0].path, "big.html");
    let missing = scan_path(&dir.join("nope"), &ScanOptions::default());
    assert!(matches!(missing, Err(ScanError::NotFound(_))));
}

#[test]
fn write_jsonl_emits_one_row_per_result_and_unique_evidence() {
    let dir = temp_dir("jsonl");
    fs::write(dir.join("a.html"), SAMPLE).unwrap();
    let report = scan_path(&dir, &ScanOptions::default()).unwrap();
    let (mut ev, mut rs) = (Vec::new(), Vec::new());
    write_jsonl(&report, &mut ev, &mut rs).unwrap();
    let result_lines: Vec<&str> = std::str::from_utf8(&rs).unwrap().lines().collect();
    let evidence_lines: Vec<&str> = std::str::from_utf8(&ev).unwrap().lines().collect();
    assert_eq!(result_lines.len(), report.findings.len());
    let ids: BTreeSet<String> = evidence_lines
        .iter()
        .map(|l| {
            serde_json::from_str::<Value>(l).unwrap()["evidence_id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        ids.len(),
        evidence_lines.len(),
        "evidence rows must be unique"
    );
    for l in result_lines {
        let row: Value = serde_json::from_str(l).unwrap();
        assert!(ids.contains(row["evidence_refs"][0].as_str().unwrap()));
    }
}

fn code(c: ExitCode) -> String {
    format!("{c:?}")
}

#[test]
fn cli_exit_codes() {
    let dir = temp_dir("cli");
    fs::write(dir.join("a.html"), SAMPLE).unwrap();
    let out = dir.join("out");
    let args = |v: &[&str]| v.iter().map(|s| s.to_string()).collect::<Vec<_>>();
    let ok = cli(&args(&[
        "source",
        "scan",
        dir.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]));
    assert_eq!(code(ok), code(ExitCode::SUCCESS));
    assert!(out.join("EVIDENCE.jsonl").is_file() && out.join("RESULTS.jsonl").is_file());
    let bad_path = cli(&args(&[
        "source",
        "scan",
        dir.join("missing").to_str().unwrap(),
    ]));
    assert_eq!(code(bad_path), code(ExitCode::from(3)));
    assert_eq!(code(cli(&args(&["source"]))), code(ExitCode::from(3)));
    assert_eq!(
        code(cli(&args(&["source", "scan"]))),
        code(ExitCode::from(3))
    );
    assert_eq!(
        code(cli(&args(&["source", "scan", "x", "--bogus"]))),
        code(ExitCode::from(3))
    );
}
