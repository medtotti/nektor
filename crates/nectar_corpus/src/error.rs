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

    /// Unknown or unsupported format.
    #[error("unknown format: {0}")]
    UnknownFormat(String),

    /// Parse error for a specific format.
    #[error("parse error ({format}): {message}")]
    ParseError {
        /// The format that failed to parse.
        format: &'static str,
        /// Description of the parse error.
        message: String,
    },

    /// I/O error.
    #[error(transparent)]
    Io(#[from] std::io::Error),

    /// JSON parsing error.
    #[error(transparent)]
    Json(#[from] serde_json::Error),
}

impl Error {
    /// Creates a parse error for the given format.
    pub fn parse(format: &'static str, message: impl Into<String>) -> Self {
        Self::ParseError {
            format,
            message: message.into(),
        }
    }
}

/// Result type alias for corpus operations.
pub type Result<T> = std::result::Result<T, Error>;
