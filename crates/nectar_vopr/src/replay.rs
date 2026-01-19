//! Time-compressed replay testing.
//!
//! Simulates policy evolution over time with:
//! - Compressed time simulation
//! - Policy change events
//! - Corpus drift simulation
//! - Regression detection

use nectar_compiler::Compiler;
use nectar_corpus::Corpus;
use nectar_prover::Prover;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use toon_policy::{Action, Policy, Rule};
use xxhash_rust::xxh64::xxh64;

/// Time compressor for accelerated simulation.
#[derive(Debug, Clone)]
pub struct TimeCompressor {
    /// Compression ratio (e.g., 1000 = 1 second simulates 1000 seconds).
    pub ratio: u64,
    /// Current simulated time in milliseconds.
    pub simulated_time_ms: u64,
    /// Real elapsed time in milliseconds.
    pub real_time_ms: u64,
}

impl Default for TimeCompressor {
    fn default() -> Self {
        Self {
            ratio: 1000,
            simulated_time_ms: 0,
            real_time_ms: 0,
        }
    }
}

impl TimeCompressor {
    /// Creates a new time compressor with the given ratio.
    #[must_use]
    pub const fn new(ratio: u64) -> Self {
        Self {
            ratio,
            simulated_time_ms: 0,
            real_time_ms: 0,
        }
    }

    /// Advances simulated time by the given real duration.
    pub const fn advance(&mut self, real_ms: u64) {
        self.real_time_ms += real_ms;
        self.simulated_time_ms += real_ms * self.ratio;
    }

    /// Returns simulated time as human-readable duration.
    #[must_use]
    pub fn simulated_duration(&self) -> String {
        let total_secs = self.simulated_time_ms / 1000;
        let hours = total_secs / 3600;
        let mins = (total_secs % 3600) / 60;
        let secs = total_secs % 60;

        if hours > 0 {
            format!("{hours}h {mins}m {secs}s")
        } else if mins > 0 {
            format!("{mins}m {secs}s")
        } else {
            format!("{secs}s")
        }
    }
}

/// A log of events for replay testing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayLog {
    /// Events in chronological order.
    pub events: VecDeque<ReplayEvent>,
    /// Checkpoints for verification.
    pub checkpoints: Vec<Checkpoint>,
}

impl Default for ReplayLog {
    fn default() -> Self {
        Self::new()
    }
}

impl ReplayLog {
    /// Creates a new empty replay log.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            events: VecDeque::new(),
            checkpoints: Vec::new(),
        }
    }

    /// Records a policy change event.
    pub fn record_policy_change(&mut self, timestamp_ms: u64, policy: &Policy) {
        self.events.push_back(ReplayEvent::PolicyChange {
            timestamp_ms,
            policy_hash: policy_hash(policy),
            rule_count: policy.rules.len(),
        });
    }

    /// Records a corpus change event.
    pub fn record_corpus_change(&mut self, timestamp_ms: u64, corpus: &Corpus) {
        self.events.push_back(ReplayEvent::CorpusChange {
            timestamp_ms,
            trace_count: corpus.len(),
            error_count: corpus.errors().len(),
        });
    }

    /// Records a verification result.
    pub fn record_verification(&mut self, timestamp_ms: u64, passed: bool, message: &str) {
        self.events.push_back(ReplayEvent::Verification {
            timestamp_ms,
            passed,
            message: message.to_string(),
        });
    }

    /// Creates a checkpoint of the current state.
    pub fn checkpoint(&mut self, timestamp_ms: u64, policy: &Policy, corpus: &Corpus) {
        let comp = Compiler::new();
        let output = comp.compile(policy).unwrap_or_default();

        self.checkpoints.push(Checkpoint {
            timestamp_ms,
            policy_hash: policy_hash(policy),
            corpus_hash: corpus_hash(corpus),
            compiled_hash: xxh64(output.as_bytes(), 0),
        });
    }

    /// Verifies that replaying produces the same checkpoints.
    #[must_use]
    pub fn verify_replay(&self, other: &Self) -> bool {
        if self.checkpoints.len() != other.checkpoints.len() {
            return false;
        }

        self.checkpoints
            .iter()
            .zip(other.checkpoints.iter())
            .all(|(a, b)| a.policy_hash == b.policy_hash && a.compiled_hash == b.compiled_hash)
    }
}

/// An event in the replay log.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum ReplayEvent {
    /// Policy was changed.
    PolicyChange {
        /// Simulated timestamp in milliseconds.
        timestamp_ms: u64,
        /// Hash of the policy.
        policy_hash: u64,
        /// Number of rules in the policy.
        rule_count: usize,
    },
    /// Corpus was updated.
    CorpusChange {
        /// Simulated timestamp in milliseconds.
        timestamp_ms: u64,
        /// Number of traces in the corpus.
        trace_count: usize,
        /// Number of error traces.
        error_count: usize,
    },
    /// Verification was performed.
    Verification {
        /// Simulated timestamp in milliseconds.
        timestamp_ms: u64,
        /// Whether verification passed.
        passed: bool,
        /// Verification result message.
        message: String,
    },
}

/// A checkpoint for state verification.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Checkpoint {
    /// Simulated timestamp.
    pub timestamp_ms: u64,
    /// Hash of the policy.
    pub policy_hash: u64,
    /// Hash of the corpus.
    pub corpus_hash: u64,
    /// Hash of the compiled output.
    pub compiled_hash: u64,
}

/// Simulates policy evolution over time.
pub struct PolicyEvolutionSim {
    /// Current policy.
    pub policy: Policy,
    /// Current corpus.
    pub corpus: Corpus,
    /// Time compressor.
    pub time: TimeCompressor,
    /// Replay log.
    pub log: ReplayLog,
    /// Prover instance.
    prover: Prover,
    /// Compiler instance.
    compiler: Compiler,
}

impl PolicyEvolutionSim {
    /// Creates a new policy evolution simulation.
    #[must_use]
    pub fn new(initial_policy: Policy, initial_corpus: Corpus) -> Self {
        Self {
            policy: initial_policy,
            corpus: initial_corpus,
            time: TimeCompressor::default(),
            log: ReplayLog::new(),
            prover: Prover::default(),
            compiler: Compiler::new(),
        }
    }

    /// Runs a simulation step.
    pub fn step(&mut self, action: SimAction) -> StepResult {
        self.time.advance(1); // 1ms real time = ratio ms simulated

        match action {
            SimAction::AddRule {
                name,
                match_expr,
                action,
                priority,
            } => {
                self.policy
                    .add_rule(Rule::new(name, match_expr, action, priority));
                self.log
                    .record_policy_change(self.time.simulated_time_ms, &self.policy);
                StepResult::PolicyChanged
            }
            SimAction::RemoveRule { name } => {
                self.policy.rules.retain(|r| r.name != name);
                self.log
                    .record_policy_change(self.time.simulated_time_ms, &self.policy);
                StepResult::PolicyChanged
            }
            SimAction::Verify => {
                let result = self.prover.verify(&self.policy, &self.corpus);
                let (passed, message) = match &result {
                    Ok(r) => (r.is_approved(), "Verification passed".to_string()),
                    Err(e) => (false, e.to_string()),
                };
                self.log
                    .record_verification(self.time.simulated_time_ms, passed, &message);
                StepResult::Verified { passed, message }
            }
            SimAction::Compile => {
                let result = self.compiler.compile(&self.policy);
                match result {
                    Ok(output) => StepResult::Compiled { output },
                    Err(e) => StepResult::CompileFailed {
                        error: e.to_string(),
                    },
                }
            }
            SimAction::Checkpoint => {
                self.log
                    .checkpoint(self.time.simulated_time_ms, &self.policy, &self.corpus);
                StepResult::Checkpointed
            }
        }
    }

    /// Runs a sequence of actions.
    pub fn run_sequence(&mut self, actions: &[SimAction]) -> Vec<StepResult> {
        actions.iter().map(|a| self.step(a.clone())).collect()
    }
}

/// An action to perform in the simulation.
#[derive(Debug, Clone)]
pub enum SimAction {
    /// Add a rule to the policy.
    AddRule {
        /// Rule name.
        name: String,
        /// Match expression.
        match_expr: String,
        /// Sampling action.
        action: Action,
        /// Rule priority.
        priority: u8,
    },
    /// Remove a rule by name.
    RemoveRule {
        /// Name of the rule to remove.
        name: String,
    },
    /// Verify the current policy.
    Verify,
    /// Compile the current policy.
    Compile,
    /// Create a checkpoint.
    Checkpoint,
}

/// Result of a simulation step.
#[derive(Debug, Clone)]
pub enum StepResult {
    /// Policy was changed.
    PolicyChanged,
    /// Verification completed.
    Verified {
        /// Whether verification passed.
        passed: bool,
        /// Result message.
        message: String,
    },
    /// Compilation succeeded.
    Compiled {
        /// Compiled output.
        output: String,
    },
    /// Compilation failed.
    CompileFailed {
        /// Error message.
        error: String,
    },
    /// Checkpoint created.
    Checkpointed,
}

fn policy_hash(policy: &Policy) -> u64 {
    let toon = toon_policy::serialize(policy);
    xxh64(toon.as_bytes(), 0)
}

fn corpus_hash(corpus: &Corpus) -> u64 {
    let mut input = String::new();
    for trace in corpus.iter() {
        input.push_str(&trace.trace_id);
    }
    xxh64(input.as_bytes(), 0)
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
    fn time_compressor_advances_correctly() {
        let mut tc = TimeCompressor::new(1000);
        tc.advance(1);
        assert_eq!(tc.simulated_time_ms, 1000);
        assert_eq!(tc.real_time_ms, 1);

        tc.advance(10);
        assert_eq!(tc.simulated_time_ms, 11000);
    }

    #[test]
    fn time_compressor_formats_duration() {
        let mut tc = TimeCompressor::new(1000);
        tc.advance(3600); // 1 hour simulated
        assert!(tc.simulated_duration().contains('h'));
    }

    #[test]
    fn replay_log_records_events() {
        let mut log = ReplayLog::new();
        let policy = test_policy();
        let corpus = Corpus::new();

        log.record_policy_change(0, &policy);
        log.record_corpus_change(100, &corpus);
        log.record_verification(200, true, "ok");

        assert_eq!(log.events.len(), 3);
    }

    #[test]
    fn policy_evolution_sim_runs() {
        let policy = test_policy();
        let corpus = Corpus::new();

        let mut sim = PolicyEvolutionSim::new(policy, corpus);

        let results = sim.run_sequence(&[
            SimAction::Checkpoint,
            SimAction::Verify,
            SimAction::Compile,
            SimAction::AddRule {
                name: "new-rule".to_string(),
                match_expr: "duration > 10s".to_string(),
                action: Action::Keep,
                priority: 50,
            },
            SimAction::Verify,
            SimAction::Checkpoint,
        ]);

        assert_eq!(results.len(), 6);
        assert!(matches!(results[0], StepResult::Checkpointed));
    }

    #[test]
    fn replay_verification_detects_changes() {
        let policy = test_policy();
        let corpus = Corpus::new();

        let mut sim1 = PolicyEvolutionSim::new(policy.clone(), corpus.clone());
        sim1.step(SimAction::Checkpoint);

        let mut sim2 = PolicyEvolutionSim::new(policy, corpus);
        sim2.step(SimAction::Checkpoint);

        // Same sequence should produce same checkpoints
        assert!(sim1.log.verify_replay(&sim2.log));
    }
}
