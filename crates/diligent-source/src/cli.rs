//! Command line entry: `source scan <path> [--out <dir>] [--json]`.

use std::fs::{self, File};
use std::io::BufWriter;
use std::path::PathBuf;
use std::process::ExitCode;

use serde_json::json;

use crate::scan::{scan_path, write_jsonl, ScanOptions, ScanReport};

const USAGE: &str = "usage: source scan <path> [--out <dir>] [--run-id <id>] [--json]\n\n\
Scans .html, .htm, .jsx, .tsx, .vue, .svelte, .astro, .twig and .liquid files for\n\
statically decidable accessibility facts.\n\
  --out <dir>  write EVIDENCE.jsonl and RESULTS.jsonl into <dir>\n\
  --run-id <id>  run id for every row and record (default: generated)\n\
  --json       print a JSON summary (with results when --out is absent)\n";

/// Exit code for usage and I/O errors.
const EXIT_ERROR: u8 = 3;

struct Args {
    path: PathBuf,
    out: Option<PathBuf>,
    run_id: Option<String>,
    json: bool,
}

fn parse(args: &[String]) -> Result<Args, String> {
    let mut it = args.iter().map(String::as_str).peekable();
    if it.peek() == Some(&"source") {
        it.next();
    }
    match it.next() {
        Some("scan") => {}
        Some(other) => return Err(format!("unknown subcommand: {other}")),
        None => return Err("missing subcommand".to_string()),
    }
    let mut path: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut run_id: Option<String> = None;
    let mut json = false;
    while let Some(arg) = it.next() {
        match arg {
            "--json" => json = true,
            "--out" => {
                let dir = it.next().ok_or("--out needs a directory")?;
                out = Some(PathBuf::from(dir));
            }
            "--run-id" => {
                let id = it.next().ok_or("--run-id needs a value")?;
                if id.is_empty() {
                    return Err("--run-id must not be empty".to_string());
                }
                run_id = Some(id.to_string());
            }
            flag if flag.starts_with("--") => return Err(format!("unknown flag: {flag}")),
            p => {
                if path.replace(PathBuf::from(p)).is_some() {
                    return Err("more than one path given".to_string());
                }
            }
        }
    }
    Ok(Args {
        path: path.ok_or("missing <path>")?,
        out,
        run_id,
        json,
    })
}

fn summary(report: &ScanReport, out: Option<&PathBuf>) -> serde_json::Value {
    let counts: serde_json::Map<String, serde_json::Value> = report
        .counts()
        .iter()
        .map(|(s, n)| (s.as_str().to_string(), json!(n)))
        .collect();
    json!({
        "root": report.root,
        "run_id": report.run_id,
        "files_scanned": report.files_scanned,
        "files_skipped": report.files_skipped,
        "skipped": report.skipped.iter().map(|s| json!({"path": s.path, "reason": s.reason})).collect::<Vec<_>>(),
        "checks_run": report.checks_run,
        "counts": counts,
        "not_tested": report.not_tested.iter().map(|(c, p)| json!({"check_id": c, "path": p})).collect::<Vec<_>>(),
        "out": out.map(|p| p.display().to_string()),
    })
}

/// Runs the command and returns the process exit code.
///
/// `args` is the argument list after the program name, beginning with
/// `source` (or directly with `scan`). Exit 0 means the scan ran, even when it
/// found failures. Exit 3 means a usage or I/O error.
pub fn cli(args: &[String]) -> ExitCode {
    let args = match parse(args) {
        Ok(a) => a,
        Err(msg) => {
            eprintln!("error: {msg}\n\n{USAGE}");
            return ExitCode::from(EXIT_ERROR);
        }
    };
    let report = match scan_path(
        &args.path,
        &ScanOptions {
            run_id: args.run_id.clone(),
            ..ScanOptions::default()
        },
    ) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::from(EXIT_ERROR);
        }
    };
    if let Some(dir) = &args.out {
        let written = fs::create_dir_all(dir).and_then(|()| {
            let ev = BufWriter::new(File::create(dir.join("EVIDENCE.jsonl"))?);
            let rs = BufWriter::new(File::create(dir.join("RESULTS.jsonl"))?);
            write_jsonl(&report, ev, rs)
        });
        if let Err(e) = written {
            eprintln!("error: cannot write to {}: {e}", dir.display());
            return ExitCode::from(EXIT_ERROR);
        }
    }
    if args.json {
        let mut value = summary(&report, args.out.as_ref());
        if args.out.is_none() {
            let evidence: Vec<_> = report.findings.iter().map(|f| &f.evidence).collect();
            let results: Vec<_> = report.findings.iter().map(|f| &f.result).collect();
            value["evidence"] = json!(evidence);
            value["results"] = json!(results);
        }
        println!("{value}");
    } else {
        println!(
            "scanned {} file(s), skipped {}",
            report.files_scanned, report.files_skipped
        );
        for (state, n) in report.counts() {
            println!("  {:<13} {n}", state.as_str());
        }
        if let Some(dir) = &args.out {
            println!(
                "wrote EVIDENCE.jsonl and RESULTS.jsonl to {}",
                dir.display()
            );
        }
    }
    ExitCode::SUCCESS
}
