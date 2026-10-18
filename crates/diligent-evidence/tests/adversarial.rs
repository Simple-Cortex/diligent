//! Adversarial tests for the evidence store.

use diligent_evidence::{
    append_line, canonical_json, cli, sha256_hex, CacheKey, Deployment, KeyRecord, KeyStatus,
    Producer, Store, ViewportKey,
};
use serde_json::{json, Value};
use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

static N: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!(
            "diligent-evidence-adv-{label}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    fn join(&self, name: impl AsRef<Path>) -> PathBuf {
        self.0.join(name)
    }
}

impl Drop for TempDir {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn make_key(route: &str, engine: &str) -> CacheKey {
    CacheKey {
        deployment: Deployment {
            commit: Some("abc123".to_string()),
            deployment_id: Some("dep-1".to_string()),
        },
        route: route.to_string(),
        role: Some("public".to_string()),
        state: Some("default".to_string()),
        viewport: Some(ViewportKey {
            width: 1280,
            height: 720,
            dpr: 1.0,
        }),
        media: Some("screen".to_string()),
        locale: Some("en-US".to_string()),
        rule_version: Some("1.0".to_string()),
        engine_version: engine.to_string(),
        protocol_version: Some("1.0".to_string()),
        collector: "diligent-runtime".to_string(),
        kind: "runtime_observation".to_string(),
    }
}

// ---- payload_ref with ../ and absolute paths ----

#[test]
fn payload_ref_with_dotdot_is_indexed_with_stored_null() {
    let tmp = TempDir::new("dotdot");
    let store = Store::open(tmp.join("store")).unwrap();
    let base = tmp.join("base");
    fs::create_dir_all(&base).unwrap();
    let sha = sha256_hex(b"hello");
    let jsonl = tmp.join("evidence.jsonl");
    let row = json!({
        "evidence_id": "ev-test-1",
        "run_id": "run-2026-10-18-0a1b2c",
        "kind": "runtime_observation",
        "payload_ref": "../secret.txt",
        "sha256": sha,
    });
    fs::write(
        &jsonl,
        format!("{}\n", serde_json::to_string(&row).unwrap()),
    )
    .unwrap();
    let report = store
        .ingest_evidence_jsonl(&jsonl, &base, Producer::Collect)
        .unwrap();
    assert_eq!(report.records_indexed, 1);
    assert_eq!(report.payload_problems.len(), 1);
    assert!(report.payload_problems[0]
        .reason
        .contains("leaves the base"));
    let record = store.find_record("ev-test-1").unwrap().unwrap();
    assert!(record.stored.is_none());
}

#[test]
fn payload_ref_absolute_path_is_indexed_with_stored_null() {
    let tmp = TempDir::new("abs");
    let store = Store::open(tmp.join("store")).unwrap();
    let base = tmp.join("base");
    fs::create_dir_all(&base).unwrap();
    let sha = sha256_hex(b"hello");
    let jsonl = tmp.join("evidence.jsonl");
    let row = json!({
        "evidence_id": "ev-test-2",
        "run_id": "run-2026-10-18-0a1b2c",
        "kind": "runtime_observation",
        "payload_ref": "/etc/passwd",
        "sha256": sha,
    });
    fs::write(
        &jsonl,
        format!("{}\n", serde_json::to_string(&row).unwrap()),
    )
    .unwrap();
    let report = store
        .ingest_evidence_jsonl(&jsonl, &base, Producer::Collect)
        .unwrap();
    assert_eq!(report.records_indexed, 1);
    assert_eq!(report.payload_problems.len(), 1);
    assert!(report.payload_problems[0].reason.contains("absolute"));
    let record = store.find_record("ev-test-2").unwrap().unwrap();
    assert!(record.stored.is_none());
}

// ---- symlink inside objects/ pointing outside ----

#[test]
fn symlink_in_objects_is_reported_by_verify_and_never_followed() {
    #[cfg(unix)]
    {
        let tmp = TempDir::new("symlink-verify");
        let store = Store::open(tmp.join("store")).unwrap();
        let outside = tmp.join("outside.txt");
        fs::write(&outside, b"secret data").unwrap();
        let sha = sha256_hex(b"secret data");
        let objects = tmp.join("store/objects");
        let link = objects.join(format!("{sha}.txt"));
        std::os::unix::fs::symlink(&outside, &link).unwrap();
        let verify = store.verify().unwrap();
        assert!(
            verify.mismatched.iter().any(|m| m.contains(&sha)),
            "symlink should be reported as mismatched, got: {:?}",
            verify.mismatched
        );
    }
}

// ---- conflicting manifest lines for one path ----

#[test]
fn conflicting_manifest_lines_for_one_path_are_an_error() {
    let tmp = TempDir::new("conflict");
    let store = Store::open(tmp.join("store")).unwrap();
    let base = tmp.join("base");
    fs::create_dir_all(&base).unwrap();
    let sha1 = sha256_hex(b"content-a");
    let sha2 = sha256_hex(b"content-b");
    let jsonl = tmp.join("evidence.jsonl");
    let row1 = json!({
        "evidence_id": "ev-conflict-1",
        "run_id": "run-2026-10-18-0a1b2c",
        "kind": "runtime_observation",
        "payload_ref": "same.txt",
        "sha256": sha1,
    });
    let row2 = json!({
        "evidence_id": "ev-conflict-2",
        "run_id": "run-2026-10-18-0a1b2c",
        "kind": "runtime_observation",
        "payload_ref": "same.txt",
        "sha256": sha2,
    });
    let text = format!(
        "{}\n{}\n",
        serde_json::to_string(&row1).unwrap(),
        serde_json::to_string(&row2).unwrap()
    );
    fs::write(&jsonl, &text).unwrap();
    fs::write(base.join("same.txt"), b"content-a").unwrap();
    let report = store
        .ingest_evidence_jsonl(&jsonl, &base, Producer::Collect)
        .unwrap();
    assert_eq!(report.records_indexed, 2);
    assert!(
        !report.payload_problems.is_empty(),
        "second row should have a payload problem"
    );
}

// ---- zero-byte object ----

#[test]
fn zero_byte_object_with_correct_hash_is_accepted() {
    let tmp = TempDir::new("zero-byte");
    let store = Store::open(tmp.join("store")).unwrap();
    let sha = sha256_hex(b"");
    let objects = tmp.join("store/objects");
    fs::write(objects.join(format!("{sha}.bin")), b"").unwrap();
    let verify = store.verify().unwrap();
    assert!(verify.ok >= 1, "zero-byte object should pass verify");
}

// ---- 10,000 cache keys then lookup returns the latest ----

#[test]
fn ten_thousand_keys_lookup_returns_latest() {
    let tmp = TempDir::new("10k-keys");
    let store = Store::open(tmp.join("store")).unwrap();
    let key = make_key("https://example.gov/", "1.0.0");
    let hash = key.hash();
    for i in 0..10_000 {
        let record = KeyRecord {
            schema_version: "1.0.0".to_string(),
            key_hash: hash.clone(),
            key: key.clone(),
            missing_reasons: BTreeMap::new(),
            status: KeyStatus::Complete,
            sha256s: vec![format!("{i:064x}")],
            run_id: format!("run-2026-10-18-{i:06x}"),
            created_at: format!("2026-10-18T00:{i:02}:00Z"),
            extensions: None,
        };
        let line = serde_json::to_string(&record).unwrap();
        append_line(&tmp.join("store/keys.jsonl"), &line).unwrap();
    }
    let found = store.lookup(&key).unwrap();
    assert_eq!(found.sha256s, vec![format!("{:064x}", 9999)]);
}

// ---- two processes appending concurrently ----

#[test]
fn two_threads_appending_concurrently_produce_no_torn_lines() {
    let tmp = TempDir::new("concurrent-append");
    let path = tmp.join("index.jsonl");
    fs::write(&path, "").unwrap();
    let path_clone = path.clone();
    let h1 = std::thread::spawn(move || {
        for i in 0..1000 {
            let line = format!(r#"{{"thread":"a","n":{i}}}"#);
            append_line(&path_clone, &line).unwrap();
        }
    });
    let path_clone2 = path.clone();
    let h2 = std::thread::spawn(move || {
        for i in 0..1000 {
            let line = format!(r#"{{"thread":"b","n":{i}}}"#);
            append_line(&path_clone2, &line).unwrap();
        }
    });
    h1.join().unwrap();
    h2.join().unwrap();
    let content = fs::read_to_string(&path).unwrap();
    let lines: Vec<&str> = content.lines().collect();
    assert_eq!(
        lines.len(),
        2000,
        "expected 2000 lines, got {}",
        lines.len()
    );
    for (i, line) in lines.iter().enumerate() {
        let _: Value =
            serde_json::from_str(line).unwrap_or_else(|e| panic!("line {i} is torn: {line}: {e}"));
    }
}

// ---- canonical_json treats {"a":1,"b":2} and {"b":2,"a":1} identically ----

#[test]
fn canonical_json_sorts_keys_and_distinguishes_int_from_float() {
    let a = serde_json::from_str(r#"{"a":1,"b":2}"#).unwrap();
    let b = serde_json::from_str(r#"{"b":2,"a":1}"#).unwrap();
    assert_eq!(canonical_json(&a), canonical_json(&b));
    let c = serde_json::from_str(r#"{"x":1}"#).unwrap();
    let d = serde_json::from_str(r#"{"x":1.5}"#).unwrap();
    assert_ne!(canonical_json(&c), canonical_json(&d));
}

// ---- plan_resume refuses reuse when engine_version differs ----

#[test]
fn plan_resume_refuses_reuse_when_engine_version_differs() {
    let tmp = TempDir::new("resume-version");
    let store = Store::open(tmp.join("store")).unwrap();
    let key_v1 = make_key("https://example.gov/", "1.0.0");
    let sha = sha256_hex(b"payload");
    let objects = tmp.join("store/objects");
    fs::write(objects.join(format!("{sha}.bin")), b"payload").unwrap();
    let record = KeyRecord {
        schema_version: "1.0.0".to_string(),
        key_hash: key_v1.hash(),
        key: key_v1.clone(),
        missing_reasons: BTreeMap::new(),
        status: KeyStatus::Complete,
        sha256s: vec![sha.clone()],
        run_id: "run-2026-10-18-0a1b2c".to_string(),
        created_at: "2026-10-18T00:00:00Z".to_string(),
        extensions: None,
    };
    let line = serde_json::to_string(&record).unwrap();
    append_line(&tmp.join("store/keys.jsonl"), &line).unwrap();
    let key_v2 = make_key("https://example.gov/", "2.0.0");
    let plan = store
        .plan_resume(&[key_v2], "run-2026-10-19-d4e5f6")
        .unwrap();
    assert_eq!(plan.reuse.len(), 0);
    assert_eq!(plan.redo.len(), 1);
    assert!(plan.reasons[0].1.contains("engine_version"));
}

// ---- mark_unavailable on a shared object keeps the file ----

#[test]
fn mark_unavailable_on_shared_object_keeps_the_file() {
    let tmp = TempDir::new("shared-tombstone");
    let store = Store::open(tmp.join("store")).unwrap();
    let sha = sha256_hex(b"shared");
    let objects = tmp.join("store/objects");
    let obj_path = objects.join(format!("{sha}.bin"));
    fs::write(&obj_path, b"shared").unwrap();
    let rec1 = serde_json::json!({
        "entry": "record",
        "schema_version": "1.0.0",
        "evidence_id": "ev-shared-1",
        "sha256": sha,
        "kind": "runtime_observation",
        "producer": "collect",
        "run_id": "run-2026-10-18-0a1b2c",
        "original_payload_ref": "a.txt",
        "stored": format!("objects/{sha}.bin"),
        "bytes": 6,
        "ingested_at": "2026-10-18T00:00:00Z",
        "record": {},
    });
    let rec2 = serde_json::json!({
        "entry": "record",
        "schema_version": "1.0.0",
        "evidence_id": "ev-shared-2",
        "sha256": sha,
        "kind": "runtime_observation",
        "producer": "collect",
        "run_id": "run-2026-10-18-0a1b2c",
        "original_payload_ref": "b.txt",
        "stored": format!("objects/{sha}.bin"),
        "bytes": 6,
        "ingested_at": "2026-10-18T00:00:01Z",
        "record": {},
    });
    append_line(
        &tmp.join("store/index.jsonl"),
        &serde_json::to_string(&rec1).unwrap(),
    )
    .unwrap();
    append_line(
        &tmp.join("store/index.jsonl"),
        &serde_json::to_string(&rec2).unwrap(),
    )
    .unwrap();
    store
        .mark_unavailable("ev-shared-1", "redacted", "cannot replay")
        .unwrap();
    assert!(
        obj_path.exists(),
        "shared object must survive until all live records are tombstoned"
    );
}

// ---- CLI exit codes 0/2/3 ----

#[test]
fn cli_exit_codes_0_2_3() {
    let tmp = TempDir::new("cli-codes");
    let store_dir = tmp.join("store");
    let store = Store::open(&store_dir).unwrap();
    let sha = sha256_hex(b"ok");
    fs::write(store_dir.join(format!("objects/{sha}.bin")), b"ok").unwrap();
    let rec = serde_json::json!({
        "entry": "record",
        "schema_version": "1.0.0",
        "evidence_id": "ev-cli-1",
        "sha256": sha,
        "kind": "runtime_observation",
        "producer": "collect",
        "run_id": "run-2026-10-18-0a1b2c",
        "original_payload_ref": "x.txt",
        "stored": format!("objects/{sha}.bin"),
        "bytes": 2,
        "ingested_at": "2026-10-18T00:00:00Z",
        "record": {},
    });
    append_line(
        &store_dir.join("index.jsonl"),
        &serde_json::to_string(&rec).unwrap(),
    )
    .unwrap();
    drop(store);
    let code = cli(&[
        "evidence".into(),
        "verify".into(),
        "--store".into(),
        store_dir.to_str().unwrap().into(),
    ]);
    assert_eq!(code, ExitCode::SUCCESS);
    fs::write(store_dir.join(format!("objects/{sha}.bin")), b"wrong").unwrap();
    let code = cli(&[
        "evidence".into(),
        "verify".into(),
        "--store".into(),
        store_dir.to_str().unwrap().into(),
    ]);
    assert_eq!(code, ExitCode::from(2));
    let code = cli(&[
        "evidence".into(),
        "verify".into(),
        "--store".into(),
        "/nonexistent/path".into(),
    ]);
    assert_eq!(code, ExitCode::from(3));
}
