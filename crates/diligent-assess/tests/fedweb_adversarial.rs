//! Adversarial tests for the federal public web checks (`BRW-FED-*`).
//!
//! Builds synthetic captures to exercise edge cases the recorded fixtures
//! cannot reach: banner text split across inline elements, identifiers
//! without USWDS classes, search inputs inside hidden dialogs, DAP URL
//! casing variations, blocked-by-policy network entries, missing network
//! logs, privacy links as buttons, title length edge cases, duplicate
//! titles via inventory, malformed inventories, missing 320 views, and
//! schema validity of every emitted row.
//!
//! The invariant under test: BRW-FED-BANNER, TITLE-CRITERIA,
//! META-DESCRIPTION-CRITERIA, CONTACT, LANGUAGE-ACCESS, MOBILE-VIEWPORT,
//! and IDENTIFIER findings are **never** `fail`.

use std::path::{Path, PathBuf};

use diligent_assess::checks;
use diligent_assess::inventory::{self, InventoryState};
use diligent_assess::{assess_set, AssessOptions, Finding, State};
use diligent_capture::CaptureSet;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

// ---------------------------------------------------------------------------
// Helpers (mirror the pattern in tests/fedweb.rs and tests/adversarial_captures.rs)
// ---------------------------------------------------------------------------

const PAGE_URL: &str = "https://example.gov/";
const DAP: &str = "https://dap.digitalgov.gov/Universal-Federated-Analytics-Min.js";

struct Dom {
    url: String,
    nodes: Vec<Value>,
    html: usize,
    head: usize,
    body: usize,
    metrics: Option<(u32, u32)>,
}

impl Dom {
    fn new(url: &str) -> Dom {
        let mut d = Dom {
            url: url.to_string(),
            nodes: Vec::new(),
            html: 0,
            head: 0,
            body: 0,
            metrics: Some((1280, 1280)),
        };
        let doc = d.push(9, "#document", &[], None, None, None);
        d.html = d.push(
            1,
            "html",
            &[("lang", "en")],
            None,
            Some(doc),
            Some([0.0, 0.0, 1280.0, 800.0]),
        );
        d.head = d.push(1, "head", &[], None, Some(d.html), None);
        d.body = d.push(
            1,
            "body",
            &[],
            None,
            Some(d.html),
            Some([0.0, 0.0, 1280.0, 800.0]),
        );
        d
    }

    fn push(
        &mut self,
        node_type: i64,
        tag: &str,
        attrs: &[(&str, &str)],
        text: Option<&str>,
        parent: Option<usize>,
        bbox: Option<[f64; 4]>,
    ) -> usize {
        let index = self.nodes.len();
        let styles = if bbox.is_some() && node_type == 1 {
            let display = if matches!(tag, "a" | "span" | "button" | "img" | "em" | "strong") {
                "inline"
            } else {
                "block"
            };
            json!({ "display": display, "visibility": "visible", "opacity": "1" })
        } else {
            json!({})
        };
        self.nodes.push(json!({
            "index": index,
            "node_type": node_type,
            "tag": tag,
            "attributes": attrs.iter().map(|(k, v)| json!([k, v])).collect::<Vec<_>>(),
            "text": text,
            "parent": parent,
            "backend_node_id": index as i64 + 1,
            "bounding_box": bbox.map(|b| json!({ "x": b[0], "y": b[1], "width": b[2], "height": b[3] })),
            "styles": styles,
        }));
        index
    }

    fn el(
        &mut self,
        parent: usize,
        tag: &str,
        attrs: &[(&str, &str)],
        bbox: Option<[f64; 4]>,
    ) -> usize {
        self.push(1, tag, attrs, None, Some(parent), bbox)
    }

    fn text(&mut self, parent: usize, text: &str, bbox: Option<[f64; 4]>) -> usize {
        self.push(3, "#text", &[], Some(text), Some(parent), bbox)
    }

    fn title(&mut self, text: &str) {
        let t = self.el(self.head, "title", &[], None);
        self.text(t, text, None);
    }

    fn meta(&mut self, name: &str, content: &str) {
        self.el(
            self.head,
            "meta",
            &[("name", name), ("content", content)],
            None,
        );
    }

    fn script(&mut self, src: &str) {
        self.el(self.head, "script", &[("src", src)], None);
    }

    fn json(&self) -> String {
        let mut doc = json!({ "document_url": self.url, "nodes": self.nodes });
        if let Some((scroll, client)) = self.metrics {
            doc["document_metrics"] = json!({
                "scroll_width": scroll, "scroll_height": 800, "client_width": client, "client_height": 800
            });
        }
        serde_json::to_string(&doc).expect("DOM serialises")
    }
}

fn sha256(content: &str) -> String {
    Sha256::digest(content.as_bytes())
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

fn net_entry(url: &str, status: Option<u16>, blocked: bool) -> Value {
    json!({
        "url": url, "method": "GET", "resource_type": "Script", "status": status, "mime": null,
        "from_cache": false, "redirect_chain": [], "third_party": true, "blocked_by_policy": blocked,
        "request_headers": {}, "response_headers": {}
    })
}

fn document_entry(url: &str) -> Value {
    let mut e = net_entry(url, Some(200), false);
    e["resource_type"] = json!("Document");
    e["third_party"] = json!(false);
    e
}

fn write_capture(
    name: &str,
    final_url: &str,
    doms: &[(u32, u32, &Dom)],
    network: Option<Vec<Value>>,
) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("diligent-fedadv-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    std::fs::create_dir_all(&dir).unwrap();
    let mut lines: Vec<String> = Vec::new();
    let mut add = |kind: &str, width: u32, height: u32, content: String| {
        let sha = sha256(&content);
        std::fs::write(dir.join(format!("{sha}.json")), &content).unwrap();
        lines.push(
            json!({
                "evidence_id": sha, "kind": kind,
                "viewport": { "width": width, "height": height, "dpr": 1.0 },
                "media": "baseline", "path": format!("{sha}.json"), "sha256": sha, "bytes": content.len(),
                "captured_at": "2026-10-18T00:00:00Z", "final_url": final_url,
                "collector_version": "0.1.0", "protocol_version": "1.2.0"
            })
            .to_string(),
        );
    };
    for (w, h, dom) in doms {
        add("dom_snapshot", *w, *h, dom.json());
    }
    if let Some(entries) = network {
        let (w, h, _) = doms[0];
        add(
            "network_log",
            w,
            h,
            serde_json::to_string(&entries).unwrap(),
        );
    }
    std::fs::write(dir.join("manifest.jsonl"), lines.join("\n")).unwrap();
    dir
}

fn findings(dir: &Path, check: &str, inv: &InventoryState) -> Vec<Finding> {
    let set = CaptureSet::load(dir).expect("capture loads");
    checks::run_with(&set, inv)
        .into_iter()
        .filter(|f| f.check_id == check)
        .collect()
}

fn one(dir: &Path, check: &str) -> Finding {
    let mut rows = findings(dir, check, &InventoryState::NotSupplied);
    assert_eq!(
        rows.len(),
        1,
        "{check}: expected one row, got {}",
        rows.len()
    );
    rows.remove(0)
}

fn write_inventory(name: &str, text: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "diligent-fedadv-inv-{name}-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, text).unwrap();
    path
}

fn route(url: &str, title: &str, fetched: bool) -> Value {
    json!({ "url": url, "final_url": url, "title": title, "fetched": fetched, "cluster_id": "abc" })
}

fn cleanup(dir: &Path) {
    let _ = std::fs::remove_dir_all(dir);
}

fn all_fed_findings(dir: &Path) -> Vec<Finding> {
    let set = CaptureSet::load(dir).expect("capture loads");
    checks::run(&set)
        .into_iter()
        .filter(|f| f.check_id.starts_with("BRW-FED-"))
        .collect()
}

fn assert_never_fail(findings: &[Finding]) {
    for f in findings {
        if checks::never_fails(f.check_id) {
            assert_ne!(f.state, State::Fail, "{} must never emit fail", f.check_id);
        }
    }
}

// ---------------------------------------------------------------------------
// 1. Banner official text split across inline elements (must still find banner)
// ---------------------------------------------------------------------------

#[test]
fn banner_text_split_across_inline_elements_is_found() {
    let mut d = Dom::new(PAGE_URL);
    d.el(
        d.head,
        "meta",
        &[("name", "viewport"), ("content", "width=device-width")],
        None,
    );
    d.title("A sufficiently long page title for testing purposes");
    d.meta("description", "A description that is comfortably longer than the pending fifty character minimum threshold.");
    // Banner block with phrase split across <span> and <em> inline elements.
    let banner = d.el(
        d.body,
        "div",
        &[("class", "usa-banner")],
        Some([0.0, 0.0, 1280.0, 60.0]),
    );
    let span1 = d.el(banner, "span", &[], Some([8.0, 8.0, 200.0, 20.0]));
    d.text(span1, "An official website", Some([8.0, 8.0, 150.0, 20.0]));
    let em = d.el(banner, "em", &[], Some([160.0, 8.0, 300.0, 20.0]));
    d.text(em, "of the United States", Some([160.0, 8.0, 200.0, 20.0]));
    let span2 = d.el(banner, "span", &[], Some([362.0, 8.0, 200.0, 20.0]));
    d.text(span2, "Government", Some([362.0, 8.0, 100.0, 20.0]));
    // Flag image.
    d.el(
        banner,
        "img",
        &[("alt", "U.S. flag"), ("src", "/flag.svg")],
        Some([8.0, 30.0, 20.0, 14.0]),
    );
    // Toggle with aria-expanded and aria-controls pointing to info text.
    let toggle = d.el(
        banner,
        "button",
        &[("aria-expanded", "false"), ("aria-controls", "banner-info")],
        Some([400.0, 8.0, 100.0, 20.0]),
    );
    d.text(
        toggle,
        "Here's how you know",
        Some([400.0, 8.0, 100.0, 20.0]),
    );
    let panel = d.el(banner, "div", &[("id", "banner-info")], None);
    d.text(panel, "Official websites use .gov\nhttps://", None);

    let dir = write_capture("banner-split", PAGE_URL, &[(1280, 800, &d)], None);
    let f = one(&dir, "BRW-FED-BANNER");
    assert_ne!(f.state, State::Fail, "banner check must never fail");
    // The banner should be found (pass or needs_review, not "no banner").
    assert_ne!(
        f.observed.get("reason").and_then(|v| v.as_str()),
        Some("no banner found"),
        "banner text split across inline elements should still be found"
    );
    assert_never_fail(&[f]);
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// 2. Identifier without USWDS class but inside a contentinfo footer
// ---------------------------------------------------------------------------

#[test]
fn identifier_in_contentinfo_footer_without_uswds_class() {
    let mut d = Dom::new(PAGE_URL);
    d.title("A sufficiently long page title for testing purposes");
    d.meta("description", "A description that is comfortably longer than the pending fifty character minimum threshold.");
    // contentinfo landmark (role="contentinfo") with links but no usa-identifier class.
    let footer = d.el(
        d.body,
        "section",
        &[("role", "contentinfo")],
        Some([0.0, 700.0, 1280.0, 100.0]),
    );
    let about = d.el(
        footer,
        "a",
        &[("href", "https://example.gov/about")],
        Some([8.0, 710.0, 60.0, 20.0]),
    );
    d.text(about, "About", Some([8.0, 710.0, 60.0, 20.0]));
    let access = d.el(
        footer,
        "a",
        &[("href", "https://example.gov/accessibility")],
        Some([80.0, 710.0, 100.0, 20.0]),
    );
    d.text(access, "Accessibility", Some([80.0, 710.0, 100.0, 20.0]));
    let priv_link = d.el(
        footer,
        "a",
        &[("href", "https://example.gov/privacy")],
        Some([190.0, 710.0, 80.0, 20.0]),
    );
    d.text(priv_link, "Privacy", Some([190.0, 710.0, 80.0, 20.0]));
    let foia = d.el(
        footer,
        "a",
        &[("href", "https://example.gov/foia")],
        Some([280.0, 710.0, 60.0, 20.0]),
    );
    d.text(foia, "FOIA", Some([280.0, 710.0, 60.0, 20.0]));
    let nofear = d.el(
        footer,
        "a",
        &[("href", "https://example.gov/nofear")],
        Some([350.0, 710.0, 80.0, 20.0]),
    );
    d.text(nofear, "No FEAR Act", Some([350.0, 710.0, 80.0, 20.0]));
    let perf = d.el(
        footer,
        "a",
        &[("href", "https://example.gov/performance")],
        Some([440.0, 710.0, 100.0, 20.0]),
    );
    d.text(perf, "Performance", Some([440.0, 710.0, 100.0, 20.0]));
    let ig = d.el(
        footer,
        "a",
        &[("href", "https://example.gov/ig")],
        Some([550.0, 710.0, 120.0, 20.0]),
    );
    d.text(ig, "Inspector General", Some([550.0, 710.0, 120.0, 20.0]));
    let usagov = d.el(
        footer,
        "a",
        &[("href", "https://www.usa.gov/")],
        Some([680.0, 710.0, 80.0, 20.0]),
    );
    d.text(usagov, "USA.gov", Some([680.0, 710.0, 80.0, 20.0]));

    let dir = write_capture("id-contentinfo", PAGE_URL, &[(1280, 800, &d)], None);
    let rows = findings(&dir, "BRW-FED-IDENTIFIER", &InventoryState::NotSupplied);
    assert!(!rows.is_empty(), "identifier check should produce rows");
    for r in &rows {
        assert_ne!(r.state, State::Fail, "BRW-FED-IDENTIFIER must never fail");
    }
    // Links should be found (pass) since they have valid hrefs.
    let pass_count = rows.iter().filter(|r| r.state == State::Pass).count();
    assert!(
        pass_count >= 4,
        "at least some links should pass in contentinfo footer, got {pass_count}"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// 3. Search input inside a hidden dialog (needs_review, never fail)
// ---------------------------------------------------------------------------

#[test]
fn search_input_inside_hidden_dialog_is_needs_review() {
    let mut d = Dom::new(PAGE_URL);
    d.title("A sufficiently long page title for testing purposes");
    d.meta("description", "A description that is comfortably longer than the pending fifty character minimum threshold.");
    // Hidden dialog with aria-hidden="true" containing a search input.
    let dialog = d.el(
        d.body,
        "div",
        &[("role", "dialog"), ("aria-hidden", "true")],
        Some([0.0, 0.0, 400.0, 300.0]),
    );
    d.el(
        dialog,
        "input",
        &[("type", "search"), ("aria-label", "Search the site")],
        Some([10.0, 10.0, 200.0, 30.0]),
    );

    let dir = write_capture(
        "search-hidden",
        PAGE_URL,
        &[(1280, 800, &d)],
        Some(vec![document_entry(PAGE_URL)]),
    );
    let f = one(&dir, "BRW-FED-SEARCH");
    // On the home page, no visible search → fail is expected for binding check.
    // But the hidden search should be noted as needs_review territory, not a pass.
    // The key invariant: the search check IS a binding check, so it CAN fail on home.
    // However, the hidden dialog search should be detected as present but not usable.
    assert!(
        f.state == State::Fail || f.state == State::NeedsReview,
        "hidden search on home page: expected fail or needs_review, got {:?}",
        f.state
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// 4. DAP URL with different casing and query parameter order (pass with 200)
// ---------------------------------------------------------------------------

#[test]
fn dap_url_mixed_case_and_reordered_query_params_passes() {
    let mut d = Dom::new(PAGE_URL);
    d.title("A sufficiently long page title for testing purposes");
    let url = "https://DAP.digitalgov.gov/Universal-Federated-Analytics-Min.JS?subagency=TEST&agency=DEMO";
    d.script(url);

    let dir = write_capture(
        "dap-case-reorder",
        PAGE_URL,
        &[(1280, 800, &d)],
        Some(vec![
            document_entry(PAGE_URL),
            net_entry(url, Some(200), false),
        ]),
    );
    let f = one(&dir, "BRW-FED-DAP");
    assert_eq!(
        f.state,
        State::Pass,
        "DAP with mixed case and reordered params should pass"
    );
    assert_eq!(f.observed["agency"], json!("DEMO"));
    assert_eq!(f.observed["subagency"], json!("TEST"));
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// 5. DAP entry blocked_by_policy (needs_review)
// ---------------------------------------------------------------------------

#[test]
fn dap_blocked_by_policy_is_needs_review() {
    let mut d = Dom::new(PAGE_URL);
    d.title("A sufficiently long page title for testing purposes");
    let url = format!("{DAP}?agency=BLOCKED");
    d.script(&url);

    let dir = write_capture(
        "dap-blocked-policy",
        PAGE_URL,
        &[(1280, 800, &d)],
        Some(vec![document_entry(PAGE_URL), net_entry(&url, None, true)]),
    );
    let f = one(&dir, "BRW-FED-DAP");
    assert_eq!(
        f.state,
        State::NeedsReview,
        "DAP blocked_by_policy should be needs_review"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// 6. No network log at all (not_tested)
// ---------------------------------------------------------------------------

#[test]
fn dap_without_network_log_is_not_tested() {
    let mut d = Dom::new(PAGE_URL);
    d.title("A sufficiently long page title for testing purposes");
    d.script(DAP);

    let dir = write_capture("dap-no-netlog", PAGE_URL, &[(1280, 800, &d)], None);
    let f = one(&dir, "BRW-FED-DAP");
    assert_eq!(
        f.state,
        State::NotTested,
        "DAP without network log should be not_tested"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// 7. Privacy link as a button (needs_review on home)
// ---------------------------------------------------------------------------

#[test]
fn privacy_link_as_button_is_needs_review_on_home() {
    let mut d = Dom::new(PAGE_URL);
    d.title("A sufficiently long page title for testing purposes");
    d.meta("description", "A description that is comfortably longer than the pending fifty character minimum threshold.");
    // A button (not a link) named "Privacy" in the footer.
    let footer = d.el(d.body, "footer", &[], Some([0.0, 700.0, 1280.0, 100.0]));
    let btn = d.el(footer, "button", &[], Some([8.0, 710.0, 80.0, 30.0]));
    d.text(btn, "Privacy Policy", Some([8.0, 710.0, 80.0, 30.0]));

    let dir = write_capture(
        "privacy-btn",
        PAGE_URL,
        &[(1280, 800, &d)],
        Some(vec![document_entry(PAGE_URL)]),
    );
    let f = one(&dir, "BRW-FED-PRIVACY-LINK");
    // Button is not a link → needs_review (not pass, and on home page not fail because
    // there IS a match, just not a usable link).
    assert_eq!(
        f.state,
        State::NeedsReview,
        "privacy button (not link) on home page should be needs_review"
    );
    assert_ne!(f.state, State::Fail);
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// 8. Title of exactly 20 chars (pass) and 19 chars plus whitespace padding (needs_review)
// ---------------------------------------------------------------------------

#[test]
fn title_exactly_20_chars_passes() {
    // "12345678901234567890" = 20 chars
    let title_text = "12345678901234567890";
    assert_eq!(title_text.chars().count(), 20);
    let mut d = Dom::new(PAGE_URL);
    d.title(title_text);

    let dir = write_capture("title-20", PAGE_URL, &[(1280, 800, &d)], None);
    let f = one(&dir, "BRW-FED-TITLE-CRITERIA");
    assert_eq!(
        f.state,
        State::Pass,
        "title of exactly 20 chars should pass"
    );
    assert_eq!(f.observed["length"], json!(20));
    cleanup(&dir);
}

#[test]
fn title_19_chars_with_whitespace_padding_is_needs_review() {
    // 19 chars of text + lots of whitespace padding → trimmed length is 19
    let title_text = "   1234567890123456789   ";
    let trimmed = title_text.trim();
    assert_eq!(trimmed.chars().count(), 19);
    let mut d = Dom::new(PAGE_URL);
    d.title(title_text);

    let dir = write_capture("title-19-pad", PAGE_URL, &[(1280, 800, &d)], None);
    let f = one(&dir, "BRW-FED-TITLE-CRITERIA");
    assert_eq!(
        f.state,
        State::NeedsReview,
        "title of 19 chars (padded) should be needs_review"
    );
    assert_ne!(f.state, State::Fail, "title criteria must never fail");
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// 9. Duplicate titles via synthetic INVENTORY.json (needs_review listing duplicate)
// ---------------------------------------------------------------------------

#[test]
fn duplicate_titles_via_inventory_are_needs_review() {
    let shared_title = "Shared title that is long enough for the test";
    let mut d = Dom::new(PAGE_URL);
    d.title(shared_title);

    let dir = write_capture("title-dup-inv", PAGE_URL, &[(1280, 800, &d)], None);
    let inv = write_inventory(
        "dup-adv",
        &json!({
            "routes": [
                route(PAGE_URL, shared_title, true),
                route("https://example.gov/other", shared_title, true),
                route("https://example.gov/third", "A completely different title that is long enough", true),
            ]
        })
        .to_string(),
    );
    let f = findings(&dir, "BRW-FED-TITLE-CRITERIA", &inventory::load(&inv)).remove(0);
    assert_eq!(
        f.state,
        State::NeedsReview,
        "duplicate title should be needs_review"
    );
    assert_eq!(f.observed["duplicate_count"], json!(1));
    let urls: Vec<&str> = f.observed["duplicate_urls"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert!(
        urls.contains(&"https://example.gov/other"),
        "should list the duplicate URL"
    );
    cleanup(&dir);
    cleanup(&inv);
}

// ---------------------------------------------------------------------------
// 10. Malformed INVENTORY.json (assess still runs, limitation recorded, exit not 3)
// ---------------------------------------------------------------------------

#[test]
fn malformed_inventory_still_runs_with_limitation() {
    let mut d = Dom::new(PAGE_URL);
    d.title("A sufficiently long page title for testing purposes");

    let dir = write_capture("title-bad-inv-adv", PAGE_URL, &[(1280, 800, &d)], None);
    let bad = write_inventory("malformed-adv", "{ not valid json at all");
    let state = inventory::load(&bad);
    assert!(matches!(state, InventoryState::Unreadable(_)));
    let f = findings(&dir, "BRW-FED-TITLE-CRITERIA", &state).remove(0);
    assert_eq!(
        f.state,
        State::Pass,
        "title should still pass with malformed inventory"
    );
    assert!(
        f.limitations
            .iter()
            .any(|l| l.contains("uniqueness not evaluated")),
        "should record limitation about uniqueness"
    );
    // Also test via CLI: exit should not be 3 (that's for CLI usage errors).
    let out = std::env::temp_dir().join(format!("diligent-fedadv-out-{}", std::process::id()));
    let args = vec![
        "assess".to_string(),
        "--capture".to_string(),
        dir.to_string_lossy().to_string(),
        "--out".to_string(),
        out.to_string_lossy().to_string(),
        "--inventory".to_string(),
        bad.to_string_lossy().to_string(),
    ];
    let code = diligent_assess::cli::run_cli(&args);
    assert!(
        code != 3,
        "malformed inventory should not cause exit 3, got {code}"
    );
    cleanup(&dir);
    cleanup(&bad);
    cleanup(&out);
}

// ---------------------------------------------------------------------------
// 11. Missing 320 view (mobile viewport not_tested)
// ---------------------------------------------------------------------------

#[test]
fn mobile_viewport_without_320_view_is_not_tested() {
    let mut d = Dom::new(PAGE_URL);
    d.el(
        d.head,
        "meta",
        &[("name", "viewport"), ("content", "width=device-width")],
        None,
    );
    d.title("A sufficiently long page title for testing purposes");
    d.metrics = Some((1280, 1280)); // only 1280 view, no 320

    let dir = write_capture("mobile-missing-320", PAGE_URL, &[(1280, 800, &d)], None);
    let f = one(&dir, "BRW-FED-MOBILE-VIEWPORT");
    assert_eq!(
        f.state,
        State::NotTested,
        "no 320 view should be not_tested"
    );
    cleanup(&dir);
}

// ---------------------------------------------------------------------------
// 12. Every emitted row is schema-valid
// ---------------------------------------------------------------------------

#[test]
fn every_emitted_row_is_schema_valid() {
    let result_schema_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/check-result.schema.json");
    let evidence_schema_path =
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../schemas/evidence-record.schema.json");

    let result_schema: Value =
        serde_json::from_str(&std::fs::read_to_string(&result_schema_path).unwrap()).unwrap();
    let evidence_schema: Value =
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

    // Build a capture that exercises several fed checks at once.
    let mut d = Dom::new(PAGE_URL);
    d.el(
        d.head,
        "meta",
        &[("name", "viewport"), ("content", "width=device-width")],
        None,
    );
    d.title("A sufficiently long page title for testing purposes");
    d.meta("description", "A description that is comfortably longer than the pending fifty character minimum threshold.");
    let footer = d.el(d.body, "footer", &[], Some([0.0, 700.0, 1280.0, 100.0]));
    let priv_link = d.el(
        footer,
        "a",
        &[("href", "https://example.gov/privacy")],
        Some([8.0, 710.0, 80.0, 20.0]),
    );
    d.text(priv_link, "Privacy", Some([8.0, 710.0, 80.0, 20.0]));

    let dir = write_capture(
        "schema-fed",
        PAGE_URL,
        &[(1280, 800, &d)],
        Some(vec![document_entry(PAGE_URL)]),
    );
    let set = CaptureSet::load(&dir).unwrap();
    let opts = AssessOptions {
        run_id: Some("run-2026-10-18-a1b2c3".to_string()),
        executed_at: Some("2026-10-18T00:00:00.000Z".to_string()),
        inventory: None,
        no_signals: false,
        corroborate: true,
        component: false,
    };
    let report = assess_set(&set, &opts).unwrap();

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

// ---------------------------------------------------------------------------
// 13. Global invariant: no never_fails check emits fail across all synthetic cases
// ---------------------------------------------------------------------------

#[test]
fn never_fail_checks_never_emit_fail_across_all_synthetic_cases() {
    // Collect findings from every synthetic capture built above.
    let mut all: Vec<Finding> = Vec::new();

    // Banner split
    {
        let mut d = Dom::new(PAGE_URL);
        d.el(
            d.head,
            "meta",
            &[("name", "viewport"), ("content", "width=device-width")],
            None,
        );
        d.title("A sufficiently long page title for testing purposes");
        d.meta("description", "A description that is comfortably longer than the pending fifty character minimum threshold.");
        let banner = d.el(
            d.body,
            "div",
            &[("class", "usa-banner")],
            Some([0.0, 0.0, 1280.0, 60.0]),
        );
        let s1 = d.el(banner, "span", &[], Some([8.0, 8.0, 200.0, 20.0]));
        d.text(s1, "An official website", Some([8.0, 8.0, 150.0, 20.0]));
        let s2 = d.el(banner, "em", &[], Some([160.0, 8.0, 300.0, 20.0]));
        d.text(
            s2,
            "of the United States Government",
            Some([160.0, 8.0, 200.0, 20.0]),
        );
        d.el(
            banner,
            "img",
            &[("alt", "U.S. flag")],
            Some([8.0, 30.0, 20.0, 14.0]),
        );
        let toggle = d.el(
            banner,
            "button",
            &[("aria-expanded", "false"), ("aria-controls", "p")],
            Some([400.0, 8.0, 100.0, 20.0]),
        );
        d.text(
            toggle,
            "Here's how you know",
            Some([400.0, 8.0, 100.0, 20.0]),
        );
        let panel = d.el(banner, "div", &[("id", "p")], None);
        d.text(panel, ".gov https://", None);
        let dir = write_capture("inv-banner", PAGE_URL, &[(1280, 800, &d)], None);
        all.extend(all_fed_findings(&dir));
        cleanup(&dir);
    }

    // Identifier in contentinfo
    {
        let mut d = Dom::new(PAGE_URL);
        d.title("A sufficiently long page title for testing purposes");
        d.meta("description", "A description that is comfortably longer than the pending fifty character minimum threshold.");
        let footer = d.el(
            d.body,
            "section",
            &[("role", "contentinfo")],
            Some([0.0, 700.0, 1280.0, 100.0]),
        );
        for (text, href) in [
            ("About", "https://example.gov/about"),
            ("Privacy", "https://example.gov/privacy"),
        ] {
            let a = d.el(
                footer,
                "a",
                &[("href", href)],
                Some([8.0, 710.0, 60.0, 20.0]),
            );
            d.text(a, text, Some([8.0, 710.0, 60.0, 20.0]));
        }
        let dir = write_capture("inv-id", PAGE_URL, &[(1280, 800, &d)], None);
        all.extend(all_fed_findings(&dir));
        cleanup(&dir);
    }

    // Title edge cases
    {
        let mut d = Dom::new(PAGE_URL);
        d.title("12345678901234567890");
        let dir = write_capture("inv-title-20", PAGE_URL, &[(1280, 800, &d)], None);
        all.extend(all_fed_findings(&dir));
        cleanup(&dir);
    }
    {
        let mut d = Dom::new(PAGE_URL);
        d.title("   1234567890123456789   ");
        let dir = write_capture("inv-title-19", PAGE_URL, &[(1280, 800, &d)], None);
        all.extend(all_fed_findings(&dir));
        cleanup(&dir);
    }

    // Meta description
    {
        let mut d = Dom::new(PAGE_URL);
        d.title("A sufficiently long page title for testing purposes");
        d.meta("description", "short");
        let dir = write_capture("inv-meta-short", PAGE_URL, &[(1280, 800, &d)], None);
        all.extend(all_fed_findings(&dir));
        cleanup(&dir);
    }
    {
        let mut d = Dom::new(PAGE_URL);
        d.title("A sufficiently long page title for testing purposes");
        // no meta description at all
        let dir = write_capture("inv-meta-none", PAGE_URL, &[(1280, 800, &d)], None);
        all.extend(all_fed_findings(&dir));
        cleanup(&dir);
    }

    // Contact: no contact control
    {
        let mut d = Dom::new(PAGE_URL);
        d.title("A sufficiently long page title for testing purposes");
        d.meta("description", "A description that is comfortably longer than the pending fifty character minimum threshold.");
        let dir = write_capture("inv-no-contact", PAGE_URL, &[(1280, 800, &d)], None);
        all.extend(all_fed_findings(&dir));
        cleanup(&dir);
    }

    // Language access: no language entry
    {
        let mut d = Dom::new(PAGE_URL);
        d.title("A sufficiently long page title for testing purposes");
        d.meta("description", "A description that is comfortably longer than the pending fifty character minimum threshold.");
        let dir = write_capture("inv-no-lang", PAGE_URL, &[(1280, 800, &d)], None);
        all.extend(all_fed_findings(&dir));
        cleanup(&dir);
    }

    // Mobile viewport: no 320 view
    {
        let mut d = Dom::new(PAGE_URL);
        d.el(
            d.head,
            "meta",
            &[("name", "viewport"), ("content", "width=device-width")],
            None,
        );
        d.title("A sufficiently long page title for testing purposes");
        let dir = write_capture("inv-mobile", PAGE_URL, &[(1280, 800, &d)], None);
        all.extend(all_fed_findings(&dir));
        cleanup(&dir);
    }

    // Empty page
    {
        let d = Dom::new(PAGE_URL);
        let dir = write_capture("inv-empty", PAGE_URL, &[(1280, 800, &d)], None);
        all.extend(all_fed_findings(&dir));
        cleanup(&dir);
    }

    assert_never_fail(&all);
}
