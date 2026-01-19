//! Simulation scenarios and results.

use nectar_corpus::Corpus;
use std::fmt;
use toon_policy::Policy;

/// A test scenario to run in the simulation.
#[derive(Debug, Clone)]
pub enum Scenario {
    /// Test that compilation is deterministic.
    CompileDeterminism {
        /// Policy to compile.
        policy: Policy,
    },
    /// Test that prover produces consistent results.
    ProverConsistency {
        /// Policy to verify.
        policy: Policy,
        /// Corpus to verify against.
        corpus: Corpus,
    },
    /// Test policy roundtrip serialization.
    RoundTrip {
        /// Policy to roundtrip.
        policy: Policy,
    },
    /// Test resilience under chaos injection.
    ChaosResilience {
        /// Policy to test.
        policy: Policy,
        /// Corpus to corrupt.
        corpus: Corpus,
    },
    /// Test handling of high-cardinality data.
    HighCardinality {
        /// Number of unique services to generate.
        unique_services: usize,
    },
}

impl Scenario {
    /// Creates a compile determinism scenario.
    #[must_use]
    pub const fn compile_determinism(policy: Policy) -> Self {
        Self::CompileDeterminism { policy }
    }

    /// Creates a prover consistency scenario.
    #[must_use]
    pub const fn prover_consistency(policy: Policy, corpus: Corpus) -> Self {
        Self::ProverConsistency { policy, corpus }
    }

    /// Creates a roundtrip scenario.
    #[must_use]
    pub const fn roundtrip(policy: Policy) -> Self {
        Self::RoundTrip { policy }
    }

    /// Creates a chaos resilience scenario.
    #[must_use]
    pub const fn chaos_resilience(policy: Policy, corpus: Corpus) -> Self {
        Self::ChaosResilience { policy, corpus }
    }

    /// Creates a high cardinality scenario.
    #[must_use]
    pub const fn high_cardinality(unique_services: usize) -> Self {
        Self::HighCardinality { unique_services }
    }

    /// Returns the name of this scenario.
    #[must_use]
    pub const fn name(&self) -> &'static str {
        match self {
            Self::CompileDeterminism { .. } => "compile_determinism",
            Self::ProverConsistency { .. } => "prover_consistency",
            Self::RoundTrip { .. } => "roundtrip",
            Self::ChaosResilience { .. } => "chaos_resilience",
            Self::HighCardinality { .. } => "high_cardinality",
        }
    }
}

/// Result of running a simulation scenario.
#[derive(Debug, Clone)]
pub struct SimResult {
    /// Name of the test.
    pub name: String,
    /// Whether the test passed.
    pub passed: bool,
    /// Whether the test was skipped.
    pub skipped: bool,
    /// Human-readable message.
    pub message: String,
    /// Detailed diagnostics (if any).
    pub diagnostics: Vec<String>,
}

impl SimResult {
    /// Creates a passing result.
    #[must_use]
    pub fn pass(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            skipped: false,
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }

    /// Creates a failing result.
    #[must_use]
    pub fn fail(name: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: false,
            skipped: false,
            message: message.into(),
            diagnostics: Vec::new(),
        }
    }

    /// Creates a skipped result.
    #[must_use]
    pub fn skip(name: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            name: name.into(),
            passed: true,
            skipped: true,
            message: reason.into(),
            diagnostics: Vec::new(),
        }
    }

    /// Adds a diagnostic message.
    #[must_use]
    pub fn with_diagnostic(mut self, diagnostic: impl Into<String>) -> Self {
        self.diagnostics.push(diagnostic.into());
        self
    }
}

impl fmt::Display for SimResult {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let status = if self.skipped {
            "SKIP"
        } else if self.passed {
            "PASS"
        } else {
            "FAIL"
        };

        write!(f, "[{status}] {}: {}", self.name, self.message)?;

        for diag in &self.diagnostics {
            write!(f, "\n  - {diag}")?;
        }

        Ok(())
    }
}

/// Aggregated results from multiple scenarios.
#[derive(Debug, Default)]
pub struct SimSummary {
    /// Total number of tests.
    pub total: usize,
    /// Number of passed tests.
    pub passed: usize,
    /// Number of failed tests.
    pub failed: usize,
    /// Number of skipped tests.
    pub skipped: usize,
    /// Individual results.
    pub results: Vec<SimResult>,
}

impl SimSummary {
    /// Creates a summary from a list of results.
    #[must_use]
    pub fn from_results(results: Vec<SimResult>) -> Self {
        let total = results.len();
        let passed = results.iter().filter(|r| r.passed && !r.skipped).count();
        let skipped = results.iter().filter(|r| r.skipped).count();
        let failed = total - passed - skipped;

        Self {
            total,
            passed,
            failed,
            skipped,
            results,
        }
    }

    /// Returns true if all tests passed.
    #[must_use]
    pub const fn all_passed(&self) -> bool {
        self.failed == 0
    }

    /// Returns true if all invariants held (no failures).
    #[must_use]
    pub const fn all_invariants_held(&self) -> bool {
        self.failed == 0
    }
}

impl fmt::Display for SimSummary {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        writeln!(f, "Simulation Summary")?;
        writeln!(f, "==================")?;
        writeln!(f, "Total:   {}", self.total)?;
        writeln!(f, "Passed:  {}", self.passed)?;
        writeln!(f, "Failed:  {}", self.failed)?;
        writeln!(f, "Skipped: {}", self.skipped)?;
        writeln!(f)?;

        for result in &self.results {
            writeln!(f, "{result}")?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sim_result_display() {
        let result = SimResult::pass("test", "it worked");
        assert!(result.to_string().contains("PASS"));
        assert!(result.to_string().contains("test"));

        let result = SimResult::fail("test", "it broke");
        assert!(result.to_string().contains("FAIL"));
    }

    #[test]
    fn sim_summary_aggregation() {
        let results = vec![
            SimResult::pass("a", "ok"),
            SimResult::fail("b", "not ok"),
            SimResult::skip("c", "skipped"),
        ];

        let summary = SimSummary::from_results(results);
        assert_eq!(summary.total, 3);
        assert_eq!(summary.passed, 1);
        assert_eq!(summary.failed, 1);
        assert_eq!(summary.skipped, 1);
        assert!(!summary.all_passed());
    }
}
