//! Tests for the content, craft, form and lab performance signals.
//!
//! One synthetic-capture test per signal, property tests over every signal row,
//! schema validation of `SIGNALS.jsonl` rows, a test that signals never change
//! a result, and checks over the recorded fixtures. Craft cases that name a
//! signal key are compared with their expected state in `fixtures.rs`.

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
// Synthetic captures
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
        "diligent-signals-{name}-{}-{}-{}",
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

fn run(dir: &Path) -> Vec<signals::Signal> {
    signals::run(&CaptureSet::load(dir).expect("capture loads"))
}

fn state_of(dir: &Path, key: &str) -> SignalState {
    run(dir)
        .iter()
        .filter(|s| s.key == key)
        .map(|s| s.state)
        .reduce(SignalState::worst)
        .unwrap_or_else(|| panic!("{key}: no row"))
}

fn signal(dir: &Path, key: &str) -> signals::Signal {
    run(dir)
        .into_iter()
        .find(|s| s.key == key)
        .unwrap_or_else(|| panic!("{key}: no row"))
}

fn page(name: &str, build: impl FnOnce(&mut B, usize)) -> PathBuf {
    let mut b = B::new();
    let main = b.main();
    build(&mut b, main);
    write_capture(name, &[dom_entry(&b)])
}

const PLAIN: &str = "We will help you apply today. ";
const DENSE: &str = "Applications are reviewed by the designated committee, and determinations are communicated to applicants following the completion of the statutory evaluation procedures that are established under the implementing regulations. ";

fn text_page(name: &str, sentence: &str, times: usize) -> PathBuf {
    page(name, |b, main| {
        b.block(main, "p", &sentence.repeat(times), &[]);
    })
}

// ---------------------------------------------------------------------------
// Content
// ---------------------------------------------------------------------------

#[test]
fn fk_grade_is_a_lead_for_dense_text_and_within_practice_for_plain_text() {
    assert_eq!(
        state_of(
            &text_page("fk-dense", DENSE, 4),
            "content.readability.fk_grade"
        ),
        Lead
    );
    assert_eq!(
        state_of(
            &text_page("fk-plain", PLAIN, 25),
            "content.readability.fk_grade"
        ),
        WithinPractice
    );
}

#[test]
fn readability_signals_need_a_hundred_words() {
    let dir = text_page("fk-short", DENSE, 1);
    for key in [
        "content.readability.fk_grade",
        "content.readability.mean_sentence_length",
        "content.readability.passive_share",
    ] {
        assert_eq!(state_of(&dir, key), NotTested, "{key}");
    }
}

#[test]
fn mean_sentence_length_is_a_lead_above_twenty_words() {
    assert_eq!(
        state_of(
            &text_page("msl-dense", DENSE, 4),
            "content.readability.mean_sentence_length"
        ),
        Lead
    );
    assert_eq!(
        state_of(
            &text_page("msl-plain", PLAIN, 25),
            "content.readability.mean_sentence_length"
        ),
        WithinPractice
    );
}

#[test]
fn passive_share_is_a_lead_above_a_fifth() {
    assert_eq!(
        state_of(
            &text_page("passive-dense", DENSE, 4),
            "content.readability.passive_share"
        ),
        Lead
    );
    assert_eq!(
        state_of(
            &text_page("passive-plain", PLAIN, 25),
            "content.readability.passive_share"
        ),
        WithinPractice
    );
}

#[test]
fn undefined_acronyms_are_a_lead_and_defined_ones_are_not() {
    let bad = format!("{}The FCC and the SBA wrote back.", PLAIN.repeat(20));
    let ok = format!(
        "{}The Federal Communications Commission (FCC) wrote back to the FCC office.",
        PLAIN.repeat(20)
    );
    let s = signal(&text_page("acr-bad", &bad, 1), "content.acronyms_undefined");
    assert_eq!(s.state, Lead);
    assert_eq!(s.value, json!(["FCC", "SBA"]));
    assert_eq!(
        state_of(&text_page("acr-ok", &ok, 1), "content.acronyms_undefined"),
        WithinPractice
    );
}

#[test]
fn generic_bare_url_and_empty_links_are_a_lead() {
    let dir = page("links-bad", |b, main| {
        let p = b.el(main, "p", &[], &[]);
        for (href, t) in [
            ("https://example.gov/a", "Click here"),
            ("https://example.gov/b", "https://example.gov/b"),
            ("https://example.gov/c", "Apply for a parking permit"),
        ] {
            let a = b.el(p, "a", &[("href", href)], &[]);
            b.text(a, t);
        }
    });
    let s = signal(&dir, "content.link_text_generic");
    assert_eq!(s.state, Lead);
    assert_eq!(s.value["count"], json!(2));
    assert_eq!(s.value["links"], json!(3));
    let ok = page("links-ok", |b, main| {
        let a = b.el(main, "a", &[("href", "https://example.gov/c")], &[]);
        b.text(a, "Read more about parking permits");
    });
    assert_eq!(state_of(&ok, "content.link_text_generic"), WithinPractice);
}

#[test]
fn heading_density_is_informational() {
    let dir = page("density", |b, main| {
        b.block(main, "h1", "Title", &[]);
        b.block(main, "p", &PLAIN.repeat(10), &[]);
    });
    let s = signal(&dir, "content.heading_density");
    assert_eq!(s.state, Informational);
    assert!(s.value.as_f64().is_some_and(|x| x > 0.0));
}

#[test]
fn empty_main_text_leaves_content_signals_not_tested() {
    let dir = page("empty-main", |_, _| {});
    for info in ALL.iter().filter(|i| i.family == signals::Family::Content) {
        assert_eq!(state_of(&dir, info.key), NotTested, "{}", info.key);
    }
}

#[test]
fn text_outside_main_does_not_count_and_nav_is_left_out_without_main() {
    let mut b = B::new();
    let nav = b.el(b.body, "nav", &[], &[]);
    b.block(nav, "p", &DENSE.repeat(4), &[]);
    let p = b.el(b.body, "p", &[], &[]);
    b.text(p, &PLAIN.repeat(25));
    let dir = write_capture("nav-left-out", &[dom_entry(&b)]);
    assert_eq!(
        state_of(&dir, "content.readability.fk_grade"),
        WithinPractice
    );
}

// ---------------------------------------------------------------------------
// Craft
// ---------------------------------------------------------------------------

#[test]
fn body_font_size_floor_is_a_lead_below_sixteen_px() {
    let long = "This sentence is long enough to count as body text here.";
    let small = page("font-small", |b, m| {
        b.block(m, "p", long, &[("font-size", "14px")]);
    });
    assert_eq!(state_of(&small, "craft.body_font_size_floor"), Lead);
    let ok = page("font-ok", |b, m| {
        b.block(m, "p", long, &[]);
        b.block(m, "p", "tiny", &[("font-size", "10px")]);
    });
    assert_eq!(state_of(&ok, "craft.body_font_size_floor"), WithinPractice);
}

#[test]
fn font_sizes_in_em_or_percent_strings_are_skipped_not_misread() {
    let dir = page("font-em", |b, m| {
        b.block(
            m,
            "p",
            "This sentence is long enough to count as body text here.",
            &[("font-size", "1.2em")],
        );
    });
    assert_eq!(state_of(&dir, "craft.body_font_size_floor"), NotTested);
}

#[test]
fn line_length_is_a_lead_above_eighty_ch() {
    let text = "word ".repeat(30);
    let wide = page("line-wide", |b, m| {
        let p = b.el(m, "p", &[], &[]);
        b.nodes[p]["bounding_box"] = json!({ "x": 0.0, "y": 0.0, "width": 1200.0, "height": 40.0 });
        b.text(p, &text);
    });
    assert_eq!(state_of(&wide, "craft.line_length_ch"), Lead);
    let narrow = page("line-narrow", |b, m| {
        let p = b.el(m, "p", &[], &[]);
        b.nodes[p]["bounding_box"] = json!({ "x": 0.0, "y": 0.0, "width": 560.0, "height": 40.0 });
        b.text(p, &text);
    });
    assert_eq!(state_of(&narrow, "craft.line_length_ch"), WithinPractice);
}

#[test]
fn font_family_count_is_a_lead_above_three() {
    let many = page("fam-many", |b, m| {
        for f in ["Georgia", "Verdana", "Tahoma", "Courier New"] {
            b.block(m, "p", "Text", &[("font-family", f)]);
        }
    });
    assert_eq!(state_of(&many, "craft.font_family_count"), Lead);
    let few = page("fam-few", |b, m| {
        b.block(m, "p", "Text", &[("font-family", "\"Georgia\", serif")]);
        b.block(m, "p", "Text", &[("font-family", "georgia")]);
    });
    let s = signal(&few, "craft.font_family_count");
    assert_eq!(s.state, WithinPractice);
    assert_eq!(s.value["count"], json!(1));
}

#[test]
fn spacing_grid_share_is_a_lead_below_four_fifths() {
    let off = page("grid-off", |b, m| {
        for px in ["5px", "7px", "9px", "13px"] {
            b.block(m, "div", "x", &[("margin-top", px)]);
        }
    });
    let s = signal(&off, "craft.spacing_grid_share");
    assert_eq!(s.state, Lead);
    let on = page("grid-on", |b, m| {
        for px in ["4px", "8px", "12px", "24px", "0px", "calc(8px)"] {
            b.block(
                m,
                "div",
                "x",
                &[("margin-top", px), ("padding-left", "16px")],
            );
        }
    });
    assert_eq!(state_of(&on, "craft.spacing_grid_share"), WithinPractice);
    let none = page("grid-none", |b, m| {
        b.block(m, "div", "x", &[]);
    });
    assert_eq!(state_of(&none, "craft.spacing_grid_share"), NotTested);
}

#[test]
fn border_radius_distinct_counts_non_zero_radii_including_percent() {
    let six = page("radius-six", |b, m| {
        for r in ["2px", "3px", "5px", "7px", "50%", "9999px"] {
            b.block(m, "div", "x", &[("border-top-left-radius", r)]);
        }
    });
    assert_eq!(state_of(&six, "craft.border_radius_distinct"), Lead);
    let two = page("radius-two", |b, m| {
        for r in ["4px", "4px", "0px", "8px"] {
            b.block(m, "div", "x", &[("border-top-left-radius", r)]);
        }
    });
    assert_eq!(
        signal(&two, "craft.border_radius_distinct").value["count"],
        json!(2)
    );
}

#[test]
fn heading_hierarchy_skips_count_jumps_in_document_order() {
    let skip = page("head-skip", |b, m| {
        b.block(m, "h1", "A", &[]);
        b.block(m, "h4", "B", &[]);
        b.block(m, "h2", "C", &[]);
    });
    assert_eq!(
        signal(&skip, "craft.heading_hierarchy_skips").value["count"],
        json!(1)
    );
    assert_eq!(state_of(&skip, "craft.heading_hierarchy_skips"), Lead);
    let ok = page("head-ok", |b, m| {
        b.block(m, "h1", "A", &[]);
        b.block(m, "h2", "B", &[]);
        b.block(m, "h3", "C", &[]);
        b.block(m, "h2", "D", &[]);
    });
    assert_eq!(
        state_of(&ok, "craft.heading_hierarchy_skips"),
        WithinPractice
    );
}

#[test]
fn contrast_margin_flags_pairs_within_ten_percent_above_aa() {
    let fragile = page("contrast-fragile", |b, m| {
        b.block(
            m,
            "p",
            "Text",
            &[
                ("color", "rgb(117, 117, 117)"),
                ("background-color", "rgb(255, 255, 255)"),
            ],
        );
    });
    let s = signal(&fragile, "craft.contrast_margin");
    assert_eq!(s.state, Lead);
    assert!(s.value["min_margin"]
        .as_f64()
        .is_some_and(|x| (1.0..=1.1).contains(&x)));
    let solid = page("contrast-solid", |b, m| {
        b.block(
            m,
            "p",
            "Text",
            &[
                ("color", "rgb(0, 0, 0)"),
                ("background-color", "rgb(255, 255, 255)"),
            ],
        );
    });
    assert_eq!(state_of(&solid, "craft.contrast_margin"), WithinPractice);
    let failing = page("contrast-failing", |b, m| {
        b.block(
            m,
            "p",
            "Text",
            &[
                ("color", "rgb(200, 200, 200)"),
                ("background-color", "rgb(255, 255, 255)"),
            ],
        );
    });
    assert_eq!(
        state_of(&failing, "craft.contrast_margin"),
        NotTested,
        "failures belong to BRW-TEXT-CONTRAST"
    );
}

#[test]
fn reduced_motion_needs_its_own_snapshot() {
    let build = |animated: bool| {
        let mut b = B::new();
        let m = b.main();
        let styles: Vec<(&str, &str)> = if animated {
            vec![("animation-name", "spin")]
        } else {
            vec![("animation-name", "none")]
        };
        b.block(m, "p", "Spinner", &styles);
        b
    };
    let base = build(true);
    let moving = build(true);
    let still = build(false);
    let mk = |b: &B| Entry {
        kind: "dom_snapshot",
        width: 1280,
        height: 800,
        media: "reduced-motion",
        content: b.json(),
    };
    let dir = write_capture("rm-lead", &[dom_entry(&base), mk(&moving)]);
    assert_eq!(state_of(&dir, "craft.reduced_motion_persists"), Lead);
    let dir = write_capture("rm-ok", &[dom_entry(&base), mk(&still)]);
    assert_eq!(
        state_of(&dir, "craft.reduced_motion_persists"),
        WithinPractice
    );
    let dir = write_capture("rm-missing", &[dom_entry(&base)]);
    assert_eq!(state_of(&dir, "craft.reduced_motion_persists"), NotTested);
}

#[test]
fn transitions_with_a_non_zero_duration_count_as_motion() {
    let mut base = B::new();
    let m = base.main();
    base.block(m, "p", "Fade", &[("transition-duration", "0s")]);
    let mut rm = B::new();
    let m = rm.main();
    rm.block(m, "p", "Fade", &[("transition-duration", "0s, 150ms")]);
    let entry = Entry {
        kind: "dom_snapshot",
        width: 1280,
        height: 800,
        media: "reduced-motion",
        content: rm.json(),
    };
    let dir = write_capture("rm-transition", &[dom_entry(&base), entry]);
    assert_eq!(state_of(&dir, "craft.reduced_motion_persists"), Lead);
}

#[test]
fn unnamed_svg_icons_are_counted_informationally() {
    let dir = page("svg", |b, m| {
        b.el(m, "svg", &[], &[]);
        b.el(m, "svg", &[("aria-hidden", "true")], &[]);
        b.el(m, "svg", &[("aria-label", "Home")], &[]);
    });
    let s = signal(&dir, "craft.svg_icons_unnamed");
    assert_eq!(s.state, Informational);
    assert_eq!(s.value["count"], json!(1));
    assert_eq!(s.value["svgs"], json!(3));
}

// ---------------------------------------------------------------------------
// Form and flow
// ---------------------------------------------------------------------------

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

#[test]
fn forms_signals_are_not_tested_without_a_form() {
    let dir = page("no-form", |b, m| {
        b.el(m, "input", &[("type", "text")], &[]);
    });
    for info in ALL.iter().filter(|i| i.family == signals::Family::FormFlow) {
        assert_eq!(state_of(&dir, info.key), NotTested, "{}", info.key);
    }
}

#[test]
fn placeholder_only_labels_are_a_lead() {
    let bad = form_page("ph-bad", |b, f| {
        b.el(
            f,
            "input",
            &[("type", "text"), ("placeholder", "Your name")],
            &[],
        );
    });
    assert_eq!(state_of(&bad, "form.placeholder_only_labels"), Lead);
    let ok = form_page("ph-ok", |b, f| {
        labelled(
            b,
            f,
            "n",
            "Name",
            &[("type", "text"), ("placeholder", "Jane")],
        );
    });
    assert_eq!(
        state_of(&ok, "form.placeholder_only_labels"),
        WithinPractice
    );
}

#[test]
fn identity_inputs_without_autocomplete_are_a_lead() {
    let bad = form_page("ac-bad", |b, f| {
        labelled(b, f, "e", "Email address", &[("type", "email")]);
        labelled(
            b,
            f,
            "z",
            "Postal code",
            &[("type", "text"), ("name", "postal")],
        );
    });
    assert_eq!(
        signal(&bad, "form.autocomplete_missing_identity").value["count"],
        json!(2)
    );
    let ok = form_page("ac-ok", |b, f| {
        labelled(
            b,
            f,
            "e",
            "Email address",
            &[("type", "email"), ("autocomplete", "email")],
        );
        labelled(b, f, "c", "Case number", &[("type", "text")]);
    });
    assert_eq!(
        state_of(&ok, "form.autocomplete_missing_identity"),
        WithinPractice
    );
}

#[test]
fn required_inputs_need_a_visible_marking() {
    let bad = form_page("req-bad", |b, f| {
        labelled(b, f, "a", "Name", &[("type", "text"), ("required", "")]);
    });
    assert_eq!(state_of(&bad, "form.required_marking"), Lead);
    let starred = form_page("req-star", |b, f| {
        labelled(b, f, "a", "Name *", &[("type", "text"), ("required", "")]);
    });
    assert_eq!(state_of(&starred, "form.required_marking"), WithinPractice);
    let page_level = form_page("req-all", |b, f| {
        b.block(f, "p", "All fields are required.", &[]);
        labelled(
            b,
            f,
            "a",
            "Name",
            &[("type", "text"), ("aria-required", "true")],
        );
    });
    assert_eq!(
        state_of(&page_level, "form.required_marking"),
        WithinPractice
    );
}

#[test]
fn an_error_summary_must_link_to_the_invalid_field() {
    let none = form_page("err-none", |b, f| {
        labelled(
            b,
            f,
            "a",
            "Name",
            &[("type", "text"), ("aria-invalid", "true")],
        );
    });
    assert_eq!(state_of(&none, "form.error_summary_pattern"), Lead);
    let ok = page("err-ok", |b, m| {
        let alert = b.el(m, "div", &[("role", "alert")], &[]);
        let link = b.el(alert, "a", &[("href", "#a")], &[]);
        b.text(link, "Enter a name");
        let f = b.el(m, "form", &[], &[]);
        labelled(
            b,
            f,
            "a",
            "Name",
            &[("type", "text"), ("aria-invalid", "true")],
        );
    });
    assert_eq!(state_of(&ok, "form.error_summary_pattern"), WithinPractice);
    let valid = form_page("err-valid", |b, f| {
        labelled(b, f, "a", "Name", &[("type", "text")]);
    });
    assert_eq!(state_of(&valid, "form.error_summary_pattern"), NotTested);
}

#[test]
fn a_long_form_without_steps_is_a_lead() {
    let long = |name: &str, steps: bool| {
        form_page(name, |b, f| {
            if steps {
                b.block(f, "p", "Step 2 of 5", &[]);
            }
            for i in 0..9 {
                labelled(
                    b,
                    f,
                    &format!("f{i}"),
                    &format!("Field {i}"),
                    &[("type", "text")],
                );
            }
        })
    };
    assert_eq!(
        state_of(&long("steps-none", false), "form.step_indicator"),
        Lead
    );
    assert_eq!(
        state_of(&long("steps-text", true), "form.step_indicator"),
        WithinPractice
    );
    let short = form_page("steps-short", |b, f| {
        labelled(b, f, "a", "Name", &[("type", "text")]);
    });
    assert_eq!(state_of(&short, "form.step_indicator"), Informational);
}

#[test]
fn a_single_date_input_is_a_lead_and_three_fields_are_not() {
    let single = form_page("date-single", |b, f| {
        labelled(b, f, "d", "Date of birth", &[("type", "date")]);
    });
    assert_eq!(state_of(&single, "form.memorable_date_inputs"), Lead);
    let three = form_page("date-three", |b, f| {
        labelled(b, f, "d", "Day", &[("type", "text")]);
        labelled(b, f, "m", "Month", &[("type", "text")]);
        labelled(b, f, "y", "Year", &[("type", "text")]);
    });
    assert_eq!(
        state_of(&three, "form.memorable_date_inputs"),
        WithinPractice
    );
    let none = form_page("date-none", |b, f| {
        labelled(b, f, "a", "Name", &[("type", "text")]);
    });
    assert_eq!(state_of(&none, "form.memorable_date_inputs"), NotTested);
}

#[test]
fn file_inputs_need_size_or_format_guidance_nearby() {
    let bad = form_page("file-bad", |b, f| {
        labelled(b, f, "u", "Upload", &[("type", "file")]);
    });
    assert_eq!(state_of(&bad, "form.upload_guidance"), Lead);
    let ok = form_page("file-ok", |b, f| {
        labelled(b, f, "u", "Upload a PDF, up to 10MB", &[("type", "file")]);
    });
    assert_eq!(state_of(&ok, "form.upload_guidance"), WithinPractice);
    let sibling = form_page("file-sibling", |b, f| {
        labelled(b, f, "u", "Upload", &[("type", "file")]);
        b.block(f, "p", "Accepted formats: JPG or PNG.", &[]);
    });
    assert_eq!(state_of(&sibling, "form.upload_guidance"), WithinPractice);
}

#[test]
fn a_form_with_five_hundred_inputs_finishes() {
    let start = std::time::Instant::now();
    let dir = form_page("many-inputs", |b, f| {
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
    let rows = run(&dir);
    assert!(start.elapsed().as_secs() < 20, "took {:?}", start.elapsed());
    assert_eq!(
        rows.iter()
            .find(|s| s.key == "form.step_indicator")
            .unwrap()
            .state,
        Lead
    );
}

// ---------------------------------------------------------------------------
// Lab performance
// ---------------------------------------------------------------------------

fn perf_capture(name: &str, p: Value) -> PathBuf {
    let mut b = B::new();
    b.main();
    write_capture(name, &[dom_entry(&b), perf_entry(p)])
}

fn metrics(over: Value) -> Value {
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

#[test]
fn lcp_reads_the_core_web_vitals_thresholds() {
    for (ms, want) in [
        (1200.0, WithinPractice),
        (2500.0, WithinPractice),
        (2500.1, Informational),
        (4000.0, Informational),
        (4000.1, Lead),
    ] {
        let dir = perf_capture("lcp", metrics(json!({ "lcp_ms": ms })));
        assert_eq!(state_of(&dir, "perf.lcp_ms"), want, "{ms}");
    }
}

#[test]
fn cls_reads_the_core_web_vitals_thresholds() {
    for (cls, want) in [
        (0.0, WithinPractice),
        (0.1, WithinPractice),
        (0.11, Informational),
        (0.25, Informational),
        (0.26, Lead),
    ] {
        let dir = perf_capture("cls", metrics(json!({ "cls": cls })));
        assert_eq!(state_of(&dir, "perf.cls"), want, "{cls}");
    }
}

#[test]
fn the_other_perf_numbers_are_informational() {
    let dir = perf_capture(
        "perf-info",
        metrics(
            json!({ "fcp_ms": 800.0, "ttfb_ms": 50.0, "transfer_bytes": 300000, "resource_count": 4, "long_tasks_ms": 0.0 }),
        ),
    );
    for key in [
        "perf.fcp_ms",
        "perf.ttfb_ms",
        "perf.transfer_bytes",
        "perf.resource_count",
        "perf.long_tasks_ms",
    ] {
        assert_eq!(state_of(&dir, key), Informational, "{key}");
    }
    let s = signal(&dir, "perf.transfer_bytes");
    assert_eq!(s.unit, Some("bytes"));
    assert!(s.limitations.iter().any(|l| l.contains("Lab value")));
}

#[test]
fn inp_is_never_tested_in_the_lab() {
    let dir = perf_capture("inp", metrics(json!({ "lcp_ms": 1.0 })));
    let s = signal(&dir, "perf.inp_ms");
    assert_eq!(s.state, NotTested);
    assert!(s
        .limitations
        .iter()
        .any(|l| l.contains("INP needs real user interaction")));
}

#[test]
fn perf_metrics_of_all_nulls_is_not_tested_throughout() {
    let dir = perf_capture(
        "perf-null",
        metrics(json!({ "notes": ["error: no observer"] })),
    );
    for info in ALL
        .iter()
        .filter(|i| i.family == signals::Family::LabPerformance)
    {
        assert_eq!(state_of(&dir, info.key), NotTested, "{}", info.key);
    }
}

#[test]
fn without_perf_metrics_every_perf_signal_is_not_tested() {
    let dir = page("no-perf", |_, _| {});
    for info in ALL
        .iter()
        .filter(|i| i.family == signals::Family::LabPerformance)
    {
        assert_eq!(state_of(&dir, info.key), NotTested, "{}", info.key);
    }
}

#[test]
fn perf_rows_are_labelled_lab_and_cite_the_perf_capture() {
    let dir = perf_capture("perf-label", metrics(json!({ "lcp_ms": 900.0 })));
    let set = CaptureSet::load(&dir).unwrap();
    let report = assess_set(&set, &opts(false)).unwrap();
    let row = report
        .signals
        .iter()
        .find(|r| r.signal_key == "perf.lcp_ms")
        .unwrap();
    assert_eq!(row.label, "lab");
    assert_eq!(row.family, "lab_performance");
    let evidence = report
        .evidence
        .iter()
        .find(|e| row.evidence_refs.contains(&e.evidence_id) && e.context["label"] == json!("lab"));
    assert!(
        evidence.is_some(),
        "the row cites an evidence record labelled lab"
    );
    for r in &row.evidence_refs {
        assert!(
            report.evidence.iter().any(|e| &e.evidence_id == r),
            "{r} resolves"
        );
    }
}

// ---------------------------------------------------------------------------
// Properties, schema and independence from results
// ---------------------------------------------------------------------------

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

fn validator() -> jsonschema::Validator {
    let text = std::fs::read_to_string(repo_root().join("schemas/signal.schema.json")).unwrap();
    let schema: Value = serde_json::from_str(&text).unwrap();
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .should_validate_formats(true)
        .build(&schema)
        .expect("signal schema compiles")
}

fn sample_dirs() -> Vec<PathBuf> {
    let mut dirs = vec![
        text_page("prop-dense", DENSE, 4),
        text_page("prop-plain", PLAIN, 25),
        perf_capture(
            "prop-perf",
            metrics(json!({ "lcp_ms": 3000.0, "cls": 0.3, "fcp_ms": 1.0 })),
        ),
        form_page("prop-form", |b, f| {
            labelled(b, f, "a", "Email", &[("type", "email"), ("required", "")]);
        }),
        page("prop-empty", |_, _| {}),
    ];
    let captures = repo_root().join("fixtures/a11y/captures");
    if let Ok(read) = std::fs::read_dir(captures) {
        let mut recorded: Vec<PathBuf> = read
            .filter_map(Result::ok)
            .map(|e| e.path())
            .filter(|p| p.is_dir())
            .collect();
        recorded.sort();
        dirs.extend(recorded);
    }
    dirs
}

#[test]
fn every_key_speaks_and_every_row_validates_against_the_schema() {
    let schema = validator();
    let mut problems: Vec<String> = Vec::new();
    let mut checked = 0usize;
    for dir in sample_dirs() {
        let set = match CaptureSet::load(&dir) {
            Ok(s) => s,
            Err(e) => {
                problems.push(format!("{}: {e}", dir.display()));
                continue;
            }
        };
        let report = assess_set(&set, &opts(false)).unwrap();
        for info in ALL.iter() {
            if !report.signals.iter().any(|r| r.signal_key == info.key) {
                problems.push(format!("{}: {} did not speak", dir.display(), info.key));
            }
        }
        let mut ids: Vec<&str> = report
            .signals
            .iter()
            .map(|r| r.signal_id.as_str())
            .collect();
        ids.sort_unstable();
        let n = ids.len();
        ids.dedup();
        if ids.len() != n {
            problems.push(format!("{}: duplicate signal ids", dir.display()));
        }
        for row in &report.signals {
            checked += 1;
            let value = serde_json::to_value(row).unwrap();
            for e in schema.iter_errors(&value) {
                problems.push(format!(
                    "{}: {} invalid: {e}",
                    dir.display(),
                    row.signal_key
                ));
            }
            if SignalState::parse(&row.state).is_none() {
                problems.push(format!(
                    "{}: {} has state `{}`",
                    dir.display(),
                    row.signal_key,
                    row.state
                ));
            }
            if row.normative || row.limitations.is_empty() {
                problems.push(format!(
                    "{}: {} is normative or has no limitation",
                    dir.display(),
                    row.signal_key
                ));
            }
            for r in &row.evidence_refs {
                if !report.evidence.iter().any(|e| &e.evidence_id == r) {
                    problems.push(format!(
                        "{}: {} cites missing evidence {r}",
                        dir.display(),
                        row.signal_key
                    ));
                }
            }
            let text = format!("{} {}", row.observed, row.limitations.join(" ")).to_lowercase();
            if text.contains("required by law") || text.contains("must comply") {
                problems.push(format!(
                    "{}: {} sounds like an obligation",
                    dir.display(),
                    row.signal_key
                ));
            }
        }
    }
    assert!(checked > 0);
    assert!(
        problems.is_empty(),
        "{} problems:\n{}",
        problems.len(),
        problems
            .iter()
            .take(40)
            .cloned()
            .collect::<Vec<_>>()
            .join("\n")
    );
}

#[test]
fn signals_never_change_results() {
    for dir in sample_dirs().into_iter().take(40) {
        let set = CaptureSet::load(&dir).unwrap();
        let with = assess_set(&set, &opts(false)).unwrap();
        let without = assess_set(&set, &opts(true)).unwrap();
        assert_eq!(
            serde_json::to_value(&with.results).unwrap(),
            serde_json::to_value(&without.results).unwrap(),
            "{}",
            dir.display()
        );
        assert!(without.signals.is_empty());
        assert!(without.summary.signals.is_none());
        let s = with.summary.signals.expect("signals summary");
        assert_eq!(
            s.leads + s.within_practice + s.informational + s.not_tested,
            with.signals.len()
        );
        for r in &with.results {
            assert!(
                !r.check_id.contains('.'),
                "a signal key leaked into RESULTS: {}",
                r.check_id
            );
        }
    }
}

#[test]
fn a_craft_lead_does_not_make_the_exit_code_worse() {
    let dir = text_page("exit-code", DENSE, 4);
    let out = std::env::temp_dir().join(format!("diligent-signals-out-{}", std::process::id()));
    let base: Vec<String> = [
        "--capture",
        dir.to_str().unwrap(),
        "--out",
        out.to_str().unwrap(),
    ]
    .iter()
    .map(|s| s.to_string())
    .collect();
    let with = diligent_assess::cli::run_cli(&base);
    assert!(out.join("SIGNALS.jsonl").is_file());
    let mut off = base.clone();
    off.push("--no-signals".to_string());
    let _ = std::fs::remove_file(out.join("SIGNALS.jsonl"));
    let without = diligent_assess::cli::run_cli(&off);
    assert_eq!(with, without);
    assert!(
        !out.join("SIGNALS.jsonl").exists(),
        "--no-signals writes no file"
    );
}

#[test]
fn a_large_text_page_is_read_in_bounded_time() {
    let start = std::time::Instant::now();
    let dir = text_page("one-megabyte", PLAIN, 35_000);
    let _ = run(&dir);
    assert!(start.elapsed().as_secs() < 30, "took {:?}", start.elapsed());
}

#[test]
fn non_english_text_is_read_without_panicking() {
    let dir = text_page(
        "non-english",
        "これは日本語の文章です。 Это русское предложение. ",
        80,
    );
    let rows = run(&dir);
    assert!(rows
        .iter()
        .all(|s| SignalState::parse(s.state.as_str()).is_some()));
}
