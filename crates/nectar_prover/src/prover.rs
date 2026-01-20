//! Main prover implementation.

use crate::checks;
use crate::error::{Error, Result};
use crate::result::{ProverResult, Violation};
use crate::simulation::{SimulationResult, Simulator};
use crate::traffic::TrafficPattern;
use nectar_corpus::Corpus;
use std::path::Path;
use toon_policy::Policy;

/// Policy prover that validates policies before compilation.
#[derive(Debug, Clone)]
pub struct Prover {
    config: ProverConfig,
}

/// Configuration for the prover.
#[derive(Debug, Clone, Default)]
pub struct ProverConfig {
    /// Maximum allowed budget per second.
    pub max_budget: Option<u64>,
    /// Whether to require explicit error handling.
    pub require_error_handling: bool,
}

impl Default for Prover {
    fn default() -> Self {
        Self::new(ProverConfig::default())
    }
}

impl Prover {
    /// Creates a new prover with the given configuration.
    #[must_use]
    pub const fn new(config: ProverConfig) -> Self {
        Self { config }
    }

    /// Verifies a policy against the given corpus.
    ///
    /// # Errors
    ///
    /// Returns an error if the policy is fundamentally invalid.
    pub fn verify(&self, policy: &Policy, corpus: &Corpus) -> Result<ProverResult> {
        if policy.rules.is_empty() {
            return Err(Error::InvalidPolicy("policy has no rules".to_string()));
        }

        let mut violations = Vec::new();
        let mut checks_passed = 0;
        let checks_total = 4;

        // Check 1: Fallback rule
        if let Some(v) = checks::check_fallback(policy) {
            violations.push(v);
        } else {
            checks_passed += 1;
        }

        // Check 2: Error handling
        if self.config.require_error_handling {
            if let Some(v) = checks::check_error_handling(policy) {
                violations.push(v);
            } else {
                checks_passed += 1;
            }
        } else {
            checks_passed += 1;
        }

        // Check 3: Must-keep coverage
        if let Err(v) = checks::check_must_keep_coverage(policy, corpus) {
            violations.push(v);
        } else {
            checks_passed += 1;
        }

        // Check 4: Budget compliance
        if let Some(budget) = policy.budget_per_second {
            if let Some(max) = self.config.max_budget {
                if budget > max {
                    violations.push(Violation::critical(
                        "budget-compliance",
                        format!("Policy budget {budget} exceeds maximum {max}"),
                    ));
                } else {
                    checks_passed += 1;
                }
            } else {
                checks_passed += 1;
            }
        } else {
            checks_passed += 1;
        }

        if violations.is_empty() {
            Ok(ProverResult::approved(checks_passed))
        } else {
            Ok(ProverResult::rejected(
                violations,
                checks_passed,
                checks_total,
            ))
        }
    }

    /// Simulates a policy against a traffic pattern.
    ///
    /// This method replays the traffic pattern against the policy to verify
    /// budget compliance under realistic conditions.
    ///
    /// # Errors
    ///
    /// Returns an error if the traffic pattern is invalid.
    pub fn simulate_traffic(
        &self,
        policy: &Policy,
        traffic: &TrafficPattern,
    ) -> Result<SimulationResult> {
        if traffic.is_empty() {
            return Err(Error::InvalidTraffic("traffic pattern is empty".to_string()));
        }

        #[allow(clippy::cast_precision_loss)]
        let budget = self.config.max_budget.unwrap_or(u64::MAX) as f64;
        let simulator = Simulator::new(budget);

        Ok(simulator.simulate(policy, traffic))
    }

    /// Simulates a policy against a traffic pattern from a CSV file.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn simulate_traffic_file(
        &self,
        policy: &Policy,
        path: impl AsRef<Path>,
    ) -> Result<SimulationResult> {
        let traffic = TrafficPattern::from_csv_file(path)?;
        self.simulate_traffic(policy, &traffic)
    }

    /// Verifies a policy with traffic pattern simulation.
    ///
    /// Combines standard verification with traffic pattern replay.
    ///
    /// # Errors
    ///
    /// Returns an error if verification or simulation fails.
    pub fn verify_with_traffic(
        &self,
        policy: &Policy,
        corpus: &Corpus,
        traffic: &TrafficPattern,
    ) -> Result<(ProverResult, SimulationResult)> {
        let prover_result = self.verify(policy, corpus)?;
        let sim_result = self.simulate_traffic(policy, traffic)?;

        Ok((prover_result, sim_result))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toon_policy::{Action, Rule};

    fn valid_policy() -> Policy {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("keep-errors", "status >= 500", Action::Keep, 100));
        policy.add_rule(Rule::new("fallback", "true", Action::Sample(0.01), 0));
        policy
    }

    #[test]
    fn verify_valid_policy() {
        let prover = Prover::default();
        let policy = valid_policy();
        let corpus = Corpus::new();

        let result = prover.verify(&policy, &corpus).unwrap();
        assert!(result.is_approved());
    }

    #[test]
    fn verify_empty_policy_fails() {
        let prover = Prover::default();
        let policy = Policy::new("empty");
        let corpus = Corpus::new();

        let result = prover.verify(&policy, &corpus);
        assert!(result.is_err());
    }

    #[test]
    fn verify_policy_without_fallback() {
        let prover = Prover::default();
        let mut policy = Policy::new("no-fallback");
        policy.add_rule(Rule::new("errors", "error", Action::Keep, 100));
        let corpus = Corpus::new();

        let result = prover.verify(&policy, &corpus).unwrap();
        assert!(result.is_rejected());
    }
}
