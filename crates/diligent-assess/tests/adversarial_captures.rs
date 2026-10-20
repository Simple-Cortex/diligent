//! Adversarial tests for the browser-capture checks.
//!
//! These tests build synthetic captures programmatically to exercise edge cases
//! and hostile inputs that the recorded fixtures don't cover.

use std::path::PathBuf;
use std::time::Instant;

use diligent_assess::checks;
use diligent_assess::{assess_set, AssessOptions};
use diligent_capture::CaptureSet;
use serde_json::json;
use sha2::{Digest, Sha256};

fn temp_dir(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("diligent-adv-{}-{}", name, std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

fn write_capture(dir: &std::path::Path, manifest_lines: &[String], files: &[(String, String)]) {
    std::fs::create_dir_all(dir).unwrap();
    let manifest_path = dir.join("manifest.jsonl");
    std::fs::write(&manifest_path, manifest_lines.join("\n")).unwrap();
    for (name, content) in files {
        std::fs::write(dir.join(name), content).unwrap();
    }
}

fn sha256(content: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(content.as_bytes());
    let result = hasher.finalize();
    result.iter().map(|b| format!("{:02x}", b)).collect()
}

fn minimal_dom() -> String {
    serde_json::to_string(&json!({
        "document_url": "http://example.com",
        "nodes": [
            {
                "index": 0,
                "node_type": 9,
                "tag": "#document",
                "attributes": [],
                "text": null,
                "parent": null,
                "backend_node_id": 1,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            },
            {
                "index": 1,
                "node_type": 1,
                "tag": "html",
                "attributes": [],
                "text": null,
                "parent": 0,
                "backend_node_id": 2,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            }
        ],
        "document_metrics": {
            "scroll_width": 1280,
            "scroll_height": 800,
            "client_width": 1280,
            "client_height": 800
        }
    }))
    .unwrap()
}

fn dom_manifest_entry(dom_sha: &str, bytes: usize) -> String {
    format!(
        r#"{{"evidence_id":"{dom_sha}","kind":"dom_snapshot","viewport":{{"width":1280,"height":800,"dpr":1.0}},"media":"baseline","path":"{dom_sha}.json","sha256":"{dom_sha}","bytes":{bytes},"captured_at":"2026-10-18T00:00:00Z","final_url":"http://example.com","collector_version":"0.1.0","protocol_version":"1.1.0"}}"#
    )
}

fn cleanup(dir: &std::path::Path) {
    let _ = std::fs::remove_dir_all(dir);
}

#[test]
fn truncated_json_file_errors_not_panics() {
    let dir = temp_dir("truncated");
    let dom_content = minimal_dom();
    let dom_sha = sha256(&dom_content);
    let truncated = &dom_content[..dom_content.len() / 2];

    let manifest = vec![dom_manifest_entry(&dom_sha, truncated.len())];
    let files = vec![(format!("{dom_sha}.json"), truncated.to_string())];

    write_capture(&dir, &manifest, &files);
    let result = CaptureSet::load(&dir);
    assert!(result.is_err(), "truncated JSON should error, not panic");

    cleanup(&dir);
}

#[test]
fn manifest_sha_mismatch_errors() {
    let dir = temp_dir("sha-mismatch");
    let dom_content = minimal_dom();
    let dom_sha = sha256(&dom_content);
    let wrong_sha = "0000000000000000000000000000000000000000000000000000000000000000";

    let manifest = vec![format!(
        r#"{{"evidence_id":"{dom_sha}","kind":"dom_snapshot","viewport":{{"width":1280,"height":800,"dpr":1.0}},"media":"baseline","path":"{dom_sha}.json","sha256":"{wrong_sha}","bytes":{},"captured_at":"2026-10-18T00:00:00Z","collector_version":"0.1.0","protocol_version":"1.1.0"}}"#,
        dom_content.len()
    )];
    let files = vec![(format!("{dom_sha}.json"), dom_content)];

    write_capture(&dir, &manifest, &files);
    let result = CaptureSet::load(&dir);
    assert!(result.is_err(), "SHA mismatch should error");

    cleanup(&dir);
}

#[test]
fn ax_node_with_no_dom_match_does_not_panic() {
    let dir = temp_dir("ax-no-dom");
    let dom_content = minimal_dom();
    let dom_sha = sha256(&dom_content);
    let ax_content = serde_json::to_string(&json!({
        "nodes": [
            {
                "node_id": "ax1",
                "backend_dom_node_id": 99999,
                "role": "button",
                "name": "Test",
                "name_source": null,
                "description": null,
                "value": null,
                "properties": [],
                "ignored": false,
                "children": []
            }
        ]
    }))
    .unwrap();
    let ax_sha = sha256(&ax_content);

    let manifest = vec![
        dom_manifest_entry(&dom_sha, dom_content.len()),
        format!(
            r#"{{"evidence_id":"{ax_sha}","kind":"ax_tree","viewport":{{"width":1280,"height":800,"dpr":1.0}},"media":"baseline","path":"{ax_sha}.json","sha256":"{ax_sha}","bytes":{},"captured_at":"2026-10-18T00:00:00Z","final_url":"http://example.com","collector_version":"0.1.0","protocol_version":"1.1.0"}}"#,
            ax_content.len()
        ),
    ];
    let files = vec![
        (format!("{dom_sha}.json"), dom_content),
        (format!("{ax_sha}.json"), ax_content),
    ];

    write_capture(&dir, &manifest, &files);
    let set = CaptureSet::load(&dir).expect("load should succeed");
    let findings = checks::run(&set);
    assert!(!findings.is_empty(), "checks should run without panic");

    cleanup(&dir);
}

#[test]
fn dom_with_parent_cycle_does_not_loop_forever() {
    let dir = temp_dir("parent-cycle");
    let dom_content = serde_json::to_string(&json!({
        "document_url": "http://example.com",
        "nodes": [
            {
                "index": 0,
                "node_type": 9,
                "tag": "#document",
                "attributes": [],
                "text": null,
                "parent": null,
                "backend_node_id": 1,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            },
            {
                "index": 1,
                "node_type": 1,
                "tag": "html",
                "attributes": [],
                "text": null,
                "parent": 2,
                "backend_node_id": 2,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            },
            {
                "index": 2,
                "node_type": 1,
                "tag": "body",
                "attributes": [],
                "text": null,
                "parent": 1,
                "backend_node_id": 3,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            }
        ],
        "document_metrics": {
            "scroll_width": 1280,
            "scroll_height": 800,
            "client_width": 1280,
            "client_height": 800
        }
    }))
    .unwrap();
    let dom_sha = sha256(&dom_content);

    let manifest = vec![dom_manifest_entry(&dom_sha, dom_content.len())];
    let files = vec![(format!("{dom_sha}.json"), dom_content)];

    write_capture(&dir, &manifest, &files);
    let start = Instant::now();
    let set = CaptureSet::load(&dir).expect("load should succeed");
    let findings = checks::run(&set);
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 5,
        "cycle should not cause infinite loop (took {:?})",
        elapsed
    );
    assert!(!findings.is_empty(), "checks should run");

    cleanup(&dir);
}

#[test]
fn large_dom_loads_and_checks_in_bounded_time() {
    let dir = temp_dir("large-dom");
    let mut nodes = vec![json!({
        "index": 0,
        "node_type": 9,
        "tag": "#document",
        "attributes": [],
        "text": null,
        "parent": null,
        "backend_node_id": 1,
        "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
        "styles": {}
    })];
    for i in 1..100_000 {
        nodes.push(json!({
            "index": i,
            "node_type": 1,
            "tag": "div",
            "attributes": [],
            "text": null,
            "parent": 0,
            "backend_node_id": (i + 1) as i64,
            "bounding_box": {"x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0},
            "styles": {}
        }));
    }
    let dom_content = serde_json::to_string(&json!({
        "document_url": "http://example.com",
        "nodes": nodes,
        "document_metrics": {
            "scroll_width": 1280,
            "scroll_height": 800,
            "client_width": 1280,
            "client_height": 800
        }
    }))
    .unwrap();
    let dom_sha = sha256(&dom_content);

    let manifest = vec![dom_manifest_entry(&dom_sha, dom_content.len())];
    let files = vec![(format!("{dom_sha}.json"), dom_content)];

    write_capture(&dir, &manifest, &files);
    let start = Instant::now();
    let set = CaptureSet::load(&dir).expect("load should succeed");
    let load_time = start.elapsed();
    eprintln!("100k-node DOM load time: {:?}", load_time);
    assert!(
        load_time.as_secs() < 10,
        "100k node DOM should load in <10s (took {:?})",
        load_time
    );

    let start = Instant::now();
    let findings = checks::run(&set);
    let check_time = start.elapsed();
    eprintln!("100k-node DOM check time: {:?}", check_time);
    assert!(
        check_time.as_secs() < 20,
        "checks should finish in <20s (took {:?})",
        check_time
    );
    assert!(!findings.is_empty(), "checks should produce findings");

    cleanup(&dir);
}

#[test]
fn unusual_color_formats_reach_needs_review_not_panic() {
    let dir = temp_dir("unusual-colors");
    let dom_content = serde_json::to_string(&json!({
        "document_url": "http://example.com",
        "nodes": [
            {
                "index": 0,
                "node_type": 9,
                "tag": "#document",
                "attributes": [],
                "text": null,
                "parent": null,
                "backend_node_id": 1,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            },
            {
                "index": 1,
                "node_type": 1,
                "tag": "html",
                "attributes": [],
                "text": null,
                "parent": 0,
                "backend_node_id": 2,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            },
            {
                "index": 2,
                "node_type": 1,
                "tag": "p",
                "attributes": [["style", "color: rgba(0,0,0,0); background: color(display-p3 1 0 0);"]],
                "text": null,
                "parent": 1,
                "backend_node_id": 3,
                "bounding_box": {"x": 10.0, "y": 10.0, "width": 200.0, "height": 30.0},
                "styles": {
                    "color": "rgba(0, 0, 0, 0)",
                    "background-color": "color(display-p3 1 0 0)",
                    "display": "block",
                    "visibility": "visible",
                    "opacity": "1",
                    "font-size": "16px",
                    "font-weight": "400"
                }
            },
            {
                "index": 3,
                "node_type": 3,
                "tag": "#text",
                "attributes": [],
                "text": "Test text with unusual colors",
                "parent": 2,
                "backend_node_id": 4,
                "bounding_box": {"x": 10.0, "y": 10.0, "width": 200.0, "height": 20.0},
                "styles": {}
            }
        ],
        "document_metrics": {
            "scroll_width": 1280,
            "scroll_height": 800,
            "client_width": 1280,
            "client_height": 800
        }
    }))
    .unwrap();
    let dom_sha = sha256(&dom_content);

    let manifest = vec![dom_manifest_entry(&dom_sha, dom_content.len())];
    let files = vec![(format!("{dom_sha}.json"), dom_content)];

    write_capture(&dir, &manifest, &files);
    let set = CaptureSet::load(&dir).expect("load should succeed");
    let findings = checks::run(&set);

    let text_contrast_nr = findings.iter().any(|f| {
        f.check_id == "BRW-TEXT-CONTRAST" && f.state == diligent_assess::State::NeedsReview
    });
    assert!(
        text_contrast_nr,
        "unparseable color(display-p3 …) background should produce needs_review for BRW-TEXT-CONTRAST"
    );

    cleanup(&dir);
}

#[test]
fn selector_builder_produces_unique_ids_for_siblings() {
    let dir = temp_dir("sibling-selectors");
    let dom_content = serde_json::to_string(&json!({
        "document_url": "http://example.com",
        "nodes": [
            {
                "index": 0,
                "node_type": 9,
                "tag": "#document",
                "attributes": [],
                "text": null,
                "parent": null,
                "backend_node_id": 1,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            },
            {
                "index": 1,
                "node_type": 1,
                "tag": "html",
                "attributes": [],
                "text": null,
                "parent": 0,
                "backend_node_id": 2,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            },
            {
                "index": 2,
                "node_type": 1,
                "tag": "body",
                "attributes": [],
                "text": null,
                "parent": 1,
                "backend_node_id": 3,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            },
            {
                "index": 3,
                "node_type": 1,
                "tag": "div",
                "attributes": [],
                "text": null,
                "parent": 2,
                "backend_node_id": 4,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 100.0, "height": 100.0},
                "styles": {}
            },
            {
                "index": 4,
                "node_type": 1,
                "tag": "div",
                "attributes": [],
                "text": null,
                "parent": 2,
                "backend_node_id": 5,
                "bounding_box": {"x": 100.0, "y": 0.0, "width": 100.0, "height": 100.0},
                "styles": {}
            },
            {
                "index": 5,
                "node_type": 1,
                "tag": "div",
                "attributes": [],
                "text": null,
                "parent": 2,
                "backend_node_id": 6,
                "bounding_box": {"x": 200.0, "y": 0.0, "width": 100.0, "height": 100.0},
                "styles": {}
            }
        ],
        "document_metrics": {
            "scroll_width": 1280,
            "scroll_height": 800,
            "client_width": 1280,
            "client_height": 800
        }
    }))
    .unwrap();
    let dom_sha = sha256(&dom_content);

    let manifest = vec![dom_manifest_entry(&dom_sha, dom_content.len())];
    let files = vec![(format!("{dom_sha}.json"), dom_content)];

    write_capture(&dir, &manifest, &files);
    let set = CaptureSet::load(&dir).expect("load should succeed");
    let view = diligent_assess::capture_view::CaptureView::new(
        &set,
        diligent_capture::Viewport {
            width: 1280,
            height: 800,
            dpr: 1.0,
        },
        "baseline",
    )
    .unwrap();

    let sel1 = view.selector(3);
    let sel2 = view.selector(4);
    let sel3 = view.selector(5);

    assert_ne!(sel1, sel2, "sibling divs should have different selectors");
    assert_ne!(sel2, sel3, "sibling divs should have different selectors");
    assert_ne!(sel1, sel3, "sibling divs should have different selectors");

    cleanup(&dir);
}

#[test]
fn mixed_case_important_in_style_attribute() {
    let dir = temp_dir("mixed-case-important");
    let dom_content = serde_json::to_string(&json!({
        "document_url": "http://example.com",
        "nodes": [
            {
                "index": 0,
                "node_type": 9,
                "tag": "#document",
                "attributes": [],
                "text": null,
                "parent": null,
                "backend_node_id": 1,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            },
            {
                "index": 1,
                "node_type": 1,
                "tag": "html",
                "attributes": [],
                "text": null,
                "parent": 0,
                "backend_node_id": 2,
                "bounding_box": {"x": 0.0, "y": 0.0, "width": 1280.0, "height": 800.0},
                "styles": {}
            },
            {
                "index": 2,
                "node_type": 1,
                "tag": "p",
                "attributes": [["style", "letter-spacing: 0 !ImPoRtAnT"]],
                "text": null,
                "parent": 1,
                "backend_node_id": 3,
                "bounding_box": {"x": 10.0, "y": 10.0, "width": 200.0, "height": 30.0},
                "styles": {
                    "letter-spacing": "0px",
                    "font-size": "16px",
                    "font-weight": "400",
                    "display": "block",
                    "visibility": "visible",
                    "opacity": "1"
                }
            },
            {
                "index": 3,
                "node_type": 3,
                "tag": "#text",
                "attributes": [],
                "text": "Test text with mixed-case !important",
                "parent": 2,
                "backend_node_id": 4,
                "bounding_box": {"x": 10.0, "y": 10.0, "width": 200.0, "height": 20.0},
                "styles": {}
            }
        ],
        "document_metrics": {
            "scroll_width": 1280,
            "scroll_height": 800,
            "client_width": 1280,
            "client_height": 800
        }
    }))
    .unwrap();
    let dom_sha = sha256(&dom_content);

    let manifest = vec![dom_manifest_entry(&dom_sha, dom_content.len())];
    let files = vec![(format!("{dom_sha}.json"), dom_content)];

    write_capture(&dir, &manifest, &files);
    let set = CaptureSet::load(&dir).expect("load should succeed");
    let findings = checks::run(&set);

    let spacing_fail = findings
        .iter()
        .any(|f| f.check_id == "BRW-TEXT-SPACING-STYLE" && f.state == diligent_assess::State::Fail);
    assert!(
        spacing_fail,
        "letter-spacing: 0 !ImPoRtAnT on a visible element should produce fail for BRW-TEXT-SPACING-STYLE"
    );

    cleanup(&dir);
}

#[test]
fn emitted_rows_validate_against_schemas() {
    let result_schema_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/check-result.schema.json");
    let evidence_schema_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/evidence-record.schema.json");

    let result_schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&result_schema_path).unwrap()).unwrap();
    let evidence_schema: serde_json::Value =
        serde_json::from_str(&std::fs::read_to_string(&evidence_schema_path).unwrap()).unwrap();

    let result_validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .should_validate_formats(true)
        .build(&result_schema)
        .unwrap();
    let evidence_validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .should_validate_formats(true)
        .build(&evidence_schema)
        .unwrap();

    let dir = temp_dir("schema-validation");
    let dom_content = minimal_dom();
    let dom_sha = sha256(&dom_content);

    let manifest = vec![dom_manifest_entry(&dom_sha, dom_content.len())];
    let files = vec![(format!("{dom_sha}.json"), dom_content)];

    write_capture(&dir, &manifest, &files);
    let set = CaptureSet::load(&dir).expect("load should succeed");
    let opts = AssessOptions {
        run_id: Some("run-2026-10-18-abc123".to_string()),
        executed_at: Some("2026-10-18T00:00:00.000Z".to_string()),
        inventory: None,
        no_signals: false,
        corroborate: true,
        component: false,
    };
    let report = assess_set(&set, &opts).expect("assess should succeed");

    for row in &report.results {
        let value = serde_json::to_value(row).unwrap();
        let errors: Vec<_> = result_validator.iter_errors(&value).collect();
        assert!(
            errors.is_empty(),
            "result row {} invalid: {:?}",
            row.result_id,
            errors
        );
    }

    for row in &report.evidence {
        let value = serde_json::to_value(row).unwrap();
        let errors: Vec<_> = evidence_validator.iter_errors(&value).collect();
        assert!(
            errors.is_empty(),
            "evidence row {} invalid: {:?}",
            row.evidence_id,
            errors
        );
    }

    cleanup(&dir);
}

#[test]
fn cli_exit_codes() {
    use diligent_assess::cli::run_cli;

    // An empty document is the home page when its URL is the root. The federal
    // binding checks for search, a privacy link and an accessibility link fail
    // there (genuinely: nothing is rendered), so the exit code is 1.
    // Off the root, the same absences are needs_review, so the exit code is 2.
    for (name, from, to, want) in [
        ("cli-exit-root", None, "", 1u8),
        (
            "cli-exit-inner",
            Some("\"document_url\":\"http://example.com\""),
            "\"document_url\":\"http://example.com/page\"",
            2u8,
        ),
    ] {
        let dir = temp_dir(name);
        let out_dir = temp_dir(&format!("{name}-out"));
        let mut dom_content = minimal_dom();
        if let Some(from) = from {
            assert!(dom_content.contains(from), "minimal_dom shape changed");
            dom_content = dom_content.replace(from, to);
        }
        let dom_sha = sha256(&dom_content);
        let manifest = vec![dom_manifest_entry(&dom_sha, dom_content.len())];
        let files = vec![(format!("{dom_sha}.json"), dom_content)];
        write_capture(&dir, &manifest, &files);
        let args = vec![
            "assess".to_string(),
            "--capture".to_string(),
            dir.to_string_lossy().to_string(),
            "--out".to_string(),
            out_dir.to_string_lossy().to_string(),
        ];
        assert_eq!(run_cli(&args), want, "{name}");
        cleanup(&dir);
        cleanup(&out_dir);
    }

    // Non-existent capture directory should error (exit 3).
    let out_dir2 = temp_dir("cli-exit-codes-out2");
    let args = vec![
        "assess".to_string(),
        "--capture".to_string(),
        "/nonexistent/path/that/does/not/exist".to_string(),
        "--out".to_string(),
        out_dir2.to_string_lossy().to_string(),
    ];
    let exit_code = run_cli(&args);
    assert_eq!(exit_code, 3, "non-existent capture directory should exit 3");

    cleanup(&out_dir2);

    // Missing arguments should error (exit 3).
    let args: Vec<String> = vec![];
    let exit_code = run_cli(&args);
    assert_eq!(exit_code, 3, "missing arguments should exit 3");
}
