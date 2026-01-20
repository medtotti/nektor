//! Main prover implementation.

use crate::analysis::{AnalysisMode, Confidence, StaticAnalysisResult, StaticAnalyzer};
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
    static_analyzer: StaticAnalyzer,
}

/// Configuration for the prover.
#[derive(Debug, Clone, Default)]
pub struct ProverConfig {
    /// Maximum allowed budget per second.
    pub max_budget: Option<u64>,
    /// Whether to require explicit error handling.
    pub require_error_handling: bool,
    /// Analysis mode to use.
    pub analysis_mode: AnalysisMode,
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
        Self {
            config,
            static_analyzer: StaticAnalyzer::new(),
        }
    }

    /// Returns the analysis mode.
    #[must_use]
    pub const fn analysis_mode(&self) -> AnalysisMode {
        self.config.analysis_mode
    }

    /// Performs static analysis only (fast path).
    ///
    /// This is O(rules) and suitable for rapid iteration.
    #[must_use]
    pub fn analyze_static(&self, policy: &Policy) -> StaticAnalysisResult {
        self.static_analyzer.analyze(policy)
    }

    /// Performs mode-aware analysis on a policy.
    ///
    /// - Static mode: Fast rule analysis only
    /// - Dynamic mode: Full traffic simulation (requires traffic pattern)
    /// - Auto mode: Static first, dynamic if traffic provided
    ///
    /// # Errors
    ///
    /// Returns an error if the policy is fundamentally invalid.
    pub fn analyze(
        &self,
        policy: &Policy,
        corpus: &Corpus,
        traffic: Option<&TrafficPattern>,
    ) -> Result<AnalysisResult> {
        let mode = self.config.analysis_mode;

        // Perform static analysis if needed
        let static_result = if mode.includes_static() {
            Some(self.analyze_static(policy))
        } else {
            None
        };

        // Perform verification (combines static checks with corpus)
        let prover_result = self.verify(policy, corpus)?;

        // Perform dynamic simulation if needed and traffic is available
        let simulation_result = if mode.includes_dynamic() {
            if let Some(traffic) = traffic {
                Some(self.simulate_traffic(policy, traffic)?)
            } else {
                None
            }
        } else {
            None
        };

        // Determine confidence level
        let confidence = determine_confidence(static_result.as_ref(), simulation_result.as_ref());

        Ok(AnalysisResult {
            mode,
            prover_result,
            static_result,
            simulation_result,
            confidence,
        })
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

    /// Replays corpus traces in timestamp order.
    ///
    /// This method replays traces from the corpus in chronological order
    /// to simulate historical traffic patterns and verify budget compliance.
    ///
    /// # Errors
    ///
    /// Returns an error if the corpus is empty.
    pub fn replay_corpus(
        &self,
        policy: &Policy,
        corpus: &Corpus,
        config: crate::replay::ReplayConfig,
    ) -> Result<crate::replay::ReplayResult> {
        let replayer = crate::replay::Replayer::new(config);
        replayer.replay(policy, corpus)
    }

    /// Replays corpus traces with default settings.
    ///
    /// Uses maximum speed and 1-second windows.
    ///
    /// # Errors
    ///
    /// Returns an error if the corpus is empty.
    pub fn replay_corpus_default(
        &self,
        policy: &Policy,
        corpus: &Corpus,
    ) -> Result<crate::replay::ReplayResult> {
        let config = crate::replay::ReplayConfig::new();
        self.replay_corpus(policy, corpus, config)
    }

    /// Replays corpus traces with budget checking.
    ///
    /// # Errors
    ///
    /// Returns an error if the corpus is empty.
    pub fn replay_corpus_with_budget(
        &self,
        policy: &Policy,
        corpus: &Corpus,
        budget_per_second: f64,
    ) -> Result<crate::replay::ReplayResult> {
        let config = crate::replay::ReplayConfig::new().with_budget(budget_per_second);
        self.replay_corpus(policy, corpus, config)
    }
}

/// Combined result from mode-aware analysis.
#[derive(Debug, Clone)]
pub struct AnalysisResult {
    /// Analysis mode used.
    pub mode: AnalysisMode,
    /// Standard prover result.
    pub prover_result: ProverResult,
    /// Static analysis result (if performed).
    pub static_result: Option<StaticAnalysisResult>,
    /// Simulation result (if performed).
    pub simulation_result: Option<SimulationResult>,
    /// Overall confidence level.
    pub confidence: Confidence,
}

impl AnalysisResult {
    /// Returns true if the policy is approved.
    #[must_use]
    pub const fn is_approved(&self) -> bool {
        self.prover_result.is_approved()
    }

    /// Returns true if static analysis passed.
    #[must_use]
    pub fn static_passed(&self) -> bool {
        self.static_result
            .as_ref()
            .map_or(true, |r| r.passed)
    }

    /// Returns true if dynamic simulation is compliant.
    #[must_use]
    pub fn simulation_compliant(&self) -> bool {
        self.simulation_result
            .as_ref()
            .map_or(true, SimulationResult::is_compliant)
    }

    /// Returns true if all checks passed.
    #[must_use]
    pub fn all_passed(&self) -> bool {
        self.is_approved() && self.static_passed() && self.simulation_compliant()
    }
}

/// Determines confidence level based on analysis results.
const fn determine_confidence(
    static_result: Option<&StaticAnalysisResult>,
    simulation_result: Option<&SimulationResult>,
) -> Confidence {
    // Dynamic simulation gives highest confidence
    if let Some(sim) = simulation_result {
        if sim.is_compliant() {
            return Confidence::High;
        }
    }

    // Static analysis gives medium confidence
    if let Some(static_res) = static_result {
        if static_res.passed {
            return Confidence::Medium;
        }
    }

    // Anything else is low confidence
    Confidence::Low
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

    #[test]
    fn analyze_static_mode() {
        let config = ProverConfig {
            analysis_mode: AnalysisMode::Static,
            ..Default::default()
        };
        let prover = Prover::new(config);
        let policy = valid_policy();
        let corpus = Corpus::new();

        let result = prover.analyze(&policy, &corpus, None).unwrap();

        assert!(result.is_approved());
        assert!(result.static_result.is_some());
        assert!(result.simulation_result.is_none());
        assert_eq!(result.confidence, Confidence::Medium);
    }

    #[test]
    fn analyze_dynamic_mode_without_traffic() {
        let config = ProverConfig {
            analysis_mode: AnalysisMode::Dynamic,
            ..Default::default()
        };
        let prover = Prover::new(config);
        let policy = valid_policy();
        let corpus = Corpus::new();

        let result = prover.analyze(&policy, &corpus, None).unwrap();

        assert!(result.is_approved());
        assert!(result.static_result.is_none());
        assert!(result.simulation_result.is_none());
        // Low confidence because no simulation without traffic
        assert_eq!(result.confidence, Confidence::Low);
    }

    #[test]
    fn analyze_auto_mode() {
        let config = ProverConfig {
            analysis_mode: AnalysisMode::Auto,
            ..Default::default()
        };
        let prover = Prover::new(config);
        let policy = valid_policy();
        let corpus = Corpus::new();

        let result = prover.analyze(&policy, &corpus, None).unwrap();

        assert!(result.is_approved());
        assert!(result.static_result.is_some());
        // No traffic provided, so no simulation
        assert!(result.simulation_result.is_none());
        assert_eq!(result.confidence, Confidence::Medium);
    }

    #[test]
    fn analyze_with_traffic_gives_high_confidence() {
        use chrono::{TimeZone, Utc};
        use crate::traffic::{TrafficPattern, TrafficPoint};

        let config = ProverConfig {
            analysis_mode: AnalysisMode::Auto,
            max_budget: Some(100_000),
            ..Default::default()
        };
        let prover = Prover::new(config);
        let policy = valid_policy();
        let corpus = Corpus::new();

        let base = Utc.with_ymd_and_hms(2024, 1, 15, 9, 0, 0).unwrap();
        let traffic = TrafficPattern::from_points(vec![
            TrafficPoint::new(base, 5000.0),
            TrafficPoint::new(base + chrono::Duration::minutes(1), 6000.0),
        ]);

        let result = prover.analyze(&policy, &corpus, Some(&traffic)).unwrap();

        assert!(result.is_approved());
        assert!(result.static_result.is_some());
        assert!(result.simulation_result.is_some());
        assert!(result.simulation_compliant());
        assert_eq!(result.confidence, Confidence::High);
    }

    #[test]
    fn analyze_all_passed() {
        let prover = Prover::default();
        let policy = valid_policy();
        let corpus = Corpus::new();

        let result = prover.analyze(&policy, &corpus, None).unwrap();

        assert!(result.all_passed());
    }

    #[test]
    fn static_analysis_on_valid_policy() {
        let prover = Prover::default();
        let policy = valid_policy();

        let result = prover.analyze_static(&policy);

        assert!(result.passed);
        assert!(result.coverage.has_fallback);
        assert!(result.coverage.has_error_handling);
    }
}
