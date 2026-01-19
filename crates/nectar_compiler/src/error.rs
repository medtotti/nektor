//! Error types for compiler operations.

use thiserror::Error;

/// Errors that can occur during compilation.
#[derive(Debug, Error)]
pub enum Error {
    /// Policy contains unsupported constructs.
    #[error("unsupported construct: {0}")]
    Unsupported(String),

    /// Failed to serialize output.
    #[error("serialization error: {0}")]
    Serialization(String),

    /// Invalid match expression.
    #[error("invalid match expression '{expr}': {reason}")]
    InvalidMatch {
        /// The invalid expression string.
        expr: String,
        /// Reason why the expression is invalid.
        reason: String,
    },

    /// YAML serialization error.
    #[error(transparent)]
    Yaml(#[from] serde_yaml::Error),

    /// JSON serialization error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

/// Result type alias for compiler operations.
pub type Result<T> = std::result::Result<T, Error>;
