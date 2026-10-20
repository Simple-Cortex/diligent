//! Deterministic assessment aggregation (blueprint 10.3 and 24.1).
//! Pure functions: no I/O, no clocks (the caller passes `now`).

pub mod run;

use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, BTreeSet, HashMap};

/// Claim boundary text for regulatory assessments.
pub const CLAIM_BOUNDARY: &str = concat!("This is an engineering assessment for the declared scope only. It is not a government certification, not a legal conclusion, and not a claim about the whole site. Diligent ", env!("CARGO_PKG_VERSION"), " automates checks only through the separately installed diligent-rs binary.");

/// Result states that indicate unresolved work.
const OPEN_RESULT_STATES: &[&str] = &["not_tested", "blocked", "error", "needs_review"];

/// Result states that indicate executed checks.
const EXECUTED_STATES: &[&str] = &["pass", "fail", "needs_review"];

/// An obligation from the applicability record.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Obligation {
    pub obligation_id: String,
    #[serde(default)]
    pub rule_id: Option<String>,
    #[serde(default)]
    pub pack_id: Option<String>,
    #[serde(default)]
    pub authority_id: Option<String>,
    pub state: String,
    #[serde(default)]
    pub mandatory: bool,
    #[serde(default)]
    pub temporal_state: Option<String>,
    #[serde(default)]
    pub effective_date: Option<String>,
    #[serde(default)]
    pub predicate_trace: Vec<serde_json::Value>,
    #[serde(default)]
    pub facts_used: Vec<String>,
    #[serde(default)]
    pub exclusion: Option<Exclusion>,
}

/// An exclusion record for not_applicable obligations.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exclusion {
    pub citation: Option<String>,
    #[serde(default)]
    pub predicate_facts: Vec<String>,
}

/// A check result from RESULTS.jsonl.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CheckResult {
    pub result_id: String,
    #[serde(default)]
    pub run_id: Option<String>,
    pub obligation_id: String,
    pub instance_id: String,
    pub check_id: String,
    pub status: String,
    #[serde(default)]
    pub stale: bool,
    #[serde(default)]
    pub supersedes_result_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Exception {
    pub exception_id: String,
    pub obligation_id: String,
    #[serde(default)]
    pub expires_on: Option<String>,
    #[serde(default)]
    pub approval_evidence_refs: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReviewItem {
    pub item_id: String,
    #[serde(default)]
    pub status: String,
    #[serde(default)]
    pub blocking_reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Unresolved {
    pub kind: String,
    pub r#ref: String,
    pub reason: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Failure {
    pub result_id: String,
    pub obligation_id: String,
    pub instance_id: String,
    pub check_id: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExceptionRecord {
    pub exception_id: String,
    pub obligation_id: String,
    pub status: String,
    pub changes_result: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Counts {
    pub selected: usize,
    pub applicable: usize,
    pub not_applicable: usize,
    pub unresolved_applicability: usize,
    pub pending: usize,
    pub required_instances: usize,
    pub executed: usize,
    pub reviewed: usize,
    pub unsupported: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Denominators {
    pub applicable: usize,
    pub not_applicable: usize,
    pub unresolved_applicability: usize,
    pub pending: usize,
    pub executed: usize,
    pub reviewed: usize,
    pub unsupported: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AggregateResult {
    pub overall: String,
    pub has_incomplete_work: bool,
    pub counts: Counts,
    pub denominators: Denominators,
    pub unresolved: Vec<Unresolved>,
    pub failures: Vec<Failure>,
    pub exceptions: Vec<ExceptionRecord>,
    pub advisory: Vec<String>,
}

#[derive(Debug, Clone, Default)]
pub struct Coverage {
    pub unsupported_obligation_ids: Vec<String>,
    pub stale_result_ids: Vec<String>,
    pub required_instances: Option<usize>,
    pub now: Option<String>,
}

/// Drop results that a later result explicitly supersedes.
#[must_use]
pub fn active_results(results: &[CheckResult]) -> Vec<CheckResult> {
    let superseded: BTreeSet<String> = results
        .iter()
        .filter_map(|r| r.supersedes_result_id.clone())
        .collect();
    results
        .iter()
        .filter(|r| !superseded.contains(&r.result_id))
        .cloned()
        .collect()
}

fn has_cited_exclusion(obligation: &Obligation) -> bool {
    if let Some(exclusion) = &obligation.exclusion {
        if let Some(citation) = &exclusion.citation {
            return !citation.trim().is_empty() && !exclusion.predicate_facts.is_empty();
        }
    }
    false
}

fn by_kind_then_ref(a: &Unresolved, b: &Unresolved) -> std::cmp::Ordering {
    if a.kind != b.kind {
        return a.kind.cmp(&b.kind);
    }
    a.r#ref.cmp(&b.r#ref)
}

/// Aggregate applicability, results, exceptions, and review queue into an assessment.
#[must_use]
pub fn aggregate(
    applicability: &[Obligation],
    results: &[CheckResult],
    exceptions: &[Exception],
    review_queue: &[ReviewItem],
    coverage: Coverage,
) -> AggregateResult {
    let mut unresolved: Vec<Unresolved> = Vec::new();
    let mut failures: Vec<Failure> = Vec::new();
    let mut advisory: Vec<String> = Vec::new();

    let add = |unresolved: &mut Vec<Unresolved>, kind: &str, r#ref: &str, reason: &str| {
        unresolved.push(Unresolved {
            kind: kind.to_string(),
            r#ref: r#ref.to_string(),
            reason: reason.to_string(),
        });
    };

    let mut by_id: BTreeMap<String, Obligation> = BTreeMap::new();
    for obligation in applicability {
        if by_id.contains_key(&obligation.obligation_id) {
            add(
                &mut unresolved,
                "duplicate_obligation",
                &obligation.obligation_id,
                "The obligation appears more than once in the applicability record.",
            );
        } else {
            by_id.insert(obligation.obligation_id.clone(), obligation.clone());
        }
    }
    let selected = by_id.len();

    let mut applicable = 0;
    let mut not_applicable = 0;
    let mut pending = 0;
    let mut unresolved_applicability = 0;
    let mut applicable_ids: BTreeSet<String> = BTreeSet::new();

    for obligation in by_id.values() {
        match obligation.state.as_str() {
            "applicable" => {
                applicable += 1;
                applicable_ids.insert(obligation.obligation_id.clone());
            }
            "not_applicable" => {
                if has_cited_exclusion(obligation) {
                    not_applicable += 1;
                } else {
                    unresolved_applicability += 1;
                    add(
                        &mut unresolved,
                        "uncited_exclusion",
                        &obligation.obligation_id,
                        "Marked not applicable without a cited authority and predicate facts. A guess is not a waiver.",
                    );
                }
            }
            "pending_effective_date" => {
                pending += 1;
                add(
                    &mut unresolved,
                    "pending_effective_date",
                    &obligation.obligation_id,
                    "The obligation is not yet effective. It stays pending even if the site already meets its technical target.",
                );
            }
            _ => {
                unresolved_applicability += 1;
                add(
                    &mut unresolved,
                    "unknown_applicability",
                    &obligation.obligation_id,
                    "Applicability is unknown. Required facts are missing or disputed.",
                );
            }
        }
    }

    let active = active_results(results);
    let stale_ids: BTreeSet<String> = coverage.stale_result_ids.into_iter().collect();
    let mut results_by_obligation: HashMap<String, Vec<CheckResult>> = HashMap::new();

    for result in &active {
        if !by_id.contains_key(&result.obligation_id) {
            add(
                &mut unresolved,
                "orphan_result",
                &result.result_id,
                &format!(
                    "The result references obligation {}, which is not in the applicability record.",
                    result.obligation_id
                ),
            );
            continue;
        }
        results_by_obligation
            .entry(result.obligation_id.clone())
            .or_default()
            .push(result.clone());
    }

    let mut instance_keys: BTreeSet<String> = BTreeSet::new();
    let mut executed_keys: BTreeSet<String> = BTreeSet::new();

    for id in applicable_ids.iter() {
        let Some(obligation) = by_id.get(id) else {
            continue;
        };
        let own = results_by_obligation.get(id).cloned().unwrap_or_default();
        if own.is_empty() {
            add(
                &mut unresolved,
                "no_result",
                id,
                "No check result exists for this applicable obligation. It is untested.",
            );
        }
        for result in &own {
            let key = format!("{}|{}", result.obligation_id, result.instance_id);
            instance_keys.insert(key.clone());
            if EXECUTED_STATES.contains(&result.status.as_str()) {
                executed_keys.insert(key);
            }
            if result.status == "fail" {
                if obligation.mandatory {
                    failures.push(Failure {
                        result_id: result.result_id.clone(),
                        obligation_id: id.clone(),
                        instance_id: result.instance_id.clone(),
                        check_id: result.check_id.clone(),
                    });
                } else {
                    advisory.push(result.result_id.clone());
                }
            } else if OPEN_RESULT_STATES.contains(&result.status.as_str()) {
                add(
                    &mut unresolved,
                    &format!("result_{}", result.status),
                    &result.result_id,
                    &format!(
                        "The check result for {} is {}. A result that is not pass or fail is unresolved.",
                        id, result.status
                    ),
                );
            }
            if result.stale || stale_ids.contains(&result.result_id) {
                add(
                    &mut unresolved,
                    "stale_evidence",
                    &result.result_id,
                    &format!("The evidence behind this result for {} is stale.", id),
                );
            }
        }
    }

    let mut unsupported_ids: Vec<String> = coverage
        .unsupported_obligation_ids
        .into_iter()
        .filter(|id| applicable_ids.contains(id))
        .collect();
    unsupported_ids.sort();
    unsupported_ids.dedup();

    for id in &unsupported_ids {
        add(
            &mut unresolved,
            "unsupported_obligation",
            id,
            "No supported channel can test this applicable obligation in this release.",
        );
    }

    let mut reviewed = 0;
    for item in review_queue {
        if item.status == "resolved" {
            reviewed += 1;
        } else {
            add(
                &mut unresolved,
                "open_review_item",
                &item.item_id,
                &format!("Review item is {}: {}", item.status, item.blocking_reason),
            );
        }
    }

    if selected == 0 {
        add(
            &mut unresolved,
            "nothing_selected",
            "applicability",
            "No obligations are selected, so nothing was assessed.",
        );
    }

    let today = coverage
        .now
        .as_deref()
        .unwrap_or("")
        .get(..10)
        .unwrap_or("");
    let mut exception_records: Vec<ExceptionRecord> = exceptions
        .iter()
        .map(|exception| {
            let status = if let Some(expires_on) = &exception.expires_on {
                if !today.is_empty() && expires_on.as_str() < today {
                    "expired"
                } else if exception.approval_evidence_refs.is_empty() {
                    "unverified"
                } else {
                    "active"
                }
            } else if exception.approval_evidence_refs.is_empty() {
                "unverified"
            } else {
                "active"
            };
            ExceptionRecord {
                exception_id: exception.exception_id.clone(),
                obligation_id: exception.obligation_id.clone(),
                status: status.to_string(),
                changes_result: false,
            }
        })
        .collect();
    exception_records.sort_by(|a, b| a.exception_id.cmp(&b.exception_id));

    unresolved.sort_by(by_kind_then_ref);
    failures.sort_by(|a, b| a.result_id.cmp(&b.result_id));

    let overall = if !failures.is_empty() {
        "fail".to_string()
    } else if !unresolved.is_empty() {
        "incomplete".to_string()
    } else if not_applicable == selected {
        "not_applicable".to_string()
    } else {
        "pass_for_declared_scope".to_string()
    };

    let required_instances = coverage.required_instances.unwrap_or(instance_keys.len());

    advisory.sort();

    AggregateResult {
        overall,
        has_incomplete_work: !unresolved.is_empty(),
        counts: Counts {
            selected,
            applicable,
            not_applicable,
            unresolved_applicability,
            pending,
            required_instances,
            executed: executed_keys.len(),
            reviewed,
            unsupported: unsupported_ids.len(),
        },
        denominators: Denominators {
            applicable: selected,
            not_applicable: selected,
            unresolved_applicability: selected,
            pending: selected,
            executed: required_instances,
            reviewed: review_queue.len(),
            unsupported: applicable,
        },
        unresolved,
        failures,
        exceptions: exception_records,
        advisory,
    }
}

/// Process exit code for a gate outcome (blueprint 20.3).
#[must_use]
pub fn exit_code_for(overall: &str) -> i32 {
    match overall {
        "pass_for_declared_scope" | "not_applicable" => 0,
        "fail" => 1,
        "incomplete" => 2,
        _ => 3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn empty_applicability_is_incomplete() {
        let result = aggregate(&[], &[], &[], &[], Coverage::default());
        assert_eq!(result.overall, "incomplete");
        assert!(result.has_incomplete_work);
    }

    #[test]
    fn all_not_applicable_with_citations() {
        let obligations = vec![Obligation {
            obligation_id: "OBL-01".to_string(),
            rule_id: None,
            pack_id: None,
            authority_id: None,
            state: "not_applicable".to_string(),
            mandatory: false,
            temporal_state: None,
            effective_date: None,
            predicate_trace: vec![],
            facts_used: vec![],
            exclusion: Some(Exclusion {
                citation: Some("Authority 1".to_string()),
                predicate_facts: vec!["Fact 1".to_string()],
            }),
        }];
        let result = aggregate(&obligations, &[], &[], &[], Coverage::default());
        assert_eq!(result.overall, "not_applicable");
    }

    #[test]
    fn failure_takes_precedence() {
        let obligations = vec![Obligation {
            obligation_id: "OBL-01".to_string(),
            rule_id: None,
            pack_id: None,
            authority_id: None,
            state: "applicable".to_string(),
            mandatory: true,
            temporal_state: None,
            effective_date: None,
            predicate_trace: vec![],
            facts_used: vec![],
            exclusion: None,
        }];
        let results = vec![CheckResult {
            result_id: "res-01".to_string(),
            run_id: None,
            obligation_id: "OBL-01".to_string(),
            instance_id: "https://example.gov/".to_string(),
            check_id: "CHECK-01".to_string(),
            status: "fail".to_string(),
            stale: false,
            supersedes_result_id: None,
        }];
        let result = aggregate(&obligations, &results, &[], &[], Coverage::default());
        assert_eq!(result.overall, "fail");
        assert_eq!(result.failures.len(), 1);
    }
}
