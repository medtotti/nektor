//! Trace data model.

use crate::span::Span;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// A trace exemplar for policy simulation.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
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
    /// Individual spans within this trace.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub spans: Vec<Span>,
    /// Number of spans in this trace.
    #[serde(default)]
    pub span_count: usize,
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
            spans: Vec::new(),
            span_count: 0,
        }
    }

    /// Creates a new trace from a collection of spans.
    ///
    /// This constructor creates a trace and automatically computes
    /// summary fields from the provided spans.
    #[must_use]
    pub fn from_spans(trace_id: impl Into<String>, spans: Vec<Span>) -> Self {
        let mut trace = Self {
            trace_id: trace_id.into(),
            duration: Duration::ZERO,
            status: None,
            service: None,
            endpoint: None,
            is_error: false,
            attributes: HashMap::new(),
            span_count: spans.len(),
            spans,
        };
        trace.compute_summary_from_spans();
        trace
    }

    /// Computes trace-level summary fields from spans.
    ///
    /// Extracts the following from spans:
    /// - `duration`: Total trace duration (from earliest start to latest end)
    /// - `service`: Service name from the root span
    /// - `endpoint`: HTTP route or operation name from the root span
    /// - `is_error`: True if any span has an error status
    /// - `status`: HTTP status code from the root span
    pub fn compute_summary_from_spans(&mut self) {
        if self.spans.is_empty() {
            return;
        }

        // Update span_count
        self.span_count = self.spans.len();

        // Find root span (no parent)
        let root_span = self
            .spans
            .iter()
            .find(|s: &&Span| s.is_root())
            .or_else(|| self.spans.first());

        // Find the earliest start and latest end times
        let mut min_start = u64::MAX;
        let mut max_end = 0u64;

        for span in &self.spans {
            min_start = min_start.min(span.start_time_ns);
            // Duration in nanos can exceed u64 for very long durations, so cap it
            #[allow(clippy::cast_possible_truncation)]
            let duration_ns = span.duration.as_nanos().min(u128::from(u64::MAX)) as u64;
            let end = span.start_time_ns.saturating_add(duration_ns);
            max_end = max_end.max(end);
        }

        // Compute total duration
        if min_start < u64::MAX && max_end > min_start {
            self.duration = Duration::from_nanos(max_end - min_start);
        }

        // Extract fields from root span
        if let Some(root) = root_span {
            if self.service.is_none() && !root.service.is_empty() {
                self.service = Some(root.service.clone());
            }

            if self.endpoint.is_none() {
                // Try http.route attribute first, then fall back to span name
                self.endpoint = root
                    .http_route()
                    .map(String::from)
                    .or_else(|| Some(root.name.clone()));
            }

            if self.status.is_none() {
                self.status = root.http_status_code();
            }
        }

        // Check for any error spans
        self.is_error = self.spans.iter().any(Span::is_error);

        // Also check HTTP status codes >= 500 as errors
        if !self.is_error {
            if let Some(status) = self.status {
                self.is_error = status >= 500;
            }
        }
    }

    /// Adds a span to this trace.
    pub fn add_span(&mut self, span: Span) {
        self.spans.push(span);
        self.span_count = self.spans.len();
    }

    /// Returns the spans in this trace.
    #[must_use]
    pub fn spans(&self) -> &[Span] {
        &self.spans
    }

    /// Returns the start time of this trace in nanoseconds since epoch.
    ///
    /// This is the earliest `start_time_ns` of any span in the trace.
    /// Returns `None` if the trace has no spans.
    #[must_use]
    pub fn start_time_ns(&self) -> Option<u64> {
        self.spans.iter().map(|s| s.start_time_ns).min()
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
    use crate::span::{SpanKind, SpanStatus};

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

    #[test]
    fn trace_from_spans() {
        let start_ns = 1_000_000_000u64; // 1 second
        let spans = vec![
            Span::new("span-1", "GET /api/users")
                .with_service("api-gateway")
                .with_start_time_ns(start_ns)
                .with_duration(Duration::from_millis(100))
                .with_kind(SpanKind::Server)
                .with_attribute("http.status_code", 200i64)
                .with_attribute("http.route", "/api/users".to_string()),
            Span::new("span-2", "db.query")
                .with_parent("span-1")
                .with_service("api-gateway")
                .with_start_time_ns(start_ns + 10_000_000) // 10ms later
                .with_duration(Duration::from_millis(50)),
        ];

        let trace = Trace::from_spans("trace-001", spans);

        assert_eq!(trace.trace_id, "trace-001");
        assert_eq!(trace.span_count, 2);
        assert_eq!(trace.service, Some("api-gateway".to_string()));
        assert_eq!(trace.endpoint, Some("/api/users".to_string()));
        assert_eq!(trace.status, Some(200));
        assert!(!trace.is_error);
        // Duration should be from earliest start to latest end
        assert!(trace.duration >= Duration::from_millis(100));
    }

    #[test]
    fn trace_error_from_span_status() {
        let spans = vec![
            Span::new("span-1", "request")
                .with_service("api")
                .with_start_time_ns(0)
                .with_duration(Duration::from_millis(50)),
            Span::new("span-2", "error-op")
                .with_parent("span-1")
                .with_service("api")
                .with_start_time_ns(10_000_000)
                .with_duration(Duration::from_millis(20))
                .with_status(SpanStatus::error("failed")),
        ];

        let trace = Trace::from_spans("trace-error", spans);
        assert!(trace.is_error);
    }

    #[test]
    fn trace_add_span() {
        let mut trace = Trace::new("trace-001");
        assert_eq!(trace.span_count, 0);

        trace.add_span(
            Span::new("span-1", "op1")
                .with_service("svc")
                .with_start_time_ns(0)
                .with_duration(Duration::from_millis(10)),
        );

        assert_eq!(trace.span_count, 1);
        assert_eq!(trace.spans().len(), 1);
    }

    #[test]
    fn compute_summary_updates_fields() {
        let mut trace = Trace::new("trace-001");
        trace.spans = vec![
            Span::new("root", "GET /health")
                .with_service("api")
                .with_start_time_ns(0)
                .with_duration(Duration::from_millis(25))
                .with_attribute("http.status_code", 500i64),
        ];

        trace.compute_summary_from_spans();

        assert_eq!(trace.span_count, 1);
        assert_eq!(trace.service, Some("api".to_string()));
        assert_eq!(trace.endpoint, Some("GET /health".to_string()));
        assert_eq!(trace.status, Some(500));
        assert!(trace.is_error); // 500 status code
    }
}
