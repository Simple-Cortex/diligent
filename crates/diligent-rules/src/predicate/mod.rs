//! Safe predicate language (audit specification 17.4): a declarative expression tree
//! with three-valued evaluation. No executable source is ever interpreted.
//!
//! * [`parse_str`] parses the string form (`a && (b || c)`).
//! * [`parse_structured`] parses the JSON object form (`{"all": [...]}`).
//! * [`evaluate`] returns a [`Truth`] and a flat, deterministic trace.

mod ast;
mod date;
mod eval;
mod parse;

pub use ast::{CmpOp, DateOp, Expr, Literal, Operand};
pub use date::{is_rfc3339, rfc3339_now, Date};
pub use eval::{evaluate, EvalOutcome, FactEntry, FactSet, FactState, TraceEntry, Truth, Val};
pub use parse::{parse_str, parse_structured, parse_value};

use crate::model::Predicate;

/// Why a predicate could not be parsed or accepted.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{message}")]
pub struct PredicateError {
    pub message: String,
}

impl PredicateError {
    pub fn new(message: impl Into<String>) -> Self {
        PredicateError {
            message: message.into(),
        }
    }
}

/// Parse a rule's predicate in either form.
pub fn parse_predicate(predicate: &Predicate) -> Result<Expr, PredicateError> {
    match predicate {
        Predicate::Named(text) => parse_str(text),
        Predicate::Structured(obj) => parse_structured(obj),
    }
}
