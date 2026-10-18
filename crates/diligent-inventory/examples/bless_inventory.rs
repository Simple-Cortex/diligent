//! Writes `expected-inventory*.json` and `expected-sample*.json` for fixture cases from the
//! current implementation. Review the diff by hand before committing; the tests compare exactly.
//!
//! Usage: `cargo run -p diligent-inventory --example bless_inventory -- fixtures/inventory/<case>...`

use std::path::Path;

use diligent_inventory::fixture::{option_files, run_case};

fn write(path: &Path, value: &serde_json::Value) {
    let text = serde_json::to_string_pretty(value).expect("serialize") + "\n";
    std::fs::write(path, text).expect("write expected file");
}

fn main() {
    for arg in std::env::args().skip(1) {
        let dir = Path::new(&arg);
        let mut base_inventory: Option<serde_json::Value> = None;
        for (suffix, file) in option_files(dir) {
            let run = run_case(dir, &suffix, &file).unwrap_or_else(|e| panic!("{arg}: {e}"));
            let inv = serde_json::to_value(&run.outcome.inventory).expect("inventory");
            let sample = serde_json::to_value(&run.outcome.sample).expect("sample");
            if suffix.is_empty() || base_inventory.as_ref() != Some(&inv) {
                write(&dir.join(format!("expected-inventory{suffix}.json")), &inv);
            }
            if suffix.is_empty() {
                base_inventory = Some(inv);
            }
            write(&dir.join(format!("expected-sample{suffix}.json")), &sample);
            println!(
                "{arg}{suffix}: wrote expected files ({} requests)",
                run.requests.len()
            );
        }
    }
}
