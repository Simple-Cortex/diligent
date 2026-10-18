//! Applicability resolution (audit specification 17.4): for each rule in the profile's
//! selected packs, evaluate the scope predicate, then temporal status, and
//! emit an APPLICABILITY.json document with the predicate trace.

use std::collections::BTreeSet;

use regex::Regex;
use serde::Serialize;
use serde_json::Value;

use crate::load::DataSet;
use crate::model::{Modality, Rule};
use crate::predicate::{
    evaluate, parse_predicate, Date, Expr, FactSet, PredicateError, TraceEntry, Truth,
};

/// Why applicability could not be resolved. The CLI maps all of these to exit 3.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ApplicabilityError {
    #[error("invalid SCOPE.json: {0}")]
    Scope(String),
    #[error("{0}")]
    Profile(String),
    #[error("unusable rule data: {0}")]
    Data(String),
    #[error("rule {rule_id}: {source}")]
    Predicate {
        rule_id: String,
        source: PredicateError,
    },
    #[error("invalid option: {0}")]
    Option(String),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ObligationState {
    Applicable,
    NotApplicable,
    Unknown,
    PendingEffectiveDate,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum TemporalState {
    Effective,
    Pending,
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Exclusion {
    pub citation: String,
    pub predicate_facts: Vec<String>,
    pub evidence_refs: Vec<String>,
}

/// One row of APPLICABILITY.json. Field order matches the schema.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Obligation {
    pub obligation_id: String,
    pub rule_id: Option<String>,
    pub pack_id: Option<String>,
    pub authority_id: Option<String>,
    pub mandatory: bool,
    pub state: ObligationState,
    pub temporal_state: TemporalState,
    pub effective_date: Option<String>,
    pub predicate_trace: Vec<TraceEntry>,
    pub facts_used: Vec<String>,
    pub exclusion: Option<Exclusion>,
}

/// APPLICABILITY.json.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ApplicabilityFile {
    pub schema_version: String,
    pub run_id: String,
    pub audit_id: String,
    pub profile_id: String,
    pub evaluated_at: String,
    pub obligations: Vec<Obligation>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extensions: Option<Value>,
}

/// Counts printed by `applicability --json`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct Summary {
    pub applicable: usize,
    pub not_applicable: usize,
    pub unknown: usize,
    pub pending_effective_date: usize,
    pub mandatory_unknown: usize,
}

impl ApplicabilityFile {
    pub fn summary(&self) -> Summary {
        let count = |s: ObligationState| self.obligations.iter().filter(|o| o.state == s).count();
        Summary {
            applicable: count(ObligationState::Applicable),
            not_applicable: count(ObligationState::NotApplicable),
            unknown: count(ObligationState::Unknown),
            pending_effective_date: count(ObligationState::PendingEffectiveDate),
            mandatory_unknown: self
                .obligations
                .iter()
                .filter(|o| o.state == ObligationState::Unknown && o.mandatory)
                .count(),
        }
    }

    /// Two-space pretty JSON, no trailing newline.
    pub fn to_json(&self) -> String {
        serde_json::to_string_pretty(self).unwrap_or_default()
    }
}

/// Inputs that vary per run.
#[derive(Debug, Clone)]
pub struct ApplicabilityOptions {
    pub as_of: Date,
    pub evaluated_at: String,
    /// Overrides the run id found in SCOPE.json.
    pub run_id: Option<String>,
}

/// Facts plus the identifying fields of a SCOPE.json.
#[derive(Debug, Clone)]
pub struct Scope {
    pub audit_id: String,
    pub run_id: String,
    pub profile_id: String,
    pub facts: FactSet,
}

fn scope_err<T>(message: impl Into<String>) -> Result<T, ApplicabilityError> {
    Err(ApplicabilityError::Scope(message.into()))
}

/// Read the parts of SCOPE.json the engine needs.
pub fn parse_scope(scope: &Value) -> Result<Scope, ApplicabilityError> {
    let Some(obj) = scope.as_object() else {
        return scope_err("the document must be an object");
    };
    let text = |key: &str| -> Result<String, ApplicabilityError> {
        match obj.get(key) {
            Some(Value::String(s)) if !s.is_empty() => Ok(s.clone()),
            _ => scope_err(format!("`{key}` must be a non-empty string")),
        }
    };
    let Some(Value::Array(items)) = obj.get("facts") else {
        return scope_err("`facts` must be an array");
    };
    let mut facts = FactSet::new();
    for (i, item) in items.iter().enumerate() {
        let Some(fact) = item.as_object() else {
            return scope_err(format!("facts[{i}] must be an object"));
        };
        let Some(Value::String(id)) = fact.get("fact_id").filter(|v| v.is_string()) else {
            return scope_err(format!("facts[{i}].fact_id must be a string"));
        };
        let state = fact.get("state").and_then(Value::as_str).unwrap_or("");
        let refs: Vec<String> = match fact.get("evidence_refs") {
            None => Vec::new(),
            Some(Value::Array(a)) => a
                .iter()
                .filter_map(|v| v.as_str().map(str::to_string))
                .collect(),
            Some(_) => return scope_err(format!("facts[{i}].evidence_refs must be an array")),
        };
        let value = fact.get("value").unwrap_or(&Value::Null);
        match FactSet::entry_from_json(state, value, refs) {
            Some(entry) => facts.insert(id, entry),
            None => {
                return scope_err(format!(
                    "facts[{i}].state must be known, unknown or disputed"
                ))
            }
        }
    }
    Ok(Scope {
        audit_id: text("audit_id")?,
        run_id: text("run_id")?,
        profile_id: text("profile_id")?,
        facts,
    })
}

fn temporal_state(rule: &Rule, as_of: Date) -> (TemporalState, Option<String>, Vec<TraceEntry>) {
    let mut trace = Vec::new();
    if let Some(crate::model::Transition::Dates(dates)) = &rule.temporal.transition {
        for (i, t) in dates.iter().enumerate() {
            trace.push(transition_entry(
                i,
                &format!("{}: {}", t.label, t.date.as_deref().unwrap_or("null")),
            ));
        }
    } else if let Some(crate::model::Transition::Text(text)) = &rule.temporal.transition {
        if !text.is_empty() {
            trace.push(transition_entry(0, text));
        }
    }
    let (state, date) = match rule.temporal.effective.as_deref() {
        None => {
            if rule.temporal.pending_effective_date {
                (TemporalState::Unknown, None)
            } else {
                (TemporalState::Effective, None)
            }
        }
        Some(text) => match Date::parse(text) {
            Some(d) if d <= as_of => (TemporalState::Effective, Some(d.to_string())),
            Some(d) => (TemporalState::Pending, Some(d.to_string())),
            None => (TemporalState::Unknown, None),
        },
    };
    (state, date, trace)
}

/// Transition dates are recorded but never change the state in this slice.
fn transition_entry(index: usize, text: &str) -> TraceEntry {
    TraceEntry {
        path: format!("t.{index}"),
        op: "transition".to_string(),
        expr: text.to_string(),
        result: "unknown".to_string(),
        fact: None,
        fact_state: None,
        reason: None,
    }
}

fn mandatory(rule: &Rule) -> bool {
    matches!(
        rule.authority.modality,
        Some(Modality::Must) | Some(Modality::Shall)
    )
}

fn citation(rule: &Rule) -> String {
    let text = format!(
        "{} {}",
        rule.authority.authority_id.as_deref().unwrap_or(""),
        rule.authority.section.as_deref().unwrap_or("")
    );
    let text = text.trim();
    if text.is_empty() {
        rule.rule_id.clone()
    } else {
        text.to_string()
    }
}

/// Parse a rule's predicate and check it only names declared facts.
fn rule_expr(rule: &Rule) -> Result<Expr, ApplicabilityError> {
    let wrap = |source| ApplicabilityError::Predicate {
        rule_id: rule.rule_id.clone(),
        source,
    };
    let expr = parse_predicate(&rule.applicability.predicate).map_err(wrap)?;
    let declared: BTreeSet<&str> = rule
        .applicability
        .required_facts
        .iter()
        .map(String::as_str)
        .collect();
    for fact in expr.facts() {
        if !declared.contains(fact.as_str()) {
            return Err(wrap(PredicateError::new(format!(
                "fact `{fact}` is not listed in required_facts"
            ))));
        }
    }
    Ok(expr)
}

fn resolve_rule(
    pack_id: &str,
    rule: &Rule,
    scope: &Scope,
    as_of: Date,
) -> Result<Obligation, ApplicabilityError> {
    let expr = rule_expr(rule)?;
    let outcome = evaluate(&expr, &scope.facts);
    let facts_used: Vec<String> = expr.facts().into_iter().collect();
    let mut trace = outcome.trace;
    let (temporal, effective_date, transition_trace) = temporal_state(rule, as_of);
    trace.extend(transition_trace);

    let mut truth = outcome.truth;
    if truth == Truth::False && facts_used.is_empty() {
        // A constant-false predicate carries no evidence; do not exclude on it.
        truth = Truth::Unknown;
        if let Some(root) = trace.iter_mut().find(|e| e.path == "0") {
            root.reason = Some("no_facts".to_string());
        }
    }

    let state = match truth {
        Truth::False => ObligationState::NotApplicable,
        Truth::Unknown => ObligationState::Unknown,
        Truth::True => match temporal {
            TemporalState::Effective => ObligationState::Applicable,
            TemporalState::Pending => ObligationState::PendingEffectiveDate,
            TemporalState::Unknown => ObligationState::Unknown,
        },
    };

    let exclusion = if state == ObligationState::NotApplicable {
        let mut refs: Vec<String> = Vec::new();
        for fact in &facts_used {
            if let Some(entry) = scope.facts.get(fact) {
                for r in &entry.evidence_refs {
                    if !refs.contains(r) {
                        refs.push(r.clone());
                    }
                }
            }
        }
        refs.sort();
        Some(Exclusion {
            citation: citation(rule),
            predicate_facts: facts_used.clone(),
            evidence_refs: refs,
        })
    } else {
        None
    };

    Ok(Obligation {
        obligation_id: rule.obligation_id.clone(),
        rule_id: Some(rule.rule_id.clone()),
        pack_id: Some(pack_id.to_string()),
        authority_id: rule.authority.authority_id.clone(),
        mandatory: mandatory(rule),
        state,
        temporal_state: temporal,
        effective_date,
        predicate_trace: trace,
        facts_used,
        exclusion,
    })
}

/// Resolve applicability for every rule in the profile's selected packs.
pub fn resolve_applicability(
    data: &DataSet,
    profile_id: &str,
    scope: &Value,
    options: &ApplicabilityOptions,
) -> Result<ApplicabilityFile, ApplicabilityError> {
    let scope = parse_scope(scope)?;
    if scope.profile_id != profile_id {
        return Err(ApplicabilityError::Scope(format!(
            "SCOPE.json is for profile `{}` but `{profile_id}` was requested",
            scope.profile_id
        )));
    }
    let Some(profile) = data.typed_profiles().find(|p| p.profile_id == profile_id) else {
        return Err(ApplicabilityError::Profile(format!(
            "profile `{profile_id}` was not found in the data root"
        )));
    };
    let run_id = options
        .run_id
        .clone()
        .unwrap_or_else(|| scope.run_id.clone());
    if !Regex::new(r"^run-\d{4}-\d{2}-\d{2}-[0-9a-f]{6}$")
        .map(|re| re.is_match(&run_id))
        .unwrap_or(false)
    {
        return Err(ApplicabilityError::Option(format!(
            "run id `{run_id}` does not match run-YYYY-MM-DD-xxxxxx"
        )));
    }

    let mut obligations = Vec::new();
    for pack_id in &profile.selected_packs {
        let Some(pack) = data.typed_packs().find(|p| &p.pack_id == pack_id) else {
            return Err(ApplicabilityError::Data(format!(
                "profile `{profile_id}` selects pack `{pack_id}`, which was not loaded"
            )));
        };
        for rule in &pack.rules {
            obligations.push(resolve_rule(&pack.pack_id, rule, &scope, options.as_of)?);
        }
    }
    obligations.sort_by(|a, b| {
        (a.pack_id.as_deref(), a.rule_id.as_deref())
            .cmp(&(b.pack_id.as_deref(), b.rule_id.as_deref()))
    });

    Ok(ApplicabilityFile {
        schema_version: "1.0.0".to_string(),
        run_id,
        audit_id: scope.audit_id,
        profile_id: profile.profile_id.clone(),
        evaluated_at: options.evaluated_at.clone(),
        obligations,
        extensions: None,
    })
}

/// Lint every shipped predicate: it parses, names only declared facts, and
/// every declared fact is used. Returns one human-readable line per problem.
pub fn lint_predicates(data: &DataSet) -> Vec<String> {
    let mut out = Vec::new();
    for pack in data.typed_packs() {
        for rule in &pack.rules {
            let label = format!("{}/{}", pack.pack_id, rule.rule_id);
            match parse_predicate(&rule.applicability.predicate) {
                Err(e) => out.push(format!("{label}: predicate does not parse: {e}")),
                Ok(expr) => {
                    let used = expr.facts();
                    let declared: BTreeSet<&str> = rule
                        .applicability
                        .required_facts
                        .iter()
                        .map(String::as_str)
                        .collect();
                    for fact in &used {
                        if !declared.contains(fact.as_str()) {
                            out.push(format!(
                                "{label}: predicate uses `{fact}`, which is not in required_facts"
                            ));
                        }
                    }
                    for fact in &declared {
                        if !used.contains(*fact) {
                            out.push(format!(
                                "{label}: warning: required_facts lists `{fact}`, which the predicate does not reference"
                            ));
                        }
                    }
                }
            }
        }
    }
    out
}
