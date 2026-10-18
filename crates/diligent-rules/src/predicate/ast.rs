//! The predicate expression tree. Both the string form and the structured
//! JSON form parse into this one type.

use std::collections::BTreeSet;
use std::fmt;

use super::date::Date;

/// A literal value inside a predicate.
#[derive(Debug, Clone, PartialEq)]
pub enum Literal {
    Str(String),
    Num(f64),
    Bool(bool),
    Null,
}

/// One side of a comparison: a fact reference or a literal.
#[derive(Debug, Clone, PartialEq)]
pub enum Operand {
    Fact(String),
    Lit(Literal),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    Ne,
    Gte,
    Lte,
    Gt,
    Lt,
}

impl CmpOp {
    pub fn name(self) -> &'static str {
        match self {
            CmpOp::Eq => "eq",
            CmpOp::Ne => "ne",
            CmpOp::Gte => "gte",
            CmpOp::Lte => "lte",
            CmpOp::Gt => "gt",
            CmpOp::Lt => "lt",
        }
    }

    pub fn symbol(self) -> &'static str {
        match self {
            CmpOp::Eq => "==",
            CmpOp::Ne => "!=",
            CmpOp::Gte => ">=",
            CmpOp::Lte => "<=",
            CmpOp::Gt => ">",
            CmpOp::Lt => "<",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DateOp {
    Before,
    OnOrAfter,
}

impl DateOp {
    pub fn name(self) -> &'static str {
        match self {
            DateOp::Before => "date_before",
            DateOp::OnOrAfter => "date_on_or_after",
        }
    }
}

/// A predicate expression.
#[derive(Debug, Clone, PartialEq)]
pub enum Expr {
    /// A bare fact in boolean position. Must resolve to a bool fact.
    Fact(String),
    /// A bare literal in boolean position. Only `true` and `false` are accepted.
    Lit(Literal),
    Not(Box<Expr>),
    All(Vec<Expr>),
    Any(Vec<Expr>),
    Cmp {
        op: CmpOp,
        left: Operand,
        right: Operand,
    },
    In {
        left: Operand,
        values: Vec<Literal>,
    },
    Exists(String),
    Date {
        op: DateOp,
        fact: String,
        date: Date,
    },
}

impl Expr {
    /// Every fact id the expression references.
    pub fn facts(&self) -> BTreeSet<String> {
        let mut out = BTreeSet::new();
        self.collect_facts(&mut out);
        out
    }

    fn collect_facts(&self, out: &mut BTreeSet<String>) {
        let operand = |o: &Operand, out: &mut BTreeSet<String>| {
            if let Operand::Fact(name) = o {
                out.insert(name.clone());
            }
        };
        match self {
            Expr::Fact(name) | Expr::Exists(name) => {
                out.insert(name.clone());
            }
            Expr::Lit(_) => {}
            Expr::Not(inner) => inner.collect_facts(out),
            Expr::All(items) | Expr::Any(items) => {
                for item in items {
                    item.collect_facts(out);
                }
            }
            Expr::Cmp { left, right, .. } => {
                operand(left, out);
                operand(right, out);
            }
            Expr::In { left, .. } => operand(left, out),
            Expr::Date { fact, .. } => {
                out.insert(fact.clone());
            }
        }
    }

    /// The operator name used in trace entries.
    pub fn op_name(&self) -> &'static str {
        match self {
            Expr::Fact(_) | Expr::Lit(_) => "fact",
            Expr::Not(_) => "not",
            Expr::All(_) => "all",
            Expr::Any(_) => "any",
            Expr::Cmp { op, .. } => op.name(),
            Expr::In { .. } => "in",
            Expr::Exists(_) => "exists",
            Expr::Date { op, .. } => op.name(),
        }
    }

    /// Binding strength used to decide where canonical parentheses go.
    fn precedence(&self) -> u8 {
        match self {
            Expr::Any(items) if items.len() > 1 => 1,
            Expr::All(items) if items.len() > 1 => 2,
            Expr::Not(_) => 3,
            Expr::Any(items) | Expr::All(items) => {
                // A single-element group renders as its only child.
                items.first().map_or(5, Expr::precedence)
            }
            _ => 5,
        }
    }
}

fn write_literal(f: &mut fmt::Formatter<'_>, lit: &Literal) -> fmt::Result {
    match lit {
        Literal::Str(s) => {
            f.write_str("'")?;
            for c in s.chars() {
                match c {
                    '\\' => f.write_str("\\\\")?,
                    '\'' => f.write_str("\\'")?,
                    other => write!(f, "{other}")?,
                }
            }
            f.write_str("'")
        }
        Literal::Num(n) => {
            if n.fract() == 0.0 && n.abs() < 1e15 {
                write!(f, "{}", *n as i64)
            } else {
                write!(f, "{n}")
            }
        }
        Literal::Bool(b) => write!(f, "{b}"),
        Literal::Null => f.write_str("null"),
    }
}

fn write_operand(f: &mut fmt::Formatter<'_>, operand: &Operand) -> fmt::Result {
    match operand {
        Operand::Fact(name) => f.write_str(name),
        Operand::Lit(lit) => write_literal(f, lit),
    }
}

fn write_child(f: &mut fmt::Formatter<'_>, child: &Expr, min_precedence: u8) -> fmt::Result {
    if child.precedence() < min_precedence {
        write!(f, "({child})")
    } else {
        write!(f, "{child}")
    }
}

/// Canonical string rendering: lowercase ops, single-quoted strings, and
/// parentheses only where precedence requires them.
impl fmt::Display for Expr {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Expr::Fact(name) => f.write_str(name),
            Expr::Exists(name) => write!(f, "exists({name})"),
            Expr::Lit(lit) => write_literal(f, lit),
            Expr::Not(inner) => {
                f.write_str("!")?;
                write_child(f, inner, 3)
            }
            Expr::All(items) => {
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" && ")?;
                    }
                    write_child(f, item, if items.len() > 1 { 3 } else { 0 })?;
                }
                Ok(())
            }
            Expr::Any(items) => {
                for (i, item) in items.iter().enumerate() {
                    if i > 0 {
                        f.write_str(" || ")?;
                    }
                    write_child(f, item, if items.len() > 1 { 2 } else { 0 })?;
                }
                Ok(())
            }
            Expr::Cmp { op, left, right } => {
                write_operand(f, left)?;
                write!(f, " {} ", op.symbol())?;
                write_operand(f, right)
            }
            Expr::In { left, values } => {
                write_operand(f, left)?;
                f.write_str(" in [")?;
                for (i, v) in values.iter().enumerate() {
                    if i > 0 {
                        f.write_str(", ")?;
                    }
                    write_literal(f, v)?;
                }
                f.write_str("]")
            }
            Expr::Date { op, fact, date } => {
                write!(f, "{}({fact}, '{date}')", op.name())
            }
        }
    }
}
