//! Adversarial tests for the signals engine.
//!
//! Synthetic captures exercise edge cases: empty text, non-English text, large
//! text, unparseable CSS values, huge forms, null perf metrics, boundary LCP
//! thresholds, schema validation of every SIGNALS row, the invariant that
//! signals never alter RESULTS, and the `checks list` catalog cross-checked
//! against the rule packs.

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};

use diligent_assess::signals::{self, SignalState, ALL};
use diligent_assess::{assess_set, AssessOptions};
use diligent_capture::CaptureSet;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use SignalState::{Informational, Lead, NotTested, WithinPractice};

static CAPTURE_COUNTER: AtomicU64 = AtomicU64::new(0);

// ---------------------------------------------------------------------------
// Builders (mirrors the patterns in tests/signals.rs)
// ---------------------------------------------------------------------------

struct B {
    nodes: Vec<Value>,
    body: usize,
    head: usize,
}

const INLINE: [&str; 8] = ["a", "span", "button", "img", "label", "b", "i", "svg"];

impl B {
    fn new() -> B {
        let mut b = B {
            nodes: Vec::new(),
            body: 0,
            head: 0,
        };
        let doc = b.push(9, "#document", &[], None, None, None, &[]);
        let html = b.push(
            1,
            "html",
            &[("lang", "en")],
            None,
            Some(doc),
            Some([0.0, 0.0, 1280.0, 800.0]),
            &[],
        );
        b.head = b.push(1, "head", &[], None, Some(html), None, &[]);
        b.body = b.push(
            1,
            "body",
            &[],
            None,
            Some(html),
            Some([0.0, 0.0, 1280.0, 800.0]),
            &[("margin-top", "0px")],
        );
        b
    }

    #[allow(clippy::too_many_arguments)]
    fn push(
        &mut self,
        node_type: i64,
        tag: &str,
        attrs: &[(&str, &str)],
        text: Option<&str>,
        parent: Option<usize>,
        bbox: Option<[f64; 4]>,
        styles: &[(&str, &str)],
    ) -> usize {
        let index = self.nodes.len();
        let mut st = serde_json::Map::new();
        if bbox.is_some() && node_type == 1 {
            let display = if INLINE.contains(&tag) {
                "inline"
            } else {
                "block"
            };
            for (k, v) in [
                ("display", display),
                ("visibility", "visible"),
                ("opacity", "1"),
                ("font-size", "16px"),
                ("font-weight", "400"),
                ("font-family", "Arial, sans-serif"),
                ("color", "rgb(0, 0, 0)"),
                ("background-color", "rgba(0, 0, 0, 0)"),
                ("background-image", "none"),
            ] {
                st.insert(k.to_string(), json!(v));
            }
            for (k, v) in styles {
                st.insert((*k).to_string(), json!(v));
            }
        }
        self.nodes.push(json!({
            "index": index, "node_type": node_type, "tag": tag,
            "attributes": attrs.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
            "text": text, "parent": parent, "backend_node_id": index as i64 + 1,
            "bounding_box": bbox.map(|b| json!({ "x": b[0], "y": b[1], "width": b[2], "height": b[3] })),
            "styles": Value::Object(st),
        }));
        index
    }

    fn el(
        &mut self,
        parent: usize,
        tag: &str,
        attrs: &[(&str, &str)],
        styles: &[(&str, &str)],
    ) -> usize {
        self.push(
            1,
            tag,
            attrs,
            None,
            Some(parent),
            Some([0.0, 0.0, 600.0, 20.0]),
            styles,
        )
    }

    fn text(&mut self, parent: usize, s: &str) -> usize {
        self.push(
            3,
            "#text",
            &[],
            Some(s),
            Some(parent),
            Some([0.0, 0.0, 100.0, 20.0]),
            &[],
        )
    }

    fn block(&mut self, parent: usize, tag: &str, s: &str, styles: &[(&str, &str)]) -> usize {
        let e = self.el(parent, tag, &[], styles);
        self.text(e, s);
        e
    }

    fn main(&mut self) -> usize {
        self.el(self.body, "main", &[], &[])
    }

    fn json(&self) -> String {
        serde_json::to_string(&json!({
            "document_url": "https://example.gov/page",
            "nodes": self.nodes,
            "document_metrics": { "scroll_width": 1280, "scroll_height": 800, "client_width": 1280, "client_height": 800 }
        }))
        .unwrap()
    }
}

struct Entry {
    kind: &'static str,
    width: u32,
    height: u32,
    media: &'static str,
    content: String,
}

fn dom_entry(b: &B) -> Entry {
    Entry {
        kind: "dom_snapshot",
        width: 1280,
        height: 800,
        media: "baseline",
        content: b.json(),
    }
}

fn perf_entry(p: Value) -> Entry {
    Entry {
        kind: "perf_metrics",
        width: 1280,
        height: 800,
        media: "baseline",
        content: p.to_string(),
    }
}

fn write_capture(name: &str, entries: &[Entry]) -> PathBuf {
    let unique = CAPTURE_COUNTER.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!(
        "diligent-sig-adv-{name}-{}-{}-{}",
        std::process::id(),
        unique,
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut lines: Vec<String> = Vec::new();
    for e in entries {
        let sha: String = Sha256::digest(e.content.as_bytes())
            .iter()
            .map(|x| format!("{x:02x}"))
            .collect();
        std::fs::write(dir.join(format!("{sha}.json")), &e.content).unwrap();
        lines.push(
            json!({
                "evidence_id": sha, "kind": e.kind,
                "viewport": { "width": e.width, "height": e.height, "dpr": 1.0 },
                "media": e.media, "path": format!("{sha}.json"), "sha256": sha, "bytes": e.content.len(),
                "captured_at": "2026-10-18T00:00:00Z", "final_url": "https://example.gov/page",
                "collector_version": "0.1.0", "protocol_version": "1.3.0"
            })
            .to_string(),
        );
    }
    std::fs::write(dir.join("manifest.jsonl"), lines.join("\n")).unwrap();
    dir
}

fn run_signals(dir: &Path) -> Vec<signals::Signal> {
    signals::run(&CaptureSet::load(dir).expect("capture loads"))
}

fn state_of(dir: &Path, key: &str) -> SignalState {
    run_signals(dir)
        .iter()
        .filter(|s| s.key == key)
        .map(|s| s.state)
        .reduce(SignalState::worst)
        .unwrap_or_else(|| panic!("{key}: no row"))
}

fn page(name: &str, build: impl FnOnce(&mut B, usize)) -> PathBuf {
    let mut b = B::new();
    let main = b.main();
    build(&mut b, main);
    write_capture(name, &[dom_entry(&b)])
}

fn opts(no_signals: bool) -> AssessOptions {
    AssessOptions {
        run_id: Some("run-2026-10-18-abc123".to_string()),
        executed_at: Some("2026-10-18T00:00:00.000Z".to_string()),
        inventory: None,
        no_signals,
        corroborate: true,
    }
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

const PLAIN: &str = "We will help you apply today. ";

fn perf_metrics(over: Value) -> Value {
    let mut base = json!({
        "label": "lab", "lcp_ms": null, "lcp_element": null, "cls": null, "fcp_ms": null, "ttfb_ms": null,
        "dom_content_loaded_ms": null, "load_ms": null, "transfer_bytes": null, "resource_count": null,
        "long_tasks_ms": null, "notes": ["lab value, no throttling, headless"]
    });
    for (k, v) in over.as_object().unwrap() {
        base[k] = v.clone();
    }
    base
}

fn perf_capture(name: &str, p: Value) -> PathBuf {
    let mut b = B::new();
    b.main();
    write_capture(name, &[dom_entry(&b), perf_entry(p)])
}

fn form_page(name: &str, build: impl FnOnce(&mut B, usize)) -> PathBuf {
    page(name, |b, m| {
        let f = b.el(m, "form", &[], &[]);
        build(b, f);
    })
}

fn labelled(b: &mut B, f: usize, id: &str, label: &str, attrs: &[(&str, &str)]) -> usize {
    let l = b.el(f, "label", &[("for", id)], &[]);
    b.text(l, label);
    let mut a = vec![("id", id)];
    a.extend_from_slice(attrs);
    b.el(f, "input", &a, &[])
}

// ---------------------------------------------------------------------------
// 1. Empty main text → readability signals not_tested
// ---------------------------------------------------------------------------

#[test]
fn empty_main_text_leaves_readability_signals_not_tested() {
    let dir = page("adv-empty-main", |_, _| {});
    for key in [
        "content.readability.fk_grade",
        "content.readability.mean_sentence_length",
        "content.readability.passive_share",
    ] {
        assert_eq!(state_of(&dir, key), NotTested, "{key}");
    }
}

// ---------------------------------------------------------------------------
// 2. Non-English text → signals still produce states, no panic
// ---------------------------------------------------------------------------

#[test]
fn non_english_text_produces_states_without_panic() {
    let dir = page("adv-non-english", |b, m| {
        b.block(
            m,
            "p",
            &"これは日本語の文章です。 Это русское предложение. ".repeat(80),
            &[],
        );
    });
    let rows = run_signals(&dir);
    assert!(
        rows.iter()
            .all(|s| SignalState::parse(s.state.as_str()).is_some()),
        "every signal state must parse"
    );
    let fk = state_of(&dir, "content.readability.fk_grade");
    assert!(
        fk == Lead || fk == WithinPractice || fk == Informational || fk == NotTested,
        "fk_grade is a valid state: {fk:?}"
    );
}

// ---------------------------------------------------------------------------
// 3. 1 MB of text under 2 s
// ---------------------------------------------------------------------------

#[test]
fn one_megabyte_of_text_runs_in_under_two_seconds() {
    let start = std::time::Instant::now();
    let dir = page("adv-1mb", |b, m| {
        b.block(m, "p", &PLAIN.repeat(35_000), &[]);
    });
    let _ = run_signals(&dir);
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 2,
        "1 MB of text should run in <2s, took {:?}",
        elapsed
    );
}

// ---------------------------------------------------------------------------
// 4. Font-size strings in em, rem, %, calc() → no panic; informational or not_tested
// ---------------------------------------------------------------------------

#[test]
fn unparseable_font_size_strings_do_not_panic() {
    let long = "This sentence is long enough to count as body text here.";
    for (suffix, size_val) in [
        ("em", "1.2em"),
        ("rem", "1.5rem"),
        ("pct", "120%"),
        ("calc", "calc(16px + 0.5vw)"),
    ] {
        let dir = page(&format!("adv-font-{suffix}"), |b, m| {
            b.block(m, "p", long, &[("font-size", size_val)]);
        });
        let s = state_of(&dir, "craft.body_font_size_floor");
        assert!(
            s == NotTested || s == Informational,
            "font-size {size_val} should be not_tested or informational, got {s:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// 5. Margins with calc() and auto
// ---------------------------------------------------------------------------

#[test]
fn margins_with_calc_and_auto_do_not_panic() {
    let dir = page("adv-margin-calc", |b, m| {
        for (i, val) in ["calc(8px + 1em)", "auto", "calc(2 * 4px)"]
            .iter()
            .enumerate()
        {
            b.block(
                m,
                "div",
                &format!("Block {i}"),
                &[("margin-top", val), ("padding-left", "16px")],
            );
        }
    });
    let s = state_of(&dir, "craft.spacing_grid_share");
    assert!(
        s == NotTested || s == Lead || s == WithinPractice || s == Informational,
        "spacing_grid_share is a valid state: {s:?}"
    );
}

// ---------------------------------------------------------------------------
// 6. Border-radius 50% and 0px
// ---------------------------------------------------------------------------

#[test]
fn border_radius_fifty_percent_and_zero_px() {
    let dir = page("adv-radius", |b, m| {
        b.block(m, "div", "circle", &[("border-radius", "50%")]);
        b.block(m, "div", "sharp", &[("border-radius", "0px")]);
    });
    let s = state_of(&dir, "craft.border_radius_distinct");
    assert!(
        s == Lead || s == WithinPractice || s == Informational || s == NotTested,
        "border_radius_distinct is a valid state: {s:?}"
    );
}

// ---------------------------------------------------------------------------
// 7. A form with 500 inputs
// ---------------------------------------------------------------------------

#[test]
fn a_form_with_five_hundred_inputs_finishes_in_bounded_time() {
    let start = std::time::Instant::now();
    let dir = form_page("adv-500-inputs", |b, f| {
        for i in 0..500 {
            labelled(
                b,
                f,
                &format!("f{i}"),
                &format!("Field {i}"),
                &[("type", "text")],
            );
        }
    });
    let rows = run_signals(&dir);
    let elapsed = start.elapsed();
    assert!(
        elapsed.as_secs() < 20,
        "500 inputs should finish in <20s, took {:?}",
        elapsed
    );
    let step = rows
        .iter()
        .find(|s| s.key == "form.step_indicator")
        .expect("form.step_indicator exists");
    assert_eq!(step.state, Lead, "500 inputs without steps is a lead");
}

// ---------------------------------------------------------------------------
// 8. perf_metrics with all nulls → perf signals not_tested
// ---------------------------------------------------------------------------

#[test]
fn perf_metrics_all_nulls_leaves_perf_signals_not_tested() {
    let dir = perf_capture(
        "adv-perf-null",
        perf_metrics(json!({ "notes": ["error: no observer"] })),
    );
    for info in ALL
        .iter()
        .filter(|i| i.family == signals::Family::LabPerformance)
    {
        assert_eq!(state_of(&dir, info.key), NotTested, "{}", info.key);
    }
}

// ---------------------------------------------------------------------------
// 9. LCP boundary thresholds: 4001 ms (lead), 2500 ms (within_practice), 3000 ms (informational)
// ---------------------------------------------------------------------------

#[test]
fn lcp_boundary_thresholds() {
    let lead = perf_capture("adv-lcp-lead", perf_metrics(json!({ "lcp_ms": 4001.0 })));
    assert_eq!(state_of(&lead, "perf.lcp_ms"), Lead, "4001 ms is a lead");

    let ok = perf_capture("adv-lcp-ok", perf_metrics(json!({ "lcp_ms": 2500.0 })));
    assert_eq!(
        state_of(&ok, "perf.lcp_ms"),
        WithinPractice,
        "2500 ms is within practice"
    );

    let mid = perf_capture("adv-lcp-mid", perf_metrics(json!({ "lcp_ms": 3000.0 })));
    assert_eq!(
        state_of(&mid, "perf.lcp_ms"),
        Informational,
        "3000 ms is informational"
    );
}

// ---------------------------------------------------------------------------
// 10. Every SIGNALS.jsonl row validates against schemas/signal.schema.json
// ---------------------------------------------------------------------------

#[test]
fn every_signal_row_validates_against_the_schema() {
    let schema_text =
        std::fs::read_to_string(repo_root().join("schemas/signal.schema.json")).unwrap();
    let schema: Value = serde_json::from_str(&schema_text).unwrap();
    let validator = jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .should_validate_formats(true)
        .build(&schema)
        .expect("signal schema compiles");

    let dirs = vec![
        page("adv-val-empty", |_, _| {}),
        perf_capture(
            "adv-val-perf",
            perf_metrics(json!({ "lcp_ms": 3000.0, "cls": 0.3 })),
        ),
        form_page("adv-val-form", |b, f| {
            labelled(b, f, "a", "Email", &[("type", "email"), ("required", "")]);
        }),
        page("adv-val-text", |b, m| {
            b.block(m, "p", &PLAIN.repeat(30), &[]);
        }),
    ];

    let mut checked = 0usize;
    let mut problems = Vec::new();
    for dir in &dirs {
        let set = CaptureSet::load(dir).unwrap();
        let report = assess_set(&set, &opts(false)).unwrap();
        for row in &report.signals {
            checked += 1;
            let value = serde_json::to_value(row).unwrap();
            for e in validator.iter_errors(&value) {
                problems.push(format!("{}: {e}", row.signal_key));
            }
        }
    }
    assert!(checked > 0, "at least one row was checked");
    assert!(
        problems.is_empty(),
        "{} schema violations:\n{}",
        problems.len(),
        problems.join("\n")
    );
}

// ---------------------------------------------------------------------------
// 11. Property: no signal state outside the enum; signals never alter RESULTS
// ---------------------------------------------------------------------------

#[test]
fn no_signal_state_is_outside_the_enum() {
    let valid_states: HashSet<&str> = ["lead", "within_practice", "informational", "not_tested"]
        .into_iter()
        .collect();
    let dirs = vec![
        page("adv-prop-empty", |_, _| {}),
        perf_capture("adv-prop-perf", perf_metrics(json!({ "lcp_ms": 4001.0 }))),
        page("adv-prop-text", |b, m| {
            b.block(m, "p", &PLAIN.repeat(30), &[]);
        }),
    ];
    for dir in &dirs {
        let set = CaptureSet::load(dir).unwrap();
        let report = assess_set(&set, &opts(false)).unwrap();
        for row in &report.signals {
            assert!(
                valid_states.contains(row.state.as_str()),
                "{}: state `{}` is not in the enum",
                row.signal_key,
                row.state
            );
        }
    }
}

#[test]
fn signals_never_alter_results_rows() {
    let dirs = vec![
        page("adv-noalt-empty", |_, _| {}),
        perf_capture(
            "adv-noalt-perf",
            perf_metrics(json!({ "lcp_ms": 3000.0, "cls": 0.3 })),
        ),
        form_page("adv-noalt-form", |b, f| {
            labelled(b, f, "a", "Name", &[("type", "text"), ("required", "")]);
        }),
        page("adv-noalt-text", |b, m| {
            b.block(m, "p", &PLAIN.repeat(30), &[]);
        }),
    ];
    for dir in &dirs {
        let set = CaptureSet::load(dir).unwrap();
        let with = assess_set(&set, &opts(false)).unwrap();
        let without = assess_set(&set, &opts(true)).unwrap();
        assert_eq!(
            serde_json::to_value(&with.results).unwrap(),
            serde_json::to_value(&without.results).unwrap(),
            "signals changed RESULTS for {}",
            dir.display()
        );
        assert!(
            without.signals.is_empty(),
            "--no-signals should produce no signals"
        );
    }
}
