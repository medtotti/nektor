//! Refinery output format types.

use serde::{Deserialize, Serialize};

/// Refinery configuration root.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefineryConfig {
    /// Sampling rules.
    #[serde(rename = "RulesBasedSampler")]
    pub rules_based_sampler: RulesBasedSampler,
}

/// Rules-based sampler configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RulesBasedSampler {
    /// List of sampling rules.
    pub rules: Vec<RefineryRule>,
}

/// A single Refinery sampling rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefineryRule {
    /// Rule name.
    pub name: String,
    /// Sample rate (1 = keep all, 0 = drop all).
    #[serde(rename = "SampleRate")]
    pub sample_rate: u32,
    /// Conditions for this rule.
    #[serde(skip_serializing_if = "Vec::is_empty", default)]
    pub conditions: Vec<RefineryCondition>,
}

/// A condition in a Refinery rule.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RefineryCondition {
    /// Field to match.
    pub field: String,
    /// Operator (=, !=, >, <, >=, <=, contains, etc.).
    pub operator: String,
    /// Value to compare against.
    pub value: ConditionValue,
}

/// Value in a condition.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ConditionValue {
    /// String value.
    String(String),
    /// Numeric value.
    Number(i64),
    /// Boolean value.
    Bool(bool),
}

impl RefineryConfig {
    /// Creates a new empty configuration.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            rules_based_sampler: RulesBasedSampler { rules: Vec::new() },
        }
    }

    /// Adds a rule to the configuration.
    pub fn add_rule(&mut self, rule: RefineryRule) {
        self.rules_based_sampler.rules.push(rule);
    }
}

impl Default for RefineryConfig {
    fn default() -> Self {
        Self::new()
    }
}

impl RefineryRule {
    /// Creates a new rule that keeps all matching traces.
    #[must_use]
    pub fn keep(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sample_rate: 1,
            conditions: Vec::new(),
        }
    }

    /// Creates a new rule that drops all matching traces.
    #[must_use]
    pub fn drop(name: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            sample_rate: 0,
            conditions: Vec::new(),
        }
    }

    /// Creates a new rule with a sample rate.
    #[must_use]
    pub fn sample(name: impl Into<String>, rate: f64) -> Self {
        // Refinery uses integer sample rates where N means keep 1/N
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let sample_rate = if rate <= 0.0 {
            0
        } else if rate >= 1.0 {
            1
        } else {
            (1.0 / rate).round() as u32
        };

        Self {
            name: name.into(),
            sample_rate,
            conditions: Vec::new(),
        }
    }

    /// Adds a condition to the rule.
    #[must_use]
    pub fn with_condition(mut self, condition: RefineryCondition) -> Self {
        self.conditions.push(condition);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn refinery_rule_sample_rate_conversion() {
        // 10% sample rate = keep 1 in 10
        let rule = RefineryRule::sample("test", 0.1);
        assert_eq!(rule.sample_rate, 10);

        // 1% sample rate = keep 1 in 100
        let rule = RefineryRule::sample("test", 0.01);
        assert_eq!(rule.sample_rate, 100);

        // 100% = keep all
        let rule = RefineryRule::sample("test", 1.0);
        assert_eq!(rule.sample_rate, 1);
    }

    #[test]
    fn serialize_refinery_config() {
        let mut config = RefineryConfig::new();
        config.add_rule(RefineryRule::keep("keep-errors"));

        let yaml = serde_yaml::to_string(&config).unwrap();
        assert!(yaml.contains("keep-errors"));
        assert!(yaml.contains("SampleRate: 1"));
    }
}
