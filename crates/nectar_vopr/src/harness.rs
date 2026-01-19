//! Deterministic simulation harness.
//!
//! Provides a controlled environment for testing Nectar components
//! with reproducible randomness and time simulation.

use crate::chaos::{ChaosConfig, ChaosInjector};
use crate::simulation::{Scenario, SimResult};
use crate::synthetic::{SyntheticConfig, SyntheticCorpus};
use nectar_compiler::Compiler;
use nectar_corpus::Corpus;
use nectar_prover::{Prover, ProverConfig};
use rand::SeedableRng;
use rand_chacha::ChaCha8Rng;
use std::time::{Duration, Instant};
use toon_policy::Policy;
use xxhash_rust::xxh64::xxh64;

/// Configuration for the simulation.
#[derive(Debug, Clone)]
pub struct SimConfig {
    /// Master seed for all randomness.
    pub seed: u64,
    /// Number of iterations to run.
    pub iterations: usize,
    /// Whether to enable chaos injection.
    pub chaos_enabled: bool,
    /// Chaos configuration.
    pub chaos_config: ChaosConfig,
    /// Synthetic corpus configuration.
    pub corpus_config: SyntheticConfig,
    /// Whether to verify determinism.
    pub verify_determinism: bool,
    /// Maximum duration for a single scenario.
    pub timeout: Duration,
}

impl Default for SimConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            iterations: 100,
            chaos_enabled: false,
            chaos_config: ChaosConfig::default(),
            corpus_config: SyntheticConfig::default(),
            verify_determinism: true,
            timeout: Duration::from_secs(30),
        }
    }
}

impl SimConfig {
    /// Creates a new config with the given seed.
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Sets the number of iterations.
    #[must_use]
    pub const fn with_iterations(mut self, iterations: usize) -> Self {
        self.iterations = iterations;
        self
    }

    /// Enables chaos injection.
    #[must_use]
    pub fn with_chaos(mut self, config: ChaosConfig) -> Self {
        self.chaos_enabled = true;
        self.chaos_config = config;
        self
    }

    /// Disables determinism verification (for chaos testing).
    #[must_use]
    pub const fn without_determinism_check(mut self) -> Self {
        self.verify_determinism = false;
        self
    }
}

/// Simulation harness for deterministic testing.
pub struct Simulation {
    config: SimConfig,
    #[allow(dead_code)]
    rng: ChaCha8Rng,
    chaos: Option<ChaosInjector>,
    #[allow(dead_code)]
    results: Vec<SimResult>,
}

impl Simulation {
    /// Creates a new simulation with the given configuration.
    #[must_use]
    pub fn new(config: SimConfig) -> Self {
        let rng = ChaCha8Rng::seed_from_u64(config.seed);
        let chaos = if config.chaos_enabled {
            Some(ChaosInjector::new(config.chaos_config.clone()))
        } else {
            None
        };

        Self {
            config,
            rng,
            chaos,
            results: Vec::new(),
        }
    }

    /// Runs a scenario and returns the result.
    pub fn run_scenario(&mut self, scenario: &Scenario) -> SimResult {
        let _start = Instant::now();

        match scenario {
            Scenario::CompileDeterminism { policy } => self.test_compile_determinism(policy),
            Scenario::ProverConsistency { policy, corpus } => {
                self.test_prover_consistency(policy, corpus)
            }
            Scenario::RoundTrip { policy } => self.test_roundtrip(policy),
            Scenario::ChaosResilience { policy, corpus } => {
                self.test_chaos_resilience(policy, corpus)
            }
            Scenario::HighCardinality { unique_services } => {
                self.test_high_cardinality(*unique_services)
            }
        }
    }

    #[allow(clippy::unused_self)]
    fn test_compile_determinism(&self, policy: &Policy) -> SimResult {
        let compiler = Compiler::new();

        // Compile multiple times
        let outputs: Vec<String> = (0..10)
            .map(|_| compiler.compile(policy).unwrap_or_default())
            .collect();

        // All outputs should be identical
        let first_hash = xxh64(outputs[0].as_bytes(), 0);
        let all_same = outputs.iter().all(|o| xxh64(o.as_bytes(), 0) == first_hash);

        if all_same {
            SimResult::pass(
                "compile_determinism",
                "All compilations produced identical output",
            )
        } else {
            SimResult::fail(
                "compile_determinism",
                "Compilation produced different outputs",
            )
        }
    }

    #[allow(clippy::unused_self)]
    fn test_prover_consistency(&self, policy: &Policy, corpus: &Corpus) -> SimResult {
        let prover = Prover::new(ProverConfig {
            require_error_handling: true,
            ..Default::default()
        });

        // Verify multiple times
        let results: Vec<_> = (0..10).map(|_| prover.verify(policy, corpus)).collect();

        // All results should be identical
        let first = &results[0];
        let all_same = results.iter().all(|r| match (r, first) {
            (Ok(a), Ok(b)) => a == b,
            (Err(_), Err(_)) => true,
            _ => false,
        });

        if all_same {
            SimResult::pass(
                "prover_consistency",
                "All verifications produced identical results",
            )
        } else {
            SimResult::fail(
                "prover_consistency",
                "Verification produced inconsistent results",
            )
        }
    }

    #[allow(clippy::unused_self)]
    fn test_roundtrip(&self, policy: &Policy) -> SimResult {
        // Serialize to TOON
        let toon = toon_policy::serialize(policy);

        // Parse back
        let parsed = match toon_policy::parse(&toon) {
            Ok(p) => p,
            Err(e) => {
                return SimResult::fail(
                    "roundtrip",
                    format!("Failed to parse serialized TOON: {e}"),
                );
            }
        };

        // Serialize again
        let toon2 = toon_policy::serialize(&parsed);

        // Should be identical
        if toon == toon2 {
            SimResult::pass("roundtrip", "Policy survives roundtrip serialization")
        } else {
            SimResult::fail("roundtrip", "Policy changed after roundtrip serialization")
        }
    }

    fn test_chaos_resilience(&mut self, policy: &Policy, corpus: &Corpus) -> SimResult {
        let Some(chaos) = &mut self.chaos else {
            return SimResult::skip("chaos_resilience", "Chaos injection not enabled");
        };

        let prover = Prover::new(ProverConfig::default());
        let compiler = Compiler::new();

        // Apply chaos to corpus
        let chaotic_corpus = chaos.corrupt_corpus(corpus);

        // Prover should handle corrupted data gracefully
        let verify_result = prover.verify(policy, &chaotic_corpus);

        // Compiler should still work
        let compile_result = compiler.compile(policy);

        match (verify_result, compile_result) {
            (Ok(_), Ok(_)) => {
                SimResult::pass("chaos_resilience", "System handled chaos gracefully")
            }
            (Err(e), _) => {
                // Prover errors are acceptable under chaos
                SimResult::pass(
                    "chaos_resilience",
                    format!("Prover correctly rejected chaotic input: {e}"),
                )
            }
            (_, Err(e)) => SimResult::fail(
                "chaos_resilience",
                format!("Compiler failed under chaos: {e}"),
            ),
        }
    }

    fn test_high_cardinality(&self, unique_services: usize) -> SimResult {
        let config = self
            .config
            .corpus_config
            .clone()
            .with_seed(self.config.seed);
        let mut gen = SyntheticCorpus::new(config);
        let corpus = gen.generate_high_cardinality(unique_services);

        // Create a policy that references high cardinality
        let mut policy = Policy::new("high-card-test");
        policy.add_rule(toon_policy::Rule::new(
            "keep-errors",
            "http.status >= 500",
            toon_policy::Action::Keep,
            100,
        ));
        policy.add_rule(toon_policy::Rule::new(
            "fallback",
            "true",
            toon_policy::Action::Sample(0.01),
            0,
        ));

        let prover = Prover::new(ProverConfig::default());
        let start = Instant::now();
        let result = prover.verify(&policy, &corpus);
        let elapsed = start.elapsed();

        // Should complete within timeout
        if elapsed > self.config.timeout {
            return SimResult::fail(
                "high_cardinality",
                format!("Verification took too long: {elapsed:?}"),
            );
        }

        match result {
            Ok(_) => SimResult::pass(
                "high_cardinality",
                format!("Handled {unique_services} unique services in {elapsed:?}"),
            ),
            Err(e) => SimResult::fail("high_cardinality", format!("Verification failed: {e}")),
        }
    }

    /// Runs all scenarios and returns aggregated results.
    pub fn run_all(&mut self, scenarios: &[Scenario]) -> Vec<SimResult> {
        scenarios.iter().map(|s| self.run_scenario(s)).collect()
    }

    /// Verifies that running the same simulation twice produces identical results.
    pub fn verify_determinism(&self) -> bool {
        if !self.config.verify_determinism {
            return true;
        }

        let corpus1 = SyntheticCorpus::new(self.config.corpus_config.clone()).generate();
        let corpus2 = SyntheticCorpus::new(self.config.corpus_config.clone()).generate();

        // Compare corpus hashes
        let hash1 = corpus_hash(&corpus1);
        let hash2 = corpus_hash(&corpus2);

        hash1 == hash2
    }
}

fn corpus_hash(corpus: &Corpus) -> u64 {
    let mut hasher_input = String::new();
    for trace in corpus.iter() {
        hasher_input.push_str(&trace.trace_id);
        if let Some(service) = &trace.service {
            hasher_input.push_str(service);
        }
        if let Some(status) = trace.status {
            hasher_input.push_str(&status.to_string());
        }
    }
    xxh64(hasher_input.as_bytes(), 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn simulation_is_deterministic() {
        let config = SimConfig::default().with_seed(12345);
        let sim = Simulation::new(config);
        assert!(sim.verify_determinism());
    }

    #[test]
    fn compile_determinism_test_passes() {
        let mut sim = Simulation::new(SimConfig::default());

        let mut policy = Policy::new("test");
        policy.add_rule(toon_policy::Rule::new(
            "keep-errors",
            "http.status >= 500",
            toon_policy::Action::Keep,
            100,
        ));
        policy.add_rule(toon_policy::Rule::new(
            "fallback",
            "true",
            toon_policy::Action::Sample(0.01),
            0,
        ));

        let result = sim.run_scenario(&Scenario::CompileDeterminism { policy });
        assert!(result.passed, "Result: {result:?}");
    }
}
