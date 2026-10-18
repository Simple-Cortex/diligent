//! Command line entry: `assess --capture <dir> --out <dir> [--run-id <id>] [--inventory <INVENTORY.json>] [--no-signals] [--no-corroborate] [--json]`.

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use serde::Serialize;

use crate::{assess, AssessOptions};

const USAGE: &str = "usage: assess --capture <dir> --out <dir> [--run-id <id>] [--inventory <INVENTORY.json>] [--no-signals] [--no-corroborate] [--json]\n\n\
Runs the browser-capture accessibility checks over a `collect` output directory.\n\
  --capture <dir>  directory holding manifest.jsonl and the capture files\n\
  --out <dir>      write RESULTS.jsonl and EVIDENCE.jsonl into <dir>\n\
  --run-id <id>    run identifier, run-YYYY-MM-DD-xxxxxx (default: a fresh one)\n\
  --inventory <f>  INVENTORY.json from `inventory`; lets the title check look for duplicate titles\n\
  --no-signals     skip the advisory signals step and SIGNALS.jsonl\n\
  --no-corroborate skip axe-core corroboration even when the capture has an axe_result\n\
  --json           print the summary as JSON on stdout\n\n\
Exit codes: 0 no fail, 1 at least one fail, 2 needs_review and no fail,\n\
3 usage or load error.\n";

/// Exit code for usage and load errors.
const EXIT_ERROR: u8 = 3;

struct Args {
    capture: PathBuf,
    out: PathBuf,
    run_id: Option<String>,
    inventory: Option<PathBuf>,
    no_signals: bool,
    no_corroborate: bool,
    json: bool,
}

enum Parsed {
    Help,
    Run(Args),
}

fn parse(args: &[String]) -> Result<Parsed, String> {
    let mut it = args.iter().map(String::as_str).peekable();
    if it.peek() == Some(&"assess") {
        it.next();
    }
    let mut capture: Option<PathBuf> = None;
    let mut out: Option<PathBuf> = None;
    let mut run_id: Option<String> = None;
    let mut inventory: Option<PathBuf> = None;
    let mut no_signals = false;
    let mut no_corroborate = false;
    let mut json = false;
    while let Some(arg) = it.next() {
        match arg {
            "-h" | "--help" => return Ok(Parsed::Help),
            "--json" => json = true,
            "--no-signals" => no_signals = true,
            "--no-corroborate" => no_corroborate = true,
            "--capture" => {
                capture = Some(PathBuf::from(
                    it.next().ok_or("--capture needs a directory")?,
                ));
            }
            "--out" => {
                out = Some(PathBuf::from(it.next().ok_or("--out needs a directory")?));
            }
            "--inventory" => {
                inventory = Some(PathBuf::from(it.next().ok_or("--inventory needs a file")?));
            }
            "--run-id" => {
                run_id = Some(it.next().ok_or("--run-id needs a value")?.to_string());
            }
            flag if flag.starts_with('-') => return Err(format!("unknown flag: {flag}")),
            other => return Err(format!("unexpected argument: {other}")),
        }
    }
    Ok(Parsed::Run(Args {
        capture: capture.ok_or("missing --capture <dir>")?,
        out: out.ok_or("missing --out <dir>")?,
        run_id,
        inventory,
        no_signals,
        no_corroborate,
        json,
    }))
}

fn write_jsonl<T: Serialize>(path: &Path, rows: &[T]) -> Result<(), String> {
    let file = File::create(path).map_err(|e| format!("{}: {e}", path.display()))?;
    let mut writer = BufWriter::new(file);
    for row in rows {
        let line = serde_json::to_string(row).map_err(|e| format!("{}: {e}", path.display()))?;
        writeln!(writer, "{line}").map_err(|e| format!("{}: {e}", path.display()))?;
    }
    writer
        .flush()
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// Writes `rows` to a temporary file next to `path`, then renames it into place.
fn write_jsonl_atomic<T: Serialize>(path: &Path, rows: &[T]) -> Result<(), String> {
    let tmp = path.with_extension("jsonl.tmp");
    write_jsonl(&tmp, rows)?;
    fs::rename(&tmp, path).map_err(|e| format!("{}: {e}", path.display()))
}

/// Runs `assess`. `args` may start with the subcommand name.
pub fn cli(args: &[String]) -> ExitCode {
    ExitCode::from(run_cli(args))
}

/// Runs `assess` and returns the exit code as a `u8`.
///
/// This is a seam for tests that need to assert on the numeric exit code,
/// since `std::process::ExitCode` does not implement `Into<u8>` on stable.
pub fn run_cli(args: &[String]) -> u8 {
    let parsed = match parse(args) {
        Ok(Parsed::Help) => {
            print!("{USAGE}");
            return 0;
        }
        Ok(Parsed::Run(a)) => a,
        Err(message) => {
            eprintln!("diligent-rs assess: {message}\n\n{USAGE}");
            return EXIT_ERROR;
        }
    };
    let opts = AssessOptions {
        run_id: parsed.run_id.clone(),
        executed_at: None,
        inventory: parsed.inventory.clone(),
        no_signals: parsed.no_signals,
        corroborate: !parsed.no_corroborate,
    };
    let report = match assess(&parsed.capture, &opts) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("diligent-rs assess: {e}");
            return EXIT_ERROR;
        }
    };
    if let Err(e) = fs::create_dir_all(&parsed.out) {
        eprintln!("diligent-rs assess: {}: {e}", parsed.out.display());
        return EXIT_ERROR;
    }
    let written = write_jsonl(&parsed.out.join("RESULTS.jsonl"), &report.results)
        .and_then(|()| write_jsonl(&parsed.out.join("EVIDENCE.jsonl"), &report.evidence));
    if let Err(e) = written {
        eprintln!("diligent-rs assess: {e}");
        return EXIT_ERROR;
    }
    if !parsed.no_signals {
        if let Err(e) = write_jsonl_atomic(&parsed.out.join("SIGNALS.jsonl"), &report.signals) {
            eprintln!("diligent-rs assess: {e}");
            return EXIT_ERROR;
        }
    }
    let s = &report.summary;
    if parsed.json {
        match serde_json::to_string_pretty(s) {
            Ok(text) => println!("{text}"),
            Err(e) => {
                eprintln!("diligent-rs assess: {e}");
                return EXIT_ERROR;
            }
        }
    } else {
        println!(
            "{} results ({} pass, {} fail, {} needs_review, {} not_tested), {} evidence records, written to {}",
            s.results,
            s.pass,
            s.fail,
            s.needs_review,
            s.not_tested,
            s.evidence,
            parsed.out.display()
        );
        if let Some(g) = &s.signals {
            println!(
                "signals: {} leads, {} within practice, {} informational, {} not tested, written to SIGNALS.jsonl (advisory, not results)",
                g.leads, g.within_practice, g.informational, g.not_tested
            );
        }
        if let Some(c) = &s.corroboration {
            println!(
                "corroboration by {} {}: {} agree, {} disagree (moved to needs_review), {} axe-only, {} no signal, {} no coverage, {} informational, {} unavailable",
                c.engine,
                c.version.as_deref().unwrap_or("(version unknown)"),
                c.agree,
                c.disagree,
                c.axe_only,
                c.no_signal,
                c.no_coverage,
                c.informational,
                c.unavailable
            );
        }
    }
    if s.fail > 0 {
        1
    } else if s.needs_review > 0 {
        2
    } else {
        0
    }
}
