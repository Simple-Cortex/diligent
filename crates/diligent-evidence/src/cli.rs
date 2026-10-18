//! `diligent-rs evidence ingest | verify | resume-plan`.

use crate::error::EvidenceError;
use crate::model::{CacheKey, IngestContext, Producer, OPTIONAL_DIMENSIONS};
use crate::store::Store;
use serde::Serialize;
use serde_json::Value;
use std::path::PathBuf;
use std::process::ExitCode;

const USAGE: &str = "usage:\n\
  diligent-rs evidence ingest --store <dir> (--captures <dir> | --jsonl <file> --base <dir> --producer <name>)\n\
      --run-id <id> --route <url> [--commit <sha>] [--deployment-id <s>] [--role <s>] [--state <s>]\n\
      [--locale <s>] [--rule-version <s>] [--engine-version <s>] [--missing-reason <dim>=<text>]... [--json]\n\
  diligent-rs evidence verify --store <dir> [--json]\n\
  diligent-rs evidence resume-plan --store <dir> --keys <file.json> --run-id <id> [--json]\n\
\n\
ingest --captures copies a `collect` directory into the store and records one cache key per\n\
(viewport, media, kind). A key with no role, state, locale or deployment needs --missing-reason\n\
<dim>=<text> for each (dim: role, state, locale, deployment). --run-id must look like\n\
run-YYYY-MM-DD-xxxxxx.\n\
ingest --jsonl indexes an EVIDENCE.jsonl from assess, infra or source (--producer collect, assess,\n\
infra, source or other) and copies payload files found under --base. Rows carry their own run id\n\
and no cache dimensions, so the dimension flags are accepted and ignored there.\n\
verify re-hashes every object and cross-checks the index.\n\
resume-plan reads a JSON array of cache keys and says which can be reused (and records the reuse)\n\
and which must be redone.\n\
\n\
Exit codes: 0 ok, 2 verify found mismatched, missing or malformed entries, 3 usage or I/O error\n\
(including input rows that could not be indexed).\n";

const EXIT_MISMATCH: u8 = 2;
const EXIT_ERROR: u8 = 3;

#[derive(Default)]
struct Args {
    store: Option<PathBuf>,
    captures: Option<PathBuf>,
    jsonl: Option<PathBuf>,
    base: Option<PathBuf>,
    producer: Option<String>,
    run_id: Option<String>,
    route: Option<String>,
    commit: Option<String>,
    deployment_id: Option<String>,
    role: Option<String>,
    state: Option<String>,
    locale: Option<String>,
    rule_version: Option<String>,
    engine_version: Option<String>,
    keys: Option<PathBuf>,
    missing: Vec<(String, String)>,
    json: bool,
    help: bool,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut out = Args::default();
    let mut it = args.iter();
    while let Some(flag) = it.next() {
        let mut value = |name: &str| -> Result<String, String> {
            it.next()
                .cloned()
                .ok_or_else(|| format!("{name} needs a value"))
        };
        match flag.as_str() {
            "-h" | "--help" => out.help = true,
            "--json" => out.json = true,
            "--store" => out.store = Some(PathBuf::from(value("--store")?)),
            "--captures" => out.captures = Some(PathBuf::from(value("--captures")?)),
            "--jsonl" => out.jsonl = Some(PathBuf::from(value("--jsonl")?)),
            "--base" => out.base = Some(PathBuf::from(value("--base")?)),
            "--keys" => out.keys = Some(PathBuf::from(value("--keys")?)),
            "--producer" => out.producer = Some(value("--producer")?),
            "--run-id" => out.run_id = Some(value("--run-id")?),
            "--route" => out.route = Some(value("--route")?),
            "--commit" => out.commit = Some(value("--commit")?),
            "--deployment-id" => out.deployment_id = Some(value("--deployment-id")?),
            "--role" => out.role = Some(value("--role")?),
            "--state" => out.state = Some(value("--state")?),
            "--locale" => out.locale = Some(value("--locale")?),
            "--rule-version" => out.rule_version = Some(value("--rule-version")?),
            "--engine-version" => out.engine_version = Some(value("--engine-version")?),
            "--missing-reason" => {
                let raw = value("--missing-reason")?;
                let (dim, text) = raw
                    .split_once('=')
                    .ok_or_else(|| format!("--missing-reason expects <dim>=<text>, got `{raw}`"))?;
                if !OPTIONAL_DIMENSIONS.contains(&dim) {
                    return Err(format!(
                        "--missing-reason: `{dim}` is not one of role, state, locale, deployment"
                    ));
                }
                out.missing.push((dim.to_string(), text.to_string()));
            }
            other if other.starts_with('-') => return Err(format!("unknown flag: {other}")),
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    Ok(out)
}

fn fail(message: impl std::fmt::Display) -> ExitCode {
    eprintln!("diligent-rs: {message}");
    ExitCode::from(EXIT_ERROR)
}

fn emit<T: Serialize>(report: &T, json: bool, human: impl FnOnce() -> String) {
    if json {
        println!(
            "{}",
            serde_json::to_string_pretty(report).unwrap_or_default()
        );
    } else {
        println!("{}", human());
    }
}

fn require<'a, T>(value: &'a Option<T>, flag: &str) -> Result<&'a T, String> {
    value.as_ref().ok_or_else(|| format!("{flag} is required"))
}

/// Entry point for `diligent-rs evidence ...`. `args` is the whole argument
/// list, starting with `evidence`.
pub fn cli(args: &[String]) -> ExitCode {
    let rest = args.strip_prefix(&["evidence".to_string()]).unwrap_or(args);
    let Some(sub) = rest.first().map(String::as_str) else {
        eprint!("{USAGE}");
        return ExitCode::from(EXIT_ERROR);
    };
    if matches!(sub, "-h" | "--help") {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let parsed = match parse(&rest[1..]) {
        Ok(a) => a,
        Err(e) => return fail(format!("{e}\n{USAGE}")),
    };
    if parsed.help {
        print!("{USAGE}");
        return ExitCode::SUCCESS;
    }
    let result = match sub {
        "ingest" => ingest(&parsed),
        "verify" => verify(&parsed),
        "resume-plan" => resume_plan(&parsed),
        other => Err(format!("unknown evidence command `{other}`\n{USAGE}")),
    };
    match result {
        Ok(code) => ExitCode::from(code),
        Err(message) => fail(message),
    }
}

fn explain(e: EvidenceError) -> String {
    e.to_string()
}

fn ingest(a: &Args) -> Result<u8, String> {
    let store_dir = require(&a.store, "--store")?;
    let store = Store::open(store_dir).map_err(explain)?;
    let report = match (&a.captures, &a.jsonl) {
        (Some(_), Some(_)) => return Err("give --captures or --jsonl, not both".to_string()),
        (None, None) => return Err("give --captures <dir> or --jsonl <file>".to_string()),
        (Some(dir), None) => {
            let mut ctx = IngestContext::new(
                require(&a.run_id, "--run-id")?.clone(),
                require(&a.route, "--route")?.clone(),
            );
            ctx.commit = a.commit.clone();
            ctx.deployment_id = a.deployment_id.clone();
            ctx.role = a.role.clone();
            ctx.state = a.state.clone();
            ctx.locale = a.locale.clone();
            ctx.rule_version = a.rule_version.clone();
            ctx.engine_version = a.engine_version.clone();
            ctx.missing_reasons = a.missing.iter().cloned().collect();
            store.ingest_captures(dir, &ctx).map_err(explain)?
        }
        (None, Some(file)) => {
            let base = require(&a.base, "--base")?;
            let name = require(&a.producer, "--producer")?;
            let producer = Producer::parse(name).ok_or_else(|| {
                format!("--producer `{name}` is not one of collect, assess, infra, source, other")
            })?;
            store
                .ingest_evidence_jsonl(file, base, producer)
                .map_err(explain)?
        }
    };
    emit(&report, a.json, || {
        let mut text = format!(
            "indexed {} records ({} already present), stored {} objects ({} already present), recorded {} cache keys ({} already present)",
            report.records_indexed,
            report.records_skipped,
            report.objects_written,
            report.objects_reused,
            report.keys_recorded,
            report.keys_skipped
        );
        for path in &report.objects_replaced {
            text.push_str(&format!("\nreplaced a corrupt object: {path}"));
        }
        for p in &report.payload_problems {
            text.push_str(&format!(
                "\npayload not stored for {} ({}): {}",
                p.evidence_id, p.payload_ref, p.reason
            ));
        }
        for r in &report.rejected {
            text.push_str(&format!("\nrejected {r}"));
        }
        text
    });
    if report.rejected.is_empty() {
        Ok(0)
    } else {
        eprintln!(
            "diligent-rs: {} input rows could not be indexed",
            report.rejected.len()
        );
        Ok(EXIT_ERROR)
    }
}

fn verify(a: &Args) -> Result<u8, String> {
    let store = Store::open_existing(require(&a.store, "--store")?).map_err(explain)?;
    let report = store.verify().map_err(explain)?;
    emit(&report, a.json, || {
        let mut text = format!(
            "{} objects, {} intact, {} mismatched, {} missing, {} tombstoned, {} malformed lines",
            report.objects,
            report.ok,
            report.mismatched.len(),
            report.missing.len(),
            report.tombstoned,
            report.malformed.len()
        );
        for p in &report.mismatched {
            text.push_str(&format!("\nmismatched: {p}"));
        }
        for id in &report.missing {
            text.push_str(&format!("\nmissing object for: {id}"));
        }
        for line in &report.malformed {
            text.push_str(&format!("\nmalformed: {line}"));
        }
        text
    });
    Ok(if report.is_clean() { 0 } else { EXIT_MISMATCH })
}

fn load_keys(path: &PathBuf) -> Result<Vec<CacheKey>, String> {
    let text = std::fs::read_to_string(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let value: Value =
        serde_json::from_str(&text).map_err(|e| format!("{}: not JSON: {e}", path.display()))?;
    let items = match value {
        Value::Array(items) => items,
        Value::Object(mut map) => match map.remove("keys") {
            Some(Value::Array(items)) => items,
            _ => {
                return Err(format!(
                    "{}: expected a JSON array of cache keys, or an object with a `keys` array",
                    path.display()
                ))
            }
        },
        _ => {
            return Err(format!(
                "{}: expected a JSON array of cache keys",
                path.display()
            ))
        }
    };
    let mut keys = Vec::with_capacity(items.len());
    for (i, item) in items.into_iter().enumerate() {
        // A key record from keys.jsonl is accepted as well as a bare key.
        let item = match item {
            Value::Object(mut map) if map.contains_key("key") && map.contains_key("key_hash") => {
                map.remove("key").unwrap_or(Value::Null)
            }
            other => other,
        };
        let key: CacheKey = serde_json::from_value(item)
            .map_err(|e| format!("{}: key {}: {e}", path.display(), i + 1))?;
        keys.push(key);
    }
    Ok(keys)
}

fn resume_plan(a: &Args) -> Result<u8, String> {
    let store = Store::open_existing(require(&a.store, "--store")?).map_err(explain)?;
    let keys = load_keys(require(&a.keys, "--keys")?)?;
    let run_id = require(&a.run_id, "--run-id")?;
    let plan = store.plan_resume(&keys, run_id).map_err(explain)?;
    emit(&plan, a.json, || {
        let mut text = format!(
            "{} keys reusable, {} to redo",
            plan.reuse.len(),
            plan.redo.len()
        );
        for record in &plan.reuse {
            text.push_str(&format!(
                "\nreuse {} from {}",
                record.key_hash, record.reused_from_run_id
            ));
        }
        for (key, why) in &plan.reasons {
            text.push_str(&format!(
                "\nredo {} ({} {}): {why}",
                key.hash(),
                key.kind,
                key.route
            ));
        }
        text
    });
    Ok(0)
}
