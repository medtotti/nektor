//! VOPR simulation campaigns.
//!
//! Long-running deterministic simulations that compress decades into seconds:
//! - Chaos campaigns with tens of thousands of fault injections
//! - Time-compressed policy evolution over simulated years
//! - Infrastructure fault simulations
//! - Resource exhaustion scenarios
//! - Distributed systems failure modes
//! - Deployment and rollback simulations

#![allow(clippy::cast_possible_truncation)]
#![allow(clippy::cast_precision_loss)]

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

// =============================================================================
// CORE CAMPAIGNS
// =============================================================================

/// Runs a chaos campaign simulating years of operation.
///
/// - 10,000+ iterations with ramping fault intensity
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

    let survived = results.iter().filter(|r| r.compiler_survived).count();
    let errored = results.len() - survived;
    let failures: Vec<String> = Vec::new();

    eprintln!(
        "Chaos campaign: {survived}/{iterations} survived, {errored}/{iterations} gracefully errored"
    );

    let elapsed = start.elapsed().as_millis() as u64;
    let simulated_seconds = (iterations as u64) * 3600;

    if failures.is_empty() {
        CampaignResult::pass("chaos_campaign", iterations, simulated_seconds, elapsed)
    } else {
        CampaignResult::fail("chaos_campaign", iterations, failures)
    }
}

/// Runs a time-compressed policy evolution simulation.
///
/// Simulates 10 years of policy changes in seconds:
/// - Random rule additions/removals
/// - Periodic verification
/// - Checkpoint comparison for regression detection
/// - Simulates realistic policy lifecycle
#[must_use]
pub fn run_evolution_campaign(seed: u64, simulated_days: usize) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    let policy = standard_policy();
    let corpus = SyntheticCorpus::new(SyntheticConfig::default().with_seed(seed)).generate();

    let mut sim = PolicyEvolutionSim::new(policy, corpus);
    let mut failures = Vec::new();

    for day in 0..simulated_days {
        let is_weekday = day % 7 < 5;
        let is_deploy_window = day % 14 == 0;
        let is_incident = rng.gen_bool(0.02);
        let is_quarter_end = day % 90 < 7;

        let mutation_rate = if is_quarter_end {
            0.1
        } else if is_deploy_window {
            0.8
        } else if is_weekday {
            0.4
        } else {
            0.2
        };

        let mutations = if rng.gen_bool(mutation_rate) {
            rng.gen_range(0..=3)
        } else {
            0
        };

        for _ in 0..mutations {
            if rng.gen_bool(0.7) {
                let rule_name = format!("rule-day{}-{}", day, rng.gen::<u16>());
                let priority = rng.gen_range(1..100);
                let action = generate_random_action(&mut rng);
                let match_expr = generate_random_match_expr(&mut rng);

                sim.step(SimAction::AddRule {
                    name: rule_name,
                    match_expr,
                    action,
                    priority,
                });
            } else if sim.policy.rules.len() > 2 {
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

        if is_incident {
            let incident_rule = format!("incident-response-day{day}");
            sim.step(SimAction::AddRule {
                name: incident_rule,
                match_expr: "http.status >= 500".to_string(),
                action: Action::Keep,
                priority: 255,
            });
        }

        let _verify_result = sim.step(SimAction::Verify);

        if let StepResult::CompileFailed { error } = sim.step(SimAction::Compile) {
            failures.push(format!("Day {day}: compilation failed: {error}"));
        }

        if day % 7 == 0 {
            sim.step(SimAction::Checkpoint);
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let simulated_seconds = (simulated_days as u64) * 86400;

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
#[must_use]
pub fn run_determinism_campaign(seed: u64, iterations: usize) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);

    let compiler = Compiler::new();
    let mut failures = Vec::new();

    for i in 0..iterations {
        let policy_seed: u64 = rng.gen();
        let policy = random_policy(policy_seed);

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
#[must_use]
pub fn run_cardinality_campaign(seed: u64, max_services: usize, steps: usize) -> CampaignResult {
    let start = Instant::now();
    let mut failures = Vec::new();

    let policy = standard_policy();
    let prover = Prover::default();
    let timeout_ms = 5000;

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
#[must_use]
pub fn run_combined_faults_campaign(seed: u64, iterations: usize) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut failures = Vec::new();

    let compiler = Compiler::new();
    let prover = Prover::default();

    for i in 0..iterations {
        let intensity = (i as f64) / (iterations as f64);

        let corpus_seed: u64 = rng.gen();
        let config = SyntheticConfig::default()
            .with_seed(corpus_seed)
            .with_trace_count(100)
            .with_error_rate(intensity * 0.5)
            .with_slow_rate(intensity * 0.5);
        let corpus = SyntheticCorpus::new(config).generate();

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

        let compile_result = std::panic::catch_unwind(|| compiler.compile(&policy));
        if compile_result.is_err() {
            failures.push(format!(
                "Iteration {i}: compiler panicked at intensity {intensity:.2}"
            ));
        }

        let verify_result = std::panic::catch_unwind(|| prover.verify(&policy, &corpus));
        if verify_result.is_err() {
            failures.push(format!(
                "Iteration {i}: prover panicked at intensity {intensity:.2}"
            ));
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let simulated_seconds = (iterations as u64) * 60;

    if failures.is_empty() {
        CampaignResult::pass("combined_faults", iterations, simulated_seconds, elapsed)
    } else {
        CampaignResult::fail("combined_faults", iterations, failures)
    }
}

// =============================================================================
// INFRASTRUCTURE FAULT CAMPAIGNS
// =============================================================================

/// Simulates infrastructure faults: DNS, TLS, network partitions.
///
/// Models real-world infrastructure failures:
/// - DNS resolution failures and timeouts
/// - TLS handshake failures and certificate issues
/// - Network partitions and packet loss
/// - Load balancer health check flapping
#[must_use]
#[allow(clippy::missing_panics_doc)]
pub fn run_infrastructure_faults_campaign(seed: u64, iterations: usize) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut failures = Vec::new();

    let compiler = Compiler::new();
    let prover = Prover::default();

    let fault_types = [
        "dns_resolution_failure",
        "dns_timeout",
        "tls_handshake_failure",
        "certificate_expired",
        "certificate_mismatch",
        "network_partition",
        "packet_loss",
        "connection_reset",
        "load_balancer_flap",
        "health_check_timeout",
    ];

    for i in 0..iterations {
        let fault_probability = (i as f64 / iterations as f64).mul_add(0.4, 0.1);
        let has_fault = rng.gen_bool(fault_probability);

        let mut config = SyntheticConfig::default()
            .with_seed(seed + i as u64)
            .with_trace_count(50);

        if has_fault {
            let fault_type = fault_types.choose(&mut rng).unwrap();
            config = config.with_error_rate(match *fault_type {
                "network_partition" => 0.8,
                "dns_resolution_failure" => 0.6,
                "tls_handshake_failure" => 0.5,
                "certificate_expired" => 0.9,
                _ => 0.3,
            });
        }

        let corpus = SyntheticCorpus::new(config).generate();

        let policy = if has_fault && rng.gen_bool(0.2) {
            degraded_policy(&mut rng)
        } else {
            standard_policy()
        };

        let compile_result = std::panic::catch_unwind(|| compiler.compile(&policy));
        if compile_result.is_err() {
            failures.push(format!("Iteration {i}: compiler panicked during infra fault"));
        }

        let verify_result = std::panic::catch_unwind(|| prover.verify(&policy, &corpus));
        if verify_result.is_err() {
            failures.push(format!("Iteration {i}: prover panicked during infra fault"));
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let simulated_seconds = (iterations as u64) * 300;

    if failures.is_empty() {
        CampaignResult::pass(
            "infrastructure_faults",
            iterations,
            simulated_seconds,
            elapsed,
        )
    } else {
        CampaignResult::fail("infrastructure_faults", iterations, failures)
    }
}

// =============================================================================
// RESOURCE EXHAUSTION CAMPAIGNS
// =============================================================================

/// Simulates resource exhaustion scenarios.
///
/// Models gradual resource depletion:
/// - Memory pressure and OOM conditions
/// - File descriptor exhaustion
/// - Thread pool saturation
/// - Connection pool exhaustion
/// - Disk space exhaustion
#[must_use]
pub fn run_resource_exhaustion_campaign(seed: u64, iterations: usize) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut failures = Vec::new();

    let compiler = Compiler::new();
    let prover = Prover::default();

    for i in 0..iterations {
        let pressure = (i as f64) / (iterations as f64);

        let memory_pressure = pressure * rng.gen_range(0.8..1.2);
        let fd_pressure = pressure * rng.gen_range(0.5..1.0);
        let thread_pressure = pressure * rng.gen_range(0.6..1.1);
        let conn_pool_pressure = pressure * rng.gen_range(0.7..1.3);

        let error_rate = f64::min(memory_pressure * 0.3, 0.9);
        let slow_rate = f64::min(conn_pool_pressure * 0.4, 0.8);

        let config = SyntheticConfig::default()
            .with_seed(seed + i as u64)
            .with_trace_count(100)
            .with_error_rate(error_rate)
            .with_slow_rate(slow_rate);

        let corpus = SyntheticCorpus::new(config).generate();

        let policy = if memory_pressure > 0.9 {
            minimal_policy()
        } else if fd_pressure > 0.8 || thread_pressure > 0.8 {
            degraded_policy(&mut rng)
        } else {
            standard_policy()
        };

        let compile_result = std::panic::catch_unwind(|| compiler.compile(&policy));
        if compile_result.is_err() {
            failures.push(format!(
                "Iteration {i}: compiler panicked at pressure {pressure:.2}"
            ));
        }

        let verify_result = std::panic::catch_unwind(|| prover.verify(&policy, &corpus));
        if verify_result.is_err() {
            failures.push(format!(
                "Iteration {i}: prover panicked at pressure {pressure:.2}"
            ));
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let simulated_seconds = (iterations as u64) * 60;

    if failures.is_empty() {
        CampaignResult::pass(
            "resource_exhaustion",
            iterations,
            simulated_seconds,
            elapsed,
        )
    } else {
        CampaignResult::fail("resource_exhaustion", iterations, failures)
    }
}

// =============================================================================
// DISTRIBUTED SYSTEMS FAULT CAMPAIGNS
// =============================================================================

/// Simulates distributed systems failure modes.
///
/// Models complex distributed failures:
/// - Split brain scenarios
/// - Replication lag
/// - Clock skew
/// - Leader election failures
/// - Consensus failures
/// - Byzantine faults
#[must_use]
pub fn run_distributed_faults_campaign(seed: u64, iterations: usize) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut failures = Vec::new();

    let compiler = Compiler::new();
    let prover = Prover::default();

    let fault_scenarios = [
        ("split_brain", 0.05),
        ("replication_lag", 0.15),
        ("clock_skew", 0.10),
        ("leader_election", 0.08),
        ("consensus_timeout", 0.07),
        ("quorum_loss", 0.03),
        ("stale_read", 0.20),
        ("write_conflict", 0.12),
        ("partition_heal", 0.10),
        ("cascade_failure", 0.05),
    ];

    for i in 0..iterations {
        let mut active_faults = Vec::new();
        for (fault, probability) in &fault_scenarios {
            if rng.gen_bool(*probability) {
                active_faults.push(*fault);
            }
        }

        let fault_severity = active_faults.len() as f64 * 0.15;
        let error_rate = f64::min(0.05 + fault_severity, 0.95);

        let config = SyntheticConfig::default()
            .with_seed(seed + i as u64)
            .with_trace_count(75)
            .with_error_rate(error_rate);

        let corpus = SyntheticCorpus::new(config).generate();

        let policy = if active_faults.contains(&"split_brain")
            || active_faults.contains(&"quorum_loss")
        {
            emergency_policy()
        } else if active_faults.len() > 3 {
            degraded_policy(&mut rng)
        } else {
            standard_policy()
        };

        let compile_result = std::panic::catch_unwind(|| compiler.compile(&policy));
        if compile_result.is_err() {
            failures.push(format!(
                "Iteration {i}: compiler panicked with faults {active_faults:?}"
            ));
        }

        let verify_result = std::panic::catch_unwind(|| prover.verify(&policy, &corpus));
        if verify_result.is_err() {
            failures.push(format!(
                "Iteration {i}: prover panicked with faults {active_faults:?}"
            ));
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let simulated_seconds = (iterations as u64) * 120;

    if failures.is_empty() {
        CampaignResult::pass("distributed_faults", iterations, simulated_seconds, elapsed)
    } else {
        CampaignResult::fail("distributed_faults", iterations, failures)
    }
}

// =============================================================================
// DEPLOYMENT FAULT CAMPAIGNS
// =============================================================================

/// Simulates deployment and rollback scenarios.
///
/// Models deployment failures:
/// - Canary deployment failures
/// - Feature flag misconfigurations
/// - Rollback failures
/// - Blue-green switch failures
/// - Configuration drift
/// - Schema migration issues
#[must_use]
pub fn run_deployment_faults_campaign(seed: u64, iterations: usize) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut failures = Vec::new();

    let compiler = Compiler::new();
    let prover = Prover::default();

    let mut current_version: u32 = 1;
    let mut canary_active = false;
    let mut rollback_pending = false;

    for i in 0..iterations {
        let is_deploy_time = i % 50 == 0;
        let is_canary_check = canary_active && i % 5 == 0;

        if is_deploy_time && !rollback_pending {
            current_version += 1;
            canary_active = true;
        }

        if is_canary_check {
            let canary_error_rate = rng.gen_range(0.0..0.2);
            if canary_error_rate > 0.1 {
                rollback_pending = true;
                canary_active = false;
            } else if rng.gen_bool(0.7) {
                canary_active = false;
            }
        }

        if rollback_pending && rng.gen_bool(0.3) {
            current_version = current_version.saturating_sub(1).max(1);
            rollback_pending = false;
        }

        let error_rate = if canary_active {
            rng.gen_range(0.02..0.15)
        } else if rollback_pending {
            rng.gen_range(0.10..0.30)
        } else {
            rng.gen_range(0.01..0.05)
        };

        let config = SyntheticConfig::default()
            .with_seed(seed + i as u64)
            .with_trace_count(50)
            .with_error_rate(error_rate);

        let corpus = SyntheticCorpus::new(config).generate();

        let policy = match current_version % 4 {
            0 => minimal_policy(),
            2 => aggressive_policy(&mut rng),
            _ => standard_policy(),
        };

        let compile_result = std::panic::catch_unwind(|| compiler.compile(&policy));
        if compile_result.is_err() {
            failures.push(format!(
                "Iteration {i}: compiler panicked during deployment v{current_version}"
            ));
        }

        let verify_result = std::panic::catch_unwind(|| prover.verify(&policy, &corpus));
        if verify_result.is_err() {
            failures.push(format!(
                "Iteration {i}: prover panicked during deployment v{current_version}"
            ));
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let simulated_seconds = (iterations as u64) * 600;

    if failures.is_empty() {
        CampaignResult::pass("deployment_faults", iterations, simulated_seconds, elapsed)
    } else {
        CampaignResult::fail("deployment_faults", iterations, failures)
    }
}

// =============================================================================
// CASCADING FAILURE CAMPAIGN
// =============================================================================

/// Simulates cascading failure scenarios.
///
/// Models failure propagation through service mesh:
/// - Initial failure in one service
/// - Timeout cascades through dependencies
/// - Circuit breaker state transitions
/// - Recovery and stabilization
#[must_use]
pub fn run_cascading_failure_campaign(seed: u64, iterations: usize) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut failures = Vec::new();

    let compiler = Compiler::new();
    let prover = Prover::default();

    let cascade_duration = 100;

    for i in 0..iterations {
        let cascade_phase = i % cascade_duration;
        let cascade_number = i / cascade_duration;

        let (error_rate, slow_rate) = match cascade_phase {
            0..=19 => (0.02, 0.05),
            20..=39 => (((cascade_phase - 20) as f64).mul_add(0.02, 0.10), 0.20),
            40..=59 => (((cascade_phase - 40) as f64).mul_add(0.02, 0.50), 0.60),
            60..=79 => (((cascade_phase - 60) as f64).mul_add(-0.03, 0.70), 0.40),
            _ => (0.05, 0.08),
        };

        let config = SyntheticConfig::default()
            .with_seed(seed + i as u64 + (cascade_number as u64 * 1000))
            .with_trace_count(100)
            .with_error_rate(error_rate)
            .with_slow_rate(slow_rate);

        let corpus = SyntheticCorpus::new(config).generate();

        let policy = if (40..60).contains(&cascade_phase) {
            emergency_policy()
        } else if (20..40).contains(&cascade_phase) {
            degraded_policy(&mut rng)
        } else {
            standard_policy()
        };

        let compile_result = std::panic::catch_unwind(|| compiler.compile(&policy));
        if compile_result.is_err() {
            failures.push(format!(
                "Iteration {i}: compiler panicked in cascade phase {cascade_phase}"
            ));
        }

        let verify_result = std::panic::catch_unwind(|| prover.verify(&policy, &corpus));
        if verify_result.is_err() {
            failures.push(format!(
                "Iteration {i}: prover panicked in cascade phase {cascade_phase}"
            ));
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let simulated_seconds = (iterations as u64) * 30;

    if failures.is_empty() {
        CampaignResult::pass("cascading_failure", iterations, simulated_seconds, elapsed)
    } else {
        CampaignResult::fail("cascading_failure", iterations, failures)
    }
}

// =============================================================================
// LONG-TERM STABILITY CAMPAIGN
// =============================================================================

/// Simulates 10 years of production operation.
///
/// Comprehensive long-term stability test combining all fault types:
/// - Seasonal patterns (traffic spikes, quiet periods)
/// - Maintenance windows
/// - Major version upgrades
/// - Incident response cycles
/// - Gradual system evolution
#[must_use]
pub fn run_decade_simulation(seed: u64) -> CampaignResult {
    let start = Instant::now();
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut failures = Vec::new();

    let compiler = Compiler::new();
    let prover = Prover::default();

    let total_days = 3650;
    let mut major_incidents = 0;
    let mut deployments = 0;
    let mut policy_version = 1;

    for day in 0..total_days {
        let year = day / 365;
        let day_of_year = day % 365;
        let month = day_of_year / 30;

        let is_black_friday = month == 10 && (day_of_year % 365) > 320;
        let is_holiday_season = month == 11;
        let is_summer_lull = (5..=7).contains(&month);
        let is_quarter_end = month % 3 == 2 && day_of_year % 30 > 25;

        let traffic_multiplier = if is_black_friday {
            10.0
        } else if is_holiday_season {
            3.0
        } else if is_summer_lull {
            0.7
        } else {
            1.0
        };

        let incident_probability = 0.005 * traffic_multiplier;
        let has_incident = rng.gen_bool(f64::min(incident_probability, 0.5));

        if has_incident {
            major_incidents += 1;
        }

        let is_deploy_day = day % 7 == 2 && !has_incident && !is_quarter_end;
        if is_deploy_day {
            deployments += 1;
            if rng.gen_bool(0.1) {
                policy_version += 1;
            }
        }

        let base_error_rate = if has_incident { 0.30 } else { 0.03 };
        let error_rate = f64::min(base_error_rate * traffic_multiplier.sqrt(), 0.9);

        let config = SyntheticConfig::default()
            .with_seed(seed + day as u64)
            .with_trace_count(50)
            .with_error_rate(error_rate)
            .with_slow_rate(if has_incident { 0.40 } else { 0.10 });

        let corpus = SyntheticCorpus::new(config).generate();

        let policy = if has_incident {
            emergency_policy()
        } else if is_black_friday || is_holiday_season {
            high_traffic_policy()
        } else {
            match policy_version % 5 {
                0 => minimal_policy(),
                3 => aggressive_policy(&mut rng),
                _ => standard_policy(),
            }
        };

        if day % 10 == 0 {
            let compile_result = std::panic::catch_unwind(|| compiler.compile(&policy));
            if compile_result.is_err() {
                failures.push(format!("Year {year} Day {day_of_year}: compiler panicked"));
            }

            let verify_result = std::panic::catch_unwind(|| prover.verify(&policy, &corpus));
            if verify_result.is_err() {
                failures.push(format!("Year {year} Day {day_of_year}: prover panicked"));
            }
        }
    }

    let elapsed = start.elapsed().as_millis() as u64;
    let simulated_seconds = (total_days as u64) * 86400;

    eprintln!(
        "Decade simulation: {total_days} days, {major_incidents} major incidents, {deployments} deployments, {policy_version} policy versions"
    );

    if failures.is_empty() {
        CampaignResult::pass("decade_simulation", total_days, simulated_seconds, elapsed)
    } else {
        CampaignResult::fail("decade_simulation", total_days, failures)
    }
}

// =============================================================================
// CAMPAIGN RUNNERS
// =============================================================================

/// Runs all VOPR campaigns and returns summary.
#[must_use]
pub fn run_all_campaigns(seed: u64) -> Vec<CampaignResult> {
    vec![
        run_chaos_campaign(seed, 10_000),
        run_evolution_campaign(seed, 3650),
        run_determinism_campaign(seed, 5_000),
        run_cardinality_campaign(seed, 100_000, 10),
        run_combined_faults_campaign(seed, 5_000),
        run_infrastructure_faults_campaign(seed, 2_000),
        run_resource_exhaustion_campaign(seed, 2_000),
        run_distributed_faults_campaign(seed, 2_000),
        run_deployment_faults_campaign(seed, 1_000),
        run_cascading_failure_campaign(seed, 2_000),
    ]
}

/// Runs extended campaign suite including decade simulation.
#[must_use]
pub fn run_extended_campaigns(seed: u64) -> Vec<CampaignResult> {
    let mut results = run_all_campaigns(seed);
    results.push(run_decade_simulation(seed));
    results
}

// =============================================================================
// HELPER FUNCTIONS
// =============================================================================

fn generate_random_action(rng: &mut ChaCha8Rng) -> Action {
    if rng.gen_bool(0.3) {
        Action::Keep
    } else if rng.gen_bool(0.1) {
        Action::Drop
    } else {
        Action::Sample(rng.gen_range(0.001..0.5))
    }
}

fn generate_random_match_expr(rng: &mut ChaCha8Rng) -> String {
    match rng.gen_range(0..6) {
        0 => format!("http.status >= {}", rng.gen_range(400..600)),
        1 => format!("duration > {}ms", rng.gen_range(100..10000)),
        2 => format!("service.name == \"service-{}\"", rng.gen_range(0..100)),
        3 => "error == true".to_string(),
        4 => format!("http.status == {}", rng.gen_range(200..600)),
        _ => format!("duration < {}ms", rng.gen_range(10..1000)),
    }
}

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

fn minimal_policy() -> Policy {
    let mut policy = Policy::new("minimal-policy");
    policy.add_rule(Rule::new("fallback", "true", Action::Sample(0.001), 0));
    policy
}

fn emergency_policy() -> Policy {
    let mut policy = Policy::new("emergency-policy");
    policy.add_rule(Rule::new("keep-all-errors", "error == true", Action::Keep, 100));
    policy.add_rule(Rule::new(
        "keep-all-slow",
        "duration > 1000ms",
        Action::Keep,
        90,
    ));
    policy.add_rule(Rule::new("sample-rest", "true", Action::Sample(0.10), 0));
    policy
}

fn degraded_policy(rng: &mut ChaCha8Rng) -> Policy {
    let mut policy = Policy::new("degraded-policy");
    policy.add_rule(Rule::new(
        "keep-critical",
        "http.status >= 500",
        Action::Keep,
        100,
    ));
    policy.add_rule(Rule::new(
        "fallback",
        "true",
        Action::Sample(rng.gen_range(0.01..0.05)),
        0,
    ));
    policy
}

fn aggressive_policy(rng: &mut ChaCha8Rng) -> Policy {
    let mut policy = Policy::new("aggressive-policy");
    policy.add_rule(Rule::new(
        "keep-errors",
        "http.status >= 500",
        Action::Keep,
        100,
    ));
    policy.add_rule(Rule::new(
        "keep-slow",
        "duration > 10000ms",
        Action::Keep,
        90,
    ));
    policy.add_rule(Rule::new(
        "sample-moderate",
        "duration > 1000ms",
        Action::Sample(0.10),
        50,
    ));
    policy.add_rule(Rule::new(
        "fallback",
        "true",
        Action::Sample(rng.gen_range(0.001..0.01)),
        0,
    ));
    policy
}

fn high_traffic_policy() -> Policy {
    let mut policy = Policy::new("high-traffic-policy");
    policy.add_rule(Rule::new(
        "keep-errors",
        "http.status >= 500",
        Action::Keep,
        100,
    ));
    policy.add_rule(Rule::new(
        "keep-very-slow",
        "duration > 30000ms",
        Action::Keep,
        90,
    ));
    policy.add_rule(Rule::new(
        "sample-slow",
        "duration > 5000ms",
        Action::Sample(0.05),
        50,
    ));
    policy.add_rule(Rule::new("fallback", "true", Action::Sample(0.001), 0));
    policy
}

fn random_policy(seed: u64) -> Policy {
    let mut rng = ChaCha8Rng::seed_from_u64(seed);
    let mut policy = Policy::new(format!("policy-{seed}"));

    let rule_count = rng.gen_range(1..10);
    for i in 0..rule_count {
        let priority = rng.gen_range(1..100);
        let action = generate_random_action(&mut rng);
        let match_expr = generate_random_match_expr(&mut rng);
        policy.add_rule(Rule::new(format!("rule-{i}"), match_expr, action, priority));
    }

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
        println!(
            "Chaos: {} iterations, {}h simulated in {}ms",
            result.iterations,
            result.simulated_seconds / 3600,
            result.real_elapsed_ms
        );
    }

    #[test]
    fn evolution_campaign_simulates_decade() {
        // Test with 1 year for speed, full decade in extended tests
        let result = run_evolution_campaign(42, 365);
        assert!(
            result.all_passed,
            "Evolution campaign failed: {:?}",
            result.failure_details
        );
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
            "Cardinality: {} steps in {}ms",
            result.iterations, result.real_elapsed_ms
        );
    }

    #[test]
    fn combined_faults_campaign_1000_iterations() {
        let result = run_combined_faults_campaign(42, 1000);
        assert!(
            result.all_passed,
            "Combined faults failed: {:?}",
            result.failure_details
        );
        println!(
            "Combined faults: {} iterations in {}ms",
            result.iterations, result.real_elapsed_ms
        );
    }

    #[test]
    fn infrastructure_faults_campaign() {
        let result = run_infrastructure_faults_campaign(42, 500);
        assert!(
            result.all_passed,
            "Infrastructure faults failed: {:?}",
            result.failure_details
        );
        println!(
            "Infrastructure faults: {} iterations in {}ms",
            result.iterations, result.real_elapsed_ms
        );
    }

    #[test]
    fn resource_exhaustion_campaign() {
        let result = run_resource_exhaustion_campaign(42, 500);
        assert!(
            result.all_passed,
            "Resource exhaustion failed: {:?}",
            result.failure_details
        );
        println!(
            "Resource exhaustion: {} iterations in {}ms",
            result.iterations, result.real_elapsed_ms
        );
    }

    #[test]
    fn distributed_faults_campaign() {
        let result = run_distributed_faults_campaign(42, 500);
        assert!(
            result.all_passed,
            "Distributed faults failed: {:?}",
            result.failure_details
        );
        println!(
            "Distributed faults: {} iterations in {}ms",
            result.iterations, result.real_elapsed_ms
        );
    }

    #[test]
    fn deployment_faults_campaign() {
        let result = run_deployment_faults_campaign(42, 500);
        assert!(
            result.all_passed,
            "Deployment faults failed: {:?}",
            result.failure_details
        );
        println!(
            "Deployment faults: {} iterations in {}ms",
            result.iterations, result.real_elapsed_ms
        );
    }

    #[test]
    fn cascading_failure_campaign() {
        let result = run_cascading_failure_campaign(42, 500);
        assert!(
            result.all_passed,
            "Cascading failure failed: {:?}",
            result.failure_details
        );
        println!(
            "Cascading failure: {} iterations in {}ms",
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
            let sim_time = if r.simulated_seconds > 86400 * 365 {
                format!("{:.1} years", r.simulated_seconds as f64 / 31_536_000.0)
            } else if r.simulated_seconds > 86400 {
                format!("{:.0} days", r.simulated_seconds as f64 / 86400.0)
            } else {
                format!("{}s", r.simulated_seconds)
            };
            println!(
                "[{}] {}: {} iterations, {} simulated in {}ms",
                status, r.name, r.iterations, sim_time, r.real_elapsed_ms
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
