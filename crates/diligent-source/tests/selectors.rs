//! Selector generation: stable CSS paths, preferring a unique `#id`.

use std::fs;
use std::path::Path;

use diligent_source::{scan_html, SourceRef};

#[test]
fn selectors_prefer_unique_ids_and_use_nth_of_type() {
    let path = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/source/selector/nested-forms.html");
    let html = fs::read_to_string(path).unwrap();
    let findings = scan_html(&html, &SourceRef::new("nested-forms.html"));
    let mut selectors: Vec<String> = findings
        .iter()
        .filter(|f| f.result.check_id == "SRC-CONTROL-NAME")
        .map(|f| {
            f.result.extensions["diligent.source"]["instance"]
                .as_str()
                .unwrap()
                .to_string()
        })
        .collect();
    selectors.sort();
    let mut want = vec![
        "html > body > main:nth-of-type(1) > form:nth-of-type(1) > input:nth-of-type(1)"
            .to_string(),
        "#uniq".to_string(),
        "html > body > main:nth-of-type(1) > form:nth-of-type(2) > input:nth-of-type(1)"
            .to_string(),
    ];
    want.sort();
    assert_eq!(selectors, want);
}

#[test]
fn instance_ids_carry_the_source_path() {
    let html = "<!DOCTYPE html><html lang=\"en\"><head><title>Instance id fixture page</title></head><body><img src=\"a.png\"></body></html>";
    let findings = scan_html(html, &SourceRef::new("pages/home.html"));
    let img = findings
        .iter()
        .find(|f| f.result.check_id == "SRC-IMG-ALT-PRESENT")
        .unwrap();
    assert_eq!(
        img.result.instance_id,
        "pages/home.html::html > body > img:nth-of-type(1)"
    );
    assert_eq!(img.evidence.payload_ref, "pages/home.html");
}

#[test]
fn cyclic_aria_labelledby_terminates() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/source/labelledby-cycle");
    for (file, check) in [
        ("self-reference.html", "SRC-CONTROL-NAME"),
        ("two-node-cycle.html", "SRC-IMG-ALT-PRESENT"),
        ("two-node-cycle.html", "SRC-LINK-NAME"),
    ] {
        let html = fs::read_to_string(root.join(file)).unwrap();
        let findings = scan_html(&html, &SourceRef::new(file));
        let states: Vec<_> = findings
            .iter()
            .filter(|f| f.result.check_id == check)
            .map(|f| f.result.status.as_str())
            .collect();
        assert!(!states.is_empty(), "{file} {check}");
        assert!(
            states.iter().all(|s| matches!(*s, "fail" | "needs_review")),
            "{file} {check}: {states:?}"
        );
    }
}

#[test]
fn long_labelledby_chain_is_bounded() {
    let mut body = String::new();
    for i in 0..3000 {
        body.push_str(&format!(
            "<span id=\"n{i}\" aria-labelledby=\"n{}\">x</span>",
            i + 1
        ));
    }
    body.push_str("<input aria-labelledby=\"n0\">");
    let html = format!("<!DOCTYPE html><html lang=\"en\"><head><title>Chain fixture page</title></head><body>{body}</body></html>");
    let findings = scan_html(&html, &SourceRef::new("chain.html"));
    assert!(findings
        .iter()
        .any(|f| f.result.check_id == "SRC-CONTROL-NAME"));
}
