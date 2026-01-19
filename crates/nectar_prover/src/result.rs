//! Prover result types.

use serde::{Deserialize, Serialize};

/// Result of policy verification.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProverResult {
    /// Overall status.
    pub status: Status,
    /// Number of checks passed.
    pub checks_passed: usize,
    /// Total number of checks.
    pub checks_total: usize,
    /// Critical violations (cause rejection).
    pub violations: Vec<Violation>,
    /// Non-critical warnings.
    pub warnings: Vec<Warning>,
}

/// Verification status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Status {
    /// Policy approved.
    Approved,
    /// Policy approved but has warnings.
    ApprovedWithWarnings,
    /// Policy rejected due to violations.
    Rejected,
}

/// A critical violation that causes rejection.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Violation {
    /// Which check failed.
    pub check: String,
    /// Severity level.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
}

/// A non-critical warning.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Warning {
    /// Which check produced the warning.
    pub check: String,
    /// Severity level.
    pub severity: Severity,
    /// Human-readable message.
    pub message: String,
}

/// Severity level.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Severity {
    /// Critical: causes rejection.
    Critical,
    /// Warning: noted but not blocking.
    Warning,
    /// Info: for informational purposes.
    Info,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Critical => write!(f, "CRITICAL"),
            Self::Warning => write!(f, "WARNING"),
            Self::Info => write!(f, "INFO"),
        }
    }
}

impl ProverResult {
    /// Creates an approved result.
    #[must_use]
    pub const fn approved(checks_passed: usize) -> Self {
        Self {
            status: Status::Approved,
            checks_passed,
            checks_total: checks_passed,
            violations: Vec::new(),
            warnings: Vec::new(),
        }
    }

    /// Creates a rejected result.
    #[must_use]
    pub const fn rejected(violations: Vec<Violation>, checks_passed: usize, checks_total: usize) -> Self {
        Self {
            status: Status::Rejected,
            checks_passed,
            checks_total,
            violations,
            warnings: Vec::new(),
        }
    }

    /// Returns true if the policy is approved.
    #[must_use]
    pub const fn is_approved(&self) -> bool {
        matches!(self.status, Status::Approved | Status::ApprovedWithWarnings)
    }

    /// Returns true if the policy is rejected.
    #[must_use]
    pub const fn is_rejected(&self) -> bool {
        matches!(self.status, Status::Rejected)
    }

    /// Adds a warning to the result.
    pub fn add_warning(&mut self, warning: Warning) {
        self.warnings.push(warning);
        if self.status == Status::Approved {
            self.status = Status::ApprovedWithWarnings;
        }
    }
}

impl Violation {
    /// Creates a new critical violation.
    #[must_use]
    pub fn critical(check: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            check: check.into(),
            severity: Severity::Critical,
            message: message.into(),
        }
    }
}

impl Warning {
    /// Creates a new warning.
    #[must_use]
    pub fn new(check: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            check: check.into(),
            severity: Severity::Warning,
            message: message.into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn approved_result_is_approved() {
        let result = ProverResult::approved(4);
        assert!(result.is_approved());
        assert!(!result.is_rejected());
    }

    #[test]
    fn rejected_result_is_rejected() {
        let violations = vec![Violation::critical("test", "failed")];
        let result = ProverResult::rejected(violations, 2, 4);
        assert!(result.is_rejected());
        assert!(!result.is_approved());
    }

    #[test]
    fn warning_changes_status() {
        let mut result = ProverResult::approved(4);
        result.add_warning(Warning::new("cardinality", "high cardinality key"));
        assert_eq!(result.status, Status::ApprovedWithWarnings);
        assert!(result.is_approved());
    }
}
