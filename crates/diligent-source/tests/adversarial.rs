//! Adversarial tests for diligent-source: robustness against edge cases,
//! boundary inputs, and stress conditions.

use std::collections::BTreeSet;

use diligent_source::checks::autofill;
use diligent_source::{scan_html, write_jsonl, SourceRef, State};

fn src(path: &str) -> SourceRef {
    SourceRef::new(path)
}

// ---------------------------------------------------------------------------
// Robustness: empty, minimal, and deeply nested documents
// ---------------------------------------------------------------------------

#[test]
fn scan_html_on_empty_string_does_not_panic() {
    let findings = scan_html("", &src("empty.html"));
    // No html element means most checks record not_tested or nothing.
    // The important thing is that it does not panic.
    let _ = findings;
}

#[test]
fn scan_html_on_bare_html_tag_does_not_panic() {
    let findings = scan_html("<html>", &src("bare.html"));
    let _ = findings;
}

#[test]
#[allow(clippy::len_zero)]
fn scan_html_on_deeply_nested_divs_does_not_panic_or_stack_overflow() {
    // 10,000 nested <div> elements. This is well under the 1 MB default cap.
    let mut html = String::with_capacity(60_000);
    for _ in 0..10_000 {
        html.push_str("<div>");
    }
    for _ in 0..10_000 {
        html.push_str("</div>");
    }
    let findings = scan_html(&html, &src("deep.html"));
    // Must complete without panicking.
    assert!(findings.is_empty() || findings.len() > 0);
}

// ---------------------------------------------------------------------------
// Image alt in a link
// ---------------------------------------------------------------------------

#[test]
fn img_alt_empty_inside_link_with_no_other_content_is_fail() {
    let html = r#"<!DOCTYPE html><html lang="en"><head><title>Test</title></head>
<body><a href="/page"><img src="photo.jpg" alt=""></a></body></html>"#;
    let findings = scan_html(html, &src("link-img.html"));
    let img_check = findings
        .iter()
        .find(|f| f.result.check_id == "SRC-IMG-ALT-PRESENT")
        .expect("SRC-IMG-ALT-PRESENT check must run");
    assert_eq!(
        img_check.result.status,
        State::Fail,
        "img with alt='' inside a link with no other text must fail"
    );
}

// ---------------------------------------------------------------------------
// Aria-labelledby cycles
// ---------------------------------------------------------------------------

#[test]
fn self_referencing_aria_labelledby_terminates() {
    let html = r#"<!DOCTYPE html><html lang="en"><head><title>Test</title></head>
<body><div id="a" aria-labelledby="a">text</div></body></html>"#;
    let findings = scan_html(html, &src("self-ref.html"));
    // Must not panic or loop. The aria-ref check should find the self-reference
    // resolves (id "a" exists), so it passes or is not_tested.
    let _ = findings;
}

#[test]
fn two_node_aria_labelledby_cycle_terminates() {
    let html = r#"<!DOCTYPE html><html lang="en"><head><title>Test</title></head>
<body>
<div id="a" aria-labelledby="b">text a</div>
<div id="b" aria-labelledby="a">text b</div>
</body></html>"#;
    let findings = scan_html(html, &src("cycle.html"));
    // Must terminate. Both ids exist so aria-ref should pass.
    let _ = findings;
}

// ---------------------------------------------------------------------------
// Duplicate ids
// ---------------------------------------------------------------------------

#[test]
fn duplicate_ids_across_500_elements_report_every_duplicate() {
    let mut html =
        String::from(r#"<!DOCTYPE html><html lang="en"><head><title>Test</title></head><body>"#);
    for _ in 0..500 {
        html.push_str(r#"<div id="dup">x</div>"#);
    }
    html.push_str("</body></html>");
    let findings = scan_html(&html, &src("dup-ids.html"));
    let dup_findings: Vec<_> = findings
        .iter()
        .filter(|f| f.result.check_id == "SRC-DUP-ID" && f.result.status == State::Fail)
        .collect();
    // The first element is the original; elements 2..500 are duplicates = 499 failures.
    assert_eq!(
        dup_findings.len(),
        499,
        "expected 499 duplicate-id failures (500 elements, first is original), got {}",
        dup_findings.len()
    );
}

// ---------------------------------------------------------------------------
// Autocomplete validation
// ---------------------------------------------------------------------------

#[test]
fn autocomplete_section_blue_shipping_tel_is_valid() {
    let result = autofill::validate("section-blue shipping tel");
    assert!(
        result.is_ok(),
        "autocomplete='section-blue shipping tel' must be valid, got {:?}",
        result
    );
}

#[test]
fn autocomplete_telephone_is_invalid() {
    let result = autofill::validate("telephone");
    assert!(
        result.is_err(),
        "autocomplete='telephone' must be invalid (not a valid token)"
    );
}

// ---------------------------------------------------------------------------
// Title in body only
// ---------------------------------------------------------------------------

#[test]
fn title_in_body_only_is_needs_review() {
    let html = r#"<!DOCTYPE html><html lang="en"><head></head>
<body><title>Page title here</title><p>content</p></body></html>"#;
    let findings = scan_html(html, &src("body-title.html"));
    let title_check = findings
        .iter()
        .find(|f| f.result.check_id == "SRC-TITLE")
        .expect("SRC-TITLE check must run");
    assert_eq!(
        title_check.result.status,
        State::NeedsReview,
        "title in body only must be needs_review, got {:?}",
        title_check.result.status
    );
}

// ---------------------------------------------------------------------------
// Lang attribute
// ---------------------------------------------------------------------------

#[test]
fn lang_en_us_passes() {
    let html = r#"<!DOCTYPE html><html lang="en-US"><head><title>Test</title></head>
<body><p>content</p></body></html>"#;
    let findings = scan_html(html, &src("lang-ok.html"));
    let lang_check = findings
        .iter()
        .find(|f| f.result.check_id == "SRC-HTML-LANG")
        .expect("SRC-HTML-LANG check must run");
    assert_eq!(
        lang_check.result.status,
        State::Pass,
        "lang='en-US' must pass, got {:?}",
        lang_check.result.status
    );
}

#[test]
fn lang_english_fails() {
    let html = r#"<!DOCTYPE html><html lang="english"><head><title>Test</title></head>
<body><p>content</p></body></html>"#;
    let findings = scan_html(html, &src("lang-bad.html"));
    let lang_check = findings
        .iter()
        .find(|f| f.result.check_id == "SRC-HTML-LANG")
        .expect("SRC-HTML-LANG check must run");
    assert_eq!(
        lang_check.result.status,
        State::Fail,
        "lang='english' must fail (not BCP 47 shape), got {:?}",
        lang_check.result.status
    );
}

// ---------------------------------------------------------------------------
// JSONL writer contract
// ---------------------------------------------------------------------------

#[test]
fn jsonl_writer_emits_one_evidence_per_result_and_refs_resolve() {
    let html = r#"<!DOCTYPE html><html lang="en"><head><title>JSONL test page</title></head>
<body><img src="a.png" alt="test"><a href="/x">Click here</a>
<input name="q" autocomplete="badtoken"><div id="dup">x</div><div id="dup">y</div>
</body></html>"#;
    let findings = scan_html(html, &src("jsonl.html"));
    assert!(
        !findings.is_empty(),
        "must have findings to test JSONL output"
    );

    let (mut ev_buf, mut rs_buf) = (Vec::new(), Vec::new());
    write_jsonl(
        &diligent_source::ScanReport {
            root: "jsonl.html".to_string(),
            run_id: findings[0].evidence.run_id.clone(),
            findings: findings.clone(),
            files_scanned: 1,
            files_skipped: 0,
            skipped: vec![],
            checks_run: vec![],
            not_tested: vec![],
        },
        &mut ev_buf,
        &mut rs_buf,
    )
    .unwrap();

    let evidence_lines: Vec<&str> = std::str::from_utf8(&ev_buf).unwrap().lines().collect();
    let result_lines: Vec<&str> = std::str::from_utf8(&rs_buf).unwrap().lines().collect();

    // One result row per finding.
    assert_eq!(
        result_lines.len(),
        findings.len(),
        "result rows must equal findings count"
    );

    // Evidence rows are deduplicated.
    let evidence_ids: BTreeSet<String> = evidence_lines
        .iter()
        .map(|l| {
            serde_json::from_str::<serde_json::Value>(l).unwrap()["evidence_id"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    assert_eq!(
        evidence_ids.len(),
        evidence_lines.len(),
        "evidence rows must be unique by evidence_id"
    );

    // Every result's evidence_refs resolve to an evidence row.
    for line in &result_lines {
        let row: serde_json::Value = serde_json::from_str(line).unwrap();
        for r in row["evidence_refs"].as_array().unwrap() {
            assert!(
                evidence_ids.contains(r.as_str().unwrap()),
                "evidence_ref {} does not resolve to an evidence row",
                r.as_str().unwrap()
            );
        }
    }
}
