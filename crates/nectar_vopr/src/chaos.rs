//! Chaos injection for robustness testing.
//!
//! Provides controlled fault injection to test system resilience:
//! - Corpus corruption (missing fields, invalid values)
//! - Policy mutation (invalid rules, missing fallbacks)
//! - Timing anomalies (simulated delays, timeouts)

use nectar_corpus::{Corpus, Trace};
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use std::time::Duration;
use toon_policy::{Action, Policy, Rule};

/// Configuration for chaos injection.
#[derive(Debug, Clone)]
pub struct ChaosConfig {
    /// Random seed for reproducible chaos.
    pub seed: u64,
    /// Probability of corrupting a trace (0.0 - 1.0).
    pub trace_corruption_rate: f64,
    /// Probability of corrupting a policy rule (0.0 - 1.0).
    pub rule_corruption_rate: f64,
    /// Types of corruption to apply.
    pub corruption_types: Vec<CorruptionType>,
}

impl Default for ChaosConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            trace_corruption_rate: 0.1,
            rule_corruption_rate: 0.1,
            corruption_types: vec![
                CorruptionType::InvalidStatus,
                CorruptionType::ZeroDuration,
                CorruptionType::EmptyServiceName,
                CorruptionType::MalformedMatchExpr,
            ],
        }
    }
}

impl ChaosConfig {
    /// Creates a config with the given seed.
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Sets the trace corruption rate.
    #[must_use]
    pub const fn with_trace_corruption_rate(mut self, rate: f64) -> Self {
        self.trace_corruption_rate = rate;
        self
    }

    /// Sets the rule corruption rate.
    #[must_use]
    pub const fn with_rule_corruption_rate(mut self, rate: f64) -> Self {
        self.rule_corruption_rate = rate;
        self
    }

    /// Creates a high-chaos config for stress testing.
    #[must_use]
    pub fn high_chaos() -> Self {
        Self {
            seed: 42,
            trace_corruption_rate: 0.5,
            rule_corruption_rate: 0.3,
            corruption_types: vec![
                CorruptionType::InvalidStatus,
                CorruptionType::ZeroDuration,
                CorruptionType::EmptyServiceName,
                CorruptionType::ExtremeValues,
                CorruptionType::MalformedMatchExpr,
            ],
        }
    }
}

/// Types of corruption that can be applied.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CorruptionType {
    /// Invalid HTTP status codes (e.g., 999).
    InvalidStatus,
    /// Zero duration values.
    ZeroDuration,
    /// Empty service names.
    EmptyServiceName,
    /// Extreme values (MAX).
    ExtremeValues,
    /// Malformed match expressions.
    MalformedMatchExpr,
    /// Remove fallback rules.
    RemoveFallback,
    /// Duplicate rule names.
    DuplicateRuleNames,
}

/// Chaos injector for controlled fault injection.
pub struct ChaosInjector {
    config: ChaosConfig,
    rng: ChaCha8Rng,
}

impl ChaosInjector {
    /// Creates a new chaos injector.
    #[must_use]
    pub fn new(config: ChaosConfig) -> Self {
        let rng = ChaCha8Rng::seed_from_u64(config.seed);
        Self { config, rng }
    }

    /// Corrupts a corpus by applying random mutations.
    #[must_use]
    pub fn corrupt_corpus(&mut self, corpus: &Corpus) -> Corpus {
        let mut corrupted = Corpus::new();

        for trace in corpus.iter() {
            if self.rng.gen_bool(self.config.trace_corruption_rate) {
                corrupted.add(self.corrupt_trace(trace));
            } else {
                corrupted.add(trace.clone());
            }
        }

        corrupted
    }

    fn corrupt_trace(&mut self, trace: &Trace) -> Trace {
        let corruption = self.config.corruption_types.choose(&mut self.rng);

        match corruption {
            Some(CorruptionType::InvalidStatus) => {
                Trace::new(&trace.trace_id)
                    .with_service(trace.service.as_deref().unwrap_or("unknown"))
                    .with_status(999) // Invalid status
                    .with_duration(trace.duration)
            }
            Some(CorruptionType::ZeroDuration) => Trace::new(&trace.trace_id)
                .with_service(trace.service.as_deref().unwrap_or("unknown"))
                .with_status(trace.status.unwrap_or(200))
                .with_duration(Duration::ZERO),
            Some(CorruptionType::EmptyServiceName) => Trace::new(&trace.trace_id)
                .with_service("")
                .with_status(trace.status.unwrap_or(200))
                .with_duration(trace.duration),
            Some(CorruptionType::ExtremeValues) => Trace::new(&trace.trace_id)
                .with_service(trace.service.as_deref().unwrap_or("unknown"))
                .with_status(u16::MAX)
                .with_duration(Duration::from_millis(u64::MAX / 1000)),
            _ => trace.clone(),
        }
    }

    /// Corrupts a policy by applying random mutations.
    #[must_use]
    pub fn corrupt_policy(&mut self, policy: &Policy) -> Policy {
        let mut corrupted = Policy::new(&policy.name);
        corrupted.budget_per_second = policy.budget_per_second;

        for rule in &policy.rules {
            if self.rng.gen_bool(self.config.rule_corruption_rate) {
                if let Some(corrupted_rule) = self.corrupt_rule(rule) {
                    corrupted.add_rule(corrupted_rule);
                }
                // Otherwise, rule is dropped (simulating removal)
            } else {
                corrupted.add_rule(rule.clone());
            }
        }

        corrupted
    }

    fn corrupt_rule(&mut self, rule: &Rule) -> Option<Rule> {
        let corruption = self.config.corruption_types.choose(&mut self.rng)?;

        match corruption {
            CorruptionType::MalformedMatchExpr => {
                Some(Rule::new(
                    &rule.name,
                    "((invalid && ||", // Malformed expression
                    rule.action.clone(),
                    rule.priority,
                ))
            }
            CorruptionType::RemoveFallback => {
                if rule.match_expr == "true" {
                    None // Remove fallback rule
                } else {
                    Some(rule.clone())
                }
            }
            CorruptionType::DuplicateRuleNames => Some(Rule::new(
                "duplicate-name",
                &rule.match_expr,
                rule.action.clone(),
                rule.priority,
            )),
            _ => Some(rule.clone()),
        }
    }

    /// Generates a completely invalid policy for negative testing.
    #[must_use]
    pub fn generate_invalid_policy(&mut self) -> Policy {
        let mut policy = Policy::new("");

        // Add rules with various problems
        policy.add_rule(Rule::new(
            "", // Empty name
            "invalid syntax >>>",
            Action::Keep,
            100,
        ));

        policy.add_rule(Rule::new(
            "negative-rate",
            "http.status >= 500",
            Action::Sample(-1.0), // Invalid rate
            50,
        ));

        // No fallback rule

        policy
    }
}

/// Runs a chaos test campaign with increasing intensity.
#[allow(clippy::cast_precision_loss)]
pub fn chaos_campaign(policy: &Policy, corpus: &Corpus, iterations: usize) -> Vec<ChaosResult> {
    let mut results = Vec::new();

    for i in 0..iterations {
        let intensity = (i as f64) / (iterations as f64);
        let config = ChaosConfig::default()
            .with_seed(i as u64)
            .with_trace_corruption_rate(intensity * 0.5)
            .with_rule_corruption_rate(intensity * 0.3);

        let mut injector = ChaosInjector::new(config);
        let chaotic_corpus = injector.corrupt_corpus(corpus);
        let chaotic_policy = injector.corrupt_policy(policy);

        // Test prover
        let prover = nectar_prover::Prover::default();
        let prover_result = prover.verify(&chaotic_policy, &chaotic_corpus);

        // Test compiler
        let compiler = nectar_compiler::Compiler::new();
        let compiler_result = compiler.compile(&chaotic_policy);

        results.push(ChaosResult {
            iteration: i,
            intensity,
            prover_survived: prover_result.is_ok(),
            compiler_survived: compiler_result.is_ok(),
            prover_error: prover_result.err().map(|e| e.to_string()),
            compiler_error: compiler_result.err().map(|e| e.to_string()),
        });
    }

    results
}

/// Result of a single chaos iteration.
#[derive(Debug, Clone)]
pub struct ChaosResult {
    /// Iteration number.
    pub iteration: usize,
    /// Chaos intensity (0.0 - 1.0).
    pub intensity: f64,
    /// Whether the prover handled the chaos.
    pub prover_survived: bool,
    /// Whether the compiler handled the chaos.
    pub compiler_survived: bool,
    /// Prover error message (if any).
    pub prover_error: Option<String>,
    /// Compiler error message (if any).
    pub compiler_error: Option<String>,
}

impl ChaosResult {
    /// Returns true if both prover and compiler survived.
    #[must_use]
    pub const fn fully_survived(&self) -> bool {
        self.prover_survived && self.compiler_survived
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_policy() -> Policy {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new(
            "keep-errors",
            "http.status >= 500",
            Action::Keep,
            100,
        ));
        policy.add_rule(Rule::new("fallback", "true", Action::Sample(0.01), 0));
        policy
    }

    #[test]
    fn chaos_injector_is_deterministic() {
        let config = ChaosConfig::default().with_seed(42);

        let mut injector1 = ChaosInjector::new(config.clone());
        let mut injector2 = ChaosInjector::new(config);

        let policy = test_policy();

        let corrupted1 = injector1.corrupt_policy(&policy);
        let corrupted2 = injector2.corrupt_policy(&policy);

        // Same seed should produce same corruption
        assert_eq!(corrupted1.rules.len(), corrupted2.rules.len());
    }

    #[test]
    fn invalid_policy_is_truly_invalid() {
        let mut injector = ChaosInjector::new(ChaosConfig::default());
        let invalid = injector.generate_invalid_policy();

        // Should not have a fallback
        assert!(!invalid.has_fallback());

        // Should have empty name
        assert!(invalid.name.is_empty());
    }

    #[test]
    fn chaos_campaign_runs_to_completion() {
        let policy = test_policy();
        let corpus = Corpus::new();

        let results = chaos_campaign(&policy, &corpus, 10);

        assert_eq!(results.len(), 10);

        // Early iterations (low intensity) should mostly survive
        assert!(results[0].compiler_survived);
    }
}
