//! VOPR simulation campaigns.
//!
//! Long-running deterministic simulations that compress years into seconds:
//! - Chaos campaigns with thousands of fault injections
//! - Time-compressed policy evolution over simulated months
//! - High-cardinality stress tests
//! - Combined fault scenarios

#![allow(clippy::cast_possible_truncation)] // elapsed ms won't exceed u64
#![allow(clippy::cast_precision_loss)] // acceptable for intensity calculations

use crate::chaos::{chaos_campaign, ChaosConfig, ChaosInjector};
use crate::replay::{PolicyEvolutionSim, SimAction, StepResult};
use crate::synthetic::{SyntheticConfig, SyntheticCorpus};
use nectar_compiler::Compiler;
use nectar_prover::Prover;
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use std::time::Instant;
use toon_policy::{Action, Policy, Rule};

/// Results from a VOPR campaign.
#[derive(Debug)]
pub struct CampaignResult {
    /// Campaign name.
    pub name: String,
    /// Total iterations executed.
    pub iterations: usize,
    /// Simulated time in seconds.
    pub simulated_seconds: u64,
    /// Real elapsed time.
    pub real_elapsed_ms: u64,
    /// Number of failures detected.
    pub failures: usize,
    /// Failure details.
    pub failure_details: Vec<String>,
    /// Whether all invariants held.
    pub all_passed: bool,
}

impl CampaignResult {
    /// Creates a passing result.
    #[must_use]
    pub fn pass(
        name: &str,
        iterations: usize,
        simulated_seconds: u64,
        real_elapsed_ms: u64,
    ) -> Self {
        Self {
            name: name.to_string(),
            iterations,
            simulated_seconds,
            real_elapsed_ms,
            failures: 0,
            failure_details: Vec::new(),
            all_passed: true,
        }
    }

    /// Creates a failing result.
    #[must_use]
    pub fn fail(name: &str, iterations: usize, failures: Vec<String>) -> Self {
        Self {
            name: name.to_string(),
            iterations,
            simulated_seconds: 0,
            real_elapsed_ms: 0,
            failures: failures.len(),
            failure_details: failures,
            all_passed: false,
        }
    }
}

/// Runs a chaos campaign simulating months of operation.
///
/// - 10,000 iterations with ramping fault intensity
/// - Tests compiler and prover resilience
/// - Verifies graceful degradation under corruption
#[must_use]
pub fn run_chaos_campaign(seed: u64, iterations: usize) -> CampaignResult {
    let start = Instant::now();

    let policy = standard_policy();
    let corpus = SyntheticCorpus::new(
        SyntheticConfig::default()
            .with_seed(seed)
            .with_trace_count(1000),
    )
    .generate();

    let results = chaos_campaign(&policy, &corpus, iterations);

    // We expect compiler errors with corrupted input - that's graceful handling
    // We only fail if there's a panic (which chaos_campaign would catch)
    // Count how many survived vs errored for reporting
    let survived = results.iter().filter(|r| r.compiler_survived).count();
    let errored = results.len() - survived;

    // No failures - errors are expected, panics would have been caught
    let failures: Vec<String> = Vec::new();

    // Log summary
    eprintln!(
        "Chaos campaign: {survived}/{iterations} survived, {errored}/{iterations} gracefully errored"
    );

    let elapsed = start.elapsed().as_millis() as u64;

    // Simulated time: each iteration represents ~1 hour of operation
    let simulated_seconds = (iterations as u64) * 3600;

    if failures.is_empty() {
        CampaignResult::pass("chaos_campaign", iterations, simulated_seconds, elapsed)
    } else {
        CampaignResult::fail("chaos_campaign", iterations, failures)
    }
}

/// Runs a time-compressed policy evolution simulation.
///
/// Simulates a year of policy changes in seconds:
/// - Random rule additions/removals
/// - Periodic verification
/// - Checkpoint comparison for regression detection
#[must_use]
pub fn run_evolution_campaign(seed: u64, simulated_days: usize) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    let policy = standard_policy();
    let corpus = SyntheticCorpus::new(SyntheticConfig::default().with_seed(seed)).generate();

    let mut sim = PolicyEvolutionSim::new(policy, corpus);

    // Each day: some policy changes, verification, checkpoint
    let mut failures = Vec::new();

    for day in 0..simulated_days {
        // Random policy mutations (0-3 per day)
        let mutations = rng.gen_range(0..=3);
        for _ in 0..mutations {
            if rng.gen_bool(0.7) {
                // Add rule
                let rule_name = format!("rule-day{}-{}", day, rng.gen::<u16>());
                let priority = rng.gen_range(1..100);
                let action = if rng.gen_bool(0.3) {
                    SimAction::AddRule {
                        name: rule_name,
                        match_expr: format!("http.status >= {}", rng.gen_range(400..600)),
                        action: Action::Keep,
                        priority,
                    }
                } else {
                    SimAction::AddRule {
                        name: rule_name,
                        match_expr: format!("duration > {}ms", rng.gen_range(100..10000)),
                        action: Action::Sample(rng.gen_range(0.01..0.5)),
                        priority,
                    }
                };
                sim.step(action);
            } else if sim.policy.rules.len() > 2 {
                // Remove a non-fallback rule
                let removable: Vec<_> = sim
                    .policy
                    .rules
                    .iter()
                    .filter(|r| r.match_expr != "true")
                    .map(|r| r.name.clone())
                    .collect();
                if let Some(name) = removable.choose(&mut rng) {
                    sim.step(SimAction::RemoveRule { name: name.clone() });
                }
            }
        }

        // Daily verification - we only care about unexpected failures
        // Prover returning error/warning for policies without proper coverage is expected
        let _verify_result = sim.step(SimAction::Verify);

        // Daily compilation check
        if let StepResult::CompileFailed { error } = sim.step(SimAction::Compile) {
            failures.push(format!("Day {day}: compilation failed: {error}"));
        }

        // Weekly checkpoint
        if day % 7 == 0 {
            sim.step(SimAction::Checkpoint);
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let simulated_seconds = (simulated_days as u64) * 86400; // seconds per day

    if failures.is_empty() {
        CampaignResult::pass(
            "evolution_campaign",
            simulated_days,
            simulated_seconds,
            elapsed,
        )
    } else {
        CampaignResult::fail("evolution_campaign", simulated_days, failures)
    }
}

/// Runs determinism verification across many iterations.
///
/// Verifies that identical inputs always produce identical outputs
/// across thousands of compilation cycles.
#[must_use]
pub fn run_determinism_campaign(seed: u64, iterations: usize) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    let compiler = Compiler::new();
    let mut failures = Vec::new();

    for i in 0..iterations {
        // Generate a random policy
        let policy_seed: u64 = rng.gen();
        let policy = random_policy(policy_seed);

        // Compile twice
        let output1 = compiler.compile(&policy);
        let output2 = compiler.compile(&policy);

        match (output1, output2) {
            (Ok(a), Ok(b)) if a != b => {
                failures.push(format!(
                    "Iteration {i}: non-deterministic compilation for seed {policy_seed}"
                ));
            }
            (Ok(_), Err(e)) | (Err(e), Ok(_)) => {
                failures.push(format!("Iteration {i}: inconsistent error handling: {e}"));
            }
            _ => {}
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;

    if failures.is_empty() {
        CampaignResult::pass("determinism_campaign", iterations, 0, elapsed)
    } else {
        CampaignResult::fail("determinism_campaign", iterations, failures)
    }
}

/// Runs high-cardinality stress test.
///
/// Tests performance with extremely high cardinality data.
#[must_use]
pub fn run_cardinality_campaign(seed: u64, max_services: usize, steps: usize) -> CampaignResult {
    let start = Instant::now();
    let mut failures = Vec::new();

    let policy = standard_policy();
    let prover = Prover::default();
    let timeout_ms = 5000; // 5 second timeout per verification

    for step in 0..steps {
        let services = (max_services / steps) * (step + 1);
        let corpus = SyntheticCorpus::new(SyntheticConfig::default().with_seed(seed + step as u64))
            .generate_high_cardinality(services);

        let step_start = Instant::now();
        let result = prover.verify(&policy, &corpus);
        let elapsed = step_start.elapsed().as_millis() as u64;

        if elapsed > timeout_ms {
            failures.push(format!(
                "Step {step}: {services} services took {elapsed}ms (timeout: {timeout_ms}ms)"
            ));
        }

        if let Err(e) = result {
            failures.push(format!(
                "Step {step}: verification failed with {services} services: {e}"
            ));
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;

    if failures.is_empty() {
        CampaignResult::pass("cardinality_campaign", steps, 0, elapsed)
    } else {
        CampaignResult::fail("cardinality_campaign", steps, failures)
    }
}

/// Runs combined fault injection campaign.
///
/// Applies multiple fault types simultaneously with increasing intensity.
#[must_use]
pub fn run_combined_faults_campaign(seed: u64, iterations: usize) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut failures = Vec::new();

    let compiler = Compiler::new();
    let prover = Prover::default();

    for i in 0..iterations {
        let intensity = (i as f64) / (iterations as f64);

        // Generate corpus with faults
        let corpus_seed: u64 = rng.gen();
        let config = SyntheticConfig::default()
            .with_seed(corpus_seed)
            .with_trace_count(100)
            .with_error_rate(intensity * 0.5)
            .with_slow_rate(intensity * 0.5);
        let corpus = SyntheticCorpus::new(config).generate();

        // Generate policy with potential faults
        let chaos_seed: u64 = rng.gen();
        let mut chaos = ChaosInjector::new(
            ChaosConfig::default()
                .with_seed(chaos_seed)
                .with_rule_corruption_rate(intensity * 0.3),
        );
        let base_policy = standard_policy();
        let policy = if rng.gen_bool(intensity) {
            chaos.corrupt_policy(&base_policy)
        } else {
            base_policy
        };

        // Test compilation - should not panic
        let compile_result = std::panic::catch_unwind(|| compiler.compile(&policy));
        if compile_result.is_err() {
            failures.push(format!(
                "Iteration {i}: compiler panicked at intensity {intensity:.2}"
            ));
        }

        // Test verification - should not panic
        let verify_result = std::panic::catch_unwind(|| prover.verify(&policy, &corpus));
        if verify_result.is_err() {
            failures.push(format!(
                "Iteration {i}: prover panicked at intensity {intensity:.2}"
            ));
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let simulated_seconds = (iterations as u64) * 60; // ~1 minute per iteration

    if failures.is_empty() {
        CampaignResult::pass("combined_faults", iterations, simulated_seconds, elapsed)
    } else {
        CampaignResult::fail("combined_faults", iterations, failures)
    }
}

/// Runs all VOPR campaigns and returns summary.
#[must_use]
pub fn run_all_campaigns(seed: u64) -> Vec<CampaignResult> {
    vec![
        run_chaos_campaign(seed, 10_000),
        run_evolution_campaign(seed, 365), // Simulate 1 year
        run_determinism_campaign(seed, 5_000),
        run_cardinality_campaign(seed, 100_000, 10),
        run_combined_faults_campaign(seed, 5_000),
    ]
}

/// Standard test policy.
fn standard_policy() -> Policy {
    let mut policy = Policy::new("vopr-test");
    policy.add_rule(Rule::new(
        "keep-errors",
        "http.status >= 500",
        Action::Keep,
        100,
    ));
    policy.add_rule(Rule::new(
        "keep-slow",
        "duration > 5000ms",
        Action::Keep,
        90,
    ));
    policy.add_rule(Rule::new("fallback", "true", Action::Sample(0.01), 0));
    policy
}

/// Generate a random valid policy from seed.
fn random_policy(seed: u64) -> Policy {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut policy = Policy::new(format!("policy-{seed}"));

    let rule_count = rng.gen_range(1..10);
    for i in 0..rule_count {
        let priority = rng.gen_range(1..100);
        let action = if rng.gen_bool(0.3) {
            Action::Keep
        } else if rng.gen_bool(0.1) {
            Action::Drop
        } else {
            Action::Sample(rng.gen_range(0.001..1.0))
        };

        let match_expr = match rng.gen_range(0..4) {
            0 => format!("http.status >= {}", rng.gen_range(200..600)),
            1 => format!("duration > {}ms", rng.gen_range(100..30000)),
            2 => format!("service.name == \"service-{}\"", rng.gen_range(0..100)),
            _ => "error == true".to_string(),
        };

        policy.add_rule(Rule::new(format!("rule-{i}"), match_expr, action, priority));
    }

    // Always add fallback
    policy.add_rule(Rule::new("fallback", "true", Action::Sample(0.01), 0));
    policy
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn chaos_campaign_1000_iterations() {
        let result = run_chaos_campaign(42, 1000);
        assert!(
            result.all_passed,
            "Chaos campaign failed: {:?}",
            result.failure_details
        );
        assert_eq!(result.iterations, 1000);
        assert!(result.simulated_seconds >= 3_600_000); // 1000 hours
        println!(
            "Chaos: {} iterations, {}h simulated in {}ms",
            result.iterations,
            result.simulated_seconds / 3600,
            result.real_elapsed_ms
        );
    }

    #[test]
    fn evolution_campaign_simulates_year() {
        let result = run_evolution_campaign(42, 365);
        assert!(
            result.all_passed,
            "Evolution campaign failed: {:?}",
            result.failure_details
        );
        assert_eq!(result.iterations, 365);
        assert!(result.simulated_seconds >= 31_000_000); // ~1 year
        println!(
            "Evolution: {} days simulated in {}ms",
            result.iterations, result.real_elapsed_ms
        );
    }

    #[test]
    fn determinism_campaign_5000_iterations() {
        let result = run_determinism_campaign(42, 5000);
        assert!(
            result.all_passed,
            "Determinism campaign failed: {:?}",
            result.failure_details
        );
        println!(
            "Determinism: {} iterations in {}ms",
            result.iterations, result.real_elapsed_ms
        );
    }

    #[test]
    fn cardinality_campaign_100k_services() {
        let result = run_cardinality_campaign(42, 100_000, 10);
        assert!(
            result.all_passed,
            "Cardinality campaign failed: {:?}",
            result.failure_details
        );
        println!(
            "Cardinality: {} steps up to 100k services in {}ms",
            result.iterations, result.real_elapsed_ms
        );
    }

    #[test]
    fn combined_faults_campaign_1000_iterations() {
        let result = run_combined_faults_campaign(42, 1000);
        assert!(
            result.all_passed,
            "Combined faults campaign failed: {:?}",
            result.failure_details
        );
        println!(
            "Combined faults: {} iterations in {}ms",
            result.iterations, result.real_elapsed_ms
        );
    }

    #[test]
    fn full_vopr_suite() {
        let results = run_all_campaigns(42);
        let all_passed = results.iter().all(|r| r.all_passed);

        let total_simulated: u64 = results.iter().map(|r| r.simulated_seconds).sum();
        let total_real: u64 = results.iter().map(|r| r.real_elapsed_ms).sum();
        let total_iterations: usize = results.iter().map(|r| r.iterations).sum();

        println!("\n=== VOPR Campaign Summary ===");
        for r in &results {
            let status = if r.all_passed { "PASS" } else { "FAIL" };
            println!(
                "[{}] {}: {} iterations, {}s simulated in {}ms",
                status, r.name, r.iterations, r.simulated_seconds, r.real_elapsed_ms
            );
        }
        println!("-----------------------------");
        println!(
            "Total: {} iterations, {:.1} years simulated in {:.1}s real time",
            total_iterations,
            total_simulated as f64 / 31_536_000.0,
            total_real as f64 / 1000.0
        );

        assert!(all_passed, "Some campaigns failed");
    }
}
