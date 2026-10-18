//! Command line entry: `infra --url <https://host/> --out <dir> [options]`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde_json::json;
use url::Url;

use crate::fetch::{DnsResolver, Options, ReqwestFetcher};
use crate::model::{InfraReport, Status};
use crate::run;

const USAGE: &str = "usage: infra --url <https://host/> --out <dir> [--timeout-ms 15000] [--max-body-bytes 2097152] [--skip-dns] [--allow-private] [--run-id <id>] [--json]\n\n\
Runs network-level checks on one https origin and writes RESULTS.jsonl, EVIDENCE.jsonl,\n\
and evidence/<sha256>.<ext> body files into <dir>.\n\
  --url <url>            https origin to check (required)\n\
  --out <dir>            output directory (required)\n\
  --timeout-ms <n>       per-request timeout (default 15000)\n\
  --max-body-bytes <n>   largest body kept per response (default 2097152)\n\
  --skip-dns             do not run DNS checks; record them as not_tested\n\
  --allow-private        exempt the --url host (only) from the private-address policy\n\
  --run-id <id>          run id for every row and evidence record (default: generated)\n\
  --json                 print a JSON summary to stdout\n\n\
Exit 0 when every row is pass, needs_review, or not_tested; 1 when any row is fail;\n\
2 when a row is blocked or error and none failed; 3 on a configuration error.\n";

/// Exit code when nothing failed or was blocked.
pub const EXIT_OK: u8 = 0;
/// Exit code when any row failed.
pub const EXIT_FAIL: u8 = 1;
/// Exit code when a row is blocked or error and none failed.
pub const EXIT_BLOCKED: u8 = 2;
/// Exit code for configuration and usage errors.
pub const EXIT_CONFIG: u8 = 3;

struct Args {
    url: Url,
    out: PathBuf,
    opts: Options,
    json: bool,
}

fn parse(args: &[String]) -> Result<Option<Args>, String> {
    let mut it = args.iter().map(String::as_str);
    let mut url: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut opts = Options::default();
    let mut json = false;
    let mut first = true;
    while let Some(arg) = it.next() {
        if first && arg == "infra" {
            first = false;
            continue;
        }
        first = false;
        let (name, inline) = match arg.split_once('=') {
            Some((n, v)) if n.starts_with("--") => (n, Some(v.to_string())),
            _ => (arg, None),
        };
        match name {
            "-h" | "--help" => return Ok(None),
            "--json" => json = true,
            "--skip-dns" => opts.skip_dns = true,
            "--allow-private" => opts.allow_private_origin = true,
            "--url" | "--out" | "--timeout-ms" | "--max-body-bytes" | "--run-id" => {
                let v = match inline {
                    Some(v) => v,
                    None => it
                        .next()
                        .map(str::to_string)
                        .ok_or_else(|| format!("{name} needs a value"))?,
                };
                match name {
                    "--url" => url = Some(v),
                    "--out" => out = Some(PathBuf::from(v)),
                    "--run-id" => {
                        if v.is_empty() {
                            return Err("--run-id must not be empty".to_string());
                        }
                        opts.run_id = Some(v)
                    }
                    "--timeout-ms" => {
                        opts.timeout_ms = v
                            .parse::<u64>()
                            .ok()
                            .filter(|n| *n > 0)
                            .ok_or("--timeout-ms must be a positive integer")?
                    }
                    _ => {
                        opts.max_body_bytes = v
                            .parse::<usize>()
                            .map_err(|_| "--max-body-bytes must be a non-negative integer")?
                    }
                }
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let raw = url.ok_or("missing --url")?;
    let url = Url::parse(&raw).map_err(|e| format!("invalid --url: {e}"))?;
    if url.scheme() != "https" {
        return Err("--url must use https".to_string());
    }
    if url.host_str().is_none() {
        return Err("--url has no host".to_string());
    }
    Ok(Some(Args {
        url,
        out: out.ok_or("missing --out")?,
        opts,
        json,
    }))
}

/// Exit code for a finished report.
pub fn exit_code(report: &InfraReport) -> u8 {
    if report.rows.iter().any(|r| r.status == Status::Fail) {
        EXIT_FAIL
    } else if report
        .rows
        .iter()
        .any(|r| matches!(r.status, Status::Blocked | Status::Error))
    {
        EXIT_BLOCKED
    } else {
        EXIT_OK
    }
}

fn write_all(out: &Path, report: &InfraReport) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    fs::create_dir_all(out.join("evidence")).map_err(io)?;
    for b in &report.bodies {
        fs::write(out.join(&b.path), &b.bytes).map_err(io)?;
    }
    let mut results = String::new();
    for r in &report.rows {
        results.push_str(&serde_json::to_string(r).map_err(|e| e.to_string())?);
        results.push('\n');
    }
    fs::write(out.join("RESULTS.jsonl"), results).map_err(io)?;
    let mut evidence = String::new();
    for e in &report.evidence {
        evidence.push_str(&serde_json::to_string(e).map_err(|e| e.to_string())?);
        evidence.push('\n');
    }
    fs::write(out.join("EVIDENCE.jsonl"), evidence).map_err(io)
}

/// Runs `infra ...` and returns the process exit code. `args` may start with `infra`.
pub fn cli(args: &[String]) -> ExitCode {
    let parsed = match parse(args) {
        Ok(Some(a)) => a,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::from(EXIT_OK);
        }
        Err(msg) => {
            eprintln!("diligent-rs infra: {msg}\n\n{USAGE}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let fetcher = match ReqwestFetcher::new(&parsed.opts) {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("diligent-rs infra: {msg}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let resolver = match DnsResolver::new(&parsed.opts) {
        Ok(r) => r,
        Err(msg) => {
            eprintln!("diligent-rs infra: {msg}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let report = run(&parsed.url, &fetcher, &resolver, &parsed.opts);
    if let Err(msg) = write_all(&parsed.out, &report) {
        eprintln!("diligent-rs infra: cannot write output: {msg}");
        return ExitCode::from(EXIT_CONFIG);
    }
    let code = exit_code(&report);
    let mut counts = serde_json::Map::new();
    for r in &report.rows {
        let n = counts
            .get(r.status.as_str())
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        counts.insert(r.status.as_str().to_string(), json!(n + 1));
    }
    if parsed.json {
        println!(
            "{}",
            json!({
                "origin": parsed.url.as_str(),
                "rows": report.rows.len(),
                "evidence": report.evidence.len(),
                "counts": counts,
                "exit_code": code,
                "out": parsed.out.display().to_string(),
            })
        );
    } else {
        for r in &report.rows {
            println!(
                "{:<13} {:<28} {}",
                r.status.as_str(),
                r.check_id,
                r.obligation_id
            );
        }
        println!(
            "{} rows, {} evidence records written to {}",
            report.rows.len(),
            report.evidence.len(),
            parsed.out.display()
        );
    }
    ExitCode::from(code)
}
