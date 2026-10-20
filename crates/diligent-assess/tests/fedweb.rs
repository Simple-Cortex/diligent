//! Tests for the federal public web checks (`BRW-FED-*`).
//!
//! Two kinds of test live here.
//!
//! * Fixture tests read the recorded captures of the authored `BRW-FED-*`
//!   cases and assert properties across all of them: pending-standard and
//!   presence-only checks never fail, binding checks never fail a page the
//!   manifest does not expect to fail, and the cases that need the home page
//!   were recorded with the page path rewritten to `/`.
//! * Synthetic tests build small captures in a temporary directory for the
//!   states a recorded page cannot reach: a DAP script that loaded with status
//!   200, an inventory with duplicate titles, a malformed inventory, and a
//!   missing 320 CSS px view.

use std::path::{Path, PathBuf};

use diligent_assess::checks;
use diligent_assess::inventory::{self, InventoryState};
use diligent_assess::{assess_set, AssessOptions, Finding, State};
use diligent_capture::CaptureSet;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

const PENDING: [&str; 3] = [
    "BRW-FED-BANNER",
    "BRW-FED-TITLE-CRITERIA",
    "BRW-FED-META-DESCRIPTION-CRITERIA",
];
const BINDING: [&str; 4] = [
    "BRW-FED-SEARCH",
    "BRW-FED-DAP",
    "BRW-FED-PRIVACY-LINK",
    "BRW-FED-ACCESSIBILITY-STATEMENT",
];

// ---------------------------------------------------------------------------
// Recorded fixtures
// ---------------------------------------------------------------------------

struct Case {
    id: String,
    check_id: String,
    expected: State,
    serve_as: Option<String>,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fed_cases() -> Vec<Case> {
    let path = repo_root().join("fixtures/a11y/authored/manifest.json");
    let doc: Value =
        serde_json::from_str(&std::fs::read_to_string(&path).expect("manifest is readable"))
            .expect("manifest is JSON");
    doc["cases"]
        .as_array()
        .expect("manifest has cases")
        .iter()
        .filter(|c| {
            c["check_id"]
                .as_str()
                .is_some_and(|id| id.starts_with("BRW-FED-"))
        })
        .map(|c| Case {
            id: c["case_id"].as_str().expect("case_id").to_string(),
            check_id: c["check_id"].as_str().expect("check_id").to_string(),
            expected: State::parse(c["expected"].as_str().expect("expected"))
                .expect("a known state"),
            serve_as: c["serve_as"].as_str().map(str::to_string),
        })
        .collect()
}

fn recorded(case: &Case) -> Result<CaptureSet, String> {
    let dir = repo_root().join("fixtures/a11y/captures").join(&case.id);
    if !dir.is_dir() {
        return Err(format!(
            "{}: no recorded capture at {}",
            case.id,
            dir.display()
        ));
    }
    CaptureSet::load(&dir).map_err(|e| format!("{}: {e}", case.id))
}

#[test]
fn the_authored_manifest_has_federal_cases_for_every_federal_check() {
    let cases = fed_cases();
    for info in checks::all()
        .iter()
        .filter(|i| i.id.starts_with("BRW-FED-"))
    {
        assert!(
            cases.iter().any(|c| c.check_id == info.id),
            "{}: no authored fixture case",
            info.id
        );
    }
}

#[test]
fn pending_and_presence_only_checks_never_fail_over_every_fixture() {
    let mut problems: Vec<String> = Vec::new();
    for case in fed_cases() {
        let set = match recorded(&case) {
            Ok(s) => s,
            Err(why) => {
                problems.push(why);
                continue;
            }
        };
        for f in checks::run(&set) {
            if checks::never_fails(f.check_id) && f.state == State::Fail {
                problems.push(format!(
                    "{}: {} emitted fail on {}",
                    case.id, f.check_id, f.instance
                ));
            }
            // Any check that informs a pending rule must stay out of fail, whatever its id.
            let pending_rule = f
                .wcag
                .iter()
                .any(|r| matches!(*r, "FED-32" | "FED-33" | "FED-34"));
            if (pending_rule || PENDING.contains(&f.check_id)) && f.state == State::Fail {
                problems.push(format!(
                    "{}: {} informs a pending rule and emitted fail",
                    case.id, f.check_id
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn binding_checks_have_precision_one_over_their_fixtures() {
    let mut problems: Vec<String> = Vec::new();
    let mut fails = 0usize;
    for case in fed_cases()
        .into_iter()
        .filter(|c| BINDING.contains(&c.check_id.as_str()))
    {
        let set = match recorded(&case) {
            Ok(s) => s,
            Err(why) => {
                problems.push(why);
                continue;
            }
        };
        let worst = checks::run(&set)
            .iter()
            .filter(|f| f.check_id == case.check_id)
            .fold(None::<State>, |w, f| {
                Some(w.map_or(f.state, |w| w.worst(f.state)))
            });
        if worst == Some(State::Fail) {
            fails += 1;
            if case.expected != State::Fail {
                problems.push(format!(
                    "{}: {} failed a page the manifest expects {}",
                    case.id,
                    case.check_id,
                    case.expected.as_str()
                ));
            }
        }
        if case.expected == State::Fail && worst != Some(State::Fail) {
            problems.push(format!(
                "{}: {} missed an expected fail",
                case.id, case.check_id
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
    assert!(
        fails > 0 || !problems.is_empty(),
        "no binding fixture produced a fail"
    );
}

#[test]
fn home_page_cases_are_served_as_root_and_recorded_at_root() {
    // Fixtures that need the home page say so with `serve_as: "/"` in the
    // manifest; the recorder then serves that file at `/`. If the recorded
    // document_url path is not `/`, the home-page fail states are unreachable.
    let mut problems: Vec<String> = Vec::new();
    let cases = fed_cases();
    for case in &cases {
        let home_named = case.id.contains("--home-");
        if home_named != case.serve_as.is_some() {
            problems.push(format!(
                "{}: serve_as {:?} does not match its name",
                case.id, case.serve_as
            ));
        }
        if case.serve_as.as_deref().is_some_and(|p| p != "/") {
            problems.push(format!("{}: serve_as must be \"/\"", case.id));
        }
    }
    for case in cases.iter().filter(|c| c.serve_as.is_some()) {
        let set = match recorded(case) {
            Ok(s) => s,
            Err(why) => {
                problems.push(why);
                continue;
            }
        };
        let url = set
            .viewports()
            .iter()
            .find_map(|v| set.dom(*v, "baseline"))
            .map(|d| d.document_url.clone())
            .unwrap_or_default();
        let path = url::Url::parse(&url)
            .map(|u| u.path().to_string())
            .unwrap_or_default();
        if path != "/" {
            problems.push(format!(
                "{}: document_url is {url}, expected path /",
                case.id
            ));
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn an_identifier_less_page_reports_fed_13_for_review_and_the_rest_not_tested() {
    let case = Case {
        id: "brw-fed-identifier--no-identifier-or-footer-needs-review".to_string(),
        check_id: "BRW-FED-IDENTIFIER".to_string(),
        expected: State::NeedsReview,
        serve_as: None,
    };
    let set = recorded(&case).expect("recorded capture");
    let rows: Vec<Finding> = checks::run(&set)
        .into_iter()
        .filter(|f| f.check_id == case.check_id)
        .collect();
    assert_eq!(rows.len(), 9, "one FED-13 row and eight not_tested rows");
    assert_eq!(rows[0].wcag.first().copied(), Some("FED-13"));
    assert_eq!(rows[0].state, State::NeedsReview);
    for r in &rows[1..] {
        assert_eq!(r.state, State::NotTested, "{:?}", r.wcag);
    }
}

// ---------------------------------------------------------------------------
// Synthetic captures
// ---------------------------------------------------------------------------

/// A small DOM built node by node.
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
            let display = if matches!(tag, "a" | "span" | "button" | "img") {
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

fn net(url: &str, status: Option<u16>, blocked: bool) -> Value {
    json!({
        "url": url, "method": "GET", "resource_type": "Script", "status": status, "mime": null,
        "from_cache": false, "redirect_chain": [], "third_party": true, "blocked_by_policy": blocked,
        "request_headers": {}, "response_headers": {}
    })
}

fn document_entry(url: &str) -> Value {
    let mut e = net(url, Some(200), false);
    e["resource_type"] = json!("Document");
    e["third_party"] = json!(false);
    e
}

/// Writes a capture directory and returns it. `doms` are `(width, height, dom)`.
fn write_capture(
    name: &str,
    final_url: &str,
    doms: &[(u32, u32, &Dom)],
    network: Option<Vec<Value>>,
) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("diligent-fedweb-{name}-{}", std::process::id()));
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

fn page(url: &str) -> Dom {
    let mut d = Dom::new(url);
    d.el(
        d.head,
        "meta",
        &[
            ("name", "viewport"),
            ("content", "width=device-width, initial-scale=1"),
        ],
        None,
    );
    d.title("A reasonably long page title for tests");
    d.meta(
        "description",
        "A description that is comfortably longer than the pending fifty character minimum.",
    );
    let h1 = d.el(d.body, "h1", &[], Some([8.0, 8.0, 400.0, 30.0]));
    d.text(h1, "Heading", Some([8.0, 8.0, 100.0, 30.0]));
    d
}

fn findings_of(dir: &Path, check: &str, inv: &InventoryState) -> Vec<Finding> {
    let set = CaptureSet::load(dir).expect("capture loads");
    checks::run_with(&set, inv)
        .into_iter()
        .filter(|f| f.check_id == check)
        .collect()
}

fn one(dir: &Path, check: &str) -> Finding {
    let mut rows = findings_of(dir, check, &InventoryState::NotSupplied);
    assert_eq!(
        rows.len(),
        1,
        "{check}: expected one row, got {}",
        rows.len()
    );
    rows.remove(0)
}

const DAP: &str = "https://dap.digitalgov.gov/Universal-Federated-Analytics-Min.js";
const PAGE_URL: &str = "https://example.gov/self";

fn dap_capture(name: &str, script: Option<&str>, log: Option<Vec<Value>>) -> PathBuf {
    let mut d = page(PAGE_URL);
    if let Some(src) = script {
        d.script(src);
    }
    write_capture(name, PAGE_URL, &[(1280, 800, &d)], log)
}

#[test]
fn dap_loaded_with_status_200_passes_and_records_the_agency() {
    let url = format!("{DAP}?agency=EXAMPLE&subagency=SUB");
    let dir = dap_capture(
        "dap-200",
        Some(&url),
        Some(vec![document_entry(PAGE_URL), net(&url, Some(200), false)]),
    );
    let f = one(&dir, "BRW-FED-DAP");
    assert_eq!(f.state, State::Pass);
    assert_eq!(f.observed["agency"], json!("EXAMPLE"));
    assert_eq!(f.observed["subagency"], json!("SUB"));
}

#[test]
fn dap_host_file_and_query_names_ignore_case_and_order() {
    let url =
        "https://DAP.DigitalGov.gov/UNIVERSAL-federated-analytics-MIN.JS?subagency=S&AGENCY=A";
    let dir = dap_capture(
        "dap-case",
        None,
        Some(vec![document_entry(PAGE_URL), net(url, Some(200), false)]),
    );
    let f = one(&dir, "BRW-FED-DAP");
    assert_eq!(f.state, State::Pass);
    assert_eq!(f.observed["agency"], json!("A"));
    assert_eq!(f.observed["subagency"], json!("S"));
}

#[test]
fn dap_blocked_by_policy_is_needs_review() {
    let url = format!("{DAP}?agency=EXAMPLE");
    let dir = dap_capture(
        "dap-blocked",
        Some(&url),
        Some(vec![document_entry(PAGE_URL), net(&url, None, true)]),
    );
    assert_eq!(one(&dir, "BRW-FED-DAP").state, State::NeedsReview);
}

#[test]
fn dap_script_in_the_dom_without_a_log_entry_is_needs_review() {
    let dir = dap_capture(
        "dap-dom-only",
        Some(DAP),
        Some(vec![document_entry(PAGE_URL)]),
    );
    let f = one(&dir, "BRW-FED-DAP");
    assert_eq!(f.state, State::NeedsReview);
    assert_eq!(f.observed["referenced_in_dom"], json!(true));
}

#[test]
fn dap_non_200_response_is_not_a_pass() {
    let dir = dap_capture(
        "dap-404",
        Some(DAP),
        Some(vec![document_entry(PAGE_URL), net(DAP, Some(404), false)]),
    );
    assert_eq!(one(&dir, "BRW-FED-DAP").state, State::NeedsReview);
}

#[test]
fn dap_absent_fails() {
    let dir = dap_capture("dap-absent", None, Some(vec![document_entry(PAGE_URL)]));
    assert_eq!(one(&dir, "BRW-FED-DAP").state, State::Fail);
}

#[test]
fn dap_lookalike_hosts_and_files_fail() {
    for (i, url) in [
        "https://dap.digitalgov.gov.example.test/Universal-Federated-Analytics-Min.js",
        "https://dap.digitalgov.gov/other-script.js",
        "https://example.gov/Universal-Federated-Analytics-Min.js",
    ]
    .iter()
    .enumerate()
    {
        let dir = dap_capture(
            &format!("dap-look-{i}"),
            None,
            Some(vec![document_entry(PAGE_URL), net(url, Some(200), false)]),
        );
        assert_eq!(one(&dir, "BRW-FED-DAP").state, State::Fail, "{url}");
    }
}

#[test]
fn dap_without_a_network_log_is_not_tested() {
    let dir = dap_capture("dap-no-log", Some(DAP), None);
    assert_eq!(one(&dir, "BRW-FED-DAP").state, State::NotTested);
    let dir = dap_capture("dap-empty-log", Some(DAP), Some(Vec::new()));
    assert_eq!(one(&dir, "BRW-FED-DAP").state, State::NotTested);
}

#[test]
fn dap_rows_cite_the_network_log_as_evidence() {
    let url = format!("{DAP}?agency=EXAMPLE");
    let dir = dap_capture(
        "dap-evidence",
        Some(&url),
        Some(vec![document_entry(PAGE_URL), net(&url, Some(200), false)]),
    );
    let set = CaptureSet::load(&dir).unwrap();
    let report = assess_set(
        &set,
        &AssessOptions {
            run_id: Some("run-2026-10-18-abc123".to_string()),
            executed_at: Some("2026-10-18T00:00:00.000Z".to_string()),
            inventory: None,
            no_signals: false,
            corroborate: true,
            component: false,
        },
    )
    .unwrap();
    let row = report
        .results
        .iter()
        .find(|r| r.check_id == "BRW-FED-DAP")
        .expect("a DAP row");
    assert_eq!(row.status, State::Pass);
    assert_eq!(row.obligation_id, "OBL-FED-27");
    let kinds: Vec<&str> = report
        .evidence
        .iter()
        .filter(|e| row.evidence_refs.contains(&e.evidence_id))
        .map(|e| e.payload_ref.as_str())
        .collect();
    assert!(
        kinds.len() >= 2,
        "the row cites the DOM snapshot and the network log: {kinds:?}"
    );
}

// ---------------------------------------------------------------------------
// Title and the inventory
// ---------------------------------------------------------------------------

fn title_capture(name: &str, title: &str) -> PathBuf {
    let mut d = Dom::new(PAGE_URL);
    d.title(title);
    write_capture(name, PAGE_URL, &[(1280, 800, &d)], None)
}

fn write_inventory(name: &str, text: &str) -> PathBuf {
    let path = std::env::temp_dir().join(format!(
        "diligent-fedweb-inv-{name}-{}.json",
        std::process::id()
    ));
    std::fs::write(&path, text).unwrap();
    path
}

fn route(url: &str, title: &str, fetched: bool) -> Value {
    json!({ "url": url, "final_url": url, "title": title, "fetched": fetched, "cluster_id": "abc", "extra": [1, 2] })
}

const TITLE: &str = "Shared title that is long enough";

#[test]
fn a_duplicate_title_in_the_inventory_is_needs_review() {
    let dir = title_capture("title-dup", TITLE);
    let inv = write_inventory(
        "dup",
        &json!({ "routes": [
            route(PAGE_URL, TITLE, true),
            route("https://example.gov/a", TITLE, true),
            route("https://example.gov/b", "  shared   TITLE that is long enough ", true),
            route("https://example.gov/unfetched", TITLE, false),
            route("https://example.gov/other", "A different title altogether", true),
        ] })
        .to_string(),
    );
    let f = findings_of(&dir, "BRW-FED-TITLE-CRITERIA", &inventory::load(&inv)).remove(0);
    assert_eq!(f.state, State::NeedsReview);
    assert_eq!(f.observed["duplicate_count"], json!(2));
    let urls: Vec<&str> = f.observed["duplicate_urls"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(Value::as_str)
        .collect();
    assert_eq!(urls, ["https://example.gov/a", "https://example.gov/b"]);
}

#[test]
fn at_most_five_duplicate_urls_are_listed() {
    let dir = title_capture("title-many", TITLE);
    let routes: Vec<Value> = (0..9)
        .map(|i| route(&format!("https://example.gov/p{i}"), TITLE, true))
        .collect();
    let inv = write_inventory("many", &json!({ "routes": routes }).to_string());
    let f = findings_of(&dir, "BRW-FED-TITLE-CRITERIA", &inventory::load(&inv)).remove(0);
    assert_eq!(f.observed["duplicate_count"], json!(9));
    assert_eq!(f.observed["duplicate_urls"].as_array().unwrap().len(), 5);
}

#[test]
fn a_unique_title_passes_with_an_inventory() {
    let dir = title_capture("title-unique", TITLE);
    let inv = write_inventory("unique", &json!({ "routes": [route(PAGE_URL, TITLE, true), route("https://example.gov/x", "Another page title that is long", true)] }).to_string());
    let f = findings_of(&dir, "BRW-FED-TITLE-CRITERIA", &inventory::load(&inv)).remove(0);
    assert_eq!(f.state, State::Pass);
    assert!(!f
        .limitations
        .iter()
        .any(|l| l.contains("uniqueness not evaluated")));
}

#[test]
fn without_an_inventory_uniqueness_is_not_evaluated() {
    let dir = title_capture("title-none", TITLE);
    let f = one(&dir, "BRW-FED-TITLE-CRITERIA");
    assert_eq!(f.state, State::Pass);
    assert!(f
        .limitations
        .iter()
        .any(|l| l.contains("uniqueness not evaluated: no inventory supplied")));
}

#[test]
fn a_malformed_inventory_still_runs_with_a_limitation() {
    let dir = title_capture("title-bad-inv", TITLE);
    let missing = std::env::temp_dir().join("diligent-fedweb-no-such-inventory.json");
    let cases = [
        write_inventory("garbage", "{ not json"),
        write_inventory("empty", ""),
        write_inventory("array", "[1, 2, 3]"),
        write_inventory("noroutes", "{\"routes\": \"none\"}"),
        missing,
    ];
    for path in cases {
        let state = inventory::load(&path);
        assert!(
            matches!(state, InventoryState::Unreadable(_)),
            "{}: {state:?}",
            path.display()
        );
        let f = findings_of(&dir, "BRW-FED-TITLE-CRITERIA", &state).remove(0);
        assert_eq!(f.state, State::Pass, "{}", path.display());
        assert!(
            f.limitations
                .iter()
                .any(|l| l.contains("uniqueness not evaluated: inventory unreadable")),
            "{}",
            path.display()
        );
    }
}

#[test]
fn an_inventory_with_odd_routes_is_read_leniently() {
    let dir = title_capture("title-odd-inv", TITLE);
    let inv = write_inventory(
        "odd",
        &json!({ "routes": [1, "x", null, { "url": 5, "title": 7, "fetched": "yes" }, { "title": TITLE, "fetched": true, "url": "https://example.gov/dup" }], "unknown": {} }).to_string(),
    );
    let state = inventory::load(&inv);
    assert!(matches!(state, InventoryState::Loaded(_)));
    let f = findings_of(&dir, "BRW-FED-TITLE-CRITERIA", &state).remove(0);
    assert_eq!(f.state, State::NeedsReview);
    assert_eq!(f.observed["duplicate_count"], json!(1));
}

#[test]
fn the_inventory_option_reaches_the_checks_through_assess_set() {
    let dir = title_capture("title-opt", TITLE);
    let inv = write_inventory(
        "opt",
        &json!({ "routes": [route("https://example.gov/a", TITLE, true)] }).to_string(),
    );
    let set = CaptureSet::load(&dir).unwrap();
    let opts = AssessOptions {
        run_id: Some("run-2026-10-18-abc123".to_string()),
        executed_at: Some("2026-10-18T00:00:00.000Z".to_string()),
        inventory: Some(inv),
        no_signals: false,
        corroborate: true,
        component: false,
    };
    let report = assess_set(&set, &opts).unwrap();
    let row = report
        .results
        .iter()
        .find(|r| r.check_id == "BRW-FED-TITLE-CRITERIA")
        .unwrap();
    assert_eq!(row.status, State::NeedsReview);
}

#[test]
fn the_cli_accepts_a_malformed_inventory_and_rejects_a_missing_value() {
    let dir = title_capture("cli-inv", TITLE);
    let out = std::env::temp_dir().join(format!("diligent-fedweb-out-{}", std::process::id()));
    let bad = write_inventory("cli-bad", "{ nope");
    let args = |extra: &[&str]| -> Vec<String> {
        let mut v: Vec<String> = [
            "--capture",
            dir.to_str().unwrap(),
            "--out",
            out.to_str().unwrap(),
        ]
        .iter()
        .map(|s| s.to_string())
        .collect();
        v.extend(extra.iter().map(|s| s.to_string()));
        v
    };
    let code = diligent_assess::cli::run_cli(&args(&["--inventory", bad.to_str().unwrap()]));
    assert!(
        code == 0 || code == 1 || code == 2,
        "exit code {code} means assess did not run"
    );
    assert_eq!(diligent_assess::cli::run_cli(&args(&["--inventory"])), 3);
}

#[test]
fn title_length_ignores_padding_and_counts_characters() {
    let cases: [(&str, State); 5] = [
        ("\n      Federal Services 19      \n", State::NeedsReview),
        ("   Federal   Services   20x   ", State::Pass),
        ("日本語のタイトルは二十文字に満たない", State::NeedsReview),
        (
            "日本語のタイトルは二十文字を超えているので十分な長さがあります",
            State::Pass,
        ),
        ("                    ", State::NeedsReview),
    ];
    for (i, (title, want)) in cases.iter().enumerate() {
        let dir = title_capture(&format!("title-len-{i}"), title);
        let f = one(&dir, "BRW-FED-TITLE-CRITERIA");
        assert_eq!(f.state, *want, "{title:?} counted {}", f.observed["length"]);
        assert_ne!(f.state, State::Fail);
    }
}

#[test]
fn a_page_without_a_title_is_needs_review_not_fail() {
    let d = Dom::new(PAGE_URL);
    let dir = write_capture("title-absent", PAGE_URL, &[(1280, 800, &d)], None);
    assert_eq!(
        one(&dir, "BRW-FED-TITLE-CRITERIA").state,
        State::NeedsReview
    );
}

// ---------------------------------------------------------------------------
// Mobile viewport
// ---------------------------------------------------------------------------

fn narrow_page(scroll: u32, with_meta: bool, metrics: bool) -> Dom {
    let mut d = Dom::new(PAGE_URL);
    if with_meta {
        d.el(
            d.head,
            "meta",
            &[("name", "viewport"), ("content", "width=device-width")],
            None,
        );
    }
    d.metrics = metrics.then_some((scroll, 320));
    d
}

#[test]
fn mobile_viewport_without_a_320_view_is_not_tested() {
    let d = narrow_page(320, true, true);
    let dir = write_capture("mobile-none", PAGE_URL, &[(1280, 800, &d)], None);
    assert_eq!(one(&dir, "BRW-FED-MOBILE-VIEWPORT").state, State::NotTested);
}

#[test]
fn mobile_viewport_without_document_metrics_is_not_tested() {
    let wide = page(PAGE_URL);
    let narrow = narrow_page(320, true, false);
    let dir = write_capture(
        "mobile-nometrics",
        PAGE_URL,
        &[(1280, 800, &wide), (320, 568, &narrow)],
        None,
    );
    assert_eq!(one(&dir, "BRW-FED-MOBILE-VIEWPORT").state, State::NotTested);
}

#[test]
fn mobile_viewport_states_follow_overflow_and_the_meta() {
    let wide = page(PAGE_URL);
    let cases = [
        (320u32, true, State::Pass),
        (321, true, State::Pass),
        (322, true, State::NeedsReview),
        (320, false, State::NeedsReview),
        (1000, false, State::NeedsReview),
    ];
    for (i, (scroll, meta, want)) in cases.iter().enumerate() {
        let narrow = narrow_page(*scroll, *meta, true);
        let dir = write_capture(
            &format!("mobile-{i}"),
            PAGE_URL,
            &[(1280, 800, &wide), (320, 568, &narrow)],
            None,
        );
        let f = one(&dir, "BRW-FED-MOBILE-VIEWPORT");
        assert_eq!(f.state, *want, "scroll {scroll}, meta {meta}");
    }
}

// ---------------------------------------------------------------------------
// Hostile and empty input
// ---------------------------------------------------------------------------

#[test]
fn the_federal_checks_survive_an_empty_page() {
    let d = Dom::new(PAGE_URL);
    let dir = write_capture(
        "empty-page",
        PAGE_URL,
        &[(1280, 800, &d)],
        Some(vec![document_entry(PAGE_URL)]),
    );
    let set = CaptureSet::load(&dir).unwrap();
    let findings = checks::run(&set);
    for info in checks::all()
        .iter()
        .filter(|i| i.id.starts_with("BRW-FED-"))
    {
        let rows: Vec<&Finding> = findings.iter().filter(|f| f.check_id == info.id).collect();
        assert!(!rows.is_empty(), "{} produced no row", info.id);
        for r in rows {
            if info.never_fails {
                assert_ne!(r.state, State::Fail, "{}", info.id);
            }
        }
    }
}

#[test]
fn every_federal_row_names_a_federal_rule_and_a_limitation() {
    let d = page(PAGE_URL);
    let dir = write_capture(
        "rows",
        PAGE_URL,
        &[(1280, 800, &d)],
        Some(vec![document_entry(PAGE_URL)]),
    );
    let set = CaptureSet::load(&dir).unwrap();
    for f in checks::run(&set)
        .iter()
        .filter(|f| f.check_id.starts_with("BRW-FED-"))
    {
        assert!(
            f.wcag.iter().all(|r| r.starts_with("FED-")),
            "{}: {:?}",
            f.check_id,
            f.wcag
        );
        assert!(!f.wcag.is_empty(), "{}", f.check_id);
        let info = checks::all().iter().find(|i| i.id == f.check_id).unwrap();
        assert!(
            f.wcag.iter().all(|r| info.informs.contains(r)),
            "{}: {:?} not in informs",
            f.check_id,
            f.wcag
        );
    }
}
