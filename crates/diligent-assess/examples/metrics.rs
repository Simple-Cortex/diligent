//! Measures every check against the ACT cases and writes `docs/a11y-check-metrics.md`.
//!
//! ```text
//! cargo run -p diligent-assess --example metrics [-- --out <path>]
//! ```
//!
//! For each check, over the ACT cases with a recorded capture, it counts the
//! cases by ACT outcome and the check's worst state, and reports precision and
//! recall with `fail` as the positive class against `act_expected == failed`,
//! and the abstention rate (`needs_review`). A `fail` on a case ACT does not
//! call failed is listed by name, because a false fail is a bug, not a metric.
//! Every figure above is the state BEFORE the optional axe-core corroboration
//! step. Where a recorded capture carries an `axe_result`, three more counts per
//! check show what the corroborator did to that check's own findings:
//! `axe_agree`, `axe_disagree` (moved to `needs_review`) and `axe_only` (leads
//! only the corroborator raised).

use std::collections::BTreeMap;
use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use diligent_assess::checks;
use diligent_assess::corroborate;
use diligent_assess::State;
use diligent_capture::CaptureSet;
use serde_json::Value;

#[derive(Default)]
struct Tally {
    /// (ACT outcome, our state) to count.
    cells: BTreeMap<(String, &'static str), usize>,
    cases: usize,
    true_fail: usize,
    false_fail: Vec<String>,
    missed_fail: usize,
    abstained: usize,
    axe_agree: usize,
    axe_disagree: usize,
    axe_only: usize,
}

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn ratio(numerator: usize, denominator: usize) -> String {
    if denominator == 0 {
        "n/a".to_string()
    } else {
        format!("{:.2}", numerator as f64 / denominator as f64)
    }
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let out_path = match args.iter().position(|a| a == "--out") {
        Some(i) => PathBuf::from(args.get(i + 1).expect("--out needs a path")),
        None => repo_root().join("docs/a11y-check-metrics.md"),
    };
    let manifest_path = repo_root().join("fixtures/a11y/act/manifest.json");
    let manifest: Value = serde_json::from_str(
        &std::fs::read_to_string(&manifest_path).expect("act manifest is readable"),
    )
    .expect("act manifest is JSON");

    let mut tallies: BTreeMap<String, Tally> = checks::all()
        .iter()
        .map(|i| (i.id.to_string(), Tally::default()))
        .collect();
    let mut no_capture: Vec<String> = Vec::new();
    let mut axe_versions: std::collections::BTreeSet<String> = std::collections::BTreeSet::new();
    let mut axe_captures = 0usize;
    let mut total_captures = 0usize;

    for case in manifest["cases"].as_array().expect("cases") {
        if !case["skipped_reason"].is_null() {
            continue;
        }
        let rule = case["rule_id"].as_str().unwrap_or_default();
        let name = case["case"].as_str().unwrap_or_default();
        let id = format!("{rule}-{name}");
        let check_id = case["check_id"].as_str().unwrap_or_default().to_string();
        let act = case["act_expected"]
            .as_str()
            .unwrap_or_default()
            .to_string();
        let dir = repo_root().join("fixtures/a11y/captures").join(&id);
        let set = match CaptureSet::load(&dir) {
            Ok(set) => set,
            Err(e) => {
                no_capture.push(format!("{id}: {e}"));
                continue;
            }
        };
        let pre = checks::run(&set);
        let mut worst: Option<State> = None;
        for f in pre.iter().filter(|f| f.check_id == check_id) {
            worst = Some(worst.map_or(f.state, |w| w.worst(f.state)));
        }
        let Some(state) = worst else { continue };
        let Some(tally) = tallies.get_mut(&check_id) else {
            continue;
        };
        total_captures += 1;
        let (post, corroboration) = corroborate::apply(&set, pre);
        if let Some(c) = corroboration {
            axe_captures += 1;
            if let Some(v) = c.version {
                axe_versions.insert(v);
            }
            for f in post.iter().filter(|f| f.check_id == check_id) {
                match f.corroboration.as_ref().and_then(|c| c["outcome"].as_str()) {
                    Some("agree") => tally.axe_agree += 1,
                    Some("disagree") => tally.axe_disagree += 1,
                    Some("axe_only") => tally.axe_only += 1,
                    _ => {}
                }
            }
        }
        tally.cases += 1;
        *tally
            .cells
            .entry((act.clone(), state.as_str()))
            .or_insert(0) += 1;
        match (state == State::Fail, act == "failed") {
            (true, true) => tally.true_fail += 1,
            (true, false) => tally.false_fail.push(id.clone()),
            (false, true) => tally.missed_fail += 1,
            (false, false) => {}
        }
        if state == State::NeedsReview {
            tally.abstained += 1;
        }
    }

    let mut md = String::new();
    let _ = writeln!(md, "# Accessibility browser check metrics\n");
    let _ = writeln!(
        md,
        "Generated on {} by `cargo run -p diligent-assess --example metrics`. Do not edit by hand.\n",
        &diligent_assess::clock::now_rfc3339()[..10]
    );
    let _ = writeln!(
        md,
        "Each ACT case is run through its check and scored by the check's worst state. Precision and recall treat `fail` as the positive class against ACT outcome `failed`. Abstention is the share of cases that end in `needs_review`. A check that never emits `fail` shows precision and recall as n/a.\n"
    );
    let _ = writeln!(
        md,
        "Corroborator: {}. Cases with an `axe_result`: {axe_captures} of {total_captures}. The axe columns count what the corroborator did to each check's own findings and never change the precision, recall or abstention figures.\n",
        if axe_versions.is_empty() {
            "none recorded".to_string()
        } else {
            format!("axe-core {}", axe_versions.iter().cloned().collect::<Vec<_>>().join(", "))
        }
    );
    let _ = writeln!(
        md,
        "| Check | Cases | Precision | Recall | Abstention | axe_agree | axe_disagree | axe_only | ACT outcome to state |"
    );
    let _ = writeln!(
        md,
        "| --- | ---: | ---: | ---: | ---: | ---: | ---: | ---: | --- |"
    );
    for (id, t) in &tallies {
        let cells: Vec<String> = t
            .cells
            .iter()
            .map(|((act, state), n)| format!("{act} to {state}: {n}"))
            .collect();
        let _ = writeln!(
            md,
            "| `{id}` | {} | {} | {} | {} | {} | {} | {} | {} |",
            t.cases,
            ratio(t.true_fail, t.true_fail + t.false_fail.len()),
            ratio(t.true_fail, t.true_fail + t.missed_fail),
            ratio(t.abstained, t.cases),
            t.axe_agree,
            t.axe_disagree,
            t.axe_only,
            if cells.is_empty() {
                "none".to_string()
            } else {
                cells.join("; ")
            },
        );
    }
    let false_fails: Vec<(&String, &String)> = tallies
        .iter()
        .flat_map(|(id, t)| t.false_fail.iter().map(move |c| (id, c)))
        .collect();
    let _ = writeln!(md, "\n## False fails\n");
    if false_fails.is_empty() {
        let _ = writeln!(md, "None. Every `fail` landed on a case ACT calls failed.");
    } else {
        let _ = writeln!(
            md,
            "A `fail` on a case ACT does not call failed. Each is a bug to fix.\n"
        );
        for (id, case) in false_fails {
            let _ = writeln!(md, "- `{id}`: {case}");
        }
    }
    if !no_capture.is_empty() {
        let _ = writeln!(md, "\n## Cases without a usable capture\n");
        for line in &no_capture {
            let _ = writeln!(md, "- {line}");
        }
    }

    signals_table(&mut md);

    std::fs::write(&out_path, md)
        .unwrap_or_else(|e| panic!("cannot write {}: {e}", out_path.display()));
    println!("wrote {}", out_path.display());
}

/// Appends the count of signal rows per state per key over every recorded capture.
///
/// Signals are advisory and have no ground truth here, so this is a table of
/// counts, not of precision or recall.
fn signals_table(md: &mut String) {
    use diligent_assess::signals::{self, SignalState, ALL};
    let dir = repo_root().join("fixtures/a11y/captures");
    let mut counts: BTreeMap<&'static str, [usize; 4]> =
        ALL.iter().map(|i| (i.key, [0; 4])).collect();
    let mut captures = 0usize;
    let mut dirs: Vec<PathBuf> = std::fs::read_dir(&dir)
        .map(|r| {
            r.filter_map(Result::ok)
                .map(|e| e.path())
                .filter(|p| p.is_dir())
                .collect()
        })
        .unwrap_or_default();
    dirs.sort();
    for d in dirs {
        let Ok(set) = CaptureSet::load(&d) else {
            continue;
        };
        captures += 1;
        for s in signals::run(&set) {
            let slot = match s.state {
                SignalState::Lead => 0,
                SignalState::WithinPractice => 1,
                SignalState::Informational => 2,
                SignalState::NotTested => 3,
            };
            if let Some(row) = counts.get_mut(s.key) {
                row[slot] += 1;
            }
        }
    }
    let _ = writeln!(md, "\n## Craft and clarity signals\n");
    let _ = writeln!(
        md,
        "Advisory signals, not check results, counted over {captures} recorded captures. There is no ground truth for a practice signal, so this table counts states and does not score them.\n"
    );
    let _ = writeln!(
        md,
        "| Signal | Lead | Within practice | Informational | Not tested |"
    );
    let _ = writeln!(md, "| --- | --- | --- | --- | --- |");
    for info in ALL.iter() {
        let c = counts[info.key];
        let _ = writeln!(
            md,
            "| `{}` | {} | {} | {} | {} |",
            info.key, c[0], c[1], c[2], c[3]
        );
    }
}
