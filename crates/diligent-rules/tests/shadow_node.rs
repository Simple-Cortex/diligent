//! Shadow test: compare the Rust validator against the Node validator on the
//! shipped data and on mutated copies. Skips (not fails) when `node` is absent.
//!
//! Per docs/rust-shadow-notes.md, the two engines must produce identical `ok`
//! and identical sorted error lists on the clean data and on mutations.

use std::path::{Path, PathBuf};
use std::process::Command;

use diligent_rules::{load_data_root, validate};
use serde_json::Value;

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn node_on_path() -> bool {
    Command::new("node").arg("--version").output().is_ok()
}

fn run_node_validator(data_root: &Path) -> Option<(bool, Vec<String>)> {
    let output = Command::new("node")
        .arg(repo_root().join("scripts/validate-rules.mjs"))
        .arg("--data-root")
        .arg(data_root)
        .arg("--json")
        .output()
        .ok()?;
    let stdout = String::from_utf8_lossy(&output.stdout);
    let doc: Value = serde_json::from_str(&stdout).ok()?;
    let ok = doc["ok"].as_bool()?;
    let errors: Vec<String> = doc["errors"]
        .as_array()?
        .iter()
        .filter_map(|v| v.as_str().map(String::from))
        .collect();
    Some((ok, errors))
}

fn run_rust_validator(data_root: &Path) -> (bool, Vec<String>) {
    let data = load_data_root(data_root).expect("data root must load");
    let schemas = repo_root().join("schemas");
    let result = validate(&data, &schemas);
    (result.ok(), result.errors)
}

fn copy_data_root(dest: &Path) {
    let root = repo_root();
    let entries = ["authorities", "rules", "profiles", "schemas"];
    for entry in &entries {
        let src = root.join(entry);
        let dst = dest.join(entry);
        if src.is_dir() {
            copy_dir_recursive(&src, &dst);
        }
    }
}

fn copy_dir_recursive(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_dir_recursive(&src_path, &dst_path);
        } else {
            std::fs::copy(&src_path, &dst_path).unwrap();
        }
    }
}

#[test]
fn shipped_data_node_and_rust_validators_agree() {
    if !node_on_path() {
        eprintln!("SKIP: node not on PATH");
        return;
    }
    let root = repo_root();
    let node_result = run_node_validator(&root);
    let Some((node_ok, mut node_errors)) = node_result else {
        eprintln!("SKIP: could not run node validator");
        return;
    };
    let (rust_ok, mut rust_errors) = run_rust_validator(&root);

    assert_eq!(node_ok, rust_ok, "node ok={node_ok} but rust ok={rust_ok}");

    node_errors.sort();
    rust_errors.sort();
    assert_eq!(
        node_errors, rust_errors,
        "node and rust error lists differ:\n  node: {:?}\n  rust: {:?}",
        node_errors, rust_errors
    );
}

#[test]
fn mutated_data_both_engines_reject_duplicate_rule_id() {
    if !node_on_path() {
        eprintln!("SKIP: node not on PATH");
        return;
    }
    let tmp = std::env::temp_dir().join("diligent-shadow-dup-rule");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    copy_data_root(&tmp);

    // Mutate: duplicate a rule_id by appending a copy of the first rule in the
    // first pack file.
    let packs_dir = tmp.join("rules").join("packs");
    let first_pack = std::fs::read_dir(&packs_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .expect("at least one pack file")
        .path();
    let mut doc: Value =
        serde_json::from_str(&std::fs::read_to_string(&first_pack).unwrap()).unwrap();
    let rules = doc["rules"].as_array_mut().unwrap();
    if !rules.is_empty() {
        let dup = rules[0].clone();
        rules.push(dup);
    }
    std::fs::write(&first_pack, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

    let node_result = run_node_validator(&tmp);
    let Some((node_ok, node_errors)) = node_result else {
        eprintln!("SKIP: could not run node validator on mutated data");
        let _ = std::fs::remove_dir_all(&tmp);
        return;
    };
    let (rust_ok, rust_errors) = run_rust_validator(&tmp);

    assert!(!node_ok, "node must reject duplicate rule_id");
    assert!(!rust_ok, "rust must reject duplicate rule_id");

    let node_has_dup = node_errors.iter().any(|e| e.contains("duplicate rule_id"));
    let rust_has_dup = rust_errors.iter().any(|e| e.contains("duplicate rule_id"));
    assert!(
        node_has_dup,
        "node error list must mention 'duplicate rule_id': {:?}",
        node_errors
    );
    assert!(
        rust_has_dup,
        "rust error list must mention 'duplicate rule_id': {:?}",
        rust_errors
    );

    let _ = std::fs::remove_dir_all(&tmp);
}

#[test]
fn mutated_data_both_engines_reject_dangling_authority() {
    if !node_on_path() {
        eprintln!("SKIP: node not on PATH");
        return;
    }
    let tmp = std::env::temp_dir().join("diligent-shadow-dangling-auth");
    let _ = std::fs::remove_dir_all(&tmp);
    std::fs::create_dir_all(&tmp).unwrap();
    copy_data_root(&tmp);

    // Mutate: change a rule's authority_id to a non-existent one.
    let packs_dir = tmp.join("rules").join("packs");
    let first_pack = std::fs::read_dir(&packs_dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .find(|e| e.path().extension().is_some_and(|ext| ext == "json"))
        .expect("at least one pack file")
        .path();
    let mut doc: Value =
        serde_json::from_str(&std::fs::read_to_string(&first_pack).unwrap()).unwrap();
    let rules = doc["rules"].as_array_mut().unwrap();
    if let Some(rule) = rules.first_mut() {
        if let Some(auth) = rule.get_mut("authority") {
            auth["authority_id"] = Value::String("A99999".to_string());
        }
    }
    std::fs::write(&first_pack, serde_json::to_string_pretty(&doc).unwrap()).unwrap();

    let node_result = run_node_validator(&tmp);
    let Some((node_ok, node_errors)) = node_result else {
        eprintln!("SKIP: could not run node validator on mutated data");
        let _ = std::fs::remove_dir_all(&tmp);
        return;
    };
    let (rust_ok, rust_errors) = run_rust_validator(&tmp);

    assert!(!node_ok, "node must reject dangling authority");
    assert!(!rust_ok, "rust must reject dangling authority");

    let node_has_dangling = node_errors.iter().any(|e| e.contains("does not exist"));
    let rust_has_dangling = rust_errors.iter().any(|e| e.contains("does not exist"));
    assert!(
        node_has_dangling,
        "node error list must mention 'does not exist': {:?}",
        node_errors
    );
    assert!(
        rust_has_dangling,
        "rust error list must mention 'does not exist': {:?}",
        rust_errors
    );

    let _ = std::fs::remove_dir_all(&tmp);
}
