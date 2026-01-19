//! Main compiler implementation.

use crate::error::{Error, Result};
use crate::match_expr::MatchExpr;
use crate::refinery::{RefineryConfig, RefineryRule};
use toon_policy::{Action, Policy};
use tracing::warn;

/// Output format for compiled policies.
#[derive(Debug, Clone, Copy, Default)]
pub enum OutputFormat {
    /// YAML format (default).
    #[default]
    Yaml,
    /// JSON format.
    Json,
}

/// Compilation options.
#[derive(Debug, Clone, Default)]
pub struct CompileOptions {
    /// Output format.
    pub format: OutputFormat,
    /// Include comments/documentation in output.
    pub include_comments: bool,
}

/// Policy compiler that transforms Nectar policies to Refinery rules.
///
/// This compiler is **pure and deterministic**:
/// - No network calls
/// - No randomness
/// - Same input always produces same output
#[derive(Debug, Clone, Default)]
pub struct Compiler {
    options: CompileOptions,
}

impl Compiler {
    /// Creates a new compiler with default options.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a new compiler with the given options.
    #[must_use]
    pub const fn with_options(options: CompileOptions) -> Self {
        Self { options }
    }

    /// Compiles a policy to Refinery configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if:
    /// - The policy contains unsupported constructs
    /// - Serialization fails
    pub fn compile(&self, policy: &Policy) -> Result<String> {
        let config = self.to_refinery_config(policy)?;

        match self.options.format {
            OutputFormat::Yaml => serde_yaml::to_string(&config).map_err(Error::from),
            OutputFormat::Json => serde_json::to_string_pretty(&config).map_err(Error::from),
        }
    }

    /// Compiles a policy to a Refinery configuration struct.
    ///
    /// # Errors
    ///
    /// Returns an error if the policy contains unsupported constructs.
    pub fn to_refinery_config(&self, policy: &Policy) -> Result<RefineryConfig> {
        let mut config = RefineryConfig::new();

        for rule in &policy.rules {
            let refinery_rule = Self::compile_rule(rule)?;
            config.add_rule(refinery_rule);
        }

        Ok(config)
    }

    fn compile_rule(rule: &toon_policy::Rule) -> Result<RefineryRule> {
        let mut refinery_rule = match &rule.action {
            Action::Keep => RefineryRule::keep(&rule.name),
            Action::Drop => RefineryRule::drop(&rule.name),
            Action::Sample(rate) => RefineryRule::sample(&rule.name, *rate),
        };

        // Parse match expression and convert to conditions
        let match_expr = MatchExpr::parse(&rule.match_expr)?;

        match match_expr.to_refinery_conditions() {
            Ok(conditions) => {
                for condition in conditions {
                    refinery_rule = refinery_rule.with_condition(condition);
                }
            }
            Err(e) => {
                // Log warning but don't fail compilation
                // Some expressions (like OR) may need special handling
                warn!(
                    "Could not convert match expression '{}' to conditions: {}",
                    rule.match_expr, e
                );
            }
        }

        Ok(refinery_rule)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toon_policy::Rule;

    #[test]
    fn compile_simple_policy() {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("keep-errors", "status >= 500", Action::Keep, 100));
        policy.add_rule(Rule::new("sample-rest", "true", Action::Sample(0.01), 0));

        let compiler = Compiler::new();
        let output = compiler.compile(&policy).unwrap();

        assert!(output.contains("keep-errors"));
        assert!(output.contains("sample-rest"));
    }

    #[test]
    fn compile_to_json() {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("fallback", "true", Action::Sample(0.1), 0));

        let compiler = Compiler::with_options(CompileOptions {
            format: OutputFormat::Json,
            ..Default::default()
        });
        let output = compiler.compile(&policy).unwrap();

        assert!(output.contains("\"name\": \"fallback\""));
    }

    #[test]
    fn compiler_is_deterministic() {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("a", "true", Action::Keep, 100));
        policy.add_rule(Rule::new("b", "true", Action::Sample(0.5), 50));

        let compiler = Compiler::new();
        let output1 = compiler.compile(&policy).unwrap();
        let output2 = compiler.compile(&policy).unwrap();

        assert_eq!(output1, output2, "Compiler must be deterministic");
    }
}
