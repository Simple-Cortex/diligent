//! Command line entry: `inventory --url <https://host/> --out <dir> [options]`.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use diligent_infra::fetch::{DnsResolver, ReqwestFetcher};
use serde_json::json;
use url::Url;

use crate::clock::SystemClock;
use crate::crawl::run;
use crate::model::{parse_processes, InventoryOptions};

const USAGE: &str = "usage: inventory --url <https://host/> --out <dir> [--run-id <id>] [--audit-id <id>] [--seed <s>]\n\
                 [--processes <file>] [--allow-origin <origin>]... [--max-fetch <n>] [--max-discovered <n>]\n\
                 [--rps <n>] [--max-time <seconds>] [--query-key <k>]... [--ignore-robots] [--allow-private] [--timeout-ms <n>] [--json]\n\n\
Reads robots.txt and sitemaps, crawls same-origin links, and writes INVENTORY.json and SAMPLE.json\n\
(a WCAG-EM 2.0 structured plus random sample) into <dir>. Fetch only: no browser, no scripts.\n\
  --url <url>             start page (required); its origin is the scope\n\
  --out <dir>             output directory (required)\n\
  --run-id <id>           run-YYYY-MM-DD-xxxxxx (default: generated)\n\
  --audit-id <id>         kebab-case audit id (default: audit)\n\
  --seed <s>              seed string for the random sample (default: the run id)\n\
  --processes <file>      JSON [{name, consequence: high|normal, steps: [url...]}]; never inferred\n\
  --allow-origin <o>      also inventory this origin (repeatable)\n\
  --max-fetch <n>         most pages fetched (default 200)\n\
  --max-discovered <n>    most URLs recorded (default 5000)\n\
  --rps <n>               most requests per second (default 2); robots Crawl-delay can lower it\n\
  --max-time <seconds>    time budget for the whole run (default 300); when it ends the run stops, is partial and exits 2\n  --query-key <k>         query key that identifies a page (repeatable); other keys are dropped\n\
  --ignore-robots         fetch pages robots.txt disallows (operator override, recorded)\n\
  --allow-private         exempt the --url host (only) from the private-address policy\n\
  --timeout-ms <n>        per-request timeout (default 15000)\n\
  --json                  print a JSON summary to stdout\n\n\
Exit 0 when complete; 2 when partial (fetch or time budget, sitemap limits, incomplete process,\n\
or robots.txt disallowing the start URL, in which case the routes come from the sitemaps and\n\
nothing is fetched or sampled); 3 on a usage error or when the start URL cannot be fetched.\n";

/// Exit code for a complete run.
pub const EXIT_OK: u8 = 0;
/// Exit code for a partial run.
pub const EXIT_PARTIAL: u8 = 2;
/// Exit code for usage errors and start-URL failures.
pub const EXIT_CONFIG: u8 = 3;

struct Args {
    url: Url,
    out: PathBuf,
    opts: InventoryOptions,
    json: bool,
}

fn valid_run_id(s: &str) -> bool {
    let b = s.as_bytes();
    // run-YYYY-MM-DD-xxxxxx
    b.len() == 21
        && s.starts_with("run-")
        && b[8] == b'-'
        && b[11] == b'-'
        && b[14] == b'-'
        && b[4..8].iter().all(u8::is_ascii_digit)
        && b[9..11].iter().all(u8::is_ascii_digit)
        && b[12..14].iter().all(u8::is_ascii_digit)
        && b[15..]
            .iter()
            .all(|c| matches!(c, b'0'..=b'9' | b'a'..=b'f'))
}

fn valid_audit_id(s: &str) -> bool {
    !s.is_empty()
        && s.split('-').all(|p| {
            !p.is_empty()
                && p.bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit())
        })
}

fn parse(args: &[String]) -> Result<Option<Args>, String> {
    let mut it = args.iter().map(String::as_str);
    let mut url: Option<String> = None;
    let mut out: Option<PathBuf> = None;
    let mut run_id: Option<String> = None;
    let mut audit_id: Option<String> = None;
    let mut seed: Option<String> = None;
    let mut processes: Option<PathBuf> = None;
    let mut allow: Vec<String> = Vec::new();
    let mut keys: Vec<String> = Vec::new();
    let mut max_fetch: Option<usize> = None;
    let mut max_discovered: Option<usize> = None;
    let mut rps: Option<f64> = None;
    let mut timeout: Option<u64> = None;
    let mut max_time: Option<f64> = None;
    let (mut ignore_robots, mut allow_private, mut json) = (false, false, false);
    let mut first = true;
    while let Some(arg) = it.next() {
        if first && arg == "inventory" {
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
            "--ignore-robots" => ignore_robots = true,
            "--allow-private" => allow_private = true,
            "--url" | "--out" | "--run-id" | "--audit-id" | "--seed" | "--processes"
            | "--allow-origin" | "--max-fetch" | "--max-discovered" | "--rps" | "--query-key"
            | "--timeout-ms" | "--max-time" => {
                let v = match inline {
                    Some(v) => v,
                    None => it
                        .next()
                        .map(str::to_string)
                        .ok_or_else(|| format!("{name} needs a value"))?,
                };
                let positive = |what: &str| -> Result<usize, String> {
                    v.parse::<usize>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or_else(|| format!("{what} must be a positive integer"))
                };
                match name {
                    "--url" => url = Some(v),
                    "--out" => out = Some(PathBuf::from(v)),
                    "--run-id" => run_id = Some(v),
                    "--audit-id" => audit_id = Some(v),
                    "--seed" => seed = Some(v),
                    "--processes" => processes = Some(PathBuf::from(v)),
                    "--allow-origin" => allow.push(v),
                    "--query-key" => keys.push(v),
                    "--max-fetch" => max_fetch = Some(positive("--max-fetch")?),
                    "--max-discovered" => max_discovered = Some(positive("--max-discovered")?),
                    "--timeout-ms" => timeout = Some(positive("--timeout-ms")? as u64),
                    "--max-time" => {
                        max_time = Some(
                            v.parse::<f64>()
                                .ok()
                                .filter(|n| n.is_finite() && *n > 0.0)
                                .ok_or("--max-time must be a positive number of seconds")?,
                        )
                    }
                    _ => {
                        rps = Some(
                            v.parse::<f64>()
                                .ok()
                                .filter(|n| n.is_finite() && *n > 0.0)
                                .ok_or("--rps must be a positive number")?,
                        )
                    }
                }
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    let raw = url.ok_or("missing --url")?;
    let url = Url::parse(&raw).map_err(|e| format!("invalid --url: {e}"))?;
    if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
        return Err("--url must be an http or https URL with a host".to_string());
    }
    if url.scheme() == "http" && !allow_private {
        return Err("--url must use https (http is allowed only with --allow-private)".to_string());
    }
    let run_id = run_id.unwrap_or_else(|| diligent_source::time::new_run_id(url.as_str()));
    if !valid_run_id(&run_id) {
        return Err("--run-id must look like run-2026-10-18-0a1b2c".to_string());
    }
    let audit_id = audit_id.unwrap_or_else(|| "audit".to_string());
    if !valid_audit_id(&audit_id) {
        return Err("--audit-id must be kebab-case letters and digits".to_string());
    }
    for o in &allow {
        if crate::urls::parse_origin(o).is_none() {
            return Err(format!("--allow-origin `{o}` is not an http(s) origin"));
        }
    }
    let mut opts = InventoryOptions::new(run_id, audit_id);
    opts.seed = seed;
    opts.allowed_origins = allow;
    opts.query_keys = keys;
    opts.ignore_robots = ignore_robots;
    opts.net.allow_private_origin = allow_private;
    if let Some(n) = max_fetch {
        opts.max_fetch = n;
    }
    if let Some(n) = max_discovered {
        opts.max_discovered = n;
    }
    if let Some(n) = rps {
        opts.requests_per_second = n;
    }
    if let Some(n) = max_time {
        opts.max_time_ms = (n * 1000.0).ceil() as u64;
    }
    if let Some(n) = timeout {
        opts.net.timeout_ms = n;
    }
    if let Some(path) = processes {
        let text = fs::read_to_string(&path)
            .map_err(|e| format!("cannot read {}: {e}", path.display()))?;
        let value: serde_json::Value = serde_json::from_str(&text)
            .map_err(|e| format!("{} is not valid JSON: {e}", path.display()))?;
        opts.processes = parse_processes(&value)?;
    }
    Ok(Some(Args {
        url,
        out: out.ok_or("missing --out")?,
        opts,
        json,
    }))
}

/// Writes both files through temp files in the same directory, then renames them into place.
fn write_atomic(out: &Path, files: &[(&str, String)]) -> Result<(), String> {
    let io = |e: std::io::Error| e.to_string();
    fs::create_dir_all(out).map_err(io)?;
    let pid = std::process::id();
    let mut temps = Vec::new();
    for (name, text) in files {
        let tmp = out.join(format!(".{name}.tmp-{pid}"));
        if let Err(e) = fs::write(&tmp, text) {
            for (t, _) in &temps {
                let _ = fs::remove_file(t);
            }
            return Err(e.to_string());
        }
        temps.push((tmp, out.join(name)));
    }
    for (tmp, dest) in temps {
        fs::rename(&tmp, &dest).map_err(io)?;
    }
    Ok(())
}

/// Runs `inventory ...` and returns the process exit code. `args` may start with `inventory`.
pub fn cli(args: &[String]) -> ExitCode {
    let parsed = match parse(args) {
        Ok(Some(a)) => a,
        Ok(None) => {
            print!("{USAGE}");
            return ExitCode::from(EXIT_OK);
        }
        Err(msg) => {
            eprintln!("diligent-rs inventory: {msg}\n\n{USAGE}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let fetcher = match ReqwestFetcher::new(&parsed.opts.net) {
        Ok(f) => f,
        Err(msg) => {
            eprintln!("diligent-rs inventory: {msg}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let resolver = match DnsResolver::new(&parsed.opts.net) {
        Ok(r) => r,
        Err(msg) => {
            eprintln!("diligent-rs inventory: {msg}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    let outcome = run(
        &parsed.url,
        &parsed.opts,
        &fetcher,
        &resolver,
        &SystemClock::new(),
    );
    if let Some(why) = &outcome.start_failure {
        eprintln!("diligent-rs inventory: {why}; nothing was written");
        return ExitCode::from(EXIT_CONFIG);
    }
    let render = |v: serde_json::Result<String>| v.map(|s| s + "\n").map_err(|e| e.to_string());
    let files = match (
        render(serde_json::to_string_pretty(&outcome.inventory)),
        render(serde_json::to_string_pretty(&outcome.sample)),
    ) {
        (Ok(i), Ok(s)) => [("INVENTORY.json", i), ("SAMPLE.json", s)],
        (Err(e), _) | (_, Err(e)) => {
            eprintln!("diligent-rs inventory: cannot serialize output: {e}");
            return ExitCode::from(EXIT_CONFIG);
        }
    };
    if let Err(msg) = write_atomic(&parsed.out, &files) {
        eprintln!("diligent-rs inventory: cannot write output: {msg}");
        return ExitCode::from(EXIT_CONFIG);
    }
    let code = if outcome.sample.partial {
        EXIT_PARTIAL
    } else {
        EXIT_OK
    };
    let c = &outcome.sample.coverage;
    if parsed.json {
        println!(
            "{}",
            json!({
                "run_id": outcome.sample.run_id,
                "seed": outcome.sample.seed,
                "coverage": c,
                "partial": outcome.sample.partial,
                "remaining_work": outcome.sample.remaining_work,
                "structured_coverage_estimate": outcome.sample.structured_coverage_estimate,
                "exit_code": code,
                "out": parsed.out.display().to_string(),
            })
        );
    } else {
        println!(
            "{} pages discovered, {} fetched, {} templates, {} structured and {} random sample pages ({} requests)",
            c.discovered, c.fetched, c.clustered, c.sampled_structured, c.sampled_random, outcome.requests
        );
        if outcome.sample.partial {
            println!("partial:");
            for r in &outcome.sample.remaining_work {
                println!("  {r}");
            }
        }
        println!(
            "wrote INVENTORY.json and SAMPLE.json to {}",
            parsed.out.display()
        );
    }
    ExitCode::from(code)
}
