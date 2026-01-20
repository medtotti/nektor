//! Pluggable trace ingestion framework.
//!
//! This module provides a trait-based abstraction for ingesting traces
//! from various formats (OTLP protobuf, Honeycomb NDJSON, plain JSON).
//!
//! # Example
//!
//! ```rust,ignore
//! use nectar_corpus::ingestor::IngestorRegistry;
//!
//! let registry = IngestorRegistry::new();
//! let traces = registry.ingest(data, Some("application/json"))?;
//! ```

mod honeycomb;
mod json;
#[cfg(feature = "otlp")]
mod otlp;

pub use honeycomb::HoneycombIngestor;
pub use json::JsonIngestor;
#[cfg(feature = "otlp")]
pub use otlp::OtlpIngestor;

use crate::error::{Error, Result};
use crate::trace::Trace;

/// A trait for ingesting traces from a specific format.
///
/// Implementations of this trait handle parsing trace data from
/// various wire formats into the internal `Trace` representation.
pub trait TraceIngestor: Send + Sync {
    /// Returns the name of this format (e.g., "json", "otlp", "honeycomb").
    fn format_name(&self) -> &'static str;

    /// Checks if this ingestor can handle the given data.
    ///
    /// Uses header bytes and optional content-type to determine compatibility.
    fn can_handle(&self, header: &[u8], content_type: Option<&str>) -> bool;

    /// Ingests trace data and returns a vector of traces.
    ///
    /// # Errors
    ///
    /// Returns an error if the data cannot be parsed.
    fn ingest(&self, data: &[u8]) -> Result<Vec<Trace>>;
}

/// Registry of available trace ingestors.
///
/// The registry maintains a priority-ordered list of ingestors and
/// provides auto-detection of trace formats.
pub struct IngestorRegistry {
    ingestors: Vec<Box<dyn TraceIngestor>>,
}

impl Default for IngestorRegistry {
    fn default() -> Self {
        Self::new()
    }
}

impl IngestorRegistry {
    /// Creates a new registry with all built-in ingestors.
    ///
    /// Ingestors are registered in priority order:
    /// 1. OTLP (if feature enabled) - most specific format
    /// 2. Honeycomb - specific NDJSON format
    /// 3. JSON - general-purpose fallback
    #[must_use]
    pub fn new() -> Self {
        let mut registry = Self {
            ingestors: Vec::new(),
        };

        // Register ingestors in priority order (most specific first)
        #[cfg(feature = "otlp")]
        registry.register(Box::new(OtlpIngestor));

        registry.register(Box::new(HoneycombIngestor));
        registry.register(Box::new(JsonIngestor));

        registry
    }

    /// Registers a new ingestor.
    ///
    /// The ingestor is added to the end of the priority list.
    pub fn register(&mut self, ingestor: Box<dyn TraceIngestor>) {
        self.ingestors.push(ingestor);
    }

    /// Ingests trace data using auto-detection.
    ///
    /// Tries each registered ingestor in order until one succeeds.
    ///
    /// # Errors
    ///
    /// Returns `Error::UnknownFormat` if no ingestor can handle the data.
    pub fn ingest(&self, data: &[u8], content_type: Option<&str>) -> Result<Vec<Trace>> {
        self.ingest_with_hint(data, content_type)
    }

    /// Ingests trace data with an optional content-type hint.
    ///
    /// If the content-type matches a specific ingestor, it is tried first.
    /// Otherwise, auto-detection is used.
    ///
    /// # Errors
    ///
    /// Returns an error if the data cannot be parsed by any ingestor.
    pub fn ingest_with_hint(&self, data: &[u8], content_type: Option<&str>) -> Result<Vec<Trace>> {
        if data.is_empty() {
            return Ok(Vec::new());
        }

        // Get first few bytes for format detection
        let header_len = data.len().min(256);
        let header = &data[..header_len];

        // Try ingestors that claim to handle this format
        for ingestor in &self.ingestors {
            if ingestor.can_handle(header, content_type) {
                match ingestor.ingest(data) {
                    Ok(traces) => return Ok(traces),
                    Err(e) => {
                        tracing::debug!(
                            "Ingestor {} failed: {}, trying next",
                            ingestor.format_name(),
                            e
                        );
                    }
                }
            }
        }

        // If no ingestor matched or all failed, try each one as fallback
        let mut last_error = None;
        for ingestor in &self.ingestors {
            match ingestor.ingest(data) {
                Ok(traces) => return Ok(traces),
                Err(e) => {
                    last_error = Some(e);
                }
            }
        }

        Err(last_error.unwrap_or_else(|| {
            Error::UnknownFormat("no ingestors registered".to_string())
        }))
    }

    /// Ingests trace data using a specific format.
    ///
    /// # Errors
    ///
    /// Returns `Error::UnknownFormat` if the format is not registered,
    /// or a parse error if the data is invalid.
    pub fn ingest_as(&self, data: &[u8], format: &str) -> Result<Vec<Trace>> {
        for ingestor in &self.ingestors {
            if ingestor.format_name() == format {
                return ingestor.ingest(data);
            }
        }
        Err(Error::UnknownFormat(format.to_string()))
    }

    /// Returns the names of all registered formats.
    #[must_use]
    pub fn formats(&self) -> Vec<&'static str> {
        self.ingestors.iter().map(|i| i.format_name()).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_has_default_ingestors() {
        let registry = IngestorRegistry::new();
        let formats = registry.formats();

        assert!(formats.contains(&"json"));
        assert!(formats.contains(&"honeycomb"));
    }

    #[test]
    fn registry_empty_data() {
        let registry = IngestorRegistry::new();
        let result = registry.ingest(&[], None);
        assert!(result.is_ok());
        assert!(result.unwrap().is_empty());
    }

    #[test]
    fn ingest_as_unknown_format() {
        let registry = IngestorRegistry::new();
        let result = registry.ingest_as(b"data", "unknown");
        assert!(matches!(result, Err(Error::UnknownFormat(_))));
    }
}
