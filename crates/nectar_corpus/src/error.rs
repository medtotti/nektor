//! Error types for corpus operations.

use thiserror::Error;

/// Errors that can occur during corpus operations.
#[derive(Debug, Error)]
pub enum Error {
    /// Failed to load trace data.
    #[error("failed to load trace: {0}")]
    LoadError(String),

    /// Failed to encode corpus to TOON.
    #[error("encoding error: {0}")]
    EncodingError(String),

    /// Invalid trace format.
    #[error("invalid trace format: {0}")]
    InvalidTrace(String),

    /// I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

/// Result type alias for corpus operations.
pub type Result<T> = std::result::Result<T, Error>;
