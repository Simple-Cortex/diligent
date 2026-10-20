//! Component mode: whole-page checks record `not_tested` with the component reason and run
//! nothing else; element-level checks are untouched.

use std::path::{Path, PathBuf};

use diligent_assess::checks::{self, COMPONENT_REASON, DOCUMENT_CHECKS};
use diligent_assess::{assess_set, AssessOptions, State};
use diligent_capture::CaptureSet;

fn capture(case: &str) -> CaptureSet {
    let dir: PathBuf = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../fixtures/a11y/captures")
        .join(case);
    CaptureSet::load(&dir).unwrap_or_else(|e| panic!("{case}: {e}"))
}

fn opts(component: bool) -> AssessOptions {
    AssessOptions {
        run_id: Some("run-2026-10-20-abc123".to_string()),
        executed_at: Some("2026-10-20T00:00:00.000Z".to_string()),
        inventory: None,
        no_signals: true,
        corroborate: false,
        component,
    }
}

#[test]
fn document_list_names_real_checks_and_scope_reflects_it() {
    for id in DOCUMENT_CHECKS {
        assert!(
            checks::all().iter().any(|i| i.id == id),
            "{id} is not a known check"
        );
        assert_eq!(checks::scope(id), "document");
    }
    let element = checks::all()
        .iter()
        .filter(|i| checks::scope(i.id) == "element")
        .count();
    assert_eq!(element, checks::all().len() - DOCUMENT_CHECKS.len());
}

#[test]
fn component_mode_marks_document_checks_not_tested_and_leaves_the_rest() {
    let set = capture("047fe0-failed-example-1");
    let normal = assess_set(&set, &opts(false)).unwrap();
    let component = assess_set(&set, &opts(true)).unwrap();
    for id in DOCUMENT_CHECKS {
        let rows: Vec<_> = component
            .results
            .iter()
            .filter(|r| r.check_id == id)
            .collect();
        assert_eq!(rows.len(), 1, "{id}: exactly one row in component mode");
        assert_eq!(rows[0].status, State::NotTested, "{id}");
        assert!(
            rows[0].limitations.iter().any(|l| l == COMPONENT_REASON),
            "{id}: reason recorded"
        );
    }
    let element_rows = |report: &diligent_assess::AssessReport| {
        report
            .results
            .iter()
            .filter(|r| checks::scope(&r.check_id) == "element")
            .map(|r| (r.check_id.clone(), r.instance_id.clone(), r.status))
            .collect::<Vec<_>>()
    };
    assert_eq!(element_rows(&normal), element_rows(&component));
}

#[test]
fn result_and_instance_ids_keep_the_query_of_the_final_url() {
    // Storybook serves every story from iframe.html and tells them apart by `?id=`.
    // Instance ids are built from the capture's final_url, which keeps the query.
    let set = capture("047fe0-failed-example-1");
    let url = set.entries().first().map(|e| e.final_url.clone()).unwrap();
    let report = assess_set(&set, &opts(false)).unwrap();
    assert!(report
        .results
        .iter()
        .all(|r| r.instance_id.starts_with(&format!("{url}::"))));
}
