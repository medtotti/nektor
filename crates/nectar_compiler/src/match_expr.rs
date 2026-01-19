//! Match expression parser for Nectar policies.
//!
//! Parses match expressions like:
//! - `http.status >= 500`
//! - `duration > 5s`
//! - `service.name == "checkout"`
//! - `error == true`
//! - `true` (match all)
//! - `http.status >= 500 || error == true` (compound)

use crate::error::{Error, Result};
use crate::refinery::{ConditionValue, RefineryCondition};

/// A parsed match expression.
#[derive(Debug, Clone, PartialEq)]
pub enum MatchExpr {
    /// Always matches (fallback rule).
    True,
    /// A simple comparison condition.
    Condition(Condition),
    /// Logical AND of multiple expressions.
    And(Vec<Self>),
    /// Logical OR of multiple expressions.
    Or(Vec<Self>),
}

/// A simple comparison condition.
#[derive(Debug, Clone, PartialEq)]
pub struct Condition {
    /// Field name (e.g., "http.status", "duration", "service.name").
    pub field: String,
    /// Comparison operator.
    pub operator: Operator,
    /// Value to compare against.
    pub value: Value,
}

/// Comparison operators.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operator {
    /// Equals (==).
    Eq,
    /// Not equals (!=).
    Ne,
    /// Greater than (>).
    Gt,
    /// Greater than or equal (>=).
    Ge,
    /// Less than (<).
    Lt,
    /// Less than or equal (<=).
    Le,
    /// Contains substring.
    Contains,
    /// Starts with prefix.
    StartsWith,
    /// Exists (has value).
    Exists,
}

/// A value in a condition.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    /// String value.
    String(String),
    /// Integer value.
    Int(i64),
    /// Float value.
    Float(f64),
    /// Boolean value.
    Bool(bool),
    /// Duration value in milliseconds.
    Duration(u64),
}

impl MatchExpr {
    /// Parses a match expression string.
    ///
    /// # Errors
    ///
    /// Returns an error if the expression is invalid.
    pub fn parse(input: &str) -> Result<Self> {
        let input = input.trim();

        // Handle "true" literal
        if input.eq_ignore_ascii_case("true") {
            return Ok(Self::True);
        }

        // Handle OR expressions
        if let Some(parts) = split_logical(input, "||") {
            let exprs: Result<Vec<_>> = parts.iter().map(|p| Self::parse(p)).collect();
            return Ok(Self::Or(exprs?));
        }

        // Handle AND expressions
        if let Some(parts) = split_logical(input, "&&") {
            let exprs: Result<Vec<_>> = parts.iter().map(|p| Self::parse(p)).collect();
            return Ok(Self::And(exprs?));
        }

        // Parse as simple condition
        let condition = parse_condition(input)?;
        Ok(Self::Condition(condition))
    }

    /// Converts this expression to Refinery conditions.
    ///
    /// Returns None if the expression is `true` (matches everything).
    ///
    /// # Errors
    ///
    /// Returns an error if the expression can't be represented in Refinery format.
    pub fn to_refinery_conditions(&self) -> Result<Vec<RefineryCondition>> {
        match self {
            Self::True => Ok(Vec::new()),
            Self::Condition(cond) => Ok(vec![cond.to_refinery()]),
            Self::And(exprs) => {
                let mut conditions = Vec::new();
                for expr in exprs {
                    conditions.extend(expr.to_refinery_conditions()?);
                }
                Ok(conditions)
            }
            Self::Or(_) => {
                // Refinery doesn't support OR within a single rule
                // We'd need to split into multiple rules
                Err(Error::Unsupported(
                    "OR expressions not supported in single rule, split into multiple rules"
                        .to_string(),
                ))
            }
        }
    }
}

impl Condition {
    fn to_refinery(&self) -> RefineryCondition {
        let operator = match self.operator {
            Operator::Eq => "=",
            Operator::Ne => "!=",
            Operator::Gt => ">",
            Operator::Ge => ">=",
            Operator::Lt => "<",
            Operator::Le => "<=",
            Operator::Contains => "contains",
            Operator::StartsWith => "starts-with",
            Operator::Exists => "exists",
        };

        let value = match &self.value {
            Value::String(s) => ConditionValue::String(s.clone()),
            Value::Int(n) => ConditionValue::Number(*n),
            Value::Float(f) => {
                // Refinery uses integers, so round
                #[allow(clippy::cast_possible_truncation)]
                ConditionValue::Number(f.round() as i64)
            }
            Value::Bool(b) => ConditionValue::Bool(*b),
            Value::Duration(ms) => {
                // Convert to milliseconds as integer
                #[allow(clippy::cast_possible_wrap)]
                ConditionValue::Number(*ms as i64)
            }
        };

        // Handle duration field name mapping
        let field = if self.field == "duration" {
            "duration_ms".to_string()
        } else {
            self.field.clone()
        };

        RefineryCondition {
            field,
            operator: operator.to_string(),
            value,
        }
    }
}

impl Operator {
    fn from_str(s: &str) -> Option<Self> {
        match s {
            "==" | "=" => Some(Self::Eq),
            "!=" | "<>" => Some(Self::Ne),
            ">" => Some(Self::Gt),
            ">=" => Some(Self::Ge),
            "<" => Some(Self::Lt),
            "<=" => Some(Self::Le),
            "contains" => Some(Self::Contains),
            "starts-with" | "startsWith" => Some(Self::StartsWith),
            "exists" => Some(Self::Exists),
            _ => None,
        }
    }
}

/// Splits an input by a logical operator (|| or &&), respecting parentheses.
fn split_logical(input: &str, op: &str) -> Option<Vec<String>> {
    let mut parts = Vec::new();
    let mut current = String::new();
    let mut paren_depth = 0;
    let mut chars = input.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '(' => {
                paren_depth += 1;
                current.push(c);
            }
            ')' => {
                paren_depth -= 1;
                current.push(c);
            }
            '|' if op == "||" && paren_depth == 0 => {
                if chars.peek() == Some(&'|') {
                    chars.next();
                    parts.push(current.trim().to_string());
                    current = String::new();
                } else {
                    current.push(c);
                }
            }
            '&' if op == "&&" && paren_depth == 0 => {
                if chars.peek() == Some(&'&') {
                    chars.next();
                    parts.push(current.trim().to_string());
                    current = String::new();
                } else {
                    current.push(c);
                }
            }
            _ => current.push(c),
        }
    }

    if !current.is_empty() {
        parts.push(current.trim().to_string());
    }

    if parts.len() > 1 {
        Some(parts)
    } else {
        None
    }
}

/// Parses a simple condition like "http.status >= 500".
fn parse_condition(input: &str) -> Result<Condition> {
    let input = input.trim();

    // Try each operator in order of length (longest first to avoid prefix conflicts)
    let operators = [">=", "<=", "!=", "==", ">", "<", "=", "contains", "starts-with", "exists"];

    for op_str in operators {
        if let Some(idx) = input.find(op_str) {
            let field = input[..idx].trim().to_string();
            let value_str = input[idx + op_str.len()..].trim();

            let operator =
                Operator::from_str(op_str).ok_or_else(|| Error::InvalidMatch {
                    expr: input.to_string(),
                    reason: format!("unknown operator: {op_str}"),
                })?;

            // Handle exists operator (no value needed)
            if operator == Operator::Exists {
                return Ok(Condition {
                    field,
                    operator,
                    value: Value::Bool(true),
                });
            }

            let value = parse_value(value_str);

            return Ok(Condition {
                field,
                operator,
                value,
            });
        }
    }

    Err(Error::InvalidMatch {
        expr: input.to_string(),
        reason: "no valid operator found".to_string(),
    })
}

/// Parses a value string.
fn parse_value(s: &str) -> Value {
    let s = s.trim();

    // Boolean
    if s.eq_ignore_ascii_case("true") {
        return Value::Bool(true);
    }
    if s.eq_ignore_ascii_case("false") {
        return Value::Bool(false);
    }

    // Duration (e.g., "5s", "100ms")
    if let Some(dur) = parse_duration_value(s) {
        return Value::Duration(dur);
    }

    // Quoted string
    if (s.starts_with('"') && s.ends_with('"')) || (s.starts_with('\'') && s.ends_with('\'')) {
        let inner = &s[1..s.len() - 1];
        return Value::String(inner.to_string());
    }

    // Integer
    if let Ok(n) = s.parse::<i64>() {
        return Value::Int(n);
    }

    // Float
    if let Ok(f) = s.parse::<f64>() {
        return Value::Float(f);
    }

    // Unquoted string
    Value::String(s.to_string())
}

/// Parses a duration string like "5s", "100ms" to milliseconds.
fn parse_duration_value(s: &str) -> Option<u64> {
    if let Some(ms_str) = s.strip_suffix("ms") {
        return ms_str.trim().parse().ok();
    }

    if let Some(s_str) = s.strip_suffix('s') {
        let secs: f64 = s_str.trim().parse().ok()?;
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        return Some((secs * 1000.0) as u64);
    }

    if let Some(m_str) = s.strip_suffix('m') {
        let mins: f64 = m_str.trim().parse().ok()?;
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        return Some((mins * 60.0 * 1000.0) as u64);
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_true() {
        assert_eq!(MatchExpr::parse("true").unwrap(), MatchExpr::True);
        assert_eq!(MatchExpr::parse("TRUE").unwrap(), MatchExpr::True);
    }

    #[test]
    fn parse_simple_comparison() {
        let expr = MatchExpr::parse("http.status >= 500").unwrap();
        if let MatchExpr::Condition(cond) = expr {
            assert_eq!(cond.field, "http.status");
            assert_eq!(cond.operator, Operator::Ge);
            assert_eq!(cond.value, Value::Int(500));
        } else {
            panic!("expected Condition");
        }
    }

    #[test]
    fn parse_string_comparison() {
        let expr = MatchExpr::parse("service.name == \"checkout\"").unwrap();
        if let MatchExpr::Condition(cond) = expr {
            assert_eq!(cond.field, "service.name");
            assert_eq!(cond.operator, Operator::Eq);
            assert_eq!(cond.value, Value::String("checkout".to_string()));
        } else {
            panic!("expected Condition");
        }
    }

    #[test]
    fn parse_boolean_comparison() {
        let expr = MatchExpr::parse("error == true").unwrap();
        if let MatchExpr::Condition(cond) = expr {
            assert_eq!(cond.field, "error");
            assert_eq!(cond.value, Value::Bool(true));
        } else {
            panic!("expected Condition");
        }
    }

    #[test]
    fn parse_duration_comparison() {
        let expr = MatchExpr::parse("duration > 5s").unwrap();
        if let MatchExpr::Condition(cond) = expr {
            assert_eq!(cond.field, "duration");
            assert_eq!(cond.operator, Operator::Gt);
            assert_eq!(cond.value, Value::Duration(5000));
        } else {
            panic!("expected Condition");
        }
    }

    #[test]
    fn parse_or_expression() {
        let expr = MatchExpr::parse("http.status >= 500 || error == true").unwrap();
        if let MatchExpr::Or(parts) = expr {
            assert_eq!(parts.len(), 2);
        } else {
            panic!("expected Or");
        }
    }

    #[test]
    fn parse_and_expression() {
        let expr = MatchExpr::parse("status >= 500 && service == \"api\"").unwrap();
        if let MatchExpr::And(parts) = expr {
            assert_eq!(parts.len(), 2);
        } else {
            panic!("expected And");
        }
    }

    #[test]
    fn to_refinery_conditions() {
        let expr = MatchExpr::parse("http.status >= 500").unwrap();
        let conditions = expr.to_refinery_conditions().unwrap();
        assert_eq!(conditions.len(), 1);
        assert_eq!(conditions[0].field, "http.status");
        assert_eq!(conditions[0].operator, ">=");
    }

    #[test]
    fn to_refinery_true_has_no_conditions() {
        let expr = MatchExpr::True;
        let conditions = expr.to_refinery_conditions().unwrap();
        assert!(conditions.is_empty());
    }

    #[test]
    fn and_conditions_flatten() {
        let expr = MatchExpr::parse("status >= 500 && error == true").unwrap();
        let conditions = expr.to_refinery_conditions().unwrap();
        assert_eq!(conditions.len(), 2);
    }

    #[test]
    fn duration_field_renamed() {
        let expr = MatchExpr::parse("duration > 1000ms").unwrap();
        let conditions = expr.to_refinery_conditions().unwrap();
        assert_eq!(conditions[0].field, "duration_ms");
    }
}
