//! Typed policy model.
//!
//! These types represent a validated, well-formed policy.
//! Invalid states are unrepresentable by construction.

use serde::{Deserialize, Serialize};

/// A validated sampling policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Policy {
    /// Policy schema version.
    pub version: u32,
    /// Human-readable policy name.
    pub name: String,
    /// Maximum traces per second budget.
    pub budget_per_second: Option<u64>,
    /// Ordered list of sampling rules.
    pub rules: Vec<Rule>,
}

/// A single sampling rule within a policy.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Rule {
    /// Unique rule identifier.
    pub name: String,
    /// Human-readable description.
    pub description: Option<String>,
    /// Match expression (evaluated against trace attributes).
    pub match_expr: String,
    /// Action to take when matched.
    pub action: Action,
    /// Priority (higher = evaluated first).
    pub priority: u8,
}

/// Sampling action.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum Action {
    /// Always keep matching traces.
    Keep,
    /// Always drop matching traces.
    Drop,
    /// Sample at the given rate (0.0 to 1.0).
    Sample(f64),
}

impl Policy {
    /// Creates a new empty policy.
    #[must_use]
    pub fn new(name: impl Into<String>) -> Self {
        Self {
            version: 1,
            name: name.into(),
            budget_per_second: None,
            rules: Vec::new(),
        }
    }

    /// Adds a rule to the policy.
    pub fn add_rule(&mut self, rule: Rule) {
        self.rules.push(rule);
        self.rules.sort_by(|a, b| b.priority.cmp(&a.priority));
    }

    /// Returns true if the policy has a fallback rule (matches all).
    #[must_use]
    pub fn has_fallback(&self) -> bool {
        self.rules.iter().any(|r| r.match_expr == "true")
    }
}

impl Rule {
    /// Creates a new rule.
    #[must_use]
    pub fn new(
        name: impl Into<String>,
        match_expr: impl Into<String>,
        action: Action,
        priority: u8,
    ) -> Self {
        Self {
            name: name.into(),
            description: None,
            match_expr: match_expr.into(),
            action,
            priority,
        }
    }

    /// Adds a description to the rule.
    #[must_use]
    pub fn with_description(mut self, description: impl Into<String>) -> Self {
        self.description = Some(description.into());
        self
    }
}

impl Action {
    /// Returns the effective keep rate for this action.
    #[must_use]
    pub const fn effective_rate(&self) -> f64 {
        match self {
            Self::Keep => 1.0,
            Self::Drop => 0.0,
            Self::Sample(rate) => *rate,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn policy_sorts_rules_by_priority() {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("low", "true", Action::Sample(0.01), 10));
        policy.add_rule(Rule::new("high", "error", Action::Keep, 100));
        policy.add_rule(Rule::new("mid", "slow", Action::Keep, 50));

        assert_eq!(policy.rules[0].name, "high");
        assert_eq!(policy.rules[1].name, "mid");
        assert_eq!(policy.rules[2].name, "low");
    }

    #[test]
    fn action_effective_rate() {
        assert!((Action::Keep.effective_rate() - 1.0).abs() < f64::EPSILON);
        assert!((Action::Drop.effective_rate() - 0.0).abs() < f64::EPSILON);
        assert!((Action::Sample(0.5).effective_rate() - 0.5).abs() < f64::EPSILON);
    }
}
