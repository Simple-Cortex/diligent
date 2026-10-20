//! Line, column and byte offset in evidence records.

use std::fs;
use std::path::{Path, PathBuf};

use diligent_source::{scan_html, Finding, SourceRef};
use serde_json::Value;

fn fixture(rel: &str) -> String {
    let path: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source")
        .join(rel);
    fs::read_to_string(&path).unwrap_or_else(|e| panic!("read {}: {e}", path.display()))
}

fn finding<'f>(findings: &'f [Finding], check: &str, select: Option<(&str, Value)>) -> &'f Finding {
    findings
        .iter()
        .find(|f| {
            f.result.check_id == check
                && select
                    .as_ref()
                    .is_none_or(|(k, v)| f.result.observed.get(*k) == Some(v))
        })
        .unwrap_or_else(|| panic!("no {check} finding"))
}

fn place(f: &Finding) -> (Option<u64>, Option<u64>, Option<u64>) {
    let c = &f.evidence.context;
    (
        c["line"].as_u64(),
        c["column"].as_u64(),
        c["byte_offset"].as_u64(),
    )
}

#[test]
fn link_line_matches_the_fixture() {
    let text = fixture("link-name/generic.html");
    let findings = scan_html(&text, &SourceRef::new("generic.html"));
    let f = finding(&findings, "SRC-LINK-NAME", None);
    let offset = text.find("<a href").unwrap() as u64;
    assert_eq!(place(f), (Some(5), Some(1), Some(offset)));
    assert_eq!(
        f.result.extensions["diligent.source"]["line"].as_u64(),
        Some(5)
    );
}

#[test]
fn second_element_on_a_line_has_its_column() {
    let text = fixture("heading-order/skipped-level.html");
    let findings = scan_html(&text, &SourceRef::new("skipped-level.html"));
    let f = finding(
        &findings,
        "SRC-HEADING-ORDER",
        Some(("issue", Value::String("skipped_level".into()))),
    );
    // `<h1>Main</h1>` is 13 characters, so the `<h3>` starts at column 14.
    assert_eq!(place(f).0, Some(5));
    assert_eq!(place(f).1, Some(14));
}

#[test]
fn tabindex_line_matches_the_fixture() {
    let text = fixture("tabindex-positive/positive.html");
    let findings = scan_html(&text, &SourceRef::new("positive.html"));
    let f = finding(&findings, "SRC-TABINDEX-POSITIVE", None);
    assert_eq!(place(f).0, Some(5));
}

#[test]
fn head_elements_report_their_own_lines() {
    let text = fixture("link-name/generic.html");
    let findings = scan_html(&text, &SourceRef::new("generic.html"));
    let title = finding(&findings, "SRC-TITLE", None);
    assert_eq!(place(title).0, Some(3));
    let lang = finding(&findings, "SRC-HTML-LANG", None);
    assert_eq!(place(lang).0, Some(2));
}

#[test]
fn adversarial_text_does_not_shift_lines() {
    let text = fixture("positions/adversarial.html");
    let findings = scan_html(&text, &SourceRef::new("adversarial.html"));
    let bad: Vec<&Finding> = findings
        .iter()
        .filter(|f| {
            f.result.check_id == "SRC-TABINDEX-POSITIVE" && f.result.status.as_str() == "fail"
        })
        .collect();
    assert_eq!(bad.len(), 1, "comment and script text must not be parsed");
    let offset = text.find("<a href=\"/real\"").unwrap() as u64;
    assert_eq!(place(bad[0]), (Some(8), Some(1), Some(offset)));
}

#[test]
fn crlf_line_endings_count_each_break_once() {
    let text = "<!DOCTYPE html>\r\n<html lang=\"en\">\r\n<body>\r\n\r\n<a href=\"/a\" tabindex=\"4\">x</a>\r\n</body>\r\n</html>\r\n";
    let findings = scan_html(text, &SourceRef::new("crlf.html"));
    let f = finding(&findings, "SRC-TABINDEX-POSITIVE", None);
    assert_eq!(place(f).0, Some(5));
    assert_eq!(place(f).1, Some(1));
}

#[test]
fn implied_elements_have_no_position() {
    let findings = scan_html("<p tabindex=\"4\">x</p>", &SourceRef::new("bare.html"));
    let f = finding(&findings, "SRC-TABINDEX-POSITIVE", None);
    assert_eq!(place(f), (Some(1), Some(1), Some(0)));
    // The not_tested row for the title is anchored at the implied `html`.
    let t = finding(&findings, "SRC-TITLE", None);
    assert_eq!(place(t), (None, None, None));
    assert!(t.evidence.context["line"].is_null());
    assert!(t.result.extensions["diligent.source"]["line"].is_null());
}

#[test]
fn multibyte_text_before_a_tag_counts_characters() {
    let text = "<body>\n<p>éé</p> <a href=\"/a\" tabindex=\"2\">x</a></body>";
    let findings = scan_html(text, &SourceRef::new("utf8.html"));
    let f = finding(&findings, "SRC-TABINDEX-POSITIVE", None);
    // `<p>éé</p> ` is 10 characters but 12 bytes.
    assert_eq!(place(f).1, Some(11));
    assert_eq!(place(f).2, Some(text.find("<a ").unwrap() as u64));
}
