//! Store behaviour over the cases in `fixtures/evidence`, plus adversarial inputs.
//!
//! Fixture stores are copied to a temporary directory before use because
//! `plan_resume` appends reuse records.

use diligent_capture::{CaptureError, CaptureSet};
use diligent_evidence::{
    cli, is_fresh, sha256_hex, CacheKey, EvidenceError, EvidenceRecord, IngestContext, KeyStatus,
    Producer, Redaction, Store,
};
use serde_json::{json, Value};
use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::sync::atomic::{AtomicU64, Ordering};

const RUN: &str = "run-2026-10-18-0a1b2c";
const RUN_2: &str = "run-2026-10-19-d4e5f6";
const CAPTURE: &str = "24afc2-failed-example-1";

static N: AtomicU64 = AtomicU64::new(0);

struct TempDir(PathBuf);

impl TempDir {
    fn new(label: &str) -> TempDir {
        let dir = std::env::temp_dir().join(format!(
            "diligent-evidence-store-{label}-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::Relaxed)
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        TempDir(dir)
    }
    fn path(&self) -> &Path {
        &self.0
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

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn fixtures() -> PathBuf {
    repo_root().join("fixtures/evidence")
}

fn read_json(path: &Path) -> Value {
    let text =
        fs::read_to_string(path).unwrap_or_else(|e| panic!("cannot read {}: {e}", path.display()));
    serde_json::from_str(&text).unwrap_or_else(|e| panic!("{} is not JSON: {e}", path.display()))
}

fn validator(name: &str) -> jsonschema::Validator {
    let schema = read_json(&repo_root().join("schemas").join(name));
    jsonschema::options()
        .with_draft(jsonschema::Draft::Draft202012)
        .should_validate_formats(true)
        .build(&schema)
        .unwrap_or_else(|e| panic!("{name} does not compile: {e}"))
}

fn assert_valid(v: &jsonschema::Validator, instance: &Value, what: &str) {
    let errors: Vec<String> = v
        .iter_errors(instance)
        .map(|e| format!("{e} at {}", e.instance_path()))
        .collect();
    assert!(
        errors.is_empty(),
        "{what} is invalid: {errors:?}\n{instance}"
    );
}

fn assert_invalid(v: &jsonschema::Validator, instance: &Value, what: &str) {
    assert!(
        !v.is_valid(instance),
        "{what} should be rejected: {instance}"
    );
}

fn lines(path: &Path) -> Vec<Value> {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|l| serde_json::from_str(l).unwrap_or_else(|e| panic!("{}: {l}: {e}", path.display())))
        .collect()
}

fn line_count(path: &Path) -> usize {
    fs::read_to_string(path)
        .unwrap()
        .lines()
        .filter(|l| !l.trim().is_empty())
        .count()
}

/// Validates every line of the store's three logs against its schema, and
/// every embedded evidence record against the evidence-record schema.
fn validate_store(dir: &Path) {
    let index = validator("evidence-index.schema.json");
    let record = validator("evidence-record.schema.json");
    let keys = validator("cache-key.schema.json");
    let reuse = validator("reuse-record.schema.json");
    for line in lines(&dir.join("index.jsonl")) {
        assert_valid(&index, &line, "index line");
        if line["entry"] == "record" {
            assert_valid(&record, &line["record"], "embedded evidence record");
            assert_eq!(line["record"]["evidence_id"], line["evidence_id"]);
            assert_eq!(
                line["record"]["sha256"].as_str().unwrap().to_lowercase(),
                line["sha256"].as_str().unwrap()
            );
        }
    }
    for line in lines(&dir.join("keys.jsonl")) {
        assert_valid(&keys, &line, "key line");
    }
    for line in lines(&dir.join("reuse.jsonl")) {
        assert_valid(&reuse, &line, "reuse line");
    }
}

fn copy_dir(from: &Path, to: &Path) {
    fs::create_dir_all(to).unwrap();
    for entry in fs::read_dir(from).unwrap() {
        let entry = entry.unwrap();
        let target = to.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir(&entry.path(), &target);
        } else {
            fs::copy(entry.path(), &target).unwrap();
        }
    }
}

fn capture_dir(name: &str) -> PathBuf {
    repo_root().join("fixtures/a11y/captures").join(name)
}

fn final_url(dir: &Path) -> String {
    CaptureSet::load(dir).unwrap().entries()[0]
        .final_url
        .clone()
}

fn context(route: String) -> IngestContext {
    let mut ctx = IngestContext::new(RUN, route);
    ctx.commit = Some("3f2a9c1d5e7b8a6c4d2e1f0a9b8c7d6e5f4a3b2c".to_string());
    ctx.locale = Some("en-US".to_string());
    ctx.rule_version = Some("a11y-wcag21-aa@0.1.1".to_string());
    ctx.missing_reasons.insert(
        "role".into(),
        "Public page; no signed-in session in this audit.".into(),
    );
    ctx.missing_reasons.insert(
        "state".into(),
        "Only the default page state was captured.".into(),
    );
    ctx
}

/// `(distinct payloads, distinct cache keys)` a capture directory should ingest to.
fn counts(dir: &Path) -> (usize, usize) {
    let set = CaptureSet::load(dir).unwrap();
    let mut shas: Vec<String> = set
        .entries()
        .iter()
        .map(|e| e.sha256.to_lowercase())
        .collect();
    shas.sort();
    shas.dedup();
    let mut keys: Vec<String> = set
        .entries()
        .iter()
        .map(|e| {
            format!(
                "{}x{}@{}|{}|{}",
                e.viewport.width, e.viewport.height, e.viewport.dpr, e.media, e.kind
            )
        })
        .collect();
    keys.sort();
    keys.dedup();
    (shas.len(), keys.len())
}

/// Payload and key counts of the single capture set most tests use.
fn n() -> (usize, usize) {
    counts(&capture_dir(CAPTURE))
}

fn fixed(ts: &'static str) -> Box<dyn Fn() -> String> {
    Box::new(move || ts.to_string())
}

/// Builds a case's store at test time from the recorded capture set, then
/// applies the case's mutation. Nothing here depends on how many captures a
/// recorded set holds, so re-recording captures cannot make it stale.
///
/// `expected.json` in the case directory holds only the fixed words the case
/// checks for. Values that depend on the build (the truncated object's path,
/// the tombstoned id, the cached engine version) are added to it here.
fn fixture_store(case: &str) -> (TempDir, Store, Value, Vec<CacheKey>) {
    let mut expected = read_json(&fixtures().join(case).join("expected.json"));
    let tmp = TempDir::new(case);
    let store = Store::open_with_clock(tmp.join("store"), fixed("2026-10-18T12:00:00Z")).unwrap();
    let capture = capture_dir(CAPTURE);
    store
        .ingest_captures(&capture, &context(final_url(&capture)))
        .unwrap();
    let mut keys: Vec<CacheKey> = store
        .key_records()
        .unwrap()
        .into_iter()
        .map(|r| r.key)
        .collect();
    let dom = store
        .index_entries()
        .unwrap()
        .into_iter()
        .find_map(|e| match e {
            diligent_evidence::IndexEntry::Record(r) if r.kind == "computed_style" => Some(r),
            _ => None,
        })
        .expect("the capture set has a dom snapshot");
    let stored = dom.stored.clone().expect("dom snapshot is stored");
    match case {
        "resume-compatible" => {}
        "resume-engine-changed" => {
            expected["cached_engine_version"] = json!(keys[0].engine_version);
            for key in &mut keys {
                key.engine_version = "0.2.0".to_string();
            }
        }
        "truncated-object" => {
            let path = tmp.join("store").join(&stored);
            let bytes = fs::read(&path).unwrap();
            fs::write(&path, &bytes[..bytes.len() / 2]).unwrap();
            expected["mismatched"] = json!([stored]);
        }
        "tombstone" => {
            store
                .mark_unavailable(
                    &dom.evidence_id,
                    "Payload removed at the owner's request after it was found to contain a session token.",
                    "The DOM snapshot cannot be replayed; checks over it cannot be re-run.",
                )
                .unwrap();
            expected["evidence_id"] = json!(dom.evidence_id);
        }
        other => panic!("unknown fixture case {other}"),
    }
    (tmp, store, expected, keys)
}

/// Rows, stored objects and DNS rows of the recorded infra output.
fn infra_counts() -> (usize, usize, usize) {
    let case = fixtures().join("infra-ingest");
    let rows = lines(&case.join("EVIDENCE.jsonl"));
    let dns = rows
        .iter()
        .filter(|r| r["payload_ref"].as_str().unwrap().starts_with("dns:"))
        .count();
    let objects = fs::read_dir(case.join("evidence")).unwrap().count();
    (rows.len(), objects, dns)
}

fn ingested_capture_store(label: &str) -> (TempDir, Store) {
    let tmp = TempDir::new(label);
    let store = Store::open(tmp.join("store")).unwrap();
    let dir = capture_dir(CAPTURE);
    store
        .ingest_captures(&dir, &context(final_url(&dir)))
        .unwrap();
    (tmp, store)
}

// ---- captures-ingest ------------------------------------------------------

#[test]
fn captures_ingest_matches_the_expected_counts() {
    let expected = read_json(&fixtures().join("captures-ingest/expected.json"));
    let tmp = TempDir::new("captures");
    let dir = tmp.join("store");
    let store = Store::open(&dir).unwrap();
    let (mut records, mut keys) = (0, 0);
    for set in expected["capture_sets"].as_array().unwrap() {
        let capture = repo_root().join(set.as_str().unwrap());
        let report = store
            .ingest_captures(&capture, &context(final_url(&capture)))
            .unwrap();
        assert!(report.rejected.is_empty() && report.payload_problems.is_empty());
        records += report.records_indexed;
        keys += report.keys_recorded;
    }
    let mut want = (0, 0);
    for set in expected["capture_sets"].as_array().unwrap() {
        let c = counts(&repo_root().join(set.as_str().unwrap()));
        want = (want.0 + c.0, want.1 + c.1);
    }
    assert_eq!((records, keys), want, "derived from the manifests");
    assert_eq!(line_count(&dir.join("index.jsonl")), records);
    assert_eq!(line_count(&dir.join("keys.jsonl")), keys);
    assert_eq!(fs::read_dir(dir.join("objects")).unwrap().count(), records);
    validate_store(&dir);

    for line in lines(&dir.join("index.jsonl")) {
        let sha = line["sha256"].as_str().unwrap();
        assert_eq!(line["evidence_id"], format!("ev-cap-{}", &sha[..12]));
        assert_eq!(line["producer"], "collect");
        assert_eq!(line["stored"], format!("objects/{sha}.json"));
        let record = &line["record"];
        assert_eq!(record["collector"], "diligent-runtime");
        let want_trust = match line["record"]["context"]["capture_kind"].as_str().unwrap() {
            "axe_result" => "inference",
            _ => "direct_observation",
        };
        assert_eq!(record["trust"], want_trust);
        assert_eq!(record["data_classification"], "public");
        assert!(record["derived_from"].is_null());
        assert!(!record["limitations"].as_array().unwrap().is_empty());
        assert_eq!(record["collector_version"], "0.1.0");
        assert_eq!(record["context"]["protocol_version"], "1.3.0");
        assert!(record["context"]["final_url"]
            .as_str()
            .unwrap()
            .starts_with("http://127.0.0.1"));
        assert_eq!(record["context"]["route"], record["context"]["final_url"]);
        let want = match line["record"]["context"]["capture_kind"].as_str().unwrap() {
            "dom_snapshot" => "computed_style",
            "ax_tree" => "accessibility_tree",
            _ => "runtime_observation",
        };
        assert_eq!(line["kind"], want);
    }
    for line in lines(&dir.join("keys.jsonl")) {
        assert_eq!(line["status"], "complete");
        assert_eq!(line["key"]["collector"], "diligent-runtime");
        assert_eq!(line["missing_reasons"].as_object().unwrap().len(), 2);
        assert_eq!(line["sha256s"].as_array().unwrap().len(), 1);
    }

    let verify = store.verify().unwrap();
    assert!(verify.is_clean(), "{verify:?}");
    assert_eq!(
        (verify.objects, verify.ok, verify.tombstoned),
        (records, records, 0)
    );

    // Ingesting again adds nothing.
    for set in expected["capture_sets"].as_array().unwrap() {
        let capture = repo_root().join(set.as_str().unwrap());
        let again = store
            .ingest_captures(&capture, &context(final_url(&capture)))
            .unwrap();
        let (per_records, per_keys) = counts(&capture);
        assert_eq!(
            (
                again.records_indexed,
                again.records_skipped,
                again.keys_recorded,
                again.keys_skipped
            ),
            (0, per_records, 0, per_keys)
        );
        assert_eq!(
            (again.objects_written, again.objects_reused),
            (0, per_records)
        );
    }
    assert_eq!(line_count(&dir.join("index.jsonl")), records);
    assert_eq!(line_count(&dir.join("keys.jsonl")), keys);
}

#[test]
fn an_empty_dimension_without_a_reason_fails_before_anything_is_written() {
    let tmp = TempDir::new("reason");
    let store = Store::open(tmp.join("store")).unwrap();
    let dir = capture_dir(CAPTURE);
    let mut ctx = context(final_url(&dir));
    ctx.missing_reasons.remove("role");
    match store.ingest_captures(&dir, &ctx) {
        Err(EvidenceError::MissingReason(dim)) => assert_eq!(dim, "role"),
        other => panic!("expected MissingReason, got {other:?}"),
    }
    ctx.missing_reasons.insert("role".into(), "   ".into());
    assert!(matches!(
        store.ingest_captures(&dir, &ctx),
        Err(EvidenceError::MissingReason(_))
    ));
    assert_eq!(line_count(&tmp.join("store/index.jsonl")), 0);
    assert_eq!(line_count(&tmp.join("store/keys.jsonl")), 0);
    assert_eq!(fs::read_dir(tmp.join("store/objects")).unwrap().count(), 0);

    ctx.missing_reasons.insert("role".into(), "ok".into());
    ctx.missing_reasons.insert("weather".into(), "sunny".into());
    assert!(matches!(
        store.ingest_captures(&dir, &ctx),
        Err(EvidenceError::Invalid(_))
    ));
    let mut bad_run = context(final_url(&dir));
    bad_run.run_id = "run-1".into();
    assert!(matches!(
        store.ingest_captures(&dir, &bad_run),
        Err(EvidenceError::Invalid(_))
    ));
    bad_run.run_id = RUN.into();
    bad_run.route = String::new();
    assert!(matches!(
        store.ingest_captures(&dir, &bad_run),
        Err(EvidenceError::Invalid(_))
    ));
}

#[test]
fn a_missing_deployment_needs_its_own_reason() {
    let tmp = TempDir::new("deployment");
    let store = Store::open(tmp.join("store")).unwrap();
    let dir = capture_dir(CAPTURE);
    let mut ctx = context(final_url(&dir));
    ctx.commit = None;
    match store.ingest_captures(&dir, &ctx) {
        Err(EvidenceError::MissingReason(dim)) => assert_eq!(dim, "deployment"),
        other => panic!("expected MissingReason, got {other:?}"),
    }
    ctx.missing_reasons.insert(
        "deployment".into(),
        "Audited a staging host with no commit stamp.".into(),
    );
    store.ingest_captures(&dir, &ctx).unwrap();
    validate_store(&tmp.join("store"));
    for line in lines(&tmp.join("store/keys.jsonl")) {
        assert!(line["missing_reasons"]["deployment"].is_string());
    }
}

#[test]
fn a_tampered_capture_is_refused_and_nothing_is_stored() {
    let tmp = TempDir::new("tampered");
    copy_dir(&capture_dir(CAPTURE), &tmp.join("cap"));
    let victim = fs::read_dir(tmp.join("cap"))
        .unwrap()
        .map(|e| e.unwrap().path())
        .find(|p| p.extension().is_some_and(|e| e == "json"))
        .unwrap();
    fs::write(&victim, b"{}").unwrap();
    let store = Store::open(tmp.join("store")).unwrap();
    let result = store.ingest_captures(&tmp.join("cap"), &context("https://example.gov/".into()));
    assert!(
        matches!(
            result,
            Err(EvidenceError::Capture(CaptureError::HashMismatch { .. }))
        ),
        "{result:?}"
    );
    assert_eq!(line_count(&tmp.join("store/index.jsonl")), 0);
}

#[test]
fn a_symlinked_capture_file_is_refused() {
    #[cfg(unix)]
    {
        let tmp = TempDir::new("symlink-capture");
        copy_dir(&capture_dir(CAPTURE), &tmp.join("cap"));
        let victim = fs::read_dir(tmp.join("cap"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "json"))
            .unwrap();
        let bytes = fs::read(&victim).unwrap();
        fs::remove_file(&victim).unwrap();
        fs::write(tmp.join("elsewhere.json"), &bytes).unwrap();
        std::os::unix::fs::symlink(tmp.join("elsewhere.json"), &victim).unwrap();
        let store = Store::open(tmp.join("store")).unwrap();
        let result =
            store.ingest_captures(&tmp.join("cap"), &context("https://example.gov/".into()));
        assert!(
            matches!(result, Err(EvidenceError::Invalid(_))),
            "{result:?}"
        );
        assert_eq!(fs::read_dir(tmp.join("store/objects")).unwrap().count(), 0);
    }
}

// ---- infra-ingest ---------------------------------------------------------

#[test]
fn infra_evidence_is_indexed_and_bodies_are_stored() {
    let case = fixtures().join("infra-ingest");
    let tmp = TempDir::new("infra");
    let dir = tmp.join("store");
    let store = Store::open(&dir).unwrap();
    let report = store
        .ingest_evidence_jsonl(&case.join("EVIDENCE.jsonl"), &case, Producer::Infra)
        .unwrap();
    let (rows, objects, dns) = infra_counts();
    assert_eq!(report.records_indexed, rows);
    assert_eq!(report.objects_written, objects);
    assert!(
        report.payload_problems.is_empty(),
        "{:?}",
        report.payload_problems
    );
    assert!(report.rejected.is_empty());
    assert_eq!(report.keys_recorded, 0);
    assert_eq!(line_count(&dir.join("keys.jsonl")), 0);

    let index = lines(&dir.join("index.jsonl"));
    assert_eq!(index.len(), rows);
    let null_stored = index.iter().filter(|l| l["stored"].is_null()).count();
    assert_eq!(null_stored, dns);
    for line in &index {
        assert_eq!(line["producer"], "infra");
        // The store never renames an id or rewrites the record.
        let id = line["evidence_id"].as_str().unwrap();
        assert!(
            id.starts_with("ev-http-") || id.starts_with("ev-dns-"),
            "{id}"
        );
        assert_eq!(line["original_payload_ref"], line["record"]["payload_ref"]);
        if line["stored"].is_null() {
            assert!(line["bytes"].is_null());
            assert!(line["original_payload_ref"]
                .as_str()
                .unwrap()
                .starts_with("dns:"));
        }
    }
    validate_store(&dir);
    let verify = store.verify().unwrap();
    assert!(verify.is_clean(), "{verify:?}");
    assert_eq!(verify.objects, objects);

    // The empty response body is a real zero-byte object.
    let empty = dir.join(format!("objects/{}.bin", sha256_hex(b"")));
    assert_eq!(fs::metadata(empty).unwrap().len(), 0);

    let again = store
        .ingest_evidence_jsonl(&case.join("EVIDENCE.jsonl"), &case, Producer::Infra)
        .unwrap();
    assert_eq!((again.records_indexed, again.records_skipped), (0, rows));
    assert_eq!(line_count(&dir.join("index.jsonl")), rows);
}

fn row(id: &str, payload_ref: &str, sha: &str) -> Value {
    json!({
        "schema_version": "1.0.0", "evidence_id": id, "run_id": RUN, "kind": "http_response",
        "collector": "diligent-infra", "collector_version": "0.1.0", "captured_at": "2026-10-18T12:00:00Z",
        "snapshot_ref": null, "context": {}, "payload_ref": payload_ref, "sha256": sha,
        "data_classification": "public_web_response", "redaction": {"applied": false, "flags": []},
        "trust": "direct_observation", "limitations": ["One GET."], "derived_from": null
    })
}

fn write_rows(path: &Path, rows: &[Value]) {
    let mut text = String::new();
    for r in rows {
        text.push_str(&r.to_string());
        text.push('\n');
    }
    fs::write(path, text).unwrap();
}

#[test]
fn hostile_payload_refs_are_indexed_without_a_stored_object() {
    let tmp = TempDir::new("hostile");
    let base = tmp.join("base");
    fs::create_dir_all(base.join("evidence")).unwrap();
    fs::write(base.join("evidence/a.txt"), b"hello").unwrap();
    fs::write(base.join("evidence/b.txt"), b"bye").unwrap();
    fs::write(base.join("evidence/empty.bin"), b"").unwrap();
    fs::write(tmp.join("outside.txt"), b"secret").unwrap();
    #[cfg(unix)]
    std::os::unix::fs::symlink(tmp.join("outside.txt"), base.join("evidence/link.txt")).unwrap();
    let hello = sha256_hex(b"hello");
    let secret = sha256_hex(b"secret");
    let empty = sha256_hex(b"");

    let mut rows = vec![
        row("ev-ok", "evidence/a.txt", &hello),
        row("ev-traversal", "../outside.txt", &secret),
        row("ev-traversal-deep", "evidence/../../outside.txt", &secret),
        row(
            "ev-absolute",
            &tmp.join("outside.txt").to_string_lossy(),
            &secret,
        ),
        row("ev-mismatch", "evidence/b.txt", &hello),
        row("ev-upper", "evidence/a.txt", &hello.to_uppercase()),
        row("ev-empty", "evidence/empty.bin", &empty),
        row("ev-dns", "dns:TXT:example.gov", &sha256_hex(b"v=spf1 -all")),
        row("ev-missing", "evidence/missing.txt", &hello),
        row("ev-dir", "evidence", &hello),
        row("ev-backslash", "evidence\\a.txt", &hello),
    ];
    #[cfg(unix)]
    rows.push(row("ev-symlink", "evidence/link.txt", &secret));
    write_rows(&tmp.join("EVIDENCE.jsonl"), &rows);

    let store = Store::open(tmp.join("store")).unwrap();
    let report = store
        .ingest_evidence_jsonl(&tmp.join("EVIDENCE.jsonl"), &base, Producer::Infra)
        .unwrap();
    assert_eq!(report.records_indexed, rows.len());
    assert!(report.rejected.is_empty());

    let problem_ids: Vec<&str> = report
        .payload_problems
        .iter()
        .map(|p| p.evidence_id.as_str())
        .collect();
    for id in [
        "ev-traversal",
        "ev-traversal-deep",
        "ev-absolute",
        "ev-mismatch",
        "ev-missing",
        "ev-dir",
        "ev-backslash",
    ] {
        assert!(
            problem_ids.contains(&id),
            "{id} should be reported: {:?}",
            report.payload_problems
        );
    }
    #[cfg(unix)]
    {
        assert!(problem_ids.contains(&"ev-symlink"));
        let p = report
            .payload_problems
            .iter()
            .find(|p| p.evidence_id == "ev-symlink")
            .unwrap();
        assert!(p.reason.contains("outside"), "{}", p.reason);
    }
    for id in ["ev-ok", "ev-upper", "ev-empty", "ev-dns"] {
        assert!(!problem_ids.contains(&id), "{id} must not be reported");
    }
    let mismatch = report
        .payload_problems
        .iter()
        .find(|p| p.evidence_id == "ev-mismatch")
        .unwrap();
    assert!(mismatch.reason.contains("hashes to"), "{}", mismatch.reason);

    let index = lines(&tmp.join("store/index.jsonl"));
    let by_id = |id: &str| {
        index
            .iter()
            .find(|l| l["evidence_id"] == id)
            .unwrap()
            .clone()
    };
    assert_eq!(by_id("ev-ok")["stored"], format!("objects/{hello}.txt"));
    assert_eq!(by_id("ev-ok")["bytes"], 5);
    assert_eq!(
        by_id("ev-upper")["sha256"],
        hello,
        "the index hash is lowercase"
    );
    assert_eq!(
        by_id("ev-upper")["record"]["sha256"],
        hello.to_uppercase(),
        "the record is kept exactly as the producer wrote it"
    );
    assert_eq!(by_id("ev-upper")["stored"], format!("objects/{hello}.txt"));
    assert_eq!(by_id("ev-empty")["bytes"], 0);
    assert_eq!(by_id("ev-empty")["stored"], format!("objects/{empty}.bin"));
    for id in [
        "ev-traversal",
        "ev-absolute",
        "ev-mismatch",
        "ev-dns",
        "ev-missing",
        "ev-dir",
        "ev-backslash",
    ] {
        assert!(by_id(id)["stored"].is_null(), "{id}");
    }
    assert!(
        !store.dir().join(format!("objects/{secret}.txt")).exists(),
        "the secret never enters the store"
    );
    assert_eq!(
        fs::read_dir(tmp.join("store/objects")).unwrap().count(),
        2,
        "only a.txt and the empty body"
    );
    let verify = store.verify().unwrap();
    assert!(verify.is_clean(), "{verify:?}");
}

#[test]
fn rows_that_cannot_be_indexed_are_rejected_and_reported() {
    let tmp = TempDir::new("rejects");
    let base = tmp.join("base");
    fs::create_dir_all(&base).unwrap();
    let hello = sha256_hex(b"hello");
    let mut text = String::new();
    text.push_str("this is not json\n");
    text.push_str(
        &json!({ "run_id": RUN, "kind": "http_response", "payload_ref": "x", "sha256": hello })
            .to_string(),
    );
    text.push('\n');
    let mut bad_kind = row("ev-badkind", "dns:TXT:x", &hello);
    bad_kind["kind"] = json!("telepathy");
    text.push_str(&format!("{bad_kind}\n"));
    text.push_str(&format!("{}\n", row("ev-shortsha", "dns:TXT:x", "abc123")));
    text.push_str(&format!(
        "{}\n",
        row("ev-nothex", "dns:TXT:x", &"z".repeat(64))
    ));
    text.push_str("[1,2,3]\n");
    text.push_str(&format!("\n{}\n", row("ev-good", "dns:TXT:x", &hello)));
    fs::write(tmp.join("EVIDENCE.jsonl"), text).unwrap();

    let store = Store::open(tmp.join("store")).unwrap();
    let report = store
        .ingest_evidence_jsonl(&tmp.join("EVIDENCE.jsonl"), &base, Producer::Other)
        .unwrap();
    assert_eq!(report.records_indexed, 1);
    assert_eq!(report.rejected.len(), 6, "{:?}", report.rejected);
    assert!(report.rejected[0].starts_with("line 1:"));
    assert_eq!(line_count(&tmp.join("store/index.jsonl")), 1);
    validate_store(&tmp.join("store"));
}

#[test]
fn source_rows_are_indexed_without_looking_for_a_file() {
    let tmp = TempDir::new("source");
    let base = tmp.join("repo");
    fs::create_dir_all(base.join("src")).unwrap();
    fs::write(base.join("src/index.html"), b"<html></html>").unwrap();
    let mut span = row(
        "ev-abcdef0123-0123456789",
        "src/index.html",
        &sha256_hex(b"<html></html>"),
    );
    span["kind"] = json!("source_span");
    write_rows(&tmp.join("EVIDENCE.jsonl"), &[span]);
    let store = Store::open(tmp.join("store")).unwrap();
    let report = store
        .ingest_evidence_jsonl(&tmp.join("EVIDENCE.jsonl"), &base, Producer::Source)
        .unwrap();
    assert_eq!(report.records_indexed, 1);
    assert!(report.payload_problems.is_empty());
    let line = &lines(&tmp.join("store/index.jsonl"))[0];
    assert!(line["stored"].is_null());
    assert_eq!(line["producer"], "source");
    assert_eq!(fs::read_dir(tmp.join("store/objects")).unwrap().count(), 0);
    validate_store(&tmp.join("store"));
}

// ---- truncated-object -----------------------------------------------------

#[test]
fn a_truncated_object_is_reported_and_its_key_is_redone() {
    let (tmp, store, expected, keys) = fixture_store("truncated-object");
    let verify = store.verify().unwrap();
    assert!(!verify.is_clean());
    let want: Vec<String> = expected["mismatched"]
        .as_array()
        .unwrap()
        .iter()
        .map(|v| v.as_str().unwrap().to_string())
        .collect();
    assert_eq!(verify.mismatched, want);
    assert!(verify.missing.is_empty());
    let (nr, nk) = n();
    assert_eq!((verify.objects, verify.ok), (nr, nr - 1));

    let plan = store
        .plan_resume(&keys, expected["run_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(plan.reuse.len(), nk - 1);
    assert_eq!(plan.redo.len(), 1);
    assert_eq!(plan.redo[0].kind, expected["redo_kind"].as_str().unwrap());
    assert_eq!(plan.reasons.len(), 1);
    assert!(
        plan.reasons[0]
            .1
            .contains(expected["reason_contains"].as_str().unwrap()),
        "{}",
        plan.reasons[0].1
    );
    assert_eq!(store.reuse_records().unwrap().len(), nk - 1);
    validate_store(&tmp.join("store"));

    // Re-ingesting the capture repairs the object and the key becomes reusable.
    let dir = capture_dir(CAPTURE);
    let report = store
        .ingest_captures(&dir, &context(final_url(&dir)))
        .unwrap();
    assert_eq!(report.objects_replaced.len(), 1);
    assert!(store.verify().unwrap().is_clean());
    let healed = store.plan_resume(&keys, RUN_2).unwrap();
    assert_eq!((healed.reuse.len(), healed.redo.len()), (nk, 0));
}

// ---- resume ---------------------------------------------------------------

#[test]
fn compatible_keys_are_all_reused_and_recorded() {
    let (tmp, store, expected, keys) = fixture_store("resume-compatible");
    assert!(store.verify().unwrap().is_clean());
    let plan = store
        .plan_resume(&keys, expected["run_id"].as_str().unwrap())
        .unwrap();
    assert_eq!(plan.reuse.len(), n().1);
    assert!(plan.redo.is_empty() && plan.reasons.is_empty());
    let on_disk = store.reuse_records().unwrap();
    assert_eq!(on_disk, plan.reuse);
    for record in &on_disk {
        assert!(record.verified);
        assert_eq!(record.run_id, RUN_2);
        assert_eq!(record.reused_from_run_id, RUN);
        assert_eq!(record.sha256s.len(), 1);
        assert!(!record.reason.is_empty());
    }
    validate_store(&tmp.join("store"));

    // The same key listed twice is planned once.
    let doubled = vec![keys[0].clone(), keys[0].clone()];
    let plan = store.plan_resume(&doubled, RUN_2).unwrap();
    assert_eq!((plan.reuse.len(), plan.redo.len()), (1, 0));
}

#[test]
fn a_changed_engine_version_redoes_everything() {
    let (tmp, store, expected, keys) = fixture_store("resume-engine-changed");
    assert!(keys.iter().all(|k| k.engine_version == "0.2.0"));
    let before = line_count(&tmp.join("store/reuse.jsonl"));
    let plan = store
        .plan_resume(&keys, expected["run_id"].as_str().unwrap())
        .unwrap();
    assert!(plan.reuse.is_empty());
    assert_eq!(plan.redo.len(), n().1);
    assert_eq!(plan.reasons.len(), n().1);
    for (_, why) in &plan.reasons {
        assert!(why.contains("engine_version"), "{why}");
        assert!(
            why.contains(expected["cached_engine_version"].as_str().unwrap()),
            "{why}"
        );
        assert!(why.contains("0.2.0"), "{why}");
    }
    assert_eq!(
        line_count(&tmp.join("store/reuse.jsonl")),
        before,
        "no reuse is recorded"
    );
    // The old key is still there and still reusable at its own version.
    let old = store.lookup(&{
        let mut k = keys[0].clone();
        k.engine_version = expected["cached_engine_version"]
            .as_str()
            .unwrap()
            .to_string();
        k
    });
    assert_eq!(old.unwrap().status, KeyStatus::Complete);
    assert!(store.lookup(&keys[0]).is_none());
}

#[test]
fn rule_and_protocol_versions_gate_reuse_too() {
    let (_tmp, store, _, keys) = fixture_store("resume-compatible");
    let mut rule = keys[0].clone();
    rule.rule_version = Some("a11y-wcag21-aa@0.2.0".into());
    let mut protocol = keys[1].clone();
    protocol.protocol_version = Some("2.0.0".into());
    let plan = store
        .plan_resume(&[rule, protocol, keys[2].clone()], RUN_2)
        .unwrap();
    assert_eq!((plan.reuse.len(), plan.redo.len()), (1, 2));
    assert!(
        plan.reasons[0].1.contains("rule_version"),
        "{}",
        plan.reasons[0].1
    );
    assert!(
        plan.reasons[1].1.contains("protocol_version"),
        "{}",
        plan.reasons[1].1
    );
}

#[test]
fn an_unknown_key_is_redone_with_a_plain_reason() {
    let (_tmp, store, _, keys) = fixture_store("resume-compatible");
    let mut other = keys[0].clone();
    other.route = "https://example.gov/elsewhere".into();
    let plan = store.plan_resume(&[other], RUN_2).unwrap();
    assert_eq!(plan.redo.len(), 1);
    assert_eq!(plan.reasons[0].1, "no cache record for this key");
}

#[test]
fn invalidated_keys_are_not_reused() {
    let (tmp, store, _, keys) = fixture_store("resume-compatible");
    let invalidated = store.invalidate(|r| r.key.kind == "dom_snapshot").unwrap();
    assert_eq!(invalidated, 1);
    assert_eq!(
        store.invalidate(|r| r.key.kind == "dom_snapshot").unwrap(),
        0,
        "already invalidated"
    );
    let dom = keys.iter().find(|k| k.kind == "dom_snapshot").unwrap();
    assert_eq!(store.lookup(dom).unwrap().status, KeyStatus::Invalidated);
    assert_eq!(
        line_count(&tmp.join("store/keys.jsonl")),
        n().1 + 1,
        "history is kept, a line is appended"
    );
    let plan = store.plan_resume(&keys, RUN_2).unwrap();
    assert_eq!((plan.reuse.len(), plan.redo.len()), (n().1 - 1, 1));
    assert!(
        plan.reasons[0].1.contains("invalidated"),
        "{}",
        plan.reasons[0].1
    );
    validate_store(&tmp.join("store"));

    // A fresh capture makes the key usable again: the newest record wins.
    let dir = capture_dir(CAPTURE);
    let report = store
        .ingest_captures(&dir, &context(final_url(&dir)))
        .unwrap();
    assert_eq!(report.keys_recorded, 1);
    assert_eq!(store.lookup(dom).unwrap().status, KeyStatus::Complete);
}

#[test]
fn a_key_record_whose_hash_does_not_match_its_key_is_ignored() {
    let (tmp, store, _, keys) = fixture_store("resume-compatible");
    let path = tmp.join("store/keys.jsonl");
    let mut forged = lines(&path)[0].clone();
    forged["key"]["route"] = json!("https://forged.example/");
    fs::write(
        &path,
        format!("{}{}\n", fs::read_to_string(&path).unwrap(), forged),
    )
    .unwrap();
    let verify = store.verify().unwrap();
    assert!(!verify.is_clean());
    assert!(
        verify.malformed.iter().any(|m| m.contains("key_hash")),
        "{:?}",
        verify.malformed
    );
    let plan = store.plan_resume(&keys, RUN_2).unwrap();
    assert_eq!(plan.reuse.len(), n().1, "the genuine records still work");
}

#[test]
fn ten_thousand_keys_resolve_quickly_and_correctly() {
    let tmp = TempDir::new("many-keys");
    let store = Store::open(tmp.join("store")).unwrap();
    let template = fixture_store("resume-compatible").3[0].clone();
    let mut text = String::new();
    let mut wanted = Vec::new();
    for i in 0..10_000 {
        let mut key = template.clone();
        key.route = format!("https://example.gov/page/{i}");
        let record = json!({
            "schema_version": "1.0.0", "key_hash": key.hash(), "key": key,
            "missing_reasons": {"role": "r", "state": "s"}, "status": "complete",
            "sha256s": [sha256_hex(i.to_string().as_bytes())], "run_id": RUN, "created_at": "2026-10-18T12:00:00Z"
        });
        text.push_str(&format!("{record}\n"));
        wanted.push(key);
    }
    fs::write(tmp.join("store/keys.jsonl"), text).unwrap();
    assert_eq!(store.key_records().unwrap().len(), 10_000);
    assert_eq!(store.lookup(&wanted[4_999]).unwrap().run_id, RUN);
    // Objects do not exist, so every key is redone, each with a reason.
    let plan = store.plan_resume(&wanted, RUN_2).unwrap();
    assert!(plan.reuse.is_empty());
    assert_eq!(plan.redo.len(), 10_000);
    assert!(
        plan.reasons[0].1.contains("is missing"),
        "{}",
        plan.reasons[0].1
    );
    // Keys at another engine version get the version reason without a quadratic scan.
    let moved: Vec<CacheKey> = wanted
        .iter()
        .map(|k| {
            let mut k = k.clone();
            k.engine_version = "9.9.9".into();
            k
        })
        .collect();
    let plan = store.plan_resume(&moved, RUN_2).unwrap();
    assert_eq!(plan.redo.len(), 10_000);
    assert!(
        plan.reasons[7].1.contains("engine_version"),
        "{}",
        plan.reasons[7].1
    );
}

// ---- tombstone ------------------------------------------------------------

#[test]
fn a_tombstoned_payload_keeps_its_provenance() {
    let (tmp, store, expected, keys) = fixture_store("tombstone");
    let id = expected["evidence_id"].as_str().unwrap();
    let verify = store.verify().unwrap();
    assert_eq!(verify.tombstoned, 1);
    assert!(
        verify.is_clean(),
        "a tombstoned payload is not missing: {verify:?}"
    );
    assert_eq!(verify.objects, n().0 - 1);

    let record = store.find_record(id).unwrap().expect("provenance survives");
    assert_eq!(record.evidence_id, id);
    assert!(
        record.stored.is_some(),
        "the record line still says where the payload was"
    );
    let dom = keys.iter().find(|k| k.kind == "dom_snapshot").unwrap();
    let key_record = store.lookup(dom).expect("lookup still answers");
    assert_eq!(key_record.run_id, RUN);
    assert_eq!(key_record.sha256s, vec![record.sha256.clone()]);

    let tombstone = lines(&tmp.join("store/index.jsonl"))
        .into_iter()
        .find(|l| l["entry"] == "tombstone")
        .unwrap();
    assert!(!tombstone["reason"].as_str().unwrap().is_empty());
    assert!(!tombstone["replay_limitation"].as_str().unwrap().is_empty());

    let plan = store.plan_resume(&keys, RUN_2).unwrap();
    assert_eq!((plan.reuse.len(), plan.redo.len()), (n().1 - 1, 1));
    assert_eq!(plan.redo[0].kind, "dom_snapshot");
    assert!(
        plan.reasons[0]
            .1
            .contains(expected["reason_contains"].as_str().unwrap()),
        "{}",
        plan.reasons[0].1
    );
    validate_store(&tmp.join("store"));

    // Marking it again changes nothing.
    let before = line_count(&tmp.join("store/index.jsonl"));
    store.mark_unavailable(id, "again", "still gone").unwrap();
    assert_eq!(line_count(&tmp.join("store/index.jsonl")), before);
}

#[test]
fn a_shared_object_survives_until_its_last_live_record_is_tombstoned() {
    let tmp = TempDir::new("shared");
    let base = tmp.join("base");
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("a.txt"), b"hello").unwrap();
    let hello = sha256_hex(b"hello");
    write_rows(
        &tmp.join("EVIDENCE.jsonl"),
        &[
            row("ev-one", "a.txt", &hello),
            row("ev-two", "a.txt", &hello),
        ],
    );
    let store = Store::open(tmp.join("store")).unwrap();
    store
        .ingest_evidence_jsonl(&tmp.join("EVIDENCE.jsonl"), &base, Producer::Other)
        .unwrap();
    let object = tmp.join(format!("store/objects/{hello}.txt"));
    assert!(object.exists());

    store
        .mark_unavailable("ev-one", "removed", "cannot replay one")
        .unwrap();
    assert!(object.exists(), "ev-two still needs it");
    assert!(store.verify().unwrap().is_clean());
    store
        .mark_unavailable("ev-two", "removed", "cannot replay two")
        .unwrap();
    assert!(!object.exists());
    let verify = store.verify().unwrap();
    assert!(verify.is_clean(), "{verify:?}");
    assert_eq!(verify.tombstoned, 2);

    assert!(matches!(
        store.mark_unavailable("ev-nope", "r", "l"),
        Err(EvidenceError::NotFound(_))
    ));
    assert!(matches!(
        store.mark_unavailable("ev-one", " ", "l"),
        Err(EvidenceError::Invalid(_))
    ));
    validate_store(&tmp.join("store"));
}

#[test]
fn a_stored_path_with_a_different_hash_is_not_deleted_while_another_record_uses_it() {
    let tmp = TempDir::new("path-share");
    let base = tmp.join("base");
    fs::create_dir_all(&base).unwrap();
    fs::write(base.join("a.txt"), b"hello").unwrap();
    let hello = sha256_hex(b"hello");
    write_rows(
        &tmp.join("EVIDENCE.jsonl"),
        &[
            row("ev-one", "a.txt", &hello),
            row("ev-two", "a.txt", &hello),
        ],
    );
    let store = Store::open(tmp.join("store")).unwrap();
    store
        .ingest_evidence_jsonl(&tmp.join("EVIDENCE.jsonl"), &base, Producer::Other)
        .unwrap();
    // Make ev-one's record claim another hash while its stored path is unchanged.
    let path = tmp.join("store/index.jsonl");
    let text = fs::read_to_string(&path).unwrap();
    let mut out = String::new();
    for l in text.lines() {
        let mut v: Value = serde_json::from_str(l).unwrap();
        if v["evidence_id"] == "ev-one" {
            v["sha256"] = json!(sha256_hex(b"other"));
        }
        out.push_str(&format!("{v}\n"));
    }
    fs::write(&path, out).unwrap();
    let object = tmp.join(format!("store/objects/{hello}.txt"));
    store
        .mark_unavailable("ev-one", "removed", "cannot replay")
        .unwrap();
    assert!(object.exists(), "ev-two still names this object");
}

// ---- derive ---------------------------------------------------------------

#[test]
fn a_redacted_derivative_links_back_to_its_parent() {
    let case = fixtures().join("infra-ingest");
    let tmp = TempDir::new("derive");
    let store = Store::open_with_clock(tmp.join("store"), fixed("2026-10-18T12:00:00Z")).unwrap();
    store
        .ingest_evidence_jsonl(&case.join("EVIDENCE.jsonl"), &case, Producer::Infra)
        .unwrap();
    let parent = lines(&tmp.join("store/index.jsonl"))
        .into_iter()
        .find(|l| l["kind"] == "http_response" && !l["stored"].is_null())
        .unwrap();
    let parent_id = parent["evidence_id"].as_str().unwrap();

    let bytes = b"<html>body with the token removed</html>";
    let record: EvidenceRecord = store
        .derive(
            parent_id,
            bytes,
            "html",
            Redaction {
                applied: false,
                flags: vec!["token_removed".into()],
            },
            "http_response",
        )
        .unwrap();
    let sha = sha256_hex(bytes);
    assert_eq!(record.evidence_id, format!("ev-drv-{}", &sha[..12]));
    assert_eq!(record.derived_from.as_deref(), Some(parent_id));
    assert!(
        record.redaction.applied,
        "a derivative is always marked redacted"
    );
    assert_eq!(record.redaction.flags, vec!["token_removed".to_string()]);
    assert_eq!(record.sha256, sha);
    assert_eq!(record.captured_at, "2026-10-18T12:00:00Z");
    assert!(record.limitations.iter().any(|l| l.contains(parent_id)));
    assert_valid(
        &validator("evidence-record.schema.json"),
        &serde_json::to_value(&record).unwrap(),
        "derived record",
    );
    assert_eq!(
        fs::read(tmp.join(format!("store/objects/{sha}.html"))).unwrap(),
        bytes
    );

    let last = lines(&tmp.join("store/index.jsonl")).pop().unwrap();
    assert_eq!(last["evidence_id"], record.evidence_id);
    assert_eq!(last["producer"], "other");
    validate_store(&tmp.join("store"));
    assert!(store.verify().unwrap().is_clean());

    // Same bytes again: same id, no second line.
    let before = line_count(&tmp.join("store/index.jsonl"));
    let again = store
        .derive(
            parent_id,
            bytes,
            "html",
            Redaction {
                applied: true,
                flags: vec![],
            },
            "http_response",
        )
        .unwrap();
    assert_eq!(again.evidence_id, record.evidence_id);
    assert_eq!(line_count(&tmp.join("store/index.jsonl")), before);

    // The parent can be tombstoned and still be derived from.
    store
        .mark_unavailable(parent_id, "contains a token", "original cannot be replayed")
        .unwrap();
    store
        .derive(
            parent_id,
            b"other bytes",
            "html",
            Redaction {
                applied: true,
                flags: vec![],
            },
            "http_response",
        )
        .unwrap();

    assert!(matches!(
        store.derive(
            "ev-missing",
            b"x",
            "txt",
            Redaction {
                applied: true,
                flags: vec![]
            },
            "http_response"
        ),
        Err(EvidenceError::NotFound(_))
    ));
    assert!(matches!(
        store.derive(
            parent_id,
            b"x",
            "txt",
            Redaction {
                applied: true,
                flags: vec![]
            },
            "telepathy"
        ),
        Err(EvidenceError::Invalid(_))
    ));
}

// ---- verify ---------------------------------------------------------------

#[test]
fn verify_flags_strays_symlinks_and_misnamed_objects() {
    let (tmp, store) = ingested_capture_store("verify");
    let objects = tmp.join("store/objects");
    assert!(store.verify().unwrap().is_clean());

    fs::write(objects.join(".stale.tmp-1-2-3"), b"half").unwrap();
    fs::write(objects.join("README.txt"), b"hi").unwrap();
    fs::write(
        objects.join(format!("{}.json", "A".repeat(64))),
        b"upper case name",
    )
    .unwrap();
    fs::create_dir(objects.join("subdir")).unwrap();
    let report = store.verify().unwrap();
    let (nr, _) = n();
    assert_eq!(report.objects, nr + 4);
    assert_eq!(report.ok, nr);
    assert_eq!(report.mismatched.len(), 4, "{:?}", report.mismatched);
    assert!(report.mismatched.iter().all(|p| p.starts_with("objects/")));
    assert!(!report.is_clean());

    #[cfg(unix)]
    {
        let (tmp2, store2) = ingested_capture_store("verify-symlink");
        let mut names: Vec<PathBuf> = fs::read_dir(tmp2.join("store/objects"))
            .unwrap()
            .map(|e| e.unwrap().path())
            .collect();
        names.sort();
        let (victim, donor) = (names[0].clone(), names[1].clone());
        fs::remove_file(&victim).unwrap();
        std::os::unix::fs::symlink(&donor, &victim).unwrap();
        let report = store2.verify().unwrap();
        assert_eq!(report.mismatched.len(), 1, "{report:?}");
        assert!(report.mismatched[0].ends_with(victim.file_name().unwrap().to_str().unwrap()));
        // Re-ingesting replaces the link with a real file.
        let dir = capture_dir(CAPTURE);
        let again = store2
            .ingest_captures(&dir, &context(final_url(&dir)))
            .unwrap();
        assert_eq!(again.objects_replaced.len(), 1);
        assert!(!fs::symlink_metadata(&victim)
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(store2.verify().unwrap().is_clean());
    }
}

#[test]
fn verify_reports_an_object_the_index_still_expects() {
    let (tmp, store) = ingested_capture_store("verify-missing");
    let victim = fs::read_dir(tmp.join("store/objects"))
        .unwrap()
        .next()
        .unwrap()
        .unwrap()
        .path();
    fs::remove_file(&victim).unwrap();
    let report = store.verify().unwrap();
    assert_eq!(report.missing.len(), 1);
    assert!(report.missing[0].starts_with("ev-cap-"));
    assert!(report.mismatched.is_empty());
    assert!(!report.is_clean());
}

#[test]
fn a_zero_byte_object_with_the_right_name_is_fine_and_a_wrong_one_is_not() {
    let tmp = TempDir::new("zero");
    let store = Store::open(tmp.join("store")).unwrap();
    fs::write(
        tmp.join(format!("store/objects/{}.bin", sha256_hex(b""))),
        b"",
    )
    .unwrap();
    assert!(store.verify().unwrap().is_clean());
    fs::write(
        tmp.join(format!("store/objects/{}.bin", sha256_hex(b"x"))),
        b"",
    )
    .unwrap();
    assert_eq!(store.verify().unwrap().mismatched.len(), 1);
}

#[test]
fn torn_and_garbage_log_lines_are_reported_not_fatal() {
    let (tmp, store) = ingested_capture_store("torn");
    let path = tmp.join("store/index.jsonl");
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("{\"entry\":\"record\",\"schema_version\":\"1.0.0\",\"evid");
    fs::write(&path, text).unwrap();
    let report = store.verify().unwrap();
    assert_eq!(report.malformed.len(), 1, "{:?}", report.malformed);
    assert!(
        report.malformed[0].starts_with(&format!("index.jsonl:{}:", n().0 + 1)),
        "{}",
        report.malformed[0]
    );
    assert!(!report.is_clean());
    assert_eq!(store.index_entries().unwrap().len(), n().0);
}

// ---- store file -----------------------------------------------------------

#[test]
fn a_store_with_another_version_is_refused() {
    let tmp = TempDir::new("version");
    fs::write(
        tmp.join("STORE.json"),
        r#"{"schema_version":"1.0.0","created_at":"x","store_version":"2"}"#,
    )
    .unwrap();
    match Store::open(tmp.path()) {
        Err(EvidenceError::StoreVersion {
            found, expected, ..
        }) => assert_eq!((found.as_str(), expected.as_str()), ("2", "1")),
        other => panic!("expected StoreVersion, got {other:?}"),
    }
    fs::write(tmp.join("STORE.json"), "not json").unwrap();
    assert!(matches!(
        Store::open(tmp.path()),
        Err(EvidenceError::Invalid(_))
    ));
    let empty = TempDir::new("not-a-store");
    assert!(matches!(
        Store::open_existing(empty.path()),
        Err(EvidenceError::NotAStore { .. })
    ));
}

#[test]
fn opening_twice_keeps_the_original_store_file() {
    let tmp = TempDir::new("reopen");
    let dir = tmp.join("store");
    Store::open_with_clock(&dir, fixed("2026-01-01T00:00:00Z")).unwrap();
    let first = fs::read(dir.join("STORE.json")).unwrap();
    Store::open_with_clock(&dir, fixed("2027-01-01T00:00:00Z")).unwrap();
    assert_eq!(fs::read(dir.join("STORE.json")).unwrap(), first);
    let value: Value = serde_json::from_slice(&first).unwrap();
    assert_eq!(value["created_at"], "2026-01-01T00:00:00Z");
    assert_eq!(value["store_version"], "1");
    assert_eq!(value["schema_version"], "1.0.0");
    for name in ["index.jsonl", "keys.jsonl", "reuse.jsonl"] {
        assert!(dir.join(name).is_file());
    }
    assert!(dir.join("objects").is_dir());
}

#[test]
fn two_openers_race_to_create_one_store_file() {
    let tmp = TempDir::new("race");
    let dir = tmp.join("store");
    std::thread::scope(|scope| {
        for ts in [
            "2026-01-01T00:00:01Z",
            "2026-01-01T00:00:02Z",
            "2026-01-01T00:00:03Z",
            "2026-01-01T00:00:04Z",
        ] {
            let dir = &dir;
            scope.spawn(move || {
                Store::open_with_clock(dir, fixed(ts)).unwrap();
            });
        }
    });
    let value = read_json(&dir.join("STORE.json"));
    assert!(value["created_at"]
        .as_str()
        .unwrap()
        .starts_with("2026-01-01T00:00:0"));
    let leftovers: Vec<_> = fs::read_dir(&dir)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert!(
        leftovers
            .iter()
            .all(|n| !n.to_string_lossy().contains(".tmp-")),
        "{leftovers:?}"
    );
}

#[test]
fn every_timestamp_comes_from_the_injected_clock() {
    let tmp = TempDir::new("clock");
    let dir = tmp.join("store");
    let store = Store::open_with_clock(&dir, fixed("2031-02-03T04:05:06Z")).unwrap();
    let capture = capture_dir(CAPTURE);
    store
        .ingest_captures(&capture, &context(final_url(&capture)))
        .unwrap();
    assert_eq!(
        read_json(&dir.join("STORE.json"))["created_at"],
        "2031-02-03T04:05:06Z"
    );
    for line in lines(&dir.join("index.jsonl")) {
        assert_eq!(line["ingested_at"], "2031-02-03T04:05:06Z");
    }
    for line in lines(&dir.join("keys.jsonl")) {
        assert_eq!(line["created_at"], "2031-02-03T04:05:06Z");
    }
    let keys: Vec<CacheKey> = store
        .key_records()
        .unwrap()
        .into_iter()
        .map(|r| r.key)
        .collect();
    store.plan_resume(&keys, RUN_2).unwrap();
    for line in lines(&dir.join("reuse.jsonl")) {
        assert_eq!(line["recorded_at"], "2031-02-03T04:05:06Z");
    }
    let store = store.with_clock(fixed("2040-01-01T00:00:00Z"));
    let first_id = lines(&dir.join("index.jsonl"))[0]["evidence_id"]
        .as_str()
        .unwrap()
        .to_string();
    store.mark_unavailable(&first_id, "r", "l").unwrap();
    let tombstone = lines(&dir.join("index.jsonl")).pop().unwrap();
    assert_eq!(tombstone["marked_at"], "2040-01-01T00:00:00Z");
}

// ---- manifest duplicates (diligent-capture) --------------------------------

fn manifest_lines(dir: &Path) -> Vec<String> {
    fs::read_to_string(dir.join("manifest.jsonl"))
        .unwrap()
        .lines()
        .map(str::to_string)
        .collect()
}

fn with_manifest(dir: &Path, lines: &[String]) {
    fs::write(
        dir.join("manifest.jsonl"),
        format!("{}\n", lines.join("\n")),
    )
    .unwrap();
}

fn modified(line: &str, f: impl FnOnce(&mut Value)) -> String {
    let mut value: Value = serde_json::from_str(line).unwrap();
    f(&mut value);
    value.to_string()
}

#[test]
fn identical_manifest_lines_collapse() {
    let tmp = TempDir::new("dup");
    copy_dir(&capture_dir(CAPTURE), &tmp.join("cap"));
    let mut lines = manifest_lines(&tmp.join("cap"));
    assert_eq!(lines.len(), n().0);
    lines.push(lines[0].clone());
    lines.push(String::new());
    lines.push(lines[2].clone());
    with_manifest(&tmp.join("cap"), &lines);
    let set = CaptureSet::load(&tmp.join("cap")).unwrap();
    assert_eq!(set.entries().len(), n().0);
    let store = Store::open(tmp.join("store")).unwrap();
    let report = store
        .ingest_captures(&tmp.join("cap"), &context("https://example.gov/".into()))
        .unwrap();
    assert_eq!((report.records_indexed, report.keys_recorded), n());
}

#[test]
fn conflicting_manifest_lines_for_one_path_are_an_error_naming_the_line() {
    let tmp = TempDir::new("conflict");
    copy_dir(&capture_dir(CAPTURE), &tmp.join("cap"));
    let original = manifest_lines(&tmp.join("cap"));
    let first = original[0].clone();
    let conflicts: Vec<(&str, String)> = vec![
        (
            "sha256",
            modified(&first, |v| v["sha256"] = json!("0".repeat(64))),
        ),
        ("kind", modified(&first, |v| v["kind"] = json!("ax_tree"))),
        ("bytes", modified(&first, |v| v["bytes"] = json!(1))),
    ];
    for (what, bad) in conflicts {
        let mut lines = original.clone();
        lines.push(bad);
        with_manifest(&tmp.join("cap"), &lines);
        match CaptureSet::load(&tmp.join("cap")) {
            Err(CaptureError::Malformed {
                line: Some(bad_line),
                message,
                ..
            }) if bad_line == original.len() + 1 => {
                assert!(message.contains("conflicting"), "{what}: {message}");
                assert!(message.contains("line 1"), "{what}: {message}");
            }
            other => {
                panic!("{what}: expected a Malformed error on the appended line, got {other:?}")
            }
        }
    }
}

#[test]
fn one_file_listed_for_two_media_is_not_a_conflict() {
    let tmp = TempDir::new("two-media");
    copy_dir(&capture_dir(CAPTURE), &tmp.join("cap"));
    let mut lines = manifest_lines(&tmp.join("cap"));
    lines.push(modified(&lines[0], |v| v["media"] = json!("dark")));
    with_manifest(&tmp.join("cap"), &lines);
    let set = CaptureSet::load(&tmp.join("cap")).unwrap();
    assert_eq!(set.entries().len(), n().0 + 1);
    let store = Store::open(tmp.join("store")).unwrap();
    let report = store
        .ingest_captures(&tmp.join("cap"), &context("https://example.gov/".into()))
        .unwrap();
    assert_eq!(
        report.records_indexed,
        n().0,
        "one evidence id per distinct payload"
    );
    assert_eq!(report.keys_recorded, n().1 + 1, "but a key per media mode");
    assert_eq!(
        fs::read_dir(tmp.join("store/objects")).unwrap().count(),
        n().0
    );
}

#[test]
fn an_upper_case_manifest_hash_is_ingested_as_lower_case() {
    let tmp = TempDir::new("hash-case");
    copy_dir(&capture_dir(CAPTURE), &tmp.join("cap"));
    let lines: Vec<String> = manifest_lines(&tmp.join("cap"))
        .iter()
        .map(|l| {
            modified(l, |v| {
                let upper = v["sha256"].as_str().unwrap().to_uppercase();
                v["sha256"] = json!(upper);
            })
        })
        .collect();
    with_manifest(&tmp.join("cap"), &lines);
    let store = Store::open(tmp.join("store")).unwrap();
    store
        .ingest_captures(&tmp.join("cap"), &context("https://example.gov/".into()))
        .unwrap();
    for line in lines_of(&tmp.join("store/index.jsonl")) {
        let sha = line["sha256"].as_str().unwrap();
        assert_eq!(sha, sha.to_lowercase());
        assert!(line["evidence_id"].as_str().unwrap().starts_with("ev-cap-"));
        assert_eq!(line["evidence_id"], format!("ev-cap-{}", &sha[..12]));
    }
    assert!(store.verify().unwrap().is_clean());
    validate_store(&tmp.join("store"));
}

fn lines_of(path: &Path) -> Vec<Value> {
    lines(path)
}

// ---- freshness ------------------------------------------------------------

fn sample_record(valid_until: Option<&str>) -> EvidenceRecord {
    serde_json::from_value(json!({
        "schema_version": "1.0.0", "evidence_id": "ev-x", "run_id": RUN, "kind": "owner_assertion",
        "collector": "c", "collector_version": "1", "captured_at": "2026-10-18T12:00:00Z", "snapshot_ref": null,
        "context": {}, "payload_ref": "p", "sha256": "0".repeat(64), "data_classification": "public",
        "redaction": {"applied": false, "flags": []}, "trust": "owner_assertion", "limitations": ["l"],
        "derived_from": null, "valid_until": valid_until
    }))
    .unwrap()
}

#[test]
fn freshness_honours_valid_until() {
    assert!(is_fresh(&sample_record(None), "2999-01-01T00:00:00Z"));
    let r = sample_record(Some("2026-10-18T12:00:00Z"));
    assert!(is_fresh(&r, "2026-10-18T11:59:59Z"));
    assert!(!is_fresh(&r, "2026-10-18T12:00:00Z"), "expiry is exclusive");
    assert!(!is_fresh(&r, "2026-10-18T12:00:01Z"));
    assert!(
        is_fresh(&r, "2026-10-18T13:59:59+02:00"),
        "offsets are honoured"
    );
    assert!(!is_fresh(&r, "2026-10-18T14:00:00+02:00"));
    assert!(is_fresh(&r, "2026-10-18T11:59:59.999Z"));
    let offset = sample_record(Some("2026-10-18T12:00:00+02:00"));
    assert!(is_fresh(&offset, "2026-10-18T09:59:59Z"));
    assert!(!is_fresh(&offset, "2026-10-18T10:00:00Z"));
    // Unreadable times fail closed.
    assert!(!is_fresh(&r, "yesterday"));
    assert!(!is_fresh(&r, "2026-02-31T00:00:00Z"));
    assert!(!is_fresh(
        &sample_record(Some("soon")),
        "2026-10-18T11:59:59Z"
    ));
}

// ---- schemas --------------------------------------------------------------

#[test]
fn the_schemas_enforce_their_conditional_rules() {
    let index = validator("evidence-index.schema.json");
    let keys = validator("cache-key.schema.json");
    let reuse = validator("reuse-record.schema.json");

    let (compatible, _store, _, _) = fixture_store("resume-compatible");
    let key_line = lines(&compatible.join("store/keys.jsonl"))[0].clone();
    assert_valid(&keys, &key_line, "fixture key line");
    let mut no_reason = key_line.clone();
    no_reason["missing_reasons"]
        .as_object_mut()
        .unwrap()
        .remove("role");
    assert_invalid(&keys, &no_reason, "null role without a reason");
    let mut empty_reason = key_line.clone();
    empty_reason["missing_reasons"]["state"] = json!("");
    assert_invalid(&keys, &empty_reason, "empty reason");
    let mut has_role = key_line.clone();
    has_role["key"]["role"] = json!("admin");
    has_role["missing_reasons"]
        .as_object_mut()
        .unwrap()
        .remove("role");
    assert_valid(&keys, &has_role, "a role needs no reason");
    let mut no_deployment = key_line.clone();
    no_deployment["key"]["deployment"] = json!({ "commit": null, "deployment_id": null });
    assert_invalid(&keys, &no_deployment, "no deployment and no reason");
    no_deployment["missing_reasons"]["deployment"] = json!("staging");
    assert_valid(&keys, &no_deployment, "no deployment with a reason");
    let mut extra = key_line.clone();
    extra["surprise"] = json!(1);
    assert_invalid(&keys, &extra, "unknown top-level field");
    let mut extra_key = key_line.clone();
    extra_key["key"]["surprise"] = json!(1);
    assert_invalid(&keys, &extra_key, "unknown key field");
    let mut missing_dim = key_line.clone();
    missing_dim["key"].as_object_mut().unwrap().remove("media");
    assert_invalid(&keys, &missing_dim, "a null must be explicit");
    let mut bad_status = key_line.clone();
    bad_status["status"] = json!("done");
    assert_invalid(&keys, &bad_status, "status enum");
    let mut bad_hash = key_line.clone();
    bad_hash["key_hash"] = json!("ABC");
    assert_invalid(&keys, &bad_hash, "hash pattern");
    let mut version = key_line.clone();
    version["schema_version"] = json!("2.0.0");
    assert_invalid(&keys, &version, "schema_version const");

    let (tombstoned, _store, _, _) = fixture_store("tombstone");
    let idx = lines(&tombstoned.join("store/index.jsonl"));
    let record_line = idx.iter().find(|l| l["entry"] == "record").unwrap().clone();
    let tombstone_line = idx
        .iter()
        .find(|l| l["entry"] == "tombstone")
        .unwrap()
        .clone();
    assert_valid(&index, &record_line, "record line");
    assert_valid(&index, &tombstone_line, "tombstone line");
    let mut no_stored = record_line.clone();
    no_stored.as_object_mut().unwrap().remove("stored");
    assert_invalid(&index, &no_stored, "record without stored");
    let mut null_stored = record_line.clone();
    null_stored["stored"] = Value::Null;
    null_stored["bytes"] = Value::Null;
    assert_valid(&index, &null_stored, "a payload we do not hold");
    let mut escape = record_line.clone();
    escape["stored"] = json!("objects/../../etc/passwd");
    assert_invalid(&index, &escape, "stored must be an object path");
    let mut hybrid = tombstone_line.clone();
    hybrid["kind"] = json!("http_response");
    assert_invalid(&index, &hybrid, "a tombstone has no record fields");
    let mut hybrid = record_line.clone();
    hybrid["reason"] = json!("why");
    assert_invalid(&index, &hybrid, "a record has no tombstone fields");
    let mut no_limit = tombstone_line.clone();
    no_limit
        .as_object_mut()
        .unwrap()
        .remove("replay_limitation");
    assert_invalid(&index, &no_limit, "tombstone needs a replay limitation");
    let mut unknown_entry = record_line.clone();
    unknown_entry["entry"] = json!("note");
    assert_invalid(&index, &unknown_entry, "entry enum");

    let (_t, store, _, keys) = fixture_store("resume-compatible");
    let plan = store.plan_resume(&keys, RUN_2).unwrap();
    let reuse_line = serde_json::to_value(&plan.reuse[0]).unwrap();
    assert_valid(&reuse, &reuse_line, "reuse line");
    let mut no_verified = reuse_line.clone();
    no_verified.as_object_mut().unwrap().remove("verified");
    assert_invalid(&reuse, &no_verified, "verified is required");
    let mut no_objects = reuse_line.clone();
    no_objects["sha256s"] = json!([]);
    assert_invalid(&reuse, &no_objects, "a reuse names at least one object");
}

// ---- CLI --------------------------------------------------------------------

fn args(list: &[&str]) -> Vec<String> {
    list.iter().map(|s| s.to_string()).collect()
}

fn code(list: &[&str]) -> String {
    format!("{:?}", cli(&args(list)))
}

fn exit(n: u8) -> String {
    format!("{:?}", ExitCode::from(n))
}

#[test]
fn the_cli_maps_outcomes_to_exit_codes() {
    let (clean, _, _, _) = fixture_store("resume-compatible");
    let (broken, _, _, _) = fixture_store("truncated-object");
    let clean_store = clean.join("store");
    let broken_store = broken.join("store");
    assert_eq!(
        code(&[
            "evidence",
            "verify",
            "--store",
            clean_store.to_str().unwrap(),
            "--json"
        ]),
        exit(0)
    );
    assert_eq!(
        code(&[
            "evidence",
            "verify",
            "--store",
            broken_store.to_str().unwrap()
        ]),
        exit(2)
    );
    let nowhere = clean.join("nowhere");
    assert_eq!(
        code(&["evidence", "verify", "--store", nowhere.to_str().unwrap()]),
        exit(3)
    );
    assert_eq!(code(&["evidence", "verify"]), exit(3));
    assert_eq!(code(&["evidence"]), exit(3));
    assert_eq!(code(&["evidence", "frobnicate"]), exit(3));
    assert_eq!(code(&["evidence", "verify", "--bogus"]), exit(3));
    assert_eq!(code(&["evidence", "--help"]), exit(0));

    let (_, _, _, wanted_keys) = fixture_store("resume-compatible");
    let keys = clean.join("keys.json");
    fs::write(&keys, serde_json::to_string(&wanted_keys).unwrap()).unwrap();
    assert_eq!(
        code(&[
            "evidence",
            "resume-plan",
            "--store",
            clean_store.to_str().unwrap(),
            "--keys",
            keys.to_str().unwrap(),
            "--run-id",
            RUN_2,
            "--json"
        ]),
        exit(0)
    );
    assert_eq!(line_count(&clean_store.join("reuse.jsonl")), n().1);
    assert_eq!(
        code(&[
            "evidence",
            "resume-plan",
            "--store",
            clean_store.to_str().unwrap(),
            "--keys",
            "/no/such/keys.json",
            "--run-id",
            RUN_2
        ]),
        exit(3)
    );
}

#[test]
fn the_cli_ingests_captures_and_jsonl() {
    let tmp = TempDir::new("cli-ingest");
    let store = tmp.join("store");
    let store = store.to_str().unwrap();
    let capture = capture_dir(CAPTURE);
    let capture = capture.to_str().unwrap();
    let route = final_url(&capture_dir(CAPTURE));
    let base = [
        "evidence",
        "ingest",
        "--store",
        store,
        "--captures",
        capture,
        "--run-id",
        RUN,
        "--route",
        &route,
        "--commit",
        "abc123",
        "--locale",
        "en-US",
    ];
    // Without reasons for role and state it must refuse.
    assert_eq!(code(&base), exit(3));
    let mut with_reasons: Vec<&str> = base.to_vec();
    with_reasons.extend([
        "--missing-reason",
        "role=public page",
        "--missing-reason",
        "state=default state only",
        "--json",
    ]);
    assert_eq!(code(&with_reasons), exit(0));
    assert_eq!(line_count(&tmp.join("store/index.jsonl")), n().0);
    assert_eq!(line_count(&tmp.join("store/keys.jsonl")), n().1);
    validate_store(&tmp.join("store"));
    // A bad dimension name is a usage error.
    assert_eq!(
        code(&[
            "evidence",
            "ingest",
            "--store",
            store,
            "--captures",
            capture,
            "--run-id",
            RUN,
            "--route",
            &route,
            "--missing-reason",
            "weather=hot"
        ]),
        exit(3)
    );
    assert_eq!(
        code(&[
            "evidence",
            "ingest",
            "--store",
            store,
            "--captures",
            capture,
            "--jsonl",
            "x",
            "--run-id",
            RUN,
            "--route",
            &route
        ]),
        exit(3)
    );
    assert_eq!(code(&["evidence", "ingest", "--store", store]), exit(3));

    let infra = fixtures().join("infra-ingest");
    let jsonl = infra.join("EVIDENCE.jsonl");
    let args_ok = [
        "evidence",
        "ingest",
        "--store",
        store,
        "--jsonl",
        jsonl.to_str().unwrap(),
        "--base",
        infra.to_str().unwrap(),
        "--producer",
        "infra",
        "--run-id",
        RUN,
        "--route",
        "https://www.example.gov/",
    ];
    assert_eq!(code(&args_ok), exit(0));
    assert_eq!(
        line_count(&tmp.join("store/index.jsonl")),
        n().0 + infra_counts().0
    );
    let mut bad_producer = args_ok.to_vec();
    let at = bad_producer.iter().position(|a| *a == "infra").unwrap();
    bad_producer[at] = "mystery";
    assert_eq!(code(&bad_producer), exit(3));

    // A file with an unindexable row still ingests the good rows but exits 3.
    let rows = tmp.join("rows.jsonl");
    fs::write(
        &rows,
        format!(
            "{}\nnot json\n",
            row("ev-fine", "dns:TXT:x", &sha256_hex(b"x"))
        ),
    )
    .unwrap();
    let rejected = [
        "evidence",
        "ingest",
        "--store",
        store,
        "--jsonl",
        rows.to_str().unwrap(),
        "--base",
        infra.to_str().unwrap(),
        "--producer",
        "other",
        "--run-id",
        RUN,
        "--route",
        "x",
    ];
    assert_eq!(code(&rejected), exit(3));
    assert_eq!(
        line_count(&tmp.join("store/index.jsonl")),
        n().0 + infra_counts().0 + 1
    );
}
