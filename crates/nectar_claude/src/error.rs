//! Error types for Claude client operations.

use thiserror::Error;

/// Errors that can occur during Claude API operations.
#[derive(Debug, Error)]
pub enum Error {
    /// API request failed.
    #[error("API request failed: {0}")]
    ApiError(String),

    /// Rate limited.
    #[error("rate limited, retry after {retry_after_seconds}s")]
    RateLimited {
        /// Number of seconds to wait before retrying.
        retry_after_seconds: u64,
    },

    /// Invalid API key.
    #[error("invalid API key")]
    InvalidApiKey,

    /// Response parsing failed.
    #[error("failed to parse response: {0}")]
    ParseError(String),

    /// TOON validation failed on Claude's output.
    #[error("TOON validation failed: {0}")]
    ToonValidationError(String),

    /// Network error.
    #[error(transparent)]
    Network(#[from] reqwest::Error),

    /// Policy parsing error.
    #[error(transparent)]
    Policy(#[from] toon_policy::Error),

    /// Corpus error.
    #[error(transparent)]
    Corpus(#[from] nectar_corpus::Error),
}

/// Result type alias for Claude operations.
pub type Result<T> = std::result::Result<T, Error>;
