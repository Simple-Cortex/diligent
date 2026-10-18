//! Regenerates `fixtures/evidence/infra-ingest` from `diligent-infra`.
//!
//! `cargo run -p diligent-evidence --example bless_evidence`. Review the diff before
//! committing. It writes `EVIDENCE.jsonl` and `evidence/` for
//! `fixtures/infra/federal-good`, exactly as `diligent-rs infra` would.
//!
//! The other cases (`captures-ingest`, `resume-*`, `truncated-object`,
//! `tombstone`) ship no generated files: the tests build their stores from the
//! recorded capture sets at test time, so re-recording captures cannot make
//! them stale.

use std::fs;
use std::path::{Path, PathBuf};

use serde_json::Value;

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn main() {
    let dir = root().join("fixtures/evidence/infra-ingest");
    for name in ["EVIDENCE.jsonl", "evidence"] {
        let path = dir.join(name);
        if path.is_dir() {
            fs::remove_dir_all(&path).expect("remove old evidence dir");
        } else if path.exists() {
            fs::remove_file(&path).expect("remove old EVIDENCE.jsonl");
        }
    }
    let text = fs::read_to_string(root().join("fixtures/infra/federal-good/observation.json"))
        .expect("observation");
    let observation: Value = serde_json::from_str(&text).expect("observation JSON");
    let report = diligent_infra::fake::run_observation(&observation).expect("valid observation");
    fs::create_dir_all(dir.join("evidence")).expect("evidence dir");
    for body in &report.bodies {
        fs::write(dir.join(&body.path), &body.bytes).expect("body");
    }
    let mut lines = String::new();
    for record in &report.evidence {
        lines.push_str(&serde_json::to_string(record).expect("record"));
        lines.push('\n');
    }
    fs::write(dir.join("EVIDENCE.jsonl"), lines).expect("EVIDENCE.jsonl");
    println!("fixtures/evidence/infra-ingest regenerated");
}
