//! Error types for prover operations.

use thiserror::Error;

/// Errors that can occur during prover operations.
#[derive(Debug, Error)]
pub enum Error {
    /// Policy is invalid and cannot be verified.
    #[error("invalid policy: {0}")]
    InvalidPolicy(String),

    /// Corpus is empty or invalid.
    #[error("invalid corpus: {0}")]
    InvalidCorpus(String),

    /// Internal prover error.
    #[error("prover error: {0}")]
    Internal(String),
}

/// Result type alias for prover operations.
pub type Result<T> = std::result::Result<T, Error>;
