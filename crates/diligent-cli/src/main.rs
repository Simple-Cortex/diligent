//! `diligent-rs`: native companion CLI.
//!
//! Implemented: `--version`, `--help`, `protocol-version`, `rules validate`,
//! `applicability`, `collect`, `assess`, `infra`, `source scan`, `inventory`,
//! `evidence`, `checks list`, and `gate`. Every other command exits 3 with a clear message. The Node CLI stays authoritative until a
//! shadow suite proves identical output; it probes this binary to report
//! which engine is available.
//!
//! Exit codes: 0 success, 2 usage error, 3 invalid data or not implemented.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

/// Version of the Node-to-native command protocol.
const PROTOCOL_VERSION: &str = "1";

const HELP: &str = "diligent-rs: native companion for Diligent\n\
\n\
Usage:\n  diligent-rs --version\n  diligent-rs --help\n  diligent-rs protocol-version\n  diligent-rs rules validate [--data-root <dir>] [--schemas <dir>] [--json] [--crosscheck]\n  diligent-rs applicability --scope <SCOPE.json> --profile <id> [options]\n  diligent-rs collect --url <url> --out <dir> [options]\n  diligent-rs assess --capture <dir> --out <dir> [--run-id <id>] [--inventory <INVENTORY.json>] [--no-signals] [--no-corroborate] [--component] [--json]\n  diligent-rs infra --url <https://host/> --out <dir> [--run-id <id>] [options]\n  diligent-rs source scan <path> [--out <dir>] [--run-id <id>] [--json]\n  diligent-rs inventory --url <https://host/> --out <dir> [--seed <s>] [--processes <file>] [--json]\n  diligent-rs evidence ingest | verify | resume-plan [options]\n  diligent-rs checks list [--json]\n  diligent-rs gate <audit-id> [--root <workspace>] [--dir <audit-directory>] [--json] [--now <iso-time>]\n\n\
rules validate checks authorities/registry.json, every rules/packs/*.json and\n\
every profiles/*.json against their schemas and the cross-checks in\n\
scripts/validate-rules.mjs. --data-root defaults to the current directory.\n\
--schemas defaults to <data-root>/schemas. --json prints { ok, errors, stats }.\n\
--crosscheck also asks the jsonschema crate for a second opinion and prints\n\
any disagreement to stderr. Exit 0 when valid, 3 when invalid or no data.\n\
\n\
applicability resolves every rule of a profile against SCOPE.json facts with\n\
three-valued logic and writes APPLICABILITY.json. Run `applicability --help`.\n\
\n\
assess runs the browser-capture accessibility checks over a collect directory and\n\
writes RESULTS.jsonl, EVIDENCE.jsonl and advisory SIGNALS.jsonl (skip with --no-signals). Run `assess --help`.\n\
\n\
inventory reads robots.txt and sitemaps, crawls same-origin links, and writes INVENTORY.json\n\
and SAMPLE.json (a WCAG-EM 2.0 structured plus random sample). Run `inventory --help`.\n\
\n\
evidence ingest, verify and resume-plan manage the content-addressed evidence store\n\
(objects, index, cache keys, reuse records). Run `evidence --help`.\n\
\n\
collect drives headless Chrome over CDP and writes content-addressed evidence\n\
plus manifest.jsonl. Run `collect --help`.\n\
\n\
infra runs network-level checks on one https origin (redirect, certificate, HSTS, SPF,\n\
DMARC, disclosure files, robots, sitemap, headers, DAP, privacy link). Run `infra --help`.\n\
\n\
source scan runs parser-based checks over saved .html files and component or template files (.jsx, .tsx, .vue, .svelte, .astro, .twig, .liquid). Run `source scan --help`.\n\
\n\
checks list prints every check and signal this build knows: browser checks, signals,\n\
source checks and infra checks, with the rules each informs. --json for machines.\n\
\n\
gate loads APPLICABILITY.json, RESULTS.jsonl, EXCEPTIONS.json, REVIEW-QUEUE.json and\n\
RULES.lock.json, validates each against its schema, and writes ASSESSMENT.json.\n\
Exit 0 pass_for_declared_scope (or not_applicable), 1 fail, 2 incomplete, 3 invalid.\n\
\n\
Every other command is not implemented in this version and exits with code 3.";

const APPLICABILITY_HELP: &str =
    "Usage: diligent-rs applicability --scope <SCOPE.json> --profile <profile_id>\n\
  [--data-root <dir>] [--as-of YYYY-MM-DD] [--evaluated-at <RFC3339>]\n\
  [--run-id <id>] [--out <APPLICABILITY.json>] [--json]\n\
\n\
Without --out the APPLICABILITY.json document is printed to stdout. With --out\n\
it is written to that file. --json (needs --out) prints a summary object\n\
{ applicable, not_applicable, unknown, pending_effective_date,\n\
mandatory_unknown } to stdout. --as-of defaults to today (UTC) and\n\
--evaluated-at to now (UTC). audit_id and run_id come from SCOPE.json;\n\
--run-id overrides the run id. Exit 0 on success, 3 on bad arguments, a\n\
missing or invalid SCOPE.json or profile, or an unparseable predicate.";

const RULES_VALIDATE_HELP: &str = "Usage: diligent-rs rules validate [--data-root <dir>] [--schemas <dir>] [--json] [--crosscheck]\n\
\n\
Exit 0 when valid, 3 when anything is invalid or when no data was found.";

const GATE_HELP: &str = "Usage: diligent-rs gate <audit-id> [--root <workspace>] [--dir <audit-directory>] [--json] [--now <iso-time>]\n\
\n\
Loads APPLICABILITY.json, RESULTS.jsonl, EXCEPTIONS.json, REVIEW-QUEUE.json and\n\
RULES.lock.json, validates each against its schema, and writes ASSESSMENT.json.\n\
\n\
Overall outcomes and exit codes:\n\
  0  pass_for_declared_scope (or not_applicable with cited predicates)\n\
  1  fail: a confirmed failure of an applicable mandatory obligation\n\
  2  incomplete: unknown or pending applicability, untested or unsupported\n\
     obligations, open review items, stale evidence, or a missing input\n\
  3  invalid input or schema (an incomplete ASSESSMENT.json is still written)\n\
\n\
Missing APPLICABILITY.json or RESULTS.jsonl is always incomplete, never a pass.\n\
Exceptions are listed separately and never change a result.\n\
Options:\n\
  --dir <path>   use an audit directory directly instead of <root>/.diligent/<audit-id>\n\
  --now <iso>    fixed computation time, for deterministic replays\n\
  --json         print the assessment as JSON";

/// Options for `rules validate`.
#[derive(Debug, PartialEq, Eq)]
struct ValidateOptions {
    data_root: Option<PathBuf>,
    schemas: Option<PathBuf>,
    json: bool,
    crosscheck: bool,
}

/// Options for `applicability`.
#[derive(Debug, PartialEq, Eq)]
struct ApplicabilityCliOptions {
    scope: PathBuf,
    profile: String,
    data_root: Option<PathBuf>,
    as_of: Option<String>,
    evaluated_at: Option<String>,
    run_id: Option<String>,
    out: Option<PathBuf>,
    json: bool,
}

/// Options for `gate`.
#[derive(Debug, PartialEq, Eq)]
struct GateCliOptions {
    audit_id: Option<String>,
    root: Option<PathBuf>,
    dir: Option<PathBuf>,
    json: bool,
    now: Option<String>,
}

/// Outcome of interpreting command-line arguments.
#[derive(Debug, PartialEq, Eq)]
enum Action {
    Print(String),
    RulesValidate(ValidateOptions),
    Applicability(ApplicabilityCliOptions),
    /// Bad `applicability` arguments: exit 3.
    ApplicabilityBad(String),
    Gate(GateCliOptions),
    /// Bad `gate` arguments: exit 2, printed with the `diligent:` prefix so the text matches the Node CLI.
    GateUsage(String),
    Usage(String),
    Source(Vec<String>),
    /// `collect`: browser evidence collection, parsed by the runtime crate.
    Collect(Vec<String>),
    Infra(Vec<String>),
    /// `assess`: accessibility checks over browser captures, parsed by the assess crate.
    Assess(Vec<String>),
    /// `inventory`: site inventory and WCAG-EM sampler, parsed by the inventory crate.
    Inventory(Vec<String>),
    /// `evidence`: content-addressed evidence store, parsed by the evidence crate.
    Evidence(Vec<String>),
    /// `checks list [--json]`.
    ChecksList {
        json: bool,
    },
    NotImplemented(String),
}

fn interpret(args: &[String]) -> Action {
    match args.first().map(String::as_str) {
        Some("--version") | Some("-V") | Some("-v") => {
            Action::Print(env!("CARGO_PKG_VERSION").to_string())
        }
        None | Some("--help") | Some("-h") => Action::Print(HELP.to_string()),
        Some("protocol-version") => Action::Print(PROTOCOL_VERSION.to_string()),
        Some("rules") => match args.get(1).map(String::as_str) {
            Some("validate") => parse_validate(&args[2..]),
            Some(other) => Action::NotImplemented(format!("rules {other}")),
            None => Action::Usage("rules needs a subcommand. Try `rules validate`.".to_string()),
        },
        Some("applicability") => parse_applicability(&args[1..]),
        Some("gate") => parse_gate(&args[1..]),
        Some("source") => Action::Source(args.to_vec()),
        Some("collect") => Action::Collect(args.to_vec()),
        Some("infra") => Action::Infra(args.to_vec()),
        Some("assess") => Action::Assess(args.to_vec()),
        Some("inventory") => Action::Inventory(args.to_vec()),
        Some("evidence") => Action::Evidence(args.to_vec()),
        Some("checks") => parse_checks(&args[1..]),
        Some(other) => Action::NotImplemented(other.to_string()),
    }
}

fn parse_gate(args: &[String]) -> Action {
    let mut options = GateCliOptions {
        audit_id: None,
        root: None,
        dir: None,
        json: false,
        now: None,
    };
    let mut i = 0;
    while i < args.len() {
        let token = args[i].as_str();
        let (name, inline) = match token.strip_prefix("--") {
            Some(rest) => match rest.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (rest, None),
            },
            None if token == "-h" => return Action::Print(GATE_HELP.to_string()),
            None => {
                if options.audit_id.is_none() {
                    options.audit_id = Some(token.to_string());
                } else {
                    return Action::GateUsage(
                        "gate takes at most one <audit-id>. Run with --help.".to_string(),
                    );
                }
                i += 1;
                continue;
            }
        };
        match name {
            "help" => return Action::Print(GATE_HELP.to_string()),
            "json" => {
                if inline.is_some() {
                    return Action::GateUsage("option --json does not take a value.".to_string());
                }
                options.json = true;
            }
            "root" | "dir" | "now" => {
                let value = match inline {
                    Some(v) => v,
                    None => {
                        i += 1;
                        match args.get(i) {
                            Some(v) if !v.starts_with("--") => v.clone(),
                            _ => {
                                return Action::GateUsage(format!(
                                    "option --{name} requires a value."
                                ))
                            }
                        }
                    }
                };
                match name {
                    "root" => options.root = Some(PathBuf::from(value)),
                    "dir" => options.dir = Some(PathBuf::from(value)),
                    "now" => options.now = Some(value),
                    _ => {}
                }
            }
            other => {
                return Action::GateUsage(format!("unknown option --{other}. Run with --help."));
            }
        }
        i += 1;
    }
    Action::Gate(options)
}

fn parse_checks(args: &[String]) -> Action {
    match args.first().map(String::as_str) {
        Some("list") => {
            let mut json = false;
            for arg in &args[1..] {
                match arg.as_str() {
                    "--json" => json = true,
                    other => {
                        return Action::Usage(format!("checks list: unknown argument `{other}`."))
                    }
                }
            }
            Action::ChecksList { json }
        }
        Some(other) => Action::NotImplemented(format!("checks {other}")),
        None => Action::Usage("checks needs a subcommand. Try `checks list`.".to_string()),
    }
}

/// The catalog `checks list --json` prints.
fn checks_catalog() -> serde_json::Value {
    use serde_json::json;
    let mut checks: Vec<_> = diligent_assess::checks::all().iter().collect();
    checks.sort_by_key(|c| c.id);
    let mut signals: Vec<_> = diligent_assess::signals::ALL.iter().collect();
    signals.sort_by_key(|s| s.key);
    let mut source: Vec<_> = diligent_source::checks::all()
        .into_iter()
        .map(|c| c.info)
        .collect();
    source.sort_by_key(|c| c.id);
    let mut infra: Vec<_> = diligent_infra::checks::CATALOG.to_vec();
    infra.sort_by_key(|c| c.id);
    json!({
        "checks": checks.iter().map(|c| json!({
            "id": c.id,
            "channel": c.channel,
            "informs": c.informs,
            "act_ids": c.act,
            "summary": c.summary,
            "never_fails": diligent_assess::checks::never_fails(c.id),
            "scope": diligent_assess::checks::scope(c.id),
        })).collect::<Vec<_>>(),
        "signals": signals.iter().map(|s| json!({
            "key": s.key,
            "family": s.family.as_str(),
            "summary": s.summary,
            "threshold": s.threshold.as_ref().map(|t| json!({
                "rule": t.rule,
                "source": t.source,
                "confidence": t.confidence,
            })),
        })).collect::<Vec<_>>(),
        "source_checks": source.iter().map(|c| json!({
            "id": c.id,
            "act": c.act,
            "informs": c.informs,
            "summary": c.summary,
        })).collect::<Vec<_>>(),
        "infra_checks": infra.iter().map(|c| json!({
            "id": c.id,
            "rule": c.rule,
            "channel": c.channel,
            "summary": c.summary,
        })).collect::<Vec<_>>(),
    })
}

fn print_checks(catalog: &serde_json::Value) {
    for (section, key, id_field) in [
        ("Browser checks (assess)", "checks", "id"),
        ("Signals (assess, advisory)", "signals", "key"),
        ("Source checks (source scan)", "source_checks", "id"),
        ("Infra checks (infra)", "infra_checks", "id"),
    ] {
        let rows = catalog[key].as_array().map_or(&[][..], Vec::as_slice);
        println!("{section}: {}", rows.len());
        for row in rows {
            println!(
                "  {:<40} {}",
                row[id_field].as_str().unwrap_or_default(),
                row["summary"].as_str().unwrap_or_default()
            );
        }
        println!();
    }
}

fn parse_validate(args: &[String]) -> Action {
    let mut options = ValidateOptions {
        data_root: None,
        schemas: None,
        json: false,
        crosscheck: false,
    };
    let mut i = 0;
    while i < args.len() {
        let token = args[i].as_str();
        let (name, inline) = match token.strip_prefix("--") {
            Some(rest) => match rest.split_once('=') {
                Some((n, v)) => (n, Some(v.to_string())),
                None => (rest, None),
            },
            None if token == "-h" => return Action::Print(RULES_VALIDATE_HELP.to_string()),
            None => {
                return Action::Usage(format!("unexpected argument `{token}`. Run with --help."))
            }
        };
        match name {
            "help" => return Action::Print(RULES_VALIDATE_HELP.to_string()),
            "json" | "crosscheck" => {
                if inline.is_some() {
                    return Action::Usage(format!("option --{name} does not take a value."));
                }
                if name == "json" {
                    options.json = true;
                } else {
                    options.crosscheck = true;
                }
            }
            "data-root" | "schemas" => {
                let value = match inline {
                    Some(v) => v,
                    None => {
                        i += 1;
                        match args.get(i) {
                            Some(v) if !v.starts_with("--") => v.clone(),
                            _ => {
                                return Action::Usage(format!("option --{name} requires a value."))
                            }
                        }
                    }
                };
                if name == "data-root" {
                    options.data_root = Some(PathBuf::from(value));
                } else {
                    options.schemas = Some(PathBuf::from(value));
                }
            }
            other => {
                return Action::Usage(format!("unknown option --{other}. Run with --help."));
            }
        }
        i += 1;
    }
    Action::RulesValidate(options)
}

fn parse_applicability(args: &[String]) -> Action {
    let mut values: std::collections::BTreeMap<String, String> = Default::default();
    let mut json = false;
    let mut i = 0;
    while i < args.len() {
        let token = args[i].as_str();
        let Some(rest) = token.strip_prefix("--") else {
            if token == "-h" {
                return Action::Print(APPLICABILITY_HELP.to_string());
            }
            return Action::ApplicabilityBad(format!("unexpected argument `{token}`."));
        };
        let (name, inline) = match rest.split_once('=') {
            Some((n, v)) => (n, Some(v.to_string())),
            None => (rest, None),
        };
        match name {
            "help" => return Action::Print(APPLICABILITY_HELP.to_string()),
            "json" => {
                if inline.is_some() {
                    return Action::ApplicabilityBad("option --json does not take a value.".into());
                }
                json = true;
            }
            "scope" | "profile" | "data-root" | "as-of" | "evaluated-at" | "run-id" | "out" => {
                let value = match inline {
                    Some(v) => v,
                    None => {
                        i += 1;
                        match args.get(i) {
                            Some(v) if !v.starts_with("--") => v.clone(),
                            _ => {
                                return Action::ApplicabilityBad(format!(
                                    "option --{name} requires a value."
                                ))
                            }
                        }
                    }
                };
                values.insert(name.to_string(), value);
            }
            other => return Action::ApplicabilityBad(format!("unknown option --{other}.")),
        }
        i += 1;
    }
    let (Some(scope), Some(profile)) = (values.remove("scope"), values.remove("profile")) else {
        return Action::ApplicabilityBad("--scope and --profile are required.".to_string());
    };
    let out = values.remove("out").map(PathBuf::from);
    if json && out.is_none() {
        return Action::ApplicabilityBad("--json needs --out.".to_string());
    }
    Action::Applicability(ApplicabilityCliOptions {
        scope: PathBuf::from(scope),
        profile,
        data_root: values.remove("data-root").map(PathBuf::from),
        as_of: values.remove("as-of"),
        evaluated_at: values.remove("evaluated-at"),
        run_id: values.remove("run-id"),
        out,
        json,
    })
}

/// Run `applicability` and return the process exit code.
fn run_applicability(options: &ApplicabilityCliOptions) -> u8 {
    let fail = |message: String| -> u8 {
        eprintln!("diligent-rs: {message}");
        3
    };
    let scope_text = match std::fs::read_to_string(&options.scope) {
        Ok(text) => text,
        Err(e) => return fail(format!("cannot read {}: {e}", options.scope.display())),
    };
    let scope: serde_json::Value = match serde_json::from_str(&scope_text) {
        Ok(v) => v,
        Err(e) => {
            return fail(format!(
                "{} is not valid JSON: {e}",
                options.scope.display()
            ))
        }
    };
    let as_of = match &options.as_of {
        Some(text) => match diligent_rules::predicate::Date::parse(text) {
            Some(d) => d,
            None => return fail(format!("--as-of `{text}` is not a YYYY-MM-DD date.")),
        },
        None => diligent_rules::predicate::Date::today_utc(),
    };
    let evaluated_at = match &options.evaluated_at {
        Some(text) if diligent_rules::predicate::is_rfc3339(text) => text.clone(),
        Some(text) => {
            return fail(format!(
                "--evaluated-at `{text}` is not an RFC 3339 date-time."
            ))
        }
        None => diligent_rules::predicate::rfc3339_now(),
    };
    let root = options
        .data_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let data = match diligent_rules::load_data_root(&root) {
        Ok(data) => data,
        Err(error) => return fail(error.to_string()),
    };
    let resolved = diligent_rules::resolve_applicability(
        &data,
        &options.profile,
        &scope,
        &diligent_rules::ApplicabilityOptions {
            as_of,
            evaluated_at,
            run_id: options.run_id.clone(),
        },
    );
    let file = match resolved {
        Ok(file) => file,
        Err(error) => return fail(error.to_string()),
    };
    let text = file.to_json();
    let summary = file.summary();
    match &options.out {
        None => println!("{text}"),
        Some(path) => {
            if let Err(e) = std::fs::write(path, format!("{text}\n")) {
                return fail(format!("cannot write {}: {e}", path.display()));
            }
            if options.json {
                println!("{}", serde_json::to_string(&summary).unwrap_or_default());
            } else {
                println!(
                    "applicable {}, not_applicable {}, unknown {}, pending_effective_date {}, mandatory_unknown {}",
                    summary.applicable,
                    summary.not_applicable,
                    summary.unknown,
                    summary.pending_effective_date,
                    summary.mandatory_unknown
                );
            }
        }
    }
    0
}

/// Run `rules validate` and return the process exit code.
fn run_rules_validate(options: &ValidateOptions) -> u8 {
    let root = options
        .data_root
        .clone()
        .unwrap_or_else(|| PathBuf::from("."));
    let schemas = options
        .schemas
        .clone()
        .unwrap_or_else(|| root.join("schemas"));
    let data = match diligent_rules::load_data_root(&root) {
        Ok(data) => data,
        Err(error) => {
            eprintln!("diligent-rs: {error}");
            return 3;
        }
    };
    let result = diligent_rules::validate(&data, &schemas);
    if options.json {
        println!("{}", result.to_json());
    } else {
        match result.render() {
            Ok(text) => println!("{text}"),
            Err(text) => eprintln!("{text}"),
        }
    }
    if options.crosscheck {
        report_crosscheck(&data, &schemas);
    }
    if result.ok() {
        0
    } else {
        3
    }
}

fn report_crosscheck(data: &diligent_rules::DataSet, schemas: &Path) {
    let lines = diligent_rules::schema_crosscheck(data, schemas);
    if lines.is_empty() {
        eprintln!("crosscheck: port and jsonschema agree on every file");
    } else {
        eprintln!("crosscheck: {} disagreement(s):", lines.len());
        for line in lines {
            eprintln!("  {line}");
        }
    }
}

/// Run `gate` and return the process exit code.
fn run_gate(options: &GateCliOptions) -> u8 {
    let result = diligent_gate::run::locate_audit_dir(
        options.audit_id.as_deref(),
        options.dir.as_deref(),
        options.root.as_deref(),
    )
    .and_then(|dir| {
        let now = options
            .now
            .clone()
            .unwrap_or_else(diligent_gate::run::now_iso);
        diligent_gate::run::run_gate(&dir, &now, options.json)
    });
    match result {
        Ok(code) => code as u8,
        Err(e) => {
            eprintln!("{}", e.message);
            e.exit_code as u8
        }
    }
}

fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match interpret(&args) {
        Action::Print(text) => {
            println!("{text}");
            ExitCode::SUCCESS
        }
        Action::RulesValidate(options) => ExitCode::from(run_rules_validate(&options)),
        Action::Applicability(options) => ExitCode::from(run_applicability(&options)),
        Action::ApplicabilityBad(message) => {
            eprintln!("diligent-rs: {message} Run `applicability --help`.");
            ExitCode::from(3)
        }
        Action::Gate(options) => ExitCode::from(run_gate(&options)),
        Action::GateUsage(message) => {
            eprintln!("diligent: {message}");
            ExitCode::from(2)
        }
        Action::Usage(message) => {
            eprintln!("diligent-rs: {message}");
            ExitCode::from(2)
        }
        Action::Source(all_args) => diligent_source::cli(&all_args),
        Action::Collect(all_args) => diligent_runtime::cli::collect_cli(&all_args),
        Action::Infra(all_args) => diligent_infra::cli::cli(&all_args),
        Action::Assess(all_args) => diligent_assess::cli(&all_args),
        Action::Inventory(all_args) => diligent_inventory::cli::cli(&all_args),
        Action::Evidence(all_args) => diligent_evidence::cli(&all_args),
        Action::ChecksList { json } => {
            let catalog = checks_catalog();
            if json {
                println!("{catalog}");
            } else {
                print_checks(&catalog);
            }
            ExitCode::SUCCESS
        }
        Action::NotImplemented(command) => {
            eprintln!("diligent-rs: `{command}` is not implemented in this version");
            ExitCode::from(3)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn args(items: &[&str]) -> Vec<String> {
        items.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn protocol_version_is_one() {
        assert_eq!(
            interpret(&args(&["protocol-version"])),
            Action::Print("1".to_string())
        );
    }

    #[test]
    fn version_matches_package() {
        assert_eq!(
            interpret(&args(&["--version"])),
            Action::Print(env!("CARGO_PKG_VERSION").to_string())
        );
    }

    #[test]
    fn help_is_printed_without_arguments() {
        assert!(matches!(interpret(&[]), Action::Print(text) if text.contains("Usage")));
    }

    #[test]
    fn gate_dispatches_to_gate_crate() {
        assert_eq!(
            interpret(&args(&["gate", "test-audit"])),
            Action::Gate(GateCliOptions {
                audit_id: Some("test-audit".to_string()),
                root: None,
                dir: None,
                json: false,
                now: None,
            })
        );
    }

    #[test]
    fn gate_usage_errors_are_gate_usage() {
        assert!(matches!(
            interpret(&args(&["gate", "one", "two"])),
            Action::GateUsage(m) if m == "gate takes at most one <audit-id>. Run with --help."
        ));
        assert!(matches!(
            interpret(&args(&["gate", "--nope"])),
            Action::GateUsage(m) if m == "unknown option --nope. Run with --help."
        ));
    }

    #[test]
    fn gate_parses_all_options() {
        let action = interpret(&args(&[
            "gate",
            "test-audit",
            "--root",
            "/r",
            "--dir=/d",
            "--json",
            "--now=2026-01-01T00:00:00.000Z",
        ]));
        assert_eq!(
            action,
            Action::Gate(GateCliOptions {
                audit_id: Some("test-audit".to_string()),
                root: Some(PathBuf::from("/r")),
                dir: Some(PathBuf::from("/d")),
                json: true,
                now: Some("2026-01-01T00:00:00.000Z".to_string()),
            })
        );
    }

    #[test]
    fn collect_dispatches_to_runtime_crate() {
        assert_eq!(
            interpret(&args(&["collect", "--url", "http://x/"])),
            Action::Collect(args(&["collect", "--url", "http://x/"]))
        );
    }

    #[test]
    fn source_dispatches_to_source_crate() {
        assert_eq!(
            interpret(&args(&["source", "scan", "x"])),
            Action::Source(args(&["source", "scan", "x"]))
        );
    }

    #[test]
    fn rules_validate_parses_all_options() {
        let action = interpret(&args(&[
            "rules",
            "validate",
            "--data-root",
            "/d",
            "--schemas=/s",
            "--json",
        ]));
        assert_eq!(
            action,
            Action::RulesValidate(ValidateOptions {
                data_root: Some(PathBuf::from("/d")),
                schemas: Some(PathBuf::from("/s")),
                json: true,
                crosscheck: false,
            })
        );
    }

    #[test]
    fn rules_validate_rejects_bad_options() {
        assert!(matches!(
            interpret(&args(&["rules", "validate", "--nope"])),
            Action::Usage(_)
        ));
        assert!(matches!(
            interpret(&args(&["rules", "validate", "--data-root"])),
            Action::Usage(_)
        ));
        assert!(matches!(
            interpret(&args(&["rules", "validate", "--json=1"])),
            Action::Usage(_)
        ));
    }

    #[test]
    fn unknown_rules_subcommand_is_not_implemented() {
        assert_eq!(
            interpret(&args(&["rules", "lock"])),
            Action::NotImplemented("rules lock".to_string())
        );
    }

    #[test]
    fn checks_list_catalog_counts_sorted_and_covered_by_packs() {
        let catalog = checks_catalog();
        let checks = catalog["checks"].as_array().unwrap();
        let signals = catalog["signals"].as_array().unwrap();
        let source = catalog["source_checks"].as_array().unwrap();
        let infra = catalog["infra_checks"].as_array().unwrap();

        assert_eq!(checks.len(), 28, "browser checks count");
        assert_eq!(signals.len(), 30, "signals count");
        assert_eq!(source.len(), 20, "source checks count");
        assert_eq!(infra.len(), 13, "infra checks count");

        let check_ids: Vec<&str> = checks.iter().map(|c| c["id"].as_str().unwrap()).collect();
        let mut sorted = check_ids.clone();
        sorted.sort();
        assert_eq!(check_ids, sorted, "check ids are sorted");

        let signal_keys: Vec<&str> = signals.iter().map(|s| s["key"].as_str().unwrap()).collect();
        let mut sorted_keys = signal_keys.clone();
        sorted_keys.sort();
        assert_eq!(signal_keys, sorted_keys, "signal keys are sorted");

        let packs_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../rules/packs");
        let mut pack_check_ids = std::collections::BTreeSet::new();
        for entry in std::fs::read_dir(&packs_dir).expect("rules/packs exists") {
            let entry = entry.unwrap();
            let path = entry.path();
            if path.extension().is_some_and(|e| e == "json") {
                let text = std::fs::read_to_string(&path).unwrap();
                let pack: serde_json::Value = serde_json::from_str(&text).unwrap();
                if let Some(rules) = pack["rules"].as_array() {
                    for rule in rules {
                        if let Some(ids) = rule["check_ids"].as_array() {
                            for id in ids {
                                if let Some(s) = id.as_str() {
                                    pack_check_ids.insert(s.to_string());
                                }
                            }
                        }
                    }
                }
            }
        }

        let mut missing = Vec::new();
        for c in checks {
            let id = c["id"].as_str().unwrap();
            if !pack_check_ids.contains(id) {
                missing.push(format!("browser check {id} not in any pack"));
            }
        }
        assert!(
            missing.is_empty(),
            "{} browser check ids not in any pack:\n{}",
            missing.len(),
            missing.join("\n")
        );

        // Source and infra check ids are in the packs too.
        let mut missing_other = Vec::new();
        for c in source {
            let id = c["id"].as_str().unwrap();
            if !pack_check_ids.contains(id) {
                missing_other.push(format!("source:{id}"));
            }
        }
        for c in infra {
            let id = c["id"].as_str().unwrap();
            if !pack_check_ids.contains(id) {
                missing_other.push(format!("infra:{id}"));
            }
        }
        assert!(
            missing_other.is_empty(),
            "{} source or infra check ids not in any pack's check_ids: {}",
            missing_other.len(),
            missing_other.join(", ")
        );
    }

    /// Source and Network cells and `check_ids` agree with the catalog the way Browser does.
    #[test]
    fn source_and_infra_checks_are_listed_by_their_rules_with_matching_cells() {
        use std::collections::BTreeMap;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        // rule id -> (check_ids, channel_support)
        let mut rules: BTreeMap<String, (Vec<String>, serde_json::Value)> = BTreeMap::new();
        for entry in std::fs::read_dir(root.join("rules/packs")).unwrap() {
            let path = entry.unwrap().path();
            if path.extension().is_some_and(|e| e == "json") {
                let pack: serde_json::Value =
                    serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
                for r in pack["rules"].as_array().unwrap() {
                    let ids = r["check_ids"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|i| i.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default();
                    rules.insert(
                        r["rule_id"].as_str().unwrap().to_string(),
                        (ids, r["channel_support"].clone()),
                    );
                }
            }
        }
        // rule id -> (Source cell, Network cell) from the matrix.
        let mut matrix: BTreeMap<String, (String, String)> = BTreeMap::new();
        for line in std::fs::read_to_string(root.join("docs/supported-checks.md"))
            .unwrap()
            .lines()
        {
            let cells: Vec<&str> = line
                .trim()
                .trim_matches('|')
                .split('|')
                .map(str::trim)
                .collect();
            if line.starts_with("| ") && cells.len() >= 9 {
                matrix.insert(
                    cells[1].to_string(),
                    (cells[3].to_string(), cells[5].to_string()),
                );
            }
        }
        let catalog = checks_catalog();
        let mut problems: Vec<String> = Vec::new();
        let mut check =
            |id: &str, rule_id: &str, channel: &str, cell_of: fn(&(String, String)) -> &String| {
                let Some((ids, support)) = rules.get(rule_id) else {
                    problems.push(format!("{id}: informs {rule_id}, which is in no pack"));
                    return;
                };
                if !ids.iter().any(|i| i == id) {
                    problems.push(format!("{rule_id}: check_ids does not list {id}"));
                }
                let value = support[channel].as_str().unwrap_or_default();
                if !matches!(value, "automated" | "assisted") {
                    problems.push(format!(
                        "{rule_id}: channel_support.{channel} is `{value}`, but {id} informs it"
                    ));
                }
                match matrix.get(rule_id) {
                    Some(m) if cell_of(m) == value => {}
                    other => problems.push(format!(
                        "{rule_id}: matrix {channel} cell {other:?} but pack says `{value}`"
                    )),
                }
            };
        for c in catalog["source_checks"].as_array().unwrap() {
            for rule in c["informs"].as_array().unwrap() {
                check(
                    c["id"].as_str().unwrap(),
                    rule.as_str().unwrap(),
                    "source",
                    |m| &m.0,
                );
            }
        }
        for c in catalog["infra_checks"].as_array().unwrap() {
            check(
                c["id"].as_str().unwrap(),
                c["rule"].as_str().unwrap(),
                "network",
                |m| &m.1,
            );
        }
        // Every non-browser id a rule lists is a source or infra check the catalog knows.
        let known: std::collections::BTreeSet<&str> = ["source_checks", "infra_checks"]
            .iter()
            .flat_map(|k| catalog[*k].as_array().unwrap().iter())
            .map(|c| c["id"].as_str().unwrap())
            .collect();
        for (rule_id, (ids, _)) in &rules {
            for id in ids.iter().filter(|i| !i.starts_with("BRW-")) {
                if !known.contains(id.as_str()) {
                    problems.push(format!(
                        "{rule_id}: lists {id}, which is not a source or infra check"
                    ));
                }
            }
        }
        assert!(problems.is_empty(), "{}", problems.join("\n"));
    }

    #[test]
    fn help_text_names_every_subcommand_interpret_accepts() {
        let subcommands = [
            "protocol-version",
            "rules",
            "applicability",
            "collect",
            "assess",
            "infra",
            "source",
            "inventory",
            "evidence",
            "checks",
            "gate",
        ];
        for sub in &subcommands {
            assert!(
                HELP.contains(sub),
                "--help text is missing subcommand `{sub}` that interpret() accepts"
            );
        }
    }
}
