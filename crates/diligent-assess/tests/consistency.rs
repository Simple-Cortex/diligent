//! The support matrix must agree with the checks.
//!
//! For every rule a check informs, the rule record in `rules/packs/a11y-*.json`
//! or `rules/packs/federal-public-web.json` must list the check id in
//! `check_ids` and give the browser channel a value of `automated` or
//! `assisted`, and the Browser cell for that rule in `docs/supported-checks.md`
//! must equal the pack value. Every channel cell of every federal rule must
//! equal the pack value too.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use diligent_assess::checks;
use serde_json::Value;

/// What the pack says about one rule.
struct PackRule {
    pack: String,
    check_ids: Vec<String>,
    browser: String,
    /// All six channel values in matrix order: source, browser, network, model, human, documentary.
    channels: Vec<String>,
}

const CHANNELS: [&str; 6] = [
    "source",
    "browser",
    "network",
    "model",
    "human",
    "documentary",
];

fn repo_root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../..")
}

fn pack_rules() -> BTreeMap<String, PackRule> {
    let dir = repo_root().join("rules/packs");
    let mut out = BTreeMap::new();
    let mut files: Vec<PathBuf> = std::fs::read_dir(&dir)
        .unwrap_or_else(|e| panic!("cannot read {}: {e}", dir.display()))
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            p.file_name().and_then(|n| n.to_str()).is_some_and(|n| {
                (n.starts_with("a11y-") && n.ends_with(".json")) || n == "federal-public-web.json"
            })
        })
        .collect();
    files.sort();
    assert!(!files.is_empty(), "no a11y packs under {}", dir.display());
    for path in files {
        let text = std::fs::read_to_string(&path).expect("pack is readable");
        let doc: Value = serde_json::from_str(&text).expect("pack is JSON");
        let pack = doc["pack_id"].as_str().unwrap_or_default().to_string();
        for rule in doc["rules"].as_array().expect("pack has rules") {
            let id = rule["rule_id"].as_str().expect("rule_id").to_string();
            out.insert(
                id,
                PackRule {
                    pack: pack.clone(),
                    check_ids: rule["check_ids"]
                        .as_array()
                        .map(|a| {
                            a.iter()
                                .filter_map(|c| c.as_str().map(str::to_string))
                                .collect()
                        })
                        .unwrap_or_default(),
                    browser: rule["channel_support"]["browser"]
                        .as_str()
                        .unwrap_or_default()
                        .to_string(),
                    channels: CHANNELS
                        .iter()
                        .map(|c| {
                            rule["channel_support"][*c]
                                .as_str()
                                .unwrap_or_default()
                                .to_string()
                        })
                        .collect(),
                },
            );
        }
    }
    out
}

/// Matrix cells by rule id from the table in `docs/supported-checks.md`: the pack, then the six channels.
fn matrix_rows() -> BTreeMap<String, (String, Vec<String>)> {
    let path = repo_root().join("docs/supported-checks.md");
    let text = std::fs::read_to_string(&path).expect("supported-checks.md is readable");
    let mut out = BTreeMap::new();
    for line in text
        .lines()
        .filter(|l| l.starts_with("| a11y-") || l.starts_with("| federal-public-web"))
    {
        let cells: Vec<&str> = line
            .trim()
            .trim_matches('|')
            .split('|')
            .map(str::trim)
            .collect();
        // Pack, Rule, Outcome, Source, Browser, Network, Model, Human, Documentary.
        if cells.len() >= 9 {
            out.insert(
                cells[1].to_string(),
                (
                    cells[0].to_string(),
                    cells[3..9].iter().map(|c| (*c).to_string()).collect(),
                ),
            );
        }
    }
    out
}

#[test]
fn informed_rules_list_their_checks_and_a_browser_channel() {
    let rules = pack_rules();
    let mut problems: Vec<String> = Vec::new();
    for info in checks::all() {
        assert!(!info.informs.is_empty(), "{} informs no rule", info.id);
        for rule_id in info.informs {
            let Some(rule) = rules.get(*rule_id) else {
                problems.push(format!(
                    "{}: informs {rule_id}, which is in no a11y pack",
                    info.id
                ));
                continue;
            };
            if !rule.check_ids.iter().any(|c| c == info.id) {
                problems.push(format!(
                    "{rule_id} ({}): check_ids does not list {}",
                    rule.pack, info.id
                ));
            }
            if !matches!(rule.browser.as_str(), "automated" | "assisted") {
                problems.push(format!(
                    "{rule_id} ({}): channel_support.browser is `{}`, but {} informs it",
                    rule.pack, rule.browser, info.id
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn listed_check_ids_name_real_checks() {
    let mut problems: Vec<String> = Vec::new();
    for (rule_id, rule) in pack_rules() {
        for id in rule.check_ids.iter().filter(|c| c.starts_with("BRW-")) {
            match checks::all().iter().find(|i| i.id == id) {
                None => problems.push(format!("{rule_id}: lists {id}, which is not a check")),
                Some(info) if !info.informs.contains(&rule_id.as_str()) => {
                    problems.push(format!("{rule_id}: lists {id}, which does not inform it"));
                }
                Some(_) => {}
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn the_matrix_browser_cell_equals_the_pack_value() {
    let rules = pack_rules();
    let cells = matrix_rows();
    let mut problems: Vec<String> = Vec::new();
    for info in checks::all() {
        for rule_id in info.informs {
            let Some(rule) = rules.get(*rule_id) else {
                continue;
            };
            match cells.get(*rule_id) {
                None => problems.push(format!("{rule_id}: no row in docs/supported-checks.md")),
                Some((pack, channels)) => {
                    let cell = &channels[1];
                    if pack != &rule.pack {
                        problems.push(format!(
                            "{rule_id}: matrix pack `{pack}` but pack file says `{}`",
                            rule.pack
                        ));
                    }
                    if cell != &rule.browser {
                        problems.push(format!(
                            "{rule_id}: matrix Browser cell `{cell}` but pack says `{}`",
                            rule.browser
                        ));
                    }
                }
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}

#[test]
fn every_federal_matrix_row_equals_the_pack_in_all_channels() {
    let rules = pack_rules();
    let rows = matrix_rows();
    let mut problems: Vec<String> = Vec::new();
    for (rule_id, rule) in rules.iter().filter(|(_, r)| r.pack == "federal-public-web") {
        let Some((_, cells)) = rows.get(rule_id) else {
            problems.push(format!("{rule_id}: no row in docs/supported-checks.md"));
            continue;
        };
        for (k, name) in CHANNELS.iter().enumerate() {
            if cells[k] != rule.channels[k] {
                problems.push(format!(
                    "{rule_id}: matrix {name} cell `{}` but pack says `{}`",
                    cells[k], rule.channels[k]
                ));
            }
        }
    }
    assert!(problems.is_empty(), "{}", problems.join("\n"));
}
