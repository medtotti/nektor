//! TOON format parser.
//!
//! Parses TOON-formatted policy documents into typed [`Policy`] structures.
//!
//! # TOON Format
//!
//! ```toon
//! nectar_policy{version,name,budget_per_second,rules}:
//!   1
//!   my-policy
//!   10000
//!   rules[2]{name,description,match,action,priority}:
//!     keep-errors,Retain all errors,http.status >= 500,keep,100
//!     sample-rest,Sample remaining,true,sample(0.01),0
//! ```

use crate::error::{Error, Result};
use crate::model::{Action, Policy, Rule};
use std::fmt::Write;

/// Parses a TOON-formatted string into a [`Policy`].
///
/// # Errors
///
/// Returns an error if:
/// - The TOON syntax is invalid
/// - Required fields are missing
/// - Array counts don't match (strict mode)
///
/// # Example
///
/// ```rust
/// use toon_policy::parse;
///
/// let input = r#"
/// nectar_policy{version,name,budget_per_second,rules}:
///   1
///   my-policy
///   1000
///   rules[1]{name,description,match,action,priority}:
///     keep-errors,Keep all errors,status >= 500,keep,100
/// "#;
///
/// let policy = parse(input).unwrap();
/// assert_eq!(policy.name, "my-policy");
/// assert_eq!(policy.rules.len(), 1);
/// ```
pub fn parse(input: &str) -> Result<Policy> {
    let mut parser = ToonParser::new(input);
    parser.parse_policy()
}

/// Internal parser state.
struct ToonParser<'a> {
    lines: Vec<(usize, &'a str)>, // (line_number, content)
    pos: usize,
}

impl<'a> ToonParser<'a> {
    fn new(input: &'a str) -> Self {
        let lines: Vec<(usize, &str)> = input
            .lines()
            .enumerate()
            .map(|(i, line)| (i + 1, line))
            .filter(|(_, line)| !line.trim().is_empty())
            .collect();
        Self { lines, pos: 0 }
    }

    fn current_line(&self) -> Option<(usize, &'a str)> {
        self.lines.get(self.pos).copied()
    }

    fn advance(&mut self) {
        self.pos += 1;
    }

    fn parse_policy(&mut self) -> Result<Policy> {
        // Find and parse the header line: nectar_policy{version,name,budget_per_second,rules}:
        let (line_num, header_line) = self.current_line().ok_or_else(|| Error::Parse {
            line: 1,
            reason: "empty input".to_string(),
        })?;

        let header_line = header_line.trim();

        // Parse header: name{fields}:
        let header = parse_header(header_line, line_num)?;
        if header.name != "nectar_policy" {
            return Err(Error::Parse {
                line: line_num,
                reason: format!("expected 'nectar_policy' header, found '{}'", header.name),
            });
        }

        self.advance();

        // Parse scalar fields
        let mut version: Option<u32> = None;
        let mut name: Option<String> = None;
        let mut budget_per_second: Option<u64> = None;
        let mut rules: Vec<Rule> = Vec::new();

        for field in &header.fields {
            match field.as_str() {
                "version" => {
                    let (line_num, value) = self.expect_value(2)?;
                    version = Some(value.parse().map_err(|_| Error::Parse {
                        line: line_num,
                        reason: format!("invalid version number: {value}"),
                    })?);
                }
                "name" => {
                    let (_, value) = self.expect_value(2)?;
                    name = Some(value.to_string());
                }
                "budget_per_second" => {
                    let (line_num, value) = self.expect_value(2)?;
                    budget_per_second = Some(value.parse().map_err(|_| Error::Parse {
                        line: line_num,
                        reason: format!("invalid budget number: {value}"),
                    })?);
                }
                "rules" => {
                    rules = self.parse_rules(2)?;
                }
                other => {
                    return Err(Error::Parse {
                        line: line_num,
                        reason: format!("unknown field: {other}"),
                    });
                }
            }
        }

        let name = name.ok_or_else(|| Error::MissingField("name".to_string()))?;

        let mut policy = Policy::new(name);
        policy.version = version.unwrap_or(1);
        policy.budget_per_second = budget_per_second;
        for rule in rules {
            policy.add_rule(rule);
        }

        Ok(policy)
    }

    fn expect_value(&mut self, expected_indent: usize) -> Result<(usize, &'a str)> {
        let (line_num, line) = self.current_line().ok_or_else(|| Error::Parse {
            line: self.lines.last().map_or(1, |(n, _)| *n),
            reason: "unexpected end of input".to_string(),
        })?;

        let indent = count_indent(line);
        if indent < expected_indent {
            return Err(Error::Parse {
                line: line_num,
                reason: format!(
                    "expected indent of at least {expected_indent} spaces, found {indent}"
                ),
            });
        }

        self.advance();
        Ok((line_num, line.trim()))
    }

    fn parse_rules(&mut self, expected_indent: usize) -> Result<Vec<Rule>> {
        // Expect: rules[N]{name,description,match,action,priority}:
        let (line_num, line) = self.current_line().ok_or_else(|| Error::Parse {
            line: self.lines.last().map_or(1, |(n, _)| *n),
            reason: "expected rules section".to_string(),
        })?;

        let indent = count_indent(line);
        if indent < expected_indent {
            return Err(Error::Parse {
                line: line_num,
                reason: format!(
                    "expected indent of at least {expected_indent} spaces for rules header"
                ),
            });
        }

        let header = parse_header(line.trim(), line_num)?;
        if header.name != "rules" {
            return Err(Error::Parse {
                line: line_num,
                reason: format!("expected 'rules' header, found '{}'", header.name),
            });
        }

        let declared_count = header.count.ok_or_else(|| Error::Parse {
            line: line_num,
            reason: "rules header must specify count [N]".to_string(),
        })?;

        self.advance();

        // Parse rule rows
        let mut rules = Vec::new();
        let row_indent = expected_indent + 2;

        while let Some((row_line_num, row_line)) = self.current_line() {
            let indent = count_indent(row_line);
            if indent < row_indent {
                // End of rules section
                break;
            }

            let rule = parse_rule_row(&header.fields, row_line_num, row_line.trim())?;
            rules.push(rule);
            self.advance();
        }

        // Validate count
        if rules.len() != declared_count {
            return Err(Error::CountMismatch {
                declared: declared_count,
                actual: rules.len(),
            });
        }

        Ok(rules)
    }
}

/// Parses a single rule row.
fn parse_rule_row(fields: &[String], line_num: usize, row: &str) -> Result<Rule> {
    let values = parse_csv_row(row);

    if values.len() != fields.len() {
        return Err(Error::Parse {
            line: line_num,
            reason: format!(
                "expected {} fields, found {} in row: {row}",
                fields.len(),
                values.len()
            ),
        });
    }

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;
    let mut match_expr: Option<String> = None;
    let mut action: Option<Action> = None;
    let mut priority: Option<u8> = None;

    for (field, value) in fields.iter().zip(values.iter()) {
        match field.as_str() {
            "name" => name = Some(value.clone()),
            "description" => {
                if !value.is_empty() {
                    description = Some(value.clone());
                }
            }
            "match" => match_expr = Some(value.clone()),
            "action" => {
                action = Some(parse_action(value).map_err(|e| Error::Parse {
                    line: line_num,
                    reason: format!("invalid action: {e}"),
                })?);
            }
            "priority" => {
                priority = Some(value.parse().map_err(|_| Error::Parse {
                    line: line_num,
                    reason: format!("invalid priority: {value}"),
                })?);
            }
            other => {
                return Err(Error::Parse {
                    line: line_num,
                    reason: format!("unknown rule field: {other}"),
                });
            }
        }
    }

    let name = name.ok_or_else(|| Error::Parse {
        line: line_num,
        reason: "rule missing 'name' field".to_string(),
    })?;

    let match_expr = match_expr.ok_or_else(|| Error::Parse {
        line: line_num,
        reason: "rule missing 'match' field".to_string(),
    })?;

    let action = action.ok_or_else(|| Error::Parse {
        line: line_num,
        reason: "rule missing 'action' field".to_string(),
    })?;

    let priority = priority.ok_or_else(|| Error::Parse {
        line: line_num,
        reason: "rule missing 'priority' field".to_string(),
    })?;

    let mut rule = Rule::new(name, match_expr, action, priority);
    if let Some(desc) = description {
        rule = rule.with_description(desc);
    }

    Ok(rule)
}

/// Parsed TOON header info.
struct Header {
    name: String,
    count: Option<usize>,
    fields: Vec<String>,
}

/// Parses a header line like `name[N]{field1,field2}:` or `name{field1,field2}:`
fn parse_header(line: &str, line_num: usize) -> Result<Header> {
    let line = line.trim();

    if !line.ends_with(':') {
        return Err(Error::Parse {
            line: line_num,
            reason: "header must end with ':'".to_string(),
        });
    }

    let line = &line[..line.len() - 1]; // Remove trailing ':'

    // Find the name (before '[' or '{')
    let name_end = line.find(['[', '{']).ok_or_else(|| Error::Parse {
        line: line_num,
        reason: "header must have fields in braces".to_string(),
    })?;

    let name = line[..name_end].to_string();

    // Check for count [N]
    let (count, fields_start) = if line[name_end..].starts_with('[') {
        let bracket_end = line[name_end..]
            .find(']')
            .ok_or_else(|| Error::Parse {
                line: line_num,
                reason: "unclosed '[' in header".to_string(),
            })?
            + name_end;

        let count_str = &line[name_end + 1..bracket_end];
        let count: usize = count_str.parse().map_err(|_| Error::Parse {
            line: line_num,
            reason: format!("invalid count in header: {count_str}"),
        })?;

        (Some(count), bracket_end + 1)
    } else {
        (None, name_end)
    };

    // Parse fields {field1,field2,...}
    if !line[fields_start..].starts_with('{') || !line.ends_with('}') {
        return Err(Error::Parse {
            line: line_num,
            reason: "header must have fields in braces".to_string(),
        });
    }

    let fields_str = &line[fields_start + 1..line.len() - 1];
    let fields: Vec<String> = fields_str.split(',').map(|s| s.trim().to_string()).collect();

    if fields.is_empty() {
        return Err(Error::Parse {
            line: line_num,
            reason: "header must declare at least one field".to_string(),
        });
    }

    Ok(Header {
        name,
        count,
        fields,
    })
}

/// Counts leading spaces in a line.
fn count_indent(line: &str) -> usize {
    line.len() - line.trim_start().len()
}

/// Parses a CSV row, handling quoted values.
fn parse_csv_row(row: &str) -> Vec<String> {
    let mut values = Vec::new();
    let mut current = String::new();
    let mut in_quotes = false;
    let mut chars = row.chars().peekable();

    while let Some(c) = chars.next() {
        match c {
            '"' => {
                if in_quotes {
                    // Check for escaped quote
                    if chars.peek() == Some(&'"') {
                        current.push('"');
                        chars.next();
                    } else {
                        in_quotes = false;
                    }
                } else {
                    in_quotes = true;
                }
            }
            ',' if !in_quotes => {
                values.push(current.trim().to_string());
                current = String::new();
            }
            _ => {
                current.push(c);
            }
        }
    }

    values.push(current.trim().to_string());
    values
}

/// Parses an action string into an [`Action`].
///
/// # Supported formats
///
/// - `keep` → `Action::Keep`
/// - `drop` → `Action::Drop`
/// - `sample(0.1)` → `Action::Sample(0.1)`
///
/// # Errors
///
/// Returns an error if the action string is not a valid format.
pub fn parse_action(s: &str) -> Result<Action> {
    let s = s.trim();
    match s {
        "keep" => Ok(Action::Keep),
        "drop" => Ok(Action::Drop),
        _ if s.starts_with("sample(") && s.ends_with(')') => {
            let rate_str = &s[7..s.len() - 1];
            let rate: f64 = rate_str.parse().map_err(|_| Error::InvalidAction {
                action: s.to_string(),
                reason: format!("invalid sample rate: {rate_str}"),
            })?;
            if !(0.0..=1.0).contains(&rate) {
                return Err(Error::InvalidAction {
                    action: s.to_string(),
                    reason: "sample rate must be between 0.0 and 1.0".to_string(),
                });
            }
            Ok(Action::Sample(rate))
        }
        _ => Err(Error::InvalidAction {
            action: s.to_string(),
            reason: "expected 'keep', 'drop', or 'sample(rate)'".to_string(),
        }),
    }
}

/// Serializes a [`Policy`] to TOON format.
pub fn serialize(policy: &Policy) -> String {
    let mut output = String::new();

    // Header
    output.push_str("nectar_policy{version,name,budget_per_second,rules}:\n");

    // Version
    let _ = writeln!(output, "  {}", policy.version);

    // Name
    let _ = writeln!(output, "  {}", policy.name);

    // Budget
    if let Some(budget) = policy.budget_per_second {
        let _ = writeln!(output, "  {budget}");
    } else {
        output.push_str("  0\n");
    }

    // Rules
    let _ = writeln!(
        output,
        "  rules[{}]{{name,description,match,action,priority}}:",
        policy.rules.len()
    );

    for rule in &policy.rules {
        let description = rule.description.as_deref().unwrap_or("");
        let action_str = match &rule.action {
            Action::Keep => "keep".to_string(),
            Action::Drop => "drop".to_string(),
            Action::Sample(rate) => format!("sample({rate})"),
        };

        // Escape values that contain commas
        let name = escape_csv_value(&rule.name);
        let description = escape_csv_value(description);
        let match_expr = escape_csv_value(&rule.match_expr);

        let _ = writeln!(
            output,
            "    {name},{description},{match_expr},{action_str},{}",
            rule.priority
        );
    }

    output
}

/// Escapes a value for CSV output if it contains special characters.
fn escape_csv_value(value: &str) -> String {
    if value.contains(',') || value.contains('"') || value.contains('\n') {
        let escaped = value.replace('"', "\"\"");
        format!("\"{escaped}\"")
    } else {
        value.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_action_keep() {
        assert_eq!(parse_action("keep").unwrap(), Action::Keep);
    }

    #[test]
    fn parse_action_drop() {
        assert_eq!(parse_action("drop").unwrap(), Action::Drop);
    }

    #[test]
    fn parse_action_sample() {
        assert_eq!(parse_action("sample(0.1)").unwrap(), Action::Sample(0.1));
        assert_eq!(parse_action("sample(0.0)").unwrap(), Action::Sample(0.0));
        assert_eq!(parse_action("sample(1.0)").unwrap(), Action::Sample(1.0));
    }

    #[test]
    fn parse_action_sample_invalid_rate() {
        assert!(parse_action("sample(1.5)").is_err());
        assert!(parse_action("sample(-0.1)").is_err());
        assert!(parse_action("sample(abc)").is_err());
    }

    #[test]
    fn parse_action_invalid() {
        assert!(parse_action("unknown").is_err());
        assert!(parse_action("").is_err());
    }

    #[test]
    fn parse_simple_policy() {
        let input = r#"
nectar_policy{version,name,budget_per_second,rules}:
  1
  test-policy
  5000
  rules[2]{name,description,match,action,priority}:
    keep-errors,Keep all errors,http.status >= 500,keep,100
    sample-rest,Sample remaining,true,sample(0.01),0
"#;

        let policy = parse(input).unwrap();
        assert_eq!(policy.version, 1);
        assert_eq!(policy.name, "test-policy");
        assert_eq!(policy.budget_per_second, Some(5000));
        assert_eq!(policy.rules.len(), 2);

        // Rules are sorted by priority (high to low)
        assert_eq!(policy.rules[0].name, "keep-errors");
        assert_eq!(policy.rules[0].priority, 100);
        assert_eq!(policy.rules[0].action, Action::Keep);

        assert_eq!(policy.rules[1].name, "sample-rest");
        assert_eq!(policy.rules[1].priority, 0);
        assert_eq!(policy.rules[1].action, Action::Sample(0.01));
    }

    #[test]
    fn parse_policy_with_complex_match_expr() {
        let input = r#"
nectar_policy{version,name,budget_per_second,rules}:
  1
  complex-policy
  10000
  rules[1]{name,description,match,action,priority}:
    keep-errors,"Keep HTTP 5xx errors","http.status >= 500 || error == true",keep,100
"#;

        let policy = parse(input).unwrap();
        assert_eq!(
            policy.rules[0].match_expr,
            "http.status >= 500 || error == true"
        );
        assert_eq!(
            policy.rules[0].description,
            Some("Keep HTTP 5xx errors".to_string())
        );
    }

    #[test]
    fn parse_policy_count_mismatch() {
        let input = r#"
nectar_policy{version,name,budget_per_second,rules}:
  1
  test-policy
  5000
  rules[3]{name,description,match,action,priority}:
    keep-errors,Keep errors,status >= 500,keep,100
    sample-rest,Sample remaining,true,sample(0.01),0
"#;

        let result = parse(input);
        assert!(matches!(
            result,
            Err(Error::CountMismatch {
                declared: 3,
                actual: 2
            })
        ));
    }

    #[test]
    fn parse_policy_missing_name() {
        let input = r#"
nectar_policy{version,budget_per_second,rules}:
  1
  5000
  rules[0]{name,description,match,action,priority}:
"#;

        let result = parse(input);
        assert!(matches!(result, Err(Error::MissingField(_))));
    }

    #[test]
    fn roundtrip_serialize_parse() {
        let mut policy = Policy::new("roundtrip-test");
        policy.version = 1;
        policy.budget_per_second = Some(8000);
        policy.add_rule(
            Rule::new("keep-errors", "status >= 500", Action::Keep, 100)
                .with_description("Retain all errors"),
        );
        policy.add_rule(
            Rule::new("sample-rest", "true", Action::Sample(0.05), 0)
                .with_description("Sample baseline"),
        );

        let serialized = serialize(&policy);
        let parsed = parse(&serialized).unwrap();

        assert_eq!(parsed.name, policy.name);
        assert_eq!(parsed.version, policy.version);
        assert_eq!(parsed.budget_per_second, policy.budget_per_second);
        assert_eq!(parsed.rules.len(), policy.rules.len());
    }

    #[test]
    fn parse_csv_row_simple() {
        let values = parse_csv_row("a,b,c");
        assert_eq!(values, vec!["a", "b", "c"]);
    }

    #[test]
    fn parse_csv_row_quoted() {
        let values = parse_csv_row(r#"a,"b,c",d"#);
        assert_eq!(values, vec!["a", "b,c", "d"]);
    }

    #[test]
    fn parse_csv_row_escaped_quotes() {
        let values = parse_csv_row(r#"a,"b""c",d"#);
        assert_eq!(values, vec!["a", "b\"c", "d"]);
    }
}
