//! Port of `scripts/validate-rules.mjs`: schema checks plus every cross-check
//! between the authority registry, rule packs and profiles.
//!
//! Error strings are built from the raw JSON documents, not the typed models,
//! so invalid data is reported exactly as the Node validator reports it. The
//! order of checks and of the resulting errors matches the Node code.

use std::collections::{HashMap, HashSet};
use std::path::Path;

use serde::Serialize;
use serde_json::Value;

use crate::jsval::{length, opt_to_js_string, to_js_string, trims_to_empty, truthy};
use crate::load::{DataFile, DataSet, FileProblem};
use crate::schema::validate_value;

/// Counts printed by `rules validate`. Key order matches the Node output.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub authorities: usize,
    pub packs: usize,
    pub rules: usize,
    pub profiles: usize,
}

/// Result of [`validate`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Validation {
    pub errors: Vec<String>,
    pub stats: Stats,
}

impl Validation {
    /// True when there are no errors.
    pub fn ok(&self) -> bool {
        self.errors.is_empty()
    }

    /// The `--json` document: `{ ok, errors, stats }`, pretty-printed with two
    /// spaces like `JSON.stringify(result, null, 2)`.
    pub fn to_json(&self) -> String {
        #[derive(Serialize)]
        struct Out<'a> {
            ok: bool,
            errors: &'a [String],
            stats: &'a Stats,
        }
        let out = Out {
            ok: self.ok(),
            errors: &self.errors,
            stats: &self.stats,
        };
        serde_json::to_string_pretty(&out).unwrap_or_default()
    }

    /// The human-readable report the Node command prints. `Ok` text goes to
    /// stdout in Node, `Err` text to stderr.
    pub fn render(&self) -> Result<String, String> {
        if self.ok() {
            let s = &self.stats;
            Ok(format!(
                "rules validate: OK. {} authorities, {} rule packs ({} rules), {} profiles validated.",
                s.authorities, s.packs, s.rules, s.profiles
            ))
        } else {
            let n = self.errors.len();
            let mut text = format!(
                "rules validate: INVALID. {n} error{}:",
                if n == 1 { "" } else { "s" }
            );
            for line in &self.errors {
                text.push_str("\n  ");
                text.push_str(line);
            }
            Err(text)
        }
    }
}

/// Loads and caches schemas from a directory, like `loadSchema`.
struct Schemas<'a> {
    dir: &'a Path,
    cache: HashMap<String, Result<Value, String>>,
}

impl<'a> Schemas<'a> {
    fn new(dir: &'a Path) -> Self {
        Schemas {
            dir,
            cache: HashMap::new(),
        }
    }

    fn get(&mut self, name: &str) -> &Result<Value, String> {
        self.cache.entry(name.to_string()).or_insert_with(|| {
            let path = self.dir.join(format!("{name}.schema.json"));
            let text = std::fs::read_to_string(&path)
                .map_err(|e| format!("{}: cannot read schema ({e})", path.display()))?;
            serde_json::from_str(&text)
                .map_err(|e| format!("{}: schema is not valid JSON ({e})", path.display()))
        })
    }

    /// `validateData(schemaName, data, label)`: `label path: message` lines.
    fn validate(&mut self, name: &str, data: &Value, label: &str) -> Vec<String> {
        match self.get(name) {
            Err(message) => vec![format!("{label}: {message}")],
            Ok(schema) => match validate_value(schema, data) {
                Err(problems) => vec![format!(
                    "{label}: Invalid schema {name}: {}",
                    problems.join("; ")
                )],
                Ok(issues) => issues
                    .into_iter()
                    .map(|e| format!("{label} {}: {}", e.path, e.message))
                    .collect(),
            },
        }
    }
}

/// Identity of a JavaScript value for `Set` / `Map` membership. Strings,
/// numbers, booleans and null compare by value. Objects and arrays never
/// compare equal across occurrences, so they get a unique key.
fn set_key(value: Option<&Value>, unique: &mut u64) -> String {
    match value {
        None => "u:".to_string(),
        Some(Value::String(s)) => format!("s:{s}"),
        Some(Value::Number(n)) => format!("n:{}", crate::jsval::number_to_js(n)),
        Some(Value::Bool(b)) => format!("b:{b}"),
        Some(Value::Null) => "z:".to_string(),
        Some(_) => {
            *unique += 1;
            format!("o:{unique}")
        }
    }
}

fn base_name(file_name: &str) -> &str {
    file_name.strip_suffix(".json").unwrap_or(file_name)
}

/// Items of `for (const x of value || [])`. Arrays yield their items and
/// strings yield one string per code point, like a JavaScript string iterator.
/// Other values (which would throw in Node, or are falsy) yield nothing.
fn iter_or_empty(value: Option<&Value>) -> Vec<Value> {
    match value {
        Some(Value::Array(a)) => a.clone(),
        Some(Value::String(s)) => s.chars().map(|c| Value::String(c.to_string())).collect(),
        _ => Vec::new(),
    }
}

fn file_problem_message(problem: &FileProblem, path: &Path) -> String {
    match problem {
        FileProblem::Missing => "file is missing".to_string(),
        FileProblem::NotJson => format!("{}: not valid JSON", path.display()),
        FileProblem::Unreadable(msg) => msg.clone(),
    }
}

/// Validate the loaded data. `schemas_dir` is the directory holding
/// `authority-registry.schema.json`, `rule-pack.schema.json` and
/// `profile.schema.json`.
pub fn validate(data: &DataSet, schemas_dir: &Path) -> Validation {
    let mut errors: Vec<String> = Vec::new();
    let mut stats = Stats::default();
    let mut schemas = Schemas::new(schemas_dir);
    let mut unique = 0u64;

    // ---- registry -------------------------------------------------------
    let mut authority_ids: HashSet<String> = HashSet::new();
    let registry_label = "authorities/registry.json";
    match &data.registry.raw {
        Err(problem) => errors.push(format!(
            "{registry_label}: {}",
            file_problem_message(problem, &data.registry.path)
        )),
        Ok(json) => {
            errors.extend(schemas.validate("authority-registry", json, registry_label));
            let authorities = json
                .get("authorities")
                .and_then(Value::as_array)
                .map(Vec::as_slice)
                .unwrap_or(&[]);
            for authority in authorities {
                stats.authorities += 1;
                let id = authority.get("authority_id");
                let key = set_key(id, &mut unique);
                if authority_ids.contains(&key) {
                    errors.push(format!(
                        "{registry_label}: duplicate authority_id {}",
                        opt_to_js_string(id)
                    ));
                }
                authority_ids.insert(key);
            }
            for authority in authorities {
                for field in ["supersedes", "superseded_by", "incorporates"] {
                    for id in &iter_or_empty(authority.get(field)) {
                        if !authority_ids.contains(&set_key(Some(id), &mut unique)) {
                            errors.push(format!(
                                "{registry_label}: {}.{field} references unknown authority {}",
                                opt_to_js_string(authority.get("authority_id")),
                                to_js_string(id)
                            ));
                        }
                    }
                }
            }
        }
    }

    // ---- file inventory -------------------------------------------------
    if data.profiles.is_empty() {
        errors.push("profiles/: no profile files found".to_string());
    }
    if data.packs.is_empty() {
        errors.push("rules/packs/: no rule pack files found".to_string());
    }
    let pack_ids: HashSet<String> = data
        .packs
        .iter()
        .map(|f| format!("s:{}", base_name(&f.file_name)))
        .collect();
    let profile_ids: HashSet<String> = data
        .profiles
        .iter()
        .map(|f| format!("s:{}", base_name(&f.file_name)))
        .collect();

    // ---- profiles -------------------------------------------------------
    for file in &data.profiles {
        validate_profile(
            file,
            &pack_ids,
            &mut schemas,
            &mut stats,
            &mut errors,
            &mut unique,
        );
    }

    // ---- packs ----------------------------------------------------------
    let mut seen_rules: HashMap<String, String> = HashMap::new();
    for file in &data.packs {
        validate_pack(
            file,
            &profile_ids,
            &authority_ids,
            &mut seen_rules,
            &mut schemas,
            &mut stats,
            &mut errors,
            &mut unique,
        );
    }

    Validation { errors, stats }
}

fn validate_profile(
    file: &DataFile<crate::model::Profile>,
    pack_ids: &HashSet<String>,
    schemas: &mut Schemas<'_>,
    stats: &mut Stats,
    errors: &mut Vec<String>,
    unique: &mut u64,
) {
    let label = format!("profiles/{}", file.file_name);
    let json = match &file.raw {
        Err(problem) => {
            errors.push(format!(
                "{label}: {}",
                file_problem_message(problem, &file.path)
            ));
            return;
        }
        Ok(json) => json,
    };
    stats.profiles += 1;
    errors.extend(schemas.validate("profile", json, &label));
    let profile_id = json.get("profile_id");
    let matches_name = profile_id.and_then(Value::as_str) == Some(base_name(&file.file_name));
    if !matches_name {
        errors.push(format!(
            "{label}: profile_id \"{}\" does not match the file name",
            opt_to_js_string(profile_id)
        ));
    }
    for pack_id in &iter_or_empty(json.get("selected_packs")) {
        if !pack_ids.contains(&set_key(Some(pack_id), unique)) {
            errors.push(format!(
                "{label}: selected_packs references missing pack \"{}\"",
                to_js_string(pack_id)
            ));
        }
    }
}

#[allow(clippy::too_many_arguments)]
fn validate_pack(
    file: &DataFile<crate::model::RulePack>,
    profile_ids: &HashSet<String>,
    authority_ids: &HashSet<String>,
    seen_rules: &mut HashMap<String, String>,
    schemas: &mut Schemas<'_>,
    stats: &mut Stats,
    errors: &mut Vec<String>,
    unique: &mut u64,
) {
    let label = format!("rules/packs/{}", file.file_name);
    let json = match &file.raw {
        Err(problem) => {
            errors.push(format!(
                "{label}: {}",
                file_problem_message(problem, &file.path)
            ));
            return;
        }
        Ok(json) => json,
    };
    stats.packs += 1;
    errors.extend(schemas.validate("rule-pack", json, &label));
    let pack_id = json.get("pack_id");
    if pack_id.and_then(Value::as_str) != Some(base_name(&file.file_name)) {
        errors.push(format!(
            "{label}: pack_id \"{}\" does not match the file name",
            opt_to_js_string(pack_id)
        ));
    }
    for profile_id in &iter_or_empty(json.get("profile_ids")) {
        if !profile_ids.contains(&set_key(Some(profile_id), unique)) {
            errors.push(format!(
                "{label}: profile_ids references missing profile \"{}\"",
                to_js_string(profile_id)
            ));
        }
    }
    let pack_normative = json.get("normative") != Some(&Value::Bool(false));
    let rules = json
        .get("rules")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .unwrap_or(&[]);
    for (index, rule) in rules.iter().enumerate() {
        stats.rules += 1;
        let rule_id = rule.get("rule_id");
        let shown_id = match rule_id {
            None | Some(Value::Null) => "(no rule_id)".to_string(),
            Some(v) => to_js_string(v),
        };
        let at = format!("{label} rules[{index}] {shown_id}");
        if truthy(rule_id) {
            let key = set_key(rule_id, unique);
            match seen_rules.get(&key) {
                Some(first) => {
                    errors.push(format!("{at}: duplicate rule_id, first seen in {first}"))
                }
                None => {
                    seen_rules.insert(key, label.clone());
                }
            }
        }
        let rule_flag = rule.get("normative");
        let rule_normative = pack_normative && rule_flag != Some(&Value::Bool(false));
        let authority_id = rule.get("authority").and_then(|a| a.get("authority_id"));
        if rule_flag == Some(&Value::Bool(false)) && !truthy(authority_id) {
            let basis = rule
                .get("extensions")
                .and_then(|e| e.get("authority_basis"));
            let ok = matches!(basis, Some(Value::String(s)) if !trims_to_empty(s));
            if !ok {
                errors.push(format!(
                    "{at}: non-normative rule without an authority_id needs a non-empty extensions.authority_basis"
                ));
            }
        }
        if rule_normative {
            if !truthy(authority_id) {
                errors.push(format!(
                    "{at}: normative rule has no authority.authority_id"
                ));
            } else if !authority_ids.contains(&set_key(authority_id, unique)) {
                errors.push(format!(
                    "{at}: authority_id \"{}\" does not exist in authorities/registry.json",
                    opt_to_js_string(authority_id)
                ));
            }
        }
        if rule.get("lifecycle").and_then(Value::as_str) == Some("published") {
            let fixtures = rule.get("fixtures");
            let non_empty =
                |key: &str| length(fixtures.and_then(|f| f.get(key))).is_some_and(|n| n > 0);
            if !non_empty("positive") {
                errors.push(format!(
                    "{at}: lifecycle \"published\" requires non-empty fixtures.positive"
                ));
            }
            if !non_empty("negative") {
                errors.push(format!(
                    "{at}: lifecycle \"published\" requires non-empty fixtures.negative"
                ));
            }
            if rule.get("source_review_status").and_then(Value::as_str) != Some("reviewed") {
                errors.push(format!(
                    "{at}: lifecycle \"published\" requires source_review_status \"reviewed\""
                ));
            }
        }
    }
}
