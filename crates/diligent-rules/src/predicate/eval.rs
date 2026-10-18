//! Three-valued evaluation with a flat, deterministic trace.

use std::collections::HashMap;

use serde::Serialize;

use super::ast::{CmpOp, DateOp, Expr, Literal, Operand};
use super::date::Date;

/// Result of evaluating a predicate.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Truth {
    True,
    False,
    Unknown,
}

impl Truth {
    pub fn as_str(self) -> &'static str {
        match self {
            Truth::True => "true",
            Truth::False => "false",
            Truth::Unknown => "unknown",
        }
    }

    fn not(self) -> Truth {
        match self {
            Truth::True => Truth::False,
            Truth::False => Truth::True,
            Truth::Unknown => Truth::Unknown,
        }
    }
}

/// A typed fact value.
#[derive(Debug, Clone, PartialEq)]
pub enum Val {
    Bool(bool),
    Num(f64),
    Str(String),
    /// An array or object. Known, but not comparable.
    Other,
}

/// The state a fact is in, as reported in the trace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FactState {
    Known,
    Unknown,
    Disputed,
    Absent,
}

impl FactState {
    pub fn as_str(self) -> &'static str {
        match self {
            FactState::Known => "known",
            FactState::Unknown => "unknown",
            FactState::Disputed => "disputed",
            FactState::Absent => "absent",
        }
    }
}

/// One fact from SCOPE.json.
#[derive(Debug, Clone, PartialEq)]
pub struct FactEntry {
    /// Effective state: `known` with a null value is reported as `unknown`.
    pub state: FactState,
    /// Present only when the state is `known`.
    pub value: Option<Val>,
    pub evidence_refs: Vec<String>,
}

/// Facts keyed by fact id.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct FactSet {
    facts: HashMap<String, FactEntry>,
}

impl FactSet {
    pub fn new() -> Self {
        FactSet::default()
    }

    /// Add a fact. A second entry for the same id makes the fact disputed
    /// (value dropped, evidence merged) unless it is identical to the first.
    pub fn insert(&mut self, id: &str, entry: FactEntry) {
        match self.facts.get_mut(id) {
            None => {
                self.facts.insert(id.to_string(), entry);
            }
            Some(existing) => {
                if *existing == entry {
                    return;
                }
                existing.state = FactState::Disputed;
                existing.value = None;
                for r in entry.evidence_refs {
                    if !existing.evidence_refs.contains(&r) {
                        existing.evidence_refs.push(r);
                    }
                }
            }
        }
    }

    pub fn get(&self, id: &str) -> Option<&FactEntry> {
        self.facts.get(id)
    }

    /// Build an entry from the declared state and JSON value.
    pub fn entry_from_json(
        state: &str,
        value: &serde_json::Value,
        evidence_refs: Vec<String>,
    ) -> Option<FactEntry> {
        use serde_json::Value;
        let state = match state {
            "known" => FactState::Known,
            "unknown" => FactState::Unknown,
            "disputed" => FactState::Disputed,
            _ => return None,
        };
        let val = match value {
            Value::Null => None,
            Value::Bool(b) => Some(Val::Bool(*b)),
            Value::Number(n) => Some(n.as_f64().map_or(Val::Other, Val::Num)),
            Value::String(s) => Some(Val::Str(s.clone())),
            Value::Array(_) | Value::Object(_) => Some(Val::Other),
        };
        let (state, value) = match (state, val) {
            (FactState::Known, Some(v)) => (FactState::Known, Some(v)),
            (FactState::Known, None) => (FactState::Unknown, None),
            (other, _) => (other, None),
        };
        Some(FactEntry {
            state,
            value,
            evidence_refs,
        })
    }
}

/// One line of the predicate trace.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct TraceEntry {
    pub path: String,
    pub op: String,
    pub expr: String,
    pub result: String,
    pub fact: Option<String>,
    pub fact_state: Option<String>,
    pub reason: Option<String>,
}

/// Truth value plus the trace, root entry last.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvalOutcome {
    pub truth: Truth,
    pub trace: Vec<TraceEntry>,
}

/// Evaluate `expr` against `facts`.
pub fn evaluate(expr: &Expr, facts: &FactSet) -> EvalOutcome {
    let mut trace = Vec::new();
    let truth = eval(expr, facts, "0", &mut trace);
    EvalOutcome { truth, trace }
}

/// What looking up an operand produced.
enum Resolved {
    Value(Val),
    Unknown { reason: Option<&'static str> },
}

struct Leaf {
    truth: Truth,
    fact: Option<String>,
    fact_state: Option<FactState>,
    reason: Option<&'static str>,
}

fn lookup(id: &str, facts: &FactSet) -> (Resolved, FactState) {
    match facts.get(id) {
        None => (
            Resolved::Unknown {
                reason: Some("missing_fact"),
            },
            FactState::Absent,
        ),
        Some(entry) => match (&entry.state, &entry.value) {
            (FactState::Known, Some(v)) => (Resolved::Value(v.clone()), FactState::Known),
            (state, _) => (Resolved::Unknown { reason: None }, *state),
        },
    }
}

fn resolve_operand(operand: &Operand, facts: &FactSet) -> (Resolved, Option<(String, FactState)>) {
    match operand {
        Operand::Fact(id) => {
            let (resolved, state) = lookup(id, facts);
            (resolved, Some((id.clone(), state)))
        }
        Operand::Lit(lit) => (
            Resolved::Value(match lit {
                Literal::Str(s) => Val::Str(s.clone()),
                Literal::Num(n) => Val::Num(*n),
                Literal::Bool(b) => Val::Bool(*b),
                // A null literal is never comparable.
                Literal::Null => Val::Other,
            }),
            None,
        ),
    }
}

fn bool_truth(b: bool) -> Truth {
    if b {
        Truth::True
    } else {
        Truth::False
    }
}

/// Compare two known values. `None` means a type mismatch.
fn compare(op: CmpOp, a: &Val, b: &Val) -> Option<bool> {
    use std::cmp::Ordering;
    let ordering: Option<Ordering> = match (a, b) {
        (Val::Num(x), Val::Num(y)) => x.partial_cmp(y),
        (Val::Str(x), Val::Str(y)) => Some(x.as_bytes().cmp(y.as_bytes())),
        (Val::Bool(x), Val::Bool(y)) => {
            return match op {
                CmpOp::Eq => Some(x == y),
                CmpOp::Ne => Some(x != y),
                _ => None,
            };
        }
        _ => None,
    };
    let ord = ordering?;
    Some(match op {
        CmpOp::Eq => ord == Ordering::Equal,
        CmpOp::Ne => ord != Ordering::Equal,
        CmpOp::Gte => ord != Ordering::Less,
        CmpOp::Lte => ord != Ordering::Greater,
        CmpOp::Gt => ord == Ordering::Greater,
        CmpOp::Lt => ord == Ordering::Less,
    })
}

fn literal_val(lit: &Literal) -> Val {
    match lit {
        Literal::Str(s) => Val::Str(s.clone()),
        Literal::Num(n) => Val::Num(*n),
        Literal::Bool(b) => Val::Bool(*b),
        Literal::Null => Val::Other,
    }
}

/// Pick the fact to report for a two-operand node: the first unresolved one,
/// otherwise the first fact.
fn pick_fact(
    a: (&Resolved, Option<(String, FactState)>),
    b: (&Resolved, Option<(String, FactState)>),
) -> Option<(String, FactState)> {
    if matches!(a.0, Resolved::Unknown { .. }) && a.1.is_some() {
        return a.1;
    }
    if matches!(b.0, Resolved::Unknown { .. }) && b.1.is_some() {
        return b.1;
    }
    a.1.or(b.1)
}

fn eval_cmp(op: CmpOp, left: &Operand, right: &Operand, facts: &FactSet) -> Leaf {
    let (lr, lf) = resolve_operand(left, facts);
    let (rr, rf) = resolve_operand(right, facts);
    let fact = pick_fact((&lr, lf), (&rr, rf));
    let (truth, reason) = match (&lr, &rr) {
        (Resolved::Value(a), Resolved::Value(b)) => match compare(op, a, b) {
            Some(r) => (bool_truth(r), None),
            None => (Truth::Unknown, Some("type_mismatch")),
        },
        (Resolved::Unknown { reason: r1 }, Resolved::Unknown { reason: r2 }) => {
            (Truth::Unknown, r1.or(*r2))
        }
        (Resolved::Unknown { reason }, _) | (_, Resolved::Unknown { reason }) => {
            (Truth::Unknown, *reason)
        }
    };
    Leaf {
        truth,
        fact: fact.as_ref().map(|(id, _)| id.clone()),
        fact_state: fact.map(|(_, s)| s),
        reason,
    }
}

fn eval_in(left: &Operand, values: &[Literal], facts: &FactSet) -> Leaf {
    let (resolved, fact) = resolve_operand(left, facts);
    let (truth, reason) = match &resolved {
        Resolved::Unknown { reason } => (Truth::Unknown, *reason),
        Resolved::Value(v) => {
            let mut mismatch = false;
            let mut hit = false;
            for lit in values {
                match compare(CmpOp::Eq, v, &literal_val(lit)) {
                    Some(true) => hit = true,
                    Some(false) => {}
                    None => mismatch = true,
                }
            }
            if hit {
                (Truth::True, None)
            } else if mismatch {
                (Truth::Unknown, Some("type_mismatch"))
            } else {
                (Truth::False, None)
            }
        }
    };
    Leaf {
        truth,
        fact: fact.as_ref().map(|(id, _)| id.clone()),
        fact_state: fact.map(|(_, s)| s),
        reason,
    }
}

fn eval_date(op: DateOp, id: &str, date: Date, facts: &FactSet) -> Leaf {
    let (resolved, state) = lookup(id, facts);
    let (truth, reason) = match resolved {
        Resolved::Unknown { reason } => (Truth::Unknown, reason),
        Resolved::Value(Val::Str(s)) => match Date::parse(&s) {
            Some(d) => (
                bool_truth(match op {
                    DateOp::Before => d < date,
                    DateOp::OnOrAfter => d >= date,
                }),
                None,
            ),
            None => (Truth::Unknown, Some("invalid_date")),
        },
        Resolved::Value(_) => (Truth::Unknown, Some("type_mismatch")),
    };
    Leaf {
        truth,
        fact: Some(id.to_string()),
        fact_state: Some(state),
        reason,
    }
}

fn eval_bare_fact(id: &str, facts: &FactSet) -> Leaf {
    let (resolved, state) = lookup(id, facts);
    let (truth, reason) = match resolved {
        Resolved::Unknown { reason } => (Truth::Unknown, reason),
        Resolved::Value(Val::Bool(b)) => (bool_truth(b), None),
        Resolved::Value(_) => (Truth::Unknown, Some("type_mismatch")),
    };
    Leaf {
        truth,
        fact: Some(id.to_string()),
        fact_state: Some(state),
        reason,
    }
}

fn eval_exists(id: &str, facts: &FactSet) -> Leaf {
    let (resolved, state) = lookup(id, facts);
    let (truth, reason) = match resolved {
        Resolved::Value(_) => (Truth::True, None),
        // Never false: missing intake data cannot manufacture non-applicability.
        Resolved::Unknown { reason } => (Truth::Unknown, reason),
    };
    Leaf {
        truth,
        fact: Some(id.to_string()),
        fact_state: Some(state),
        reason,
    }
}

fn push(trace: &mut Vec<TraceEntry>, path: &str, expr: &Expr, leaf: Leaf) -> Truth {
    trace.push(TraceEntry {
        path: path.to_string(),
        op: expr.op_name().to_string(),
        expr: expr.to_string(),
        result: leaf.truth.as_str().to_string(),
        fact: leaf.fact,
        fact_state: leaf.fact_state.map(|s| s.as_str().to_string()),
        reason: leaf.reason.map(str::to_string),
    });
    leaf.truth
}

fn compound(trace: &mut Vec<TraceEntry>, path: &str, expr: &Expr, truth: Truth) -> Truth {
    push(
        trace,
        path,
        expr,
        Leaf {
            truth,
            fact: None,
            fact_state: None,
            reason: None,
        },
    )
}

fn eval(expr: &Expr, facts: &FactSet, path: &str, trace: &mut Vec<TraceEntry>) -> Truth {
    match expr {
        Expr::Fact(id) => push(trace, path, expr, eval_bare_fact(id, facts)),
        Expr::Lit(lit) => {
            let (truth, reason) = match lit {
                Literal::Bool(b) => (bool_truth(*b), None),
                _ => (Truth::Unknown, Some("type_mismatch")),
            };
            push(
                trace,
                path,
                expr,
                Leaf {
                    truth,
                    fact: None,
                    fact_state: None,
                    reason,
                },
            )
        }
        Expr::Not(inner) => {
            let t = eval(inner, facts, &format!("{path}.0"), trace).not();
            compound(trace, path, expr, t)
        }
        Expr::All(items) => {
            let results: Vec<Truth> = items
                .iter()
                .enumerate()
                .map(|(i, item)| eval(item, facts, &format!("{path}.{i}"), trace))
                .collect();
            let t = if results.contains(&Truth::False) {
                Truth::False
            } else if results.contains(&Truth::Unknown) {
                Truth::Unknown
            } else {
                Truth::True
            };
            compound(trace, path, expr, t)
        }
        Expr::Any(items) => {
            let results: Vec<Truth> = items
                .iter()
                .enumerate()
                .map(|(i, item)| eval(item, facts, &format!("{path}.{i}"), trace))
                .collect();
            let t = if results.contains(&Truth::True) {
                Truth::True
            } else if results.contains(&Truth::Unknown) {
                Truth::Unknown
            } else {
                Truth::False
            };
            compound(trace, path, expr, t)
        }
        Expr::Cmp { op, left, right } => push(trace, path, expr, eval_cmp(*op, left, right, facts)),
        Expr::In { left, values } => push(trace, path, expr, eval_in(left, values, facts)),
        Expr::Exists(id) => push(trace, path, expr, eval_exists(id, facts)),
        Expr::Date { op, fact, date } => {
            push(trace, path, expr, eval_date(*op, fact, *date, facts))
        }
    }
}
