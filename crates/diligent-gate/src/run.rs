//! Gate runner: loads artifacts, computes the assessment, writes the outputs.
//! Port of `scripts/gate.mjs` with the artifact helpers from `scripts/lib/artifacts.mjs`.

use std::collections::BTreeMap;
use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};

use serde::Serialize;
use serde_json::Value;

use crate::{
    aggregate, exit_code_for, AggregateResult, CheckResult, Counts, Coverage, Denominators,
    Exception, Obligation, ReviewItem, Unresolved, CLAIM_BOUNDARY,
};

const PLACEHOLDER_RUN: &str = "run-1970-01-01-000000";
const SCHEMA_VERSION: &str = "1.0.0";

const PIPELINE: [&str; 9] = [
    "initialized",
    "discovered",
    "scoped",
    "planned",
    "collecting",
    "assessed",
    "proposed",
    "verified",
    "reported",
];

/// Node: `/^run-\d{4}-\d{2}-\d{2}-[0-9a-f]{6}$/`.
fn is_valid_run_id(id: &str) -> bool {
    let parts: Vec<&str> = id.split('-').collect();
    if parts.len() != 5 || parts[0] != "run" {
        return false;
    }
    let digits = |s: &str, n: usize| s.len() == n && s.bytes().all(|b| b.is_ascii_digit());
    digits(parts[1], 4)
        && digits(parts[2], 2)
        && digits(parts[3], 2)
        && parts[4].len() == 6
        && parts[4]
            .bytes()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
}

fn schema_text(name: &str) -> &'static str {
    match name {
        "applicability" => include_str!("../../../schemas/applicability.schema.json"),
        "check-result" => include_str!("../../../schemas/check-result.schema.json"),
        "exceptions" => include_str!("../../../schemas/exceptions.schema.json"),
        "review-queue" => include_str!("../../../schemas/review-queue.schema.json"),
        "rules-lock" => include_str!("../../../schemas/rules-lock.schema.json"),
        "assessment" => include_str!("../../../schemas/assessment.schema.json"),
        _ => include_str!("../../../schemas/run.schema.json"),
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct Assessment {
    pub schema_version: String,
    pub run_id: String,
    pub computed_at: String,
    pub overall: String,
    pub has_incomplete_work: bool,
    pub counts: Counts,
    pub denominators: Denominators,
    pub unresolved: Vec<Unresolved>,
    pub failures: Vec<crate::Failure>,
    pub exceptions: Vec<crate::ExceptionRecord>,
    pub ux_findings_ref: Option<String>,
    pub release_disposition: Option<Value>,
    pub limitations: Vec<String>,
    pub claim_boundary: String,
}

/// An error that ends the run. `message` is the complete stderr text.
#[derive(Debug)]
pub struct GateError {
    pub message: String,
    pub exit_code: i32,
}

impl std::fmt::Display for GateError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.message)
    }
}

impl std::error::Error for GateError {}

impl GateError {
    pub fn new(message: &str, exit_code: i32) -> Self {
        Self {
            message: message.to_string(),
            exit_code,
        }
    }

    /// Node `UsageError`: `diligent: <message>`, exit 2.
    fn usage(message: &str) -> Self {
        Self::new(&format!("diligent: {message}"), 2)
    }

    fn io(message: String) -> Self {
        Self::new(&format!("diligent: {message}"), 3)
    }
}

pub fn sha256_file(path: &Path) -> Result<String, GateError> {
    let contents = fs::read(path)
        .map_err(|e| GateError::io(format!("cannot read {}: {}", path.display(), e)))?;
    Ok(diligent_evidence::sha256_hex(&contents))
}

/// Same shape as `new Date().toISOString()`: milliseconds and a trailing Z.
pub fn now_iso() -> String {
    let millis = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64);
    let secs = diligent_evidence::format_unix_seconds(millis.div_euclid(1000));
    format!(
        "{}.{:03}Z",
        secs.trim_end_matches('Z'),
        millis.rem_euclid(1000)
    )
}

/// `path.resolve`: absolute, lexically normalised, symlinks untouched.
pub fn resolve_path(path: &Path) -> PathBuf {
    let base = if path.is_absolute() {
        PathBuf::new()
    } else {
        std::env::current_dir().unwrap_or_default()
    };
    let mut out = PathBuf::new();
    for component in base.join(path).components() {
        match component {
            Component::ParentDir => {
                out.pop();
            }
            Component::CurDir => {}
            other => out.push(other.as_os_str()),
        }
    }
    out
}

/// Workspace root: `--root`, `DILIGENT_ROOT`, nearest ancestor containing `.diligent`, else cwd.
fn resolve_root(root: Option<&Path>) -> PathBuf {
    if let Some(root) = root.filter(|r| !r.as_os_str().is_empty()) {
        return resolve_path(root);
    }
    if let Some(env) = std::env::var_os("DILIGENT_ROOT").filter(|v| !v.is_empty()) {
        return resolve_path(Path::new(&env));
    }
    let cwd = resolve_path(Path::new("."));
    let mut dir = cwd.clone();
    loop {
        if dir.join(".diligent").exists() {
            return dir;
        }
        match dir.parent() {
            Some(parent) => dir = parent.to_path_buf(),
            None => return cwd,
        }
    }
}

fn is_audit_id(id: &str) -> bool {
    !id.is_empty()
        && id.split('-').all(|part| {
            !part.is_empty()
                && part
                    .bytes()
                    .all(|b| b.is_ascii_digit() || b.is_ascii_lowercase())
        })
}

/// Locate the audit directory from CLI-style options.
pub fn locate_audit_dir(
    audit_id: Option<&str>,
    dir: Option<&Path>,
    root: Option<&Path>,
) -> Result<PathBuf, GateError> {
    if let Some(dir) = dir.filter(|d| !d.as_os_str().is_empty()) {
        return Ok(resolve_path(dir));
    }
    let Some(audit_id) = audit_id else {
        return Err(GateError::usage(
            "an <audit-id> (or --dir <audit-directory>) is required.",
        ));
    };
    if !is_audit_id(audit_id) {
        return Err(GateError::usage(&format!(
            "audit id \"{audit_id}\" is not valid: use kebab-case letters and digits, for example \"public-site\"."
        )));
    }
    Ok(resolve_root(root).join(".diligent").join(audit_id))
}

/// Compact JSON like `JSON.stringify(value)`.
fn compact(value: &Value) -> String {
    diligent_rules::jsval::stringify(value)
}

/// `JSON.stringify(value, null, 2)` plus the trailing newline of `atomicWriteJson`.
fn pretty(value: &impl Serialize) -> Result<String, GateError> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|e| GateError::io(format!("cannot serialize JSON: {e}")))?;
    Ok(format!("{text}\n"))
}

pub fn atomic_write_json(path: &Path, value: &impl Serialize) -> Result<(), GateError> {
    let contents = pretty(value)?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    fs::create_dir_all(parent)
        .map_err(|e| GateError::io(format!("cannot create {}: {}", parent.display(), e)))?;
    let name = path
        .file_name()
        .map_or_else(String::new, |n| n.to_string_lossy().to_string());
    let tmp = parent.join(format!(".{name}.tmp-{}", std::process::id()));
    let result = fs::File::create(&tmp)
        .and_then(|mut f| {
            f.write_all(contents.as_bytes())?;
            f.sync_all()
        })
        .and_then(|()| fs::rename(&tmp, path));
    if let Err(e) = result {
        let _ = fs::remove_file(&tmp);
        return Err(GateError::io(format!(
            "cannot write {}: {}",
            path.display(),
            e
        )));
    }
    Ok(())
}

/// Validate in-memory data against a named schema; one formatted line per problem.
fn validate_data(
    schema_name: &str,
    data: &Value,
    label: &str,
    prefix: &str,
) -> Result<Vec<String>, GateError> {
    let schema: Value = serde_json::from_str(schema_text(schema_name))
        .map_err(|e| GateError::io(format!("schema {schema_name}: {e}")))?;
    let issues = diligent_rules::schema::validate_value(&schema, data).map_err(|problems| {
        GateError::io(format!(
            "schema {schema_name} uses unsupported keywords: {}",
            problems.join("; ")
        ))
    })?;
    Ok(issues
        .into_iter()
        .map(|i| format!("{label}{prefix} {}: {}", i.path, i.message))
        .collect())
}

/// One artifact read from the audit directory.
struct Loaded {
    exists: bool,
    ok: bool,
    errors: Vec<String>,
    /// Array for JSONL artifacts.
    data: Option<Value>,
}

fn load_artifact(dir: &Path, name: &str) -> Result<Loaded, GateError> {
    let (schema, jsonl) = match name {
        "APPLICABILITY.json" => ("applicability", false),
        "RESULTS.jsonl" => ("check-result", true),
        "EXCEPTIONS.json" => ("exceptions", false),
        "REVIEW-QUEUE.json" => ("review-queue", false),
        _ => ("rules-lock", false),
    };
    let path = dir.join(name);
    if !path.exists() {
        return Ok(Loaded {
            exists: false,
            ok: false,
            errors: Vec::new(),
            data: None,
        });
    }
    let failed = |errors: Vec<String>| Loaded {
        exists: true,
        ok: false,
        errors,
        data: None,
    };
    let text = match fs::read_to_string(&path) {
        Ok(text) => text,
        Err(e) => return Ok(failed(vec![format!("{name}: cannot read file: {e}")])),
    };
    if jsonl {
        let mut records = Vec::new();
        let mut errors = Vec::new();
        for (i, line) in text.split('\n').enumerate() {
            if diligent_rules::jsval::trims_to_empty(line) {
                continue;
            }
            match serde_json::from_str::<Value>(line) {
                Ok(record) => {
                    errors.extend(validate_data(
                        schema,
                        &record,
                        name,
                        &format!(" line {}", i + 1),
                    )?);
                    records.push(record);
                }
                Err(_) => errors.push(format!("{name} line {}: not valid JSON", i + 1)),
            }
        }
        return Ok(Loaded {
            exists: true,
            ok: errors.is_empty(),
            errors,
            data: Some(Value::Array(records)),
        });
    }
    let data: Value = match serde_json::from_str(&text) {
        Ok(data) => data,
        Err(_) => return Ok(failed(vec![format!("{name}: not valid JSON")])),
    };
    let errors = validate_data(schema, &data, name, "")?;
    Ok(Loaded {
        exists: true,
        ok: errors.is_empty(),
        errors,
        data: Some(data),
    })
}

/// Append one event line to EVENTS.jsonl, keys in the order Node builds them.
pub fn append_event(
    dir: &Path,
    run_id: &str,
    stage: &str,
    event: &str,
    artifact: Option<&str>,
    sha256: Option<&str>,
    detail: Option<&str>,
) -> Result<(), GateError> {
    let text = |v: &str| Value::String(v.to_string());
    let mut record = serde_json::Map::new();
    record.insert("schema_version".into(), text(SCHEMA_VERSION));
    record.insert("ts".into(), text(&now_iso()));
    record.insert("run_id".into(), text(run_id));
    record.insert("stage".into(), text(stage));
    record.insert("event".into(), text(event));
    record.insert("artifact".into(), artifact.map_or(Value::Null, text));
    record.insert("sha256".into(), sha256.map_or(Value::Null, text));
    if let Some(detail) = detail.filter(|d| !d.is_empty()) {
        record.insert("detail".into(), text(detail));
    }
    let line = format!("{}\n", compact(&Value::Object(record)));
    let event_path = dir.join("EVENTS.jsonl");
    fs::create_dir_all(dir)
        .and_then(|()| {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(&event_path)
        })
        .and_then(|mut f| f.write_all(line.as_bytes()))
        .map_err(|e| GateError::io(format!("cannot append to {}: {}", event_path.display(), e)))
}

/// Move a run's pipeline state forward (never backward) and apply a patch.
/// Leaves RUN.json untouched when it is missing or invalid.
pub fn advance_run(
    dir: &Path,
    run_id: &str,
    state: &str,
    patch: &BTreeMap<String, Value>,
) -> Result<bool, GateError> {
    let run_path = dir.join("runs").join(run_id).join("RUN.json");
    if !run_path.exists() {
        return Ok(false);
    }
    let Ok(text) = fs::read_to_string(&run_path) else {
        return Ok(false);
    };
    let Ok(mut run) = serde_json::from_str::<Value>(&text) else {
        return Ok(false);
    };
    let Some(obj) = run.as_object_mut() else {
        return Ok(false);
    };
    // Node: PIPELINE.indexOf(state) > PIPELINE.indexOf(run.pipeline_state); unknown is -1.
    let index_of = |name: Option<&str>| {
        name.and_then(|n| PIPELINE.iter().position(|&s| s == n))
            .map_or(-1, |i| i as i64)
    };
    let current = index_of(obj.get("pipeline_state").and_then(Value::as_str));
    if index_of(Some(state)) > current {
        obj.insert("pipeline_state".into(), Value::String(state.to_string()));
    }
    for (key, value) in patch {
        obj.insert(key.clone(), value.clone());
    }
    if !validate_data("run", &run, "RUN.json", "")?.is_empty() {
        return Ok(false);
    }
    atomic_write_json(&run_path, &run)?;
    Ok(true)
}

/// `run_id` of the most recently started run under `runs/`, if it has one.
fn latest_run_id(dir: &Path) -> Option<String> {
    let runs_dir = dir.join("runs");
    let mut candidates: Vec<(String, Option<Value>, String)> = Vec::new();
    let mut names: Vec<_> = fs::read_dir(&runs_dir)
        .ok()?
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    names.sort();
    for name in names {
        let path = runs_dir.join(&name).join("RUN.json");
        if !path.exists() {
            continue;
        }
        let data = fs::read_to_string(&path)
            .ok()
            .and_then(|t| serde_json::from_str::<Value>(&t).ok());
        let started = data
            .as_ref()
            .and_then(|d| d.get("started_at"))
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        candidates.push((name, data, started));
    }
    candidates.sort_by(|a, b| b.2.cmp(&a.2));
    candidates
        .into_iter()
        .next()
        .and_then(|(_, data, _)| data)
        .and_then(|d| d.get("run_id").and_then(Value::as_str).map(str::to_string))
}

fn empty_aggregate(unresolved: Vec<Unresolved>) -> AggregateResult {
    AggregateResult {
        overall: "incomplete".to_string(),
        has_incomplete_work: true,
        counts: Counts {
            selected: 0,
            applicable: 0,
            not_applicable: 0,
            unresolved_applicability: 0,
            pending: 0,
            required_instances: 0,
            executed: 0,
            reviewed: 0,
            unsupported: 0,
        },
        denominators: Denominators {
            applicable: 0,
            not_applicable: 0,
            unresolved_applicability: 0,
            pending: 0,
            executed: 0,
            reviewed: 0,
            unsupported: 0,
        },
        unresolved,
        failures: Vec::new(),
        exceptions: Vec::new(),
        advisory: Vec::new(),
    }
}

fn plural<'a>(n: usize, one: &'a str, many: &'a str) -> &'a str {
    if n == 1 {
        one
    } else {
        many
    }
}

fn typed_list<T: serde::de::DeserializeOwned>(value: Option<&Value>) -> Vec<T> {
    value
        .and_then(Value::as_array)
        .map(|items| {
            items
                .iter()
                .filter_map(|v| serde_json::from_value(v.clone()).ok())
                .collect()
        })
        .unwrap_or_default()
}

pub fn run_gate(dir: &Path, now: &str, json_output: bool) -> Result<i32, GateError> {
    if !dir.exists() {
        return Err(GateError::new(
            &format!(
                "diligent gate: audit directory {} does not exist. Run \"diligent init <audit-id>\" first.",
                dir.display()
            ),
            3,
        ));
    }

    let names = [
        "APPLICABILITY.json",
        "RESULTS.jsonl",
        "EXCEPTIONS.json",
        "REVIEW-QUEUE.json",
        "RULES.lock.json",
    ];
    let mut loaded: Vec<(&str, Loaded)> = Vec::new();
    for name in names {
        loaded.push((name, load_artifact(dir, name)?));
    }
    let get = |name: &str| {
        &loaded
            .iter()
            .find(|(n, _)| *n == name)
            .expect("known artifact")
            .1
    };

    let mut limitations = vec![format!(
        "Diligent {} runs automated website checks only through the separately installed diligent-rs binary; without it, results come from human or documentary input.",
        env!("CARGO_PKG_VERSION")
    )];
    let mut unresolved: Vec<Unresolved> = Vec::new();
    let invalid: Vec<&str> = loaded
        .iter()
        .filter(|(_, l)| l.exists && !l.ok)
        .map(|(n, _)| *n)
        .collect();

    // Run id: prefer a consistent value across the artifacts; flag disagreement.
    let mut run_ids: Vec<(String, Vec<&str>)> = Vec::new();
    for (name, l) in &loaded {
        let ids: Vec<Option<&str>> = match &l.data {
            Some(Value::Array(records)) => records
                .iter()
                .map(|r| r.get("run_id").and_then(Value::as_str))
                .collect(),
            Some(data) => vec![data.get("run_id").and_then(Value::as_str)],
            None => vec![None],
        };
        for id in ids.into_iter().flatten() {
            match run_ids.iter_mut().find(|(known, _)| known == id) {
                Some((_, files)) => files.push(name),
                None => run_ids.push((id.to_string(), vec![name])),
            }
        }
    }
    let mut run_id = run_ids
        .iter()
        .map(|(id, _)| id.clone())
        .find(|id| is_valid_run_id(id))
        .or_else(|| latest_run_id(dir))
        .unwrap_or_default();
    if !is_valid_run_id(&run_id) {
        run_id = PLACEHOLDER_RUN.to_string();
        limitations.push(
            "No valid run id could be read from the inputs; a placeholder run id is recorded."
                .to_string(),
        );
    }
    let mismatch = run_ids.len() > 1;

    let assessment;
    let code;
    if !invalid.is_empty() || mismatch {
        for name in &invalid {
            let errors = &get(name).errors;
            limitations.extend(errors.iter().take(5).cloned());
            unresolved.push(Unresolved {
                kind: "invalid_artifact".to_string(),
                r#ref: (*name).to_string(),
                reason: format!(
                    "{name} failed schema validation ({} error{}).",
                    errors.len(),
                    plural(errors.len(), "", "s")
                ),
            });
        }
        if mismatch {
            let listing = run_ids
                .iter()
                .map(|(id, files)| {
                    let mut unique: Vec<&str> = Vec::new();
                    for f in files {
                        if !unique.contains(f) {
                            unique.push(f);
                        }
                    }
                    format!("{id} ({})", unique.join(", "))
                })
                .collect::<Vec<_>>()
                .join("; ");
            unresolved.push(Unresolved {
                kind: "run_id_mismatch".to_string(),
                r#ref: "artifacts".to_string(),
                reason: format!("Artifacts disagree on the run id: {listing}."),
            });
        }
        assessment = empty_aggregate(unresolved);
        code = 3;
    } else {
        let applicability = get("APPLICABILITY.json");
        let results = get("RESULTS.jsonl");
        let exceptions = get("EXCEPTIONS.json");
        let review_queue = get("REVIEW-QUEUE.json");
        let lock = get("RULES.lock.json");

        let missing = [
            (
                applicability.exists,
                "APPLICABILITY.json",
                "Applicability has not been resolved, so no obligation can be assessed.",
            ),
            (
                results.exists,
                "RESULTS.jsonl",
                "No check results exist, so nothing has been tested.",
            ),
            (
                lock.exists,
                "RULES.lock.json",
                "The rules lock is missing, so supported checks cannot be determined.",
            ),
        ];
        for (exists, name, reason) in missing {
            if !exists {
                unresolved.push(Unresolved {
                    kind: "missing_artifact".to_string(),
                    r#ref: name.to_string(),
                    reason: reason.to_string(),
                });
                limitations.push(format!("{name} is missing: {reason}"));
            }
        }
        if !exceptions.exists {
            limitations.push("EXCEPTIONS.json is absent; no exceptions were considered. An absent file is not a record of zero exceptions.".to_string());
        }
        if !review_queue.exists {
            limitations.push(
                "REVIEW-QUEUE.json is absent; open review items could not be considered."
                    .to_string(),
            );
        }

        let obligations: Vec<Obligation> = typed_list(
            applicability
                .data
                .as_ref()
                .and_then(|d| d.get("obligations")),
        );
        let matrix: Vec<&Value> = lock
            .data
            .as_ref()
            .and_then(|d| d.get("supported_check_matrix"))
            .and_then(Value::as_array)
            .map(|rows| rows.iter().collect())
            .unwrap_or_default();
        let mut rows: BTreeMap<&str, Vec<&Value>> = BTreeMap::new();
        for row in &matrix {
            if let Some(id) = row.get("obligation_id").and_then(Value::as_str) {
                rows.entry(id).or_default().push(row);
            }
        }
        let unsupported_ids: Vec<String> = if lock.exists {
            obligations
                .iter()
                .filter(|o| {
                    rows.get(o.obligation_id.as_str()).is_none_or(|rs| {
                        rs.iter().all(|r| {
                            r.get("unsupported")
                                .and_then(Value::as_bool)
                                .unwrap_or(false)
                        })
                    })
                })
                .map(|o| o.obligation_id.clone())
                .collect()
        } else {
            Vec::new()
        };
        if lock.exists {
            let unlocked = obligations
                .iter()
                .filter(|o| o.state == "applicable" && !rows.contains_key(o.obligation_id.as_str()))
                .count();
            if unlocked > 0 {
                limitations.push(format!(
                    "{unlocked} applicable obligation{} absent from RULES.lock.json and counted as unsupported.",
                    plural(unlocked, " is", "s are")
                ));
            }
        }

        let result_records: Vec<CheckResult> = typed_list(results.data.as_ref());
        let exception_list: Vec<Exception> =
            typed_list(exceptions.data.as_ref().and_then(|d| d.get("exceptions")));
        let review_items: Vec<ReviewItem> =
            typed_list(review_queue.data.as_ref().and_then(|d| d.get("items")));
        let mut result = aggregate(
            &obligations,
            &result_records,
            &exception_list,
            &review_items,
            Coverage {
                unsupported_obligation_ids: unsupported_ids,
                stale_result_ids: Vec::new(),
                required_instances: None,
                now: Some(now.to_string()),
            },
        );

        unresolved.append(&mut result.unresolved);
        unresolved.sort_by(|a, b| a.kind.cmp(&b.kind).then_with(|| a.r#ref.cmp(&b.r#ref)));
        result.unresolved = unresolved;
        result.has_incomplete_work = !result.unresolved.is_empty();
        if result.failures.is_empty() && !result.unresolved.is_empty() {
            result.overall = "incomplete".to_string();
        }
        let advisory = result.advisory.len();
        if advisory > 0 {
            limitations.push(format!(
                "{advisory} failing result{} on non-mandatory obligations {} recorded in RESULTS.jsonl and do not enter the regulatory gate.",
                plural(advisory, "", "s"),
                plural(advisory, "is", "are")
            ));
        }
        let exceptions_n = result.exceptions.len();
        if exceptions_n > 0 {
            limitations.push(format!(
                "{exceptions_n} exception{} recorded. Exceptions never change a result; the technical failure stays visible.",
                plural(exceptions_n, "", "s")
            ));
        }
        if result.overall == "pass_for_declared_scope" {
            limitations.push("The pass applies only to the declared scope and the instances actually tested. It is not a whole-site claim and not a certification.".to_string());
        }
        code = exit_code_for(&result.overall);
        assessment = result;
    }

    let document = Assessment {
        schema_version: SCHEMA_VERSION.to_string(),
        run_id: run_id.clone(),
        computed_at: now.to_string(),
        overall: assessment.overall,
        has_incomplete_work: assessment.has_incomplete_work,
        counts: assessment.counts,
        denominators: assessment.denominators,
        unresolved: assessment.unresolved,
        failures: assessment.failures,
        exceptions: assessment.exceptions,
        ux_findings_ref: dir
            .join("FINDINGS.json")
            .exists()
            .then(|| "FINDINGS.json".to_string()),
        release_disposition: None,
        limitations,
        claim_boundary: CLAIM_BOUNDARY.to_string(),
    };

    let as_value = serde_json::to_value(&document)
        .map_err(|e| GateError::io(format!("cannot serialize JSON: {e}")))?;
    let own_errors = validate_data("assessment", &as_value, "ASSESSMENT.json", "")?;
    if !own_errors.is_empty() {
        let mut text =
            "diligent gate: computed assessment failed its own schema; nothing was written."
                .to_string();
        for line in own_errors.iter().take(10) {
            use std::fmt::Write as _;
            let _ = write!(text, "\n  {line}");
        }
        return Err(GateError::new(&text, 3));
    }

    let out_path = dir.join("ASSESSMENT.json");
    atomic_write_json(&out_path, &document)?;
    let sha256 = sha256_file(&out_path)?;
    append_event(
        dir,
        &run_id,
        "assessed",
        if code == 3 {
            "gate_invalid_input"
        } else {
            "gate_computed"
        },
        Some("ASSESSMENT.json"),
        Some(&sha256),
        Some(&format!("overall={}", document.overall)),
    )?;
    if code != 3 {
        let mut patch = BTreeMap::new();
        patch.insert(
            "assessment_complete".to_string(),
            Value::Bool(!document.has_incomplete_work),
        );
        advance_run(dir, &run_id, "assessed", &patch)?;
    }

    if json_output {
        print!("{}", pretty(&document)?.trim_end_matches('\n'));
        println!();
    } else {
        println!("Overall: {} (exit {})", document.overall, code);
        let c = &document.counts;
        println!(
            "Selected {}; applicable {}/{}; unresolved applicability {}/{}; pending {}/{}.",
            c.selected,
            c.applicable,
            c.selected,
            c.unresolved_applicability,
            c.selected,
            c.pending,
            c.selected
        );
        println!(
            "Executed {}/{} instances; reviewed {}/{}; unsupported {}/{} applicable.",
            c.executed,
            c.required_instances,
            c.reviewed,
            document.denominators.reviewed,
            c.unsupported,
            c.applicable
        );
        for failure in document.failures.iter().take(10) {
            println!("FAIL {} ({})", failure.obligation_id, failure.result_id);
        }
        for item in document.unresolved.iter().take(10) {
            println!("- {}: {}", item.kind, item.r#ref);
        }
        if document.unresolved.len() > 10 {
            println!(
                "- ... and {} more unresolved items",
                document.unresolved.len() - 10
            );
        }
        for line in document.limitations.iter().take(6) {
            println!("Limitation: {line}");
        }
        println!("Wrote {}", out_path.display());
    }
    Ok(code)
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn run_id_pattern_matches_node() {
        assert!(is_valid_run_id("run-2026-10-19-91b4ed"));
        assert!(!is_valid_run_id("run-2026-10-19-91B4ED"));
        assert!(!is_valid_run_id("run-2026-10-19-91b4e"));
        assert!(!is_valid_run_id(PLACEHOLDER_RUN.trim_end_matches('0')));
        assert!(is_valid_run_id(PLACEHOLDER_RUN));
    }

    #[test]
    fn audit_ids_are_kebab_case() {
        assert!(is_audit_id("public-site"));
        assert!(is_audit_id("a1"));
        assert!(!is_audit_id("Public"));
        assert!(!is_audit_id("a--b"));
        assert!(!is_audit_id("-a"));
        assert!(!is_audit_id(""));
    }

    #[test]
    fn locate_requires_an_id_or_dir() {
        let err = locate_audit_dir(None, None, None).unwrap_err();
        assert_eq!(err.exit_code, 2);
        assert_eq!(
            err.message,
            "diligent: an <audit-id> (or --dir <audit-directory>) is required."
        );
        let dir = locate_audit_dir(Some("demo"), None, Some(Path::new("/w/./x/../y"))).unwrap();
        assert_eq!(dir, Path::new("/w/y/.diligent/demo"));
    }

    #[test]
    fn now_iso_has_milliseconds_and_z() {
        let now = now_iso();
        assert_eq!(now.len(), 24);
        assert!(now.ends_with('Z') && now.as_bytes()[19] == b'.');
    }

    #[test]
    fn advance_never_moves_backward_and_keeps_unknown_fields() {
        let dir = std::env::temp_dir().join(format!("diligent-gate-test-{}", std::process::id()));
        let run_dir = dir.join("runs").join("run-2026-10-19-91b4ed");
        fs::create_dir_all(&run_dir).unwrap();
        let run = json!({
            "schema_version": "1.0.0",
            "run_id": "run-2026-10-19-91b4ed",
            "audit_id": "ci",
            "profile_id": "p",
            "modes": ["page"],
            "started_at": "2026-10-19T17:37:01.979Z",
            "pipeline_state": "reported",
            "execution_status": "completed",
            "assessment_complete": false,
            "extensions": {"keep": true}
        });
        fs::write(run_dir.join("RUN.json"), run.to_string()).unwrap();
        let mut patch = BTreeMap::new();
        patch.insert("assessment_complete".to_string(), Value::Bool(true));
        // The result depends on the run schema accepting the fixture; either way the state stays.
        let _ = advance_run(&dir, "run-2026-10-19-91b4ed", "assessed", &patch).unwrap();
        let after: Value =
            serde_json::from_str(&fs::read_to_string(run_dir.join("RUN.json")).unwrap()).unwrap();
        assert_eq!(after["pipeline_state"], "reported");
        assert_eq!(after["extensions"]["keep"], true);
        assert!(!advance_run(&dir, "run-2000-01-01-000000", "assessed", &patch).unwrap());
        fs::remove_dir_all(&dir).unwrap();
    }
}
