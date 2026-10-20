//! `data-diligent-src` stamped by the build plugin comes through as `source_ref`.

use std::path::{Path, PathBuf};

use diligent_assess::{assess_set, AssessOptions};
use diligent_capture::CaptureSet;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

fn node(
    index: usize,
    tag: &str,
    attrs: &[(&str, &str)],
    parent: Option<usize>,
    bbox: bool,
) -> Value {
    json!({
        "index": index,
        "node_type": 1,
        "tag": tag,
        "attributes": attrs.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
        "text": null,
        "parent": parent,
        "backend_node_id": index as i64 + 1,
        "bounding_box": if bbox { json!({ "x": 0.0, "y": 0.0, "width": 100.0, "height": 20.0 }) } else { Value::Null },
        "styles": if bbox { json!({ "display": "block", "visibility": "visible", "opacity": "1" }) } else { json!({}) },
    })
}

fn capture(name: &str, nodes: Vec<Value>) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("diligent-srcref-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let content = serde_json::to_string(
        &json!({ "document_url": "https://agency.example.gov/", "nodes": nodes }),
    )
    .unwrap();
    let sha: String = Sha256::digest(content.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    std::fs::write(dir.join(format!("{sha}.json")), &content).unwrap();
    let line = json!({
        "evidence_id": sha, "kind": "dom_snapshot",
        "viewport": { "width": 1280, "height": 800, "dpr": 1.0 },
        "media": "baseline", "path": format!("{sha}.json"), "sha256": sha, "bytes": content.len(),
        "captured_at": "2026-10-20T00:00:00Z", "final_url": "https://agency.example.gov/",
        "collector_version": "0.1.0", "protocol_version": "1.2.0"
    });
    std::fs::write(dir.join("manifest.jsonl"), line.to_string()).unwrap();
    dir
}

fn page(divs: &[&[(&str, &str)]]) -> Vec<Value> {
    let mut nodes = vec![
        node(0, "html", &[], None, true),
        node(1, "head", &[], Some(0), false),
        node(2, "body", &[], Some(0), true),
    ];
    for attrs in divs {
        let i = nodes.len();
        nodes.push(node(i, "div", attrs, Some(2), true));
    }
    nodes
}

fn assess(dir: &Path) -> (Vec<Value>, Vec<Value>) {
    let set = CaptureSet::load(dir).expect("capture loads");
    let report = assess_set(
        &set,
        &AssessOptions {
            run_id: Some("run-2026-10-20-abc123".to_string()),
            executed_at: Some("2026-10-20T00:00:00.000Z".to_string()),
            inventory: None,
            component: false,
            no_signals: true,
            corroborate: false,
        },
    )
    .unwrap();
    let results = report
        .results
        .iter()
        .map(|r| serde_json::to_value(r).unwrap())
        .collect();
    let evidence = report
        .evidence
        .iter()
        .map(|e| serde_json::to_value(e).unwrap())
        .collect();
    (results, evidence)
}

fn aria_rows(results: &[Value]) -> Vec<&Value> {
    results
        .iter()
        .filter(|r| r["check_id"] == "BRW-ARIA-ATTR-VALID" && r["status"] == "fail")
        .collect()
}

#[test]
fn a_stamped_element_carries_its_source_ref() {
    let dir = capture(
        "stamped",
        page(&[
            &[("role", "bogus"), ("data-diligent-src", "src/Card.tsx:12")],
            &[("role", "bogus")],
        ]),
    );
    let (results, evidence) = assess(&dir);
    let rows = aria_rows(&results);
    assert_eq!(rows.len(), 2, "{rows:?}");

    let stamped = rows
        .iter()
        .find(|r| r["extensions"]["diligent.assess"]["source_ref"].is_string())
        .expect("one row has a source_ref");
    assert_eq!(
        stamped["extensions"]["diligent.assess"]["source_ref"],
        "src/Card.tsx:12"
    );
    let plain = rows
        .iter()
        .find(|r| {
            r["extensions"]["diligent.assess"]
                .get("source_ref")
                .is_none()
        })
        .expect("the other row has none");
    assert_ne!(stamped["result_id"], plain["result_id"]);

    let instance = stamped["extensions"]["diligent.assess"]["instance"]
        .as_str()
        .unwrap();
    let dom = evidence
        .iter()
        .find(|e| e["context"].get("source_refs").is_some())
        .expect("the snapshot evidence lists source refs");
    assert_eq!(dom["context"]["source_refs"][instance], "src/Card.tsx:12");
    assert_eq!(dom["context"]["source_refs"].as_object().unwrap().len(), 1);
}

#[test]
fn without_stamps_output_has_no_source_ref_keys() {
    let dir = capture("plain", page(&[&[("role", "bogus")]]));
    let (results, evidence) = assess(&dir);
    assert!(!results.is_empty());
    for r in &results {
        assert!(r["extensions"]["diligent.assess"]
            .get("source_ref")
            .is_none());
    }
    for e in &evidence {
        assert!(e["context"].get("source_refs").is_none());
    }
}

#[test]
fn a_blank_or_very_long_value_is_handled() {
    let long = "x".repeat(2000);
    let dir = capture(
        "odd",
        page(&[
            &[("role", "bogus"), ("data-diligent-src", "   ")],
            &[("role", "bogus"), ("data-diligent-src", long.as_str())],
        ]),
    );
    let (results, _) = assess(&dir);
    let refs: Vec<&str> = aria_rows(&results)
        .iter()
        .filter_map(|r| r["extensions"]["diligent.assess"]["source_ref"].as_str())
        .collect();
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].chars().count(), 300);
}
