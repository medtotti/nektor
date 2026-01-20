//! Span data model.
//!
//! Spans represent individual units of work within a trace, providing
//! finer-grained detail than the trace-level summary.

use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::time::Duration;

/// The kind of span (client, server, internal, etc.).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SpanKind {
    /// Unspecified span kind.
    #[default]
    Unspecified,
    /// An internal operation within an application.
    Internal,
    /// Handling a synchronous request from a client.
    Server,
    /// Making a synchronous request to a server.
    Client,
    /// Initiating an asynchronous request.
    Producer,
    /// Handling an asynchronous request.
    Consumer,
}

impl SpanKind {
    /// Converts an OTLP span kind integer to `SpanKind`.
    #[must_use]
    pub const fn from_otlp(value: i32) -> Self {
        match value {
            1 => Self::Internal,
            2 => Self::Server,
            3 => Self::Client,
            4 => Self::Producer,
            5 => Self::Consumer,
            _ => Self::Unspecified,
        }
    }
}

/// Status code indicating span success or failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StatusCode {
    /// Status not set.
    #[default]
    Unset,
    /// The operation completed successfully.
    Ok,
    /// The operation resulted in an error.
    Error,
}

impl StatusCode {
    /// Converts an OTLP status code integer to `StatusCode`.
    #[must_use]
    pub const fn from_otlp(value: i32) -> Self {
        match value {
            1 => Self::Ok,
            2 => Self::Error,
            _ => Self::Unset,
        }
    }

    /// Returns true if this status represents an error.
    #[must_use]
    pub fn is_error(self) -> bool {
        self == Self::Error
    }
}

/// Status of a span operation.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct SpanStatus {
    /// The status code.
    pub code: StatusCode,
    /// Optional status message (typically for errors).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub message: Option<String>,
}

impl SpanStatus {
    /// Creates a new span status with the given code.
    #[must_use]
    pub const fn new(code: StatusCode) -> Self {
        Self {
            code,
            message: None,
        }
    }

    /// Creates an error status with a message.
    #[must_use]
    pub fn error(message: impl Into<String>) -> Self {
        Self {
            code: StatusCode::Error,
            message: Some(message.into()),
        }
    }

    /// Creates an OK status.
    #[must_use]
    pub const fn ok() -> Self {
        Self::new(StatusCode::Ok)
    }

    /// Returns true if this status represents an error.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.code.is_error()
    }
}

/// A value that can be stored as a span attribute.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum AttributeValue {
    /// A string value.
    String(String),
    /// A 64-bit integer value.
    Int(i64),
    /// A 64-bit floating-point value.
    Float(f64),
    /// A boolean value.
    Bool(bool),
    /// An array of string values.
    StringArray(Vec<String>),
}

impl AttributeValue {
    /// Converts this value to a string representation.
    #[must_use]
    pub fn as_string(&self) -> String {
        match self {
            Self::String(s) => s.clone(),
            Self::Int(i) => i.to_string(),
            Self::Float(f) => f.to_string(),
            Self::Bool(b) => b.to_string(),
            Self::StringArray(arr) => arr.join(","),
        }
    }

    /// Returns the value as an i64 if it is an integer.
    #[must_use]
    pub const fn as_i64(&self) -> Option<i64> {
        match self {
            Self::Int(i) => Some(*i),
            _ => None,
        }
    }

    /// Returns the value as a string reference if it is a string.
    #[must_use]
    pub fn as_str(&self) -> Option<&str> {
        match self {
            Self::String(s) => Some(s),
            _ => None,
        }
    }
}

impl From<String> for AttributeValue {
    fn from(s: String) -> Self {
        Self::String(s)
    }
}

impl From<&str> for AttributeValue {
    fn from(s: &str) -> Self {
        Self::String(s.to_string())
    }
}

impl From<i64> for AttributeValue {
    fn from(i: i64) -> Self {
        Self::Int(i)
    }
}

impl From<f64> for AttributeValue {
    fn from(f: f64) -> Self {
        Self::Float(f)
    }
}

impl From<bool> for AttributeValue {
    fn from(b: bool) -> Self {
        Self::Bool(b)
    }
}

/// A span representing a unit of work within a trace.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Span {
    /// Unique identifier for this span.
    pub span_id: String,
    /// Parent span ID, if this span has a parent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub parent_span_id: Option<String>,
    /// The operation name.
    pub name: String,
    /// The service name that generated this span.
    pub service: String,
    /// The duration of this span.
    pub duration: Duration,
    /// Start time in nanoseconds since Unix epoch.
    pub start_time_ns: u64,
    /// The kind of span.
    #[serde(default)]
    pub kind: SpanKind,
    /// The span status.
    #[serde(default)]
    pub status: SpanStatus,
    /// Span attributes.
    #[serde(default, skip_serializing_if = "HashMap::is_empty")]
    pub attributes: HashMap<String, AttributeValue>,
}

impl Span {
    /// Creates a new span with the given ID and name.
    #[must_use]
    pub fn new(span_id: impl Into<String>, name: impl Into<String>) -> Self {
        Self {
            span_id: span_id.into(),
            parent_span_id: None,
            name: name.into(),
            service: String::new(),
            duration: Duration::ZERO,
            start_time_ns: 0,
            kind: SpanKind::default(),
            status: SpanStatus::default(),
            attributes: HashMap::new(),
        }
    }

    /// Sets the parent span ID.
    #[must_use]
    pub fn with_parent(mut self, parent_id: impl Into<String>) -> Self {
        self.parent_span_id = Some(parent_id.into());
        self
    }

    /// Sets the service name.
    #[must_use]
    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = service.into();
        self
    }

    /// Sets the duration.
    #[must_use]
    pub const fn with_duration(mut self, duration: Duration) -> Self {
        self.duration = duration;
        self
    }

    /// Sets the start time in nanoseconds.
    #[must_use]
    pub const fn with_start_time_ns(mut self, start_time_ns: u64) -> Self {
        self.start_time_ns = start_time_ns;
        self
    }

    /// Sets the span kind.
    #[must_use]
    pub const fn with_kind(mut self, kind: SpanKind) -> Self {
        self.kind = kind;
        self
    }

    /// Sets the span status.
    #[must_use]
    pub fn with_status(mut self, status: SpanStatus) -> Self {
        self.status = status;
        self
    }

    /// Adds an attribute.
    #[must_use]
    pub fn with_attribute(mut self, key: impl Into<String>, value: impl Into<AttributeValue>) -> Self {
        self.attributes.insert(key.into(), value.into());
        self
    }

    /// Returns true if this is a root span (no parent).
    #[must_use]
    pub const fn is_root(&self) -> bool {
        self.parent_span_id.is_none()
    }

    /// Returns true if this span represents an error.
    #[must_use]
    pub fn is_error(&self) -> bool {
        self.status.is_error()
    }

    /// Gets an attribute value by key.
    #[must_use]
    pub fn get_attribute(&self, key: &str) -> Option<&AttributeValue> {
        self.attributes.get(key)
    }

    /// Gets the HTTP status code from attributes if present.
    #[must_use]
    pub fn http_status_code(&self) -> Option<u16> {
        self.attributes
            .get("http.status_code")
            .and_then(AttributeValue::as_i64)
            .and_then(|code| u16::try_from(code).ok())
    }

    /// Gets the HTTP route from attributes if present.
    #[must_use]
    pub fn http_route(&self) -> Option<&str> {
        self.attributes
            .get("http.route")
            .and_then(AttributeValue::as_str)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn span_builder() {
        let span = Span::new("span-001", "GET /users")
            .with_service("api")
            .with_duration(Duration::from_millis(50))
            .with_kind(SpanKind::Server)
            .with_attribute("http.status_code", 200i64);

        assert_eq!(span.span_id, "span-001");
        assert_eq!(span.name, "GET /users");
        assert_eq!(span.service, "api");
        assert_eq!(span.kind, SpanKind::Server);
        assert_eq!(span.http_status_code(), Some(200));
        assert!(span.is_root());
    }

    #[test]
    fn span_with_parent() {
        let span = Span::new("span-002", "db.query")
            .with_parent("span-001");

        assert!(!span.is_root());
        assert_eq!(span.parent_span_id, Some("span-001".to_string()));
    }

    #[test]
    fn span_error_detection() {
        let ok = Span::new("ok", "op").with_status(SpanStatus::ok());
        let err = Span::new("err", "op").with_status(SpanStatus::error("failed"));

        assert!(!ok.is_error());
        assert!(err.is_error());
    }

    #[test]
    fn span_kind_from_otlp() {
        assert_eq!(SpanKind::from_otlp(0), SpanKind::Unspecified);
        assert_eq!(SpanKind::from_otlp(1), SpanKind::Internal);
        assert_eq!(SpanKind::from_otlp(2), SpanKind::Server);
        assert_eq!(SpanKind::from_otlp(3), SpanKind::Client);
        assert_eq!(SpanKind::from_otlp(4), SpanKind::Producer);
        assert_eq!(SpanKind::from_otlp(5), SpanKind::Consumer);
    }

    #[test]
    fn status_code_from_otlp() {
        assert_eq!(StatusCode::from_otlp(0), StatusCode::Unset);
        assert_eq!(StatusCode::from_otlp(1), StatusCode::Ok);
        assert_eq!(StatusCode::from_otlp(2), StatusCode::Error);
    }

    #[test]
    fn attribute_value_conversions() {
        let s = AttributeValue::from("hello");
        assert_eq!(s.as_string(), "hello");

        let i = AttributeValue::from(42i64);
        assert_eq!(i.as_i64(), Some(42));
        assert_eq!(i.as_string(), "42");

        let f = AttributeValue::from(3.14f64);
        assert!(f.as_string().starts_with("3.14"));

        let b = AttributeValue::from(true);
        assert_eq!(b.as_string(), "true");
    }
}
