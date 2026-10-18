//! Parsers for the string form and the structured JSON form of a predicate.
//! Neither parser panics on any input; every failure is a [`PredicateError`].

use serde_json::{Map, Value};

use super::ast::{CmpOp, DateOp, Expr, Literal, Operand};
use super::date::Date;
use super::PredicateError;

/// Maximum nesting depth accepted from either form.
const MAX_DEPTH: usize = 64;

fn err<T>(message: impl Into<String>) -> Result<T, PredicateError> {
    Err(PredicateError::new(message))
}

// ---------------------------------------------------------------------------
// String form
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq)]
enum Tok {
    Ident(String),
    Str(String),
    Num(f64),
    True,
    False,
    Null,
    In,
    AndAnd,
    OrOr,
    Bang,
    EqEq,
    NotEq,
    Gte,
    Lte,
    Gt,
    Lt,
    LParen,
    RParen,
    LBracket,
    RBracket,
    Comma,
}

fn lex(text: &str) -> Result<Vec<(Tok, usize)>, PredicateError> {
    let chars: Vec<char> = text.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        let start = i;
        if c.is_whitespace() {
            i += 1;
            continue;
        }
        let next = chars.get(i + 1).copied();
        let tok = match c {
            '(' => Tok::LParen,
            ')' => Tok::RParen,
            '[' => Tok::LBracket,
            ']' => Tok::RBracket,
            ',' => Tok::Comma,
            '&' if next == Some('&') => {
                i += 1;
                Tok::AndAnd
            }
            '|' if next == Some('|') => {
                i += 1;
                Tok::OrOr
            }
            '=' if next == Some('=') => {
                i += 1;
                Tok::EqEq
            }
            '!' if next == Some('=') => {
                i += 1;
                Tok::NotEq
            }
            '!' => Tok::Bang,
            '>' if next == Some('=') => {
                i += 1;
                Tok::Gte
            }
            '>' => Tok::Gt,
            '<' if next == Some('=') => {
                i += 1;
                Tok::Lte
            }
            '<' => Tok::Lt,
            '\'' | '"' => {
                let quote = c;
                let mut s = String::new();
                i += 1;
                loop {
                    match chars.get(i) {
                        None => return err(format!("unterminated string starting at {start}")),
                        Some('\\') => match chars.get(i + 1) {
                            Some(&e @ ('\\' | '\'' | '"')) => {
                                s.push(e);
                                i += 2;
                            }
                            _ => return err(format!("invalid escape in string at {i}")),
                        },
                        Some(&q) if q == quote => break,
                        Some(&other) => {
                            s.push(other);
                            i += 1;
                        }
                    }
                }
                Tok::Str(s)
            }
            '-' | '0'..='9' => {
                let mut j = i;
                if chars[j] == '-' {
                    j += 1;
                }
                let digits_start = j;
                while j < chars.len() && chars[j].is_ascii_digit() {
                    j += 1;
                }
                if j == digits_start {
                    return err(format!("unexpected character '-' at {start}"));
                }
                if chars.get(j) == Some(&'.') {
                    let frac = j + 1;
                    j = frac;
                    while j < chars.len() && chars[j].is_ascii_digit() {
                        j += 1;
                    }
                    if j == frac {
                        return err(format!("malformed number at {start}"));
                    }
                }
                let literal: String = chars[i..j].iter().collect();
                i = j - 1;
                match literal.parse::<f64>() {
                    Ok(n) if n.is_finite() => Tok::Num(n),
                    _ => return err(format!("malformed number '{literal}' at {start}")),
                }
            }
            'a'..='z' | '_' => {
                let mut j = i;
                while j < chars.len()
                    && (chars[j].is_ascii_lowercase()
                        || chars[j].is_ascii_digit()
                        || chars[j] == '_')
                {
                    j += 1;
                }
                let word: String = chars[i..j].iter().collect();
                i = j - 1;
                match word.as_str() {
                    "true" => Tok::True,
                    "false" => Tok::False,
                    "null" => Tok::Null,
                    "in" => Tok::In,
                    _ => Tok::Ident(word),
                }
            }
            '&' | '|' | '=' => return err(format!("unknown operator '{c}' at {start}")),
            other => return err(format!("unexpected character '{other}' at {start}")),
        };
        out.push((tok, start));
        i += 1;
    }
    Ok(out)
}

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
    depth: usize,
}

impl Parser {
    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|(t, _)| t)
    }

    fn here(&self) -> usize {
        self.toks.get(self.pos).map_or(usize::MAX, |(_, p)| *p)
    }

    fn describe_here(&self) -> String {
        match self.toks.get(self.pos) {
            Some((t, p)) => format!("unexpected token {t:?} at {p}"),
            None => "unexpected end of predicate".to_string(),
        }
    }

    fn eat(&mut self, tok: &Tok) -> bool {
        if self.peek() == Some(tok) {
            self.pos += 1;
            true
        } else {
            false
        }
    }

    fn enter(&mut self) -> Result<(), PredicateError> {
        self.depth += 1;
        if self.depth > MAX_DEPTH {
            return err("predicate is nested too deeply");
        }
        Ok(())
    }

    fn parse_or(&mut self) -> Result<Expr, PredicateError> {
        let mut items = vec![self.parse_and()?];
        while self.eat(&Tok::OrOr) {
            items.push(self.parse_and()?);
        }
        Ok(if items.len() == 1 {
            items.remove(0)
        } else {
            Expr::Any(items)
        })
    }

    fn parse_and(&mut self) -> Result<Expr, PredicateError> {
        let mut items = vec![self.parse_unary()?];
        while self.eat(&Tok::AndAnd) {
            items.push(self.parse_unary()?);
        }
        Ok(if items.len() == 1 {
            items.remove(0)
        } else {
            Expr::All(items)
        })
    }

    fn parse_unary(&mut self) -> Result<Expr, PredicateError> {
        self.enter()?;
        let result = if self.eat(&Tok::Bang) {
            self.parse_unary().map(|e| Expr::Not(Box::new(e)))
        } else {
            self.parse_cmp()
        };
        self.depth -= 1;
        result
    }

    fn parse_cmp(&mut self) -> Result<Expr, PredicateError> {
        let left = self.parse_primary()?;
        let op = match self.peek() {
            Some(Tok::EqEq) => Some(CmpOp::Eq),
            Some(Tok::NotEq) => Some(CmpOp::Ne),
            Some(Tok::Gte) => Some(CmpOp::Gte),
            Some(Tok::Lte) => Some(CmpOp::Lte),
            Some(Tok::Gt) => Some(CmpOp::Gt),
            Some(Tok::Lt) => Some(CmpOp::Lt),
            _ => None,
        };
        if let Some(op) = op {
            self.pos += 1;
            let left = to_operand(left)?;
            let right = to_operand(self.parse_primary()?)?;
            check_ordering_operand(op, &left)?;
            check_ordering_operand(op, &right)?;
            return Ok(Expr::Cmp { op, left, right });
        }
        if self.eat(&Tok::In) {
            let left = to_operand(left)?;
            let values = self.parse_list()?;
            return Ok(Expr::In { left, values });
        }
        Ok(left)
    }

    fn parse_list(&mut self) -> Result<Vec<Literal>, PredicateError> {
        if !self.eat(&Tok::LBracket) {
            return err(format!("`in` needs a [list]; {}", self.describe_here()));
        }
        let mut values = Vec::new();
        if self.eat(&Tok::RBracket) {
            return Ok(values);
        }
        loop {
            values.push(self.parse_literal()?);
            if self.eat(&Tok::Comma) {
                continue;
            }
            if self.eat(&Tok::RBracket) {
                return Ok(values);
            }
            return err(format!("unterminated list; {}", self.describe_here()));
        }
    }

    fn parse_literal(&mut self) -> Result<Literal, PredicateError> {
        let lit = match self.peek() {
            Some(Tok::Str(s)) => Literal::Str(s.clone()),
            Some(Tok::Num(n)) => Literal::Num(*n),
            Some(Tok::True) => Literal::Bool(true),
            Some(Tok::False) => Literal::Bool(false),
            Some(Tok::Null) => Literal::Null,
            _ => return err(format!("expected a literal; {}", self.describe_here())),
        };
        self.pos += 1;
        Ok(lit)
    }

    fn parse_primary(&mut self) -> Result<Expr, PredicateError> {
        match self.peek().cloned() {
            Some(Tok::LParen) => {
                self.pos += 1;
                self.enter()?;
                let inner = self.parse_or()?;
                self.depth -= 1;
                if !self.eat(&Tok::RParen) {
                    return err(format!("unbalanced parentheses; {}", self.describe_here()));
                }
                Ok(inner)
            }
            Some(Tok::Ident(name)) => {
                self.pos += 1;
                if self.peek() == Some(&Tok::LParen)
                    && matches!(name.as_str(), "exists" | "date_before" | "date_on_or_after")
                {
                    return self.parse_call(&name);
                }
                Ok(Expr::Fact(name))
            }
            Some(Tok::Str(_) | Tok::Num(_) | Tok::True | Tok::False | Tok::Null) => {
                self.parse_literal().map(Expr::Lit)
            }
            _ => err(self.describe_here()),
        }
    }

    /// `exists(fact)`, `date_before(fact, 'YYYY-MM-DD')`, `date_on_or_after(...)`.
    fn parse_call(&mut self, name: &str) -> Result<Expr, PredicateError> {
        self.pos += 1; // '('
        let fact = match self.peek().cloned() {
            Some(Tok::Ident(f)) => {
                self.pos += 1;
                f
            }
            _ => return err(format!("{name} needs a fact id; {}", self.describe_here())),
        };
        let expr = if name == "exists" {
            Expr::Exists(fact)
        } else {
            if !self.eat(&Tok::Comma) {
                return err(format!("{name} needs a date; {}", self.describe_here()));
            }
            let date = match self.parse_literal()? {
                Literal::Str(s) => parse_date_const(&s)?,
                _ => return err(format!("{name} date must be a 'YYYY-MM-DD' string")),
            };
            let op = if name == "date_before" {
                DateOp::Before
            } else {
                DateOp::OnOrAfter
            };
            Expr::Date { op, fact, date }
        };
        if !self.eat(&Tok::RParen) {
            return err(format!("unbalanced parentheses; {}", self.describe_here()));
        }
        Ok(expr)
    }
}

fn to_operand(expr: Expr) -> Result<Operand, PredicateError> {
    match expr {
        Expr::Fact(name) => Ok(Operand::Fact(name)),
        Expr::Lit(lit) => Ok(Operand::Lit(lit)),
        _ => err("comparison operands must be facts or literals"),
    }
}

fn check_ordering_operand(op: CmpOp, operand: &Operand) -> Result<(), PredicateError> {
    if matches!(op, CmpOp::Eq | CmpOp::Ne) {
        return Ok(());
    }
    match operand {
        Operand::Lit(Literal::Bool(_) | Literal::Null) => {
            err(format!("`{}` needs number or string operands", op.symbol()))
        }
        _ => Ok(()),
    }
}

fn parse_date_const(text: &str) -> Result<Date, PredicateError> {
    match Date::parse(text) {
        Some(d) => Ok(d),
        None => err(format!("'{text}' is not a valid YYYY-MM-DD date")),
    }
}

/// Parse the string form.
pub fn parse_str(text: &str) -> Result<Expr, PredicateError> {
    let toks = lex(text)?;
    if toks.is_empty() {
        return err("empty predicate");
    }
    let mut parser = Parser {
        toks,
        pos: 0,
        depth: 0,
    };
    let expr = parser.parse_or()?;
    if parser.pos < parser.toks.len() {
        return err(format!("{} (at {})", parser.describe_here(), parser.here()));
    }
    check_boolean_positions(&expr)?;
    Ok(expr)
}

/// Literals other than `true` and `false` cannot stand alone as booleans.
fn check_boolean_positions(expr: &Expr) -> Result<(), PredicateError> {
    match expr {
        Expr::Lit(Literal::Bool(_)) => Ok(()),
        Expr::Lit(_) => err("only true and false may stand alone as a boolean"),
        Expr::Not(inner) => check_boolean_positions(inner),
        Expr::All(items) | Expr::Any(items) => items.iter().try_for_each(check_boolean_positions),
        _ => Ok(()),
    }
}

// ---------------------------------------------------------------------------
// Structured form
// ---------------------------------------------------------------------------

fn json_literal(value: &Value) -> Result<Literal, PredicateError> {
    match value {
        Value::String(s) => Ok(Literal::Str(s.clone())),
        Value::Number(n) => match n.as_f64() {
            Some(f) if f.is_finite() => Ok(Literal::Num(f)),
            _ => err("number out of range"),
        },
        Value::Bool(b) => Ok(Literal::Bool(*b)),
        Value::Null => Ok(Literal::Null),
        _ => err("literal must be a string, number, boolean or null"),
    }
}

fn only_keys(obj: &Map<String, Value>, op: &str, keys: &[&str]) -> Result<(), PredicateError> {
    for key in obj.keys() {
        if !keys.contains(&key.as_str()) {
            return err(format!("`{op}` does not accept key `{key}`"));
        }
    }
    for key in keys {
        if !obj.contains_key(*key) {
            return err(format!("`{op}` needs key `{key}`"));
        }
    }
    Ok(())
}

fn fact_name(obj: &Map<String, Value>, op: &str) -> Result<String, PredicateError> {
    match obj.get("fact") {
        Some(Value::String(s)) if is_ident(s) => Ok(s.clone()),
        _ => err(format!(
            "`{op}` needs a fact id string matching [a-z_][a-z0-9_]*"
        )),
    }
}

fn is_ident(s: &str) -> bool {
    let mut chars = s.chars();
    matches!(chars.next(), Some('a'..='z' | '_'))
        && chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn from_value(value: &Value, depth: usize) -> Result<Expr, PredicateError> {
    if depth > MAX_DEPTH {
        return err("predicate is nested too deeply");
    }
    let Value::Object(obj) = value else {
        return err("a structured predicate node must be an object");
    };
    from_object(obj, depth)
}

fn from_object(obj: &Map<String, Value>, depth: usize) -> Result<Expr, PredicateError> {
    if depth > MAX_DEPTH {
        return err("predicate is nested too deeply");
    }
    if obj.len() != 1 {
        return err("a structured predicate node must have exactly one operator key");
    }
    let (op, arg) = obj.iter().next().expect("length checked above");
    match op.as_str() {
        "all" | "any" => {
            let Value::Array(items) = arg else {
                return err(format!("`{op}` needs an array"));
            };
            if items.is_empty() {
                return err(format!("`{op}` needs at least one operand"));
            }
            let parsed = items
                .iter()
                .map(|v| from_value(v, depth + 1))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(if op == "all" {
                Expr::All(parsed)
            } else {
                Expr::Any(parsed)
            })
        }
        "not" => Ok(Expr::Not(Box::new(from_value(arg, depth + 1)?))),
        "fact" => match arg {
            Value::String(s) if is_ident(s) => Ok(Expr::Fact(s.clone())),
            _ => err("`fact` needs a fact id string matching [a-z_][a-z0-9_]*"),
        },
        "exists" => match arg {
            Value::String(s) if is_ident(s) => Ok(Expr::Exists(s.clone())),
            _ => err("`exists` needs a fact id string matching [a-z_][a-z0-9_]*"),
        },
        "eq" | "ne" | "gte" | "lte" | "gt" | "lt" => {
            let Value::Object(body) = arg else {
                return err(format!("`{op}` needs an object with `fact` and `value`"));
            };
            only_keys(body, op, &["fact", "value"])?;
            let cmp = match op.as_str() {
                "eq" => CmpOp::Eq,
                "ne" => CmpOp::Ne,
                "gte" => CmpOp::Gte,
                "lte" => CmpOp::Lte,
                "gt" => CmpOp::Gt,
                _ => CmpOp::Lt,
            };
            let right = Operand::Lit(json_literal(&body["value"])?);
            check_ordering_operand(cmp, &right)?;
            Ok(Expr::Cmp {
                op: cmp,
                left: Operand::Fact(fact_name(body, op)?),
                right,
            })
        }
        "in" => {
            let Value::Object(body) = arg else {
                return err("`in` needs an object with `fact` and `values`");
            };
            only_keys(body, op, &["fact", "values"])?;
            let Value::Array(values) = &body["values"] else {
                return err("`in` needs `values` as an array");
            };
            Ok(Expr::In {
                left: Operand::Fact(fact_name(body, op)?),
                values: values
                    .iter()
                    .map(json_literal)
                    .collect::<Result<Vec<_>, _>>()?,
            })
        }
        "date_before" | "date_on_or_after" => {
            let Value::Object(body) = arg else {
                return err(format!("`{op}` needs an object with `fact` and `date`"));
            };
            only_keys(body, op, &["fact", "date"])?;
            let Value::String(date) = &body["date"] else {
                return err(format!("`{op}` needs `date` as a 'YYYY-MM-DD' string"));
            };
            Ok(Expr::Date {
                op: if op == "date_before" {
                    DateOp::Before
                } else {
                    DateOp::OnOrAfter
                },
                fact: fact_name(body, op)?,
                date: parse_date_const(date)?,
            })
        }
        other => err(format!("unknown operator `{other}`")),
    }
}

/// Parse the structured form from a JSON object.
pub fn parse_structured(obj: &Map<String, Value>) -> Result<Expr, PredicateError> {
    from_object(obj, 0)
}

/// Parse the structured form from any JSON value.
pub fn parse_value(value: &Value) -> Result<Expr, PredicateError> {
    from_value(value, 0)
}
