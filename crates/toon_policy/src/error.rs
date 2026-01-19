//! Error types for TOON policy parsing and validation.

use thiserror::Error;

/// Errors that can occur during TOON policy operations.
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to parse TOON syntax.
    #[error("parse error at line {line}: {reason}")]
    Parse {
        /// Line number where the error occurred.
        line: usize,
        /// Reason for the parse failure.
        reason: String,
    },

    /// TOON structure is valid but policy semantics are wrong.
    #[error("validation error: {0}")]
    Validation(String),

    /// Array count mismatch (strict mode).
    #[error("count mismatch: declared {declared}, found {actual}")]
    CountMismatch {
        /// Declared count in the TOON header.
        declared: usize,
        /// Actual number of items found.
        actual: usize,
    },

    /// Missing required field.
    #[error("missing required field: {0}")]
    MissingField(String),

    /// Invalid action syntax.
    #[error("invalid action '{action}': {reason}")]
    InvalidAction {
        /// The invalid action string.
        action: String,
        /// Reason why the action is invalid.
        reason: String,
    },

    /// Invalid match expression.
    #[error("invalid match expression '{expr}': {reason}")]
    InvalidMatch {
        /// The invalid expression string.
        expr: String,
        /// Reason why the expression is invalid.
        reason: String,
    },
}

/// Result type alias for TOON policy operations.
pub type Result<T> = std::result::Result<T, Error>;
