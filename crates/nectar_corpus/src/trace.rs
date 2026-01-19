//! Trace data model.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// A trace exemplar for policy simulation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Trace {
    /// Unique trace identifier.
    pub trace_id: String,
    /// Trace duration.
    pub duration: Duration,
    /// HTTP status code (if applicable).
    pub status: Option<u16>,
    /// Service name.
    pub service: Option<String>,
    /// Endpoint/operation name.
    pub endpoint: Option<String>,
    /// Whether this trace represents an error.
    pub is_error: bool,
    /// Additional attributes.
    pub attributes: HashMap<String, String>,
}

impl Trace {
    /// Creates a new trace with the given ID.
    #[must_use]
    pub fn new(trace_id: impl Into<String>) -> Self {
        Self {
            trace_id: trace_id.into(),
            duration: Duration::ZERO,
            status: None,
            service: None,
            endpoint: None,
            is_error: false,
            attributes: HashMap::new(),
        }
    }

    /// Sets the duration.
    #[must_use]
    pub const fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Sets the HTTP status.
    #[must_use]
    pub const fn with_status(mut self, status: u16) -> Self {
        self.status = Some(status);
        self.is_error = status >= 500;
        self
    }

    /// Sets the service name.
    #[must_use]
    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = Some(service.into());
        self
    }

    /// Sets the endpoint.
    #[must_use]
    pub fn with_endpoint(mut self, endpoint: impl Into<String>) -> Self {
        self.endpoint = Some(endpoint.into());
        self
    }

    /// Adds an attribute.
    #[must_use]
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<String>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn trace_builder() {
        let trace = Trace::new("abc123")
            .with_duration(Duration::from_millis(150))
            .with_status(200)
            .with_service("api")
            .with_endpoint("/users");

        assert_eq!(trace.trace_id, "abc123");
        assert_eq!(trace.duration, Duration::from_millis(150));
        assert_eq!(trace.status, Some(200));
        assert!(!trace.is_error);
    }

    #[test]
    fn trace_error_detection() {
        let ok = Trace::new("ok").with_status(200);
        let error = Trace::new("err").with_status(500);

        assert!(!ok.is_error);
        assert!(error.is_error);
    }
}
