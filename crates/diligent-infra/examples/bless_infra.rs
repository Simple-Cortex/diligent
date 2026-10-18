//! Regenerates `fixtures/infra/*/expected-results.jsonl` from the current checks.
//!
//! `cargo run -p diligent-infra --example bless_infra`. Review the diff before committing.

use std::fs;
use std::path::Path;

use diligent_infra::fake::run_observation;

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/infra");
    let mut dirs: Vec<_> = fs::read_dir(&root)
        .expect("fixtures/infra is readable")
        .filter_map(Result::ok)
        .map(|e| e.path())
        .filter(|p| p.join("observation.json").is_file())
        .collect();
    dirs.sort();
    for dir in dirs {
        let text =
            fs::read_to_string(dir.join("observation.json")).expect("observation.json is readable");
        let value: serde_json::Value =
            serde_json::from_str(&text).expect("observation.json is JSON");
        let report = run_observation(&value).expect("observation has a valid origin");
        let mut out = String::new();
        for row in &report.rows {
            out.push_str(&serde_json::to_string(row).expect("row serializes"));
            out.push('\n');
        }
        fs::write(dir.join("expected-results.jsonl"), out)
            .expect("expected-results.jsonl is writable");
        println!("{}: {} rows", dir.display(), report.rows.len());
    }
}
