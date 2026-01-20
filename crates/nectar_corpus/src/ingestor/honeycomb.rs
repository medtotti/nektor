//! Honeycomb NDJSON trace ingestor.
//!
//! Handles Honeycomb query result export format, which is NDJSON with
//! `trace.trace_id`, `trace.span_id`, and `trace.parent_id` fields.

use crate::error::{Error, Result};
use crate::ingestor::TraceIngestor;
use crate::span::{AttributeValue, Span, SpanKind, SpanStatus, StatusCode};
use crate::trace::Trace;
use serde_json::Value;
use std::collections::HashMap;
use std::time::Duration;

/// Ingestor for Honeycomb NDJSON export format.
///
/// Parses newline-delimited JSON where each line is a span with
/// Honeycomb-style trace fields:
/// - `trace.trace_id`: The trace identifier
/// - `trace.span_id`: The span identifier
/// - `trace.parent_id`: The parent span identifier (optional)
/// - `duration_ms`: Span duration in milliseconds
/// - `service.name` or `service_name`: Service name
pub struct HoneycombIngestor;

impl TraceIngestor for HoneycombIngestor {
    fn format_name(&self) -> &'static str {
        "honeycomb"
    }

    fn can_handle(&self, header: &[u8], content_type: Option<&str>) -> bool {
        // Check content-type
        if let Some(ct) = content_type {
            if ct.contains("application/x-ndjson") || ct.contains("application/x-honeycomb") {
                return true;
            }
        }

        // Check for Honeycomb-specific fields in the first line
        let first_line = get_first_line(header);
        if first_line.is_empty() {
            return false;
        }

        // Must start with { (JSON object)
        if first_line[0] != b'{' {
            return false;
        }

        // Check for Honeycomb trace fields
        let Ok(line_str) = std::str::from_utf8(first_line) else {
            return false;
        };

        line_str.contains("\"trace.trace_id\"")
            || line_str.contains("\"trace.span_id\"")
            || (line_str.contains("\"trace_id\"") && line_str.contains("\"span_id\""))
    }

    fn ingest(&self, data: &[u8]) -> Result<Vec<Trace>> {
        let text = std::str::from_utf8(data)
            .map_err(|e| Error::parse("honeycomb", format!("invalid UTF-8: {e}")))?;

        // Parse each line as a span and group by trace_id
        let mut traces_map: HashMap<String, Vec<RawSpan>> = HashMap::new();

        for (line_num, line) in text.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() {
                continue;
            }

            match parse_honeycomb_span(line) {
                Ok(span) => {
                    traces_map
                        .entry(span.trace_id.clone())
                        .or_default()
                        .push(span);
                }
                Err(e) => {
                    tracing::warn!("Skipping invalid span at line {}: {}", line_num + 1, e);
                }
            }
        }

        // Convert grouped spans to traces
        let mut traces = Vec::with_capacity(traces_map.len());
        for (trace_id, raw_spans) in traces_map {
            let spans: Vec<Span> = raw_spans.into_iter().map(RawSpan::into_span).collect();
            traces.push(Trace::from_spans(trace_id, spans));
        }

        Ok(traces)
    }
}

/// Raw span data parsed from Honeycomb JSON.
struct RawSpan {
    trace_id: String,
    span_id: String,
    parent_id: Option<String>,
    name: String,
    service: String,
    duration_ms: f64,
    start_time_ms: Option<f64>,
    status_code: Option<i32>,
    status_message: Option<String>,
    is_error: bool,
    span_kind: Option<i32>,
    attributes: HashMap<String, AttributeValue>,
}

impl RawSpan {
    fn into_span(self) -> Span {
        let mut span = Span::new(&self.span_id, &self.name)
            .with_service(&self.service)
            .with_duration(Duration::from_secs_f64(self.duration_ms / 1000.0));

        if let Some(parent) = self.parent_id {
            span = span.with_parent(parent);
        }

        if let Some(start_ms) = self.start_time_ms {
            // Convert milliseconds to nanoseconds
            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
            let start_ns = (start_ms * 1_000_000.0) as u64;
            span = span.with_start_time_ns(start_ns);
        }

        // Set status
        let status = match (self.is_error, self.status_code) {
            (true, _) => SpanStatus::error(self.status_message.unwrap_or_default()),
            (false, Some(code)) => SpanStatus::new(StatusCode::from_otlp(code)),
            _ => SpanStatus::default(),
        };
        span = span.with_status(status);

        // Set span kind
        if let Some(kind) = self.span_kind {
            span = span.with_kind(SpanKind::from_otlp(kind));
        }

        // Add attributes
        for (key, value) in self.attributes {
            span = span.with_attribute(key, value);
        }

        span
    }
}

/// Parses a single Honeycomb span from a JSON line.
fn parse_honeycomb_span(line: &str) -> Result<RawSpan> {
    let obj: Value = serde_json::from_str(line)
        .map_err(|e| Error::parse("honeycomb", format!("invalid JSON: {e}")))?;

    let obj = obj
        .as_object()
        .ok_or_else(|| Error::parse("honeycomb", "expected JSON object"))?;

    // Extract trace_id (try multiple field names)
    let trace_id = get_string_field(obj, &["trace.trace_id", "trace_id"])
        .ok_or_else(|| Error::parse("honeycomb", "missing trace_id"))?;

    // Extract span_id
    let span_id = get_string_field(obj, &["trace.span_id", "span_id"])
        .ok_or_else(|| Error::parse("honeycomb", "missing span_id"))?;

    // Extract parent_id (optional)
    let parent_id = get_string_field(obj, &["trace.parent_id", "parent_id"]);

    // Extract service name
    let service = get_string_field(obj, &["service.name", "service_name", "service"])
        .unwrap_or_default();

    // Extract operation name
    let name = get_string_field(obj, &["name", "operation", "span.name"])
        .unwrap_or_else(|| "unknown".to_string());

    // Extract duration
    let duration_ms = get_number_field(obj, &["duration_ms", "duration"])
        .unwrap_or(0.0);

    // Extract start time
    let start_time_ms = get_number_field(obj, &["timestamp_ms", "start_time_ms", "time"]);

    // Extract error status
    let is_error = get_bool_field(obj, &["error", "is_error"])
        || get_number_field(obj, &["http.status_code", "status_code"])
            .is_some_and(|code| code >= 500.0);

    // Extract status
    #[allow(clippy::cast_possible_truncation)]
    let status_code = get_number_field(obj, &["status.code", "status_code"])
        .map(|n| n as i32);
    let status_message = get_string_field(obj, &["status.message", "status_message"]);

    // Extract span kind
    #[allow(clippy::cast_possible_truncation)]
    let span_kind = get_number_field(obj, &["span.kind", "kind"])
        .map(|n| n as i32);

    // Extract remaining attributes
    let mut attributes = HashMap::new();
    for (key, value) in obj {
        // Skip fields we've already processed
        if is_known_field(key) {
            continue;
        }

        if let Some(attr_val) = json_to_attribute(value) {
            attributes.insert(key.clone(), attr_val);
        }
    }

    Ok(RawSpan {
        trace_id,
        span_id,
        parent_id,
        name,
        service,
        duration_ms,
        start_time_ms,
        status_code,
        status_message,
        is_error,
        span_kind,
        attributes,
    })
}

/// Gets the first line from a byte slice.
fn get_first_line(data: &[u8]) -> &[u8] {
    let trimmed = trim_leading_whitespace(data);
    trimmed
        .iter()
        .position(|&b| b == b'\n')
        .map_or(trimmed, |pos| &trimmed[..pos])
}

/// Trims leading whitespace bytes.
fn trim_leading_whitespace(data: &[u8]) -> &[u8] {
    for (i, &byte) in data.iter().enumerate() {
        if !byte.is_ascii_whitespace() {
            return &data[i..];
        }
    }
    &[]
}

/// Gets a string field from the object, trying multiple field names.
fn get_string_field(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<String> {
    for key in keys {
        if let Some(Value::String(s)) = obj.get(*key) {
            return Some(s.clone());
        }
    }
    None
}

/// Gets a number field from the object, trying multiple field names.
#[allow(clippy::cast_precision_loss)]
fn get_number_field(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> Option<f64> {
    for key in keys {
        if let Some(value) = obj.get(*key) {
            if let Some(n) = value.as_f64() {
                return Some(n);
            }
            if let Some(n) = value.as_i64() {
                // Precision loss is acceptable for typical numeric values
                return Some(n as f64);
            }
        }
    }
    None
}

/// Gets a boolean field from the object, trying multiple field names.
fn get_bool_field(obj: &serde_json::Map<String, Value>, keys: &[&str]) -> bool {
    for key in keys {
        if let Some(Value::Bool(b)) = obj.get(*key) {
            return *b;
        }
    }
    false
}

/// Checks if a field name is a known/processed field.
fn is_known_field(key: &str) -> bool {
    matches!(
        key,
        "trace.trace_id"
            | "trace_id"
            | "trace.span_id"
            | "span_id"
            | "trace.parent_id"
            | "parent_id"
            | "service.name"
            | "service_name"
            | "service"
            | "name"
            | "operation"
            | "span.name"
            | "duration_ms"
            | "duration"
            | "timestamp_ms"
            | "start_time_ms"
            | "time"
            | "error"
            | "is_error"
            | "status.code"
            | "status_code"
            | "status.message"
            | "status_message"
            | "span.kind"
            | "kind"
    )
}

/// Converts a JSON value to an attribute value.
fn json_to_attribute(value: &Value) -> Option<AttributeValue> {
    match value {
        Value::String(s) => Some(AttributeValue::String(s.clone())),
        Value::Number(n) => n
            .as_i64()
            .map(AttributeValue::Int)
            .or_else(|| n.as_f64().map(AttributeValue::Float)),
        Value::Bool(b) => Some(AttributeValue::Bool(*b)),
        Value::Array(arr) => {
            let strings: Vec<String> = arr
                .iter()
                .filter_map(|v| v.as_str().map(String::from))
                .collect();
            if strings.is_empty() {
                None
            } else {
                Some(AttributeValue::StringArray(strings))
            }
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honeycomb_ingestor_format_name() {
        let ingestor = HoneycombIngestor;
        assert_eq!(ingestor.format_name(), "honeycomb");
    }

    #[test]
    fn honeycomb_ingestor_can_handle_trace_fields() {
        let ingestor = HoneycombIngestor;
        let data = br#"{"trace.trace_id":"abc","trace.span_id":"123"}"#;
        assert!(ingestor.can_handle(data, None));
    }

    #[test]
    fn honeycomb_ingestor_can_handle_content_type() {
        let ingestor = HoneycombIngestor;
        assert!(ingestor.can_handle(b"{}", Some("application/x-ndjson")));
    }

    #[test]
    fn honeycomb_ingestor_rejects_plain_json_array() {
        let ingestor = HoneycombIngestor;
        // Plain JSON array without Honeycomb fields
        assert!(!ingestor.can_handle(b"[{\"id\":1}]", None));
    }

    #[test]
    fn honeycomb_ingestor_ingest_single_span() {
        let ingestor = HoneycombIngestor;
        let data = br#"{"trace.trace_id":"trace-1","trace.span_id":"span-1","name":"GET /api","service.name":"api","duration_ms":100}"#;

        let traces = ingestor.ingest(data).unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].trace_id, "trace-1");
        assert_eq!(traces[0].span_count, 1);
        assert_eq!(traces[0].service, Some("api".to_string()));
    }

    #[test]
    fn honeycomb_ingestor_ingest_multiple_spans_same_trace() {
        let ingestor = HoneycombIngestor;
        let data = br#"{"trace.trace_id":"trace-1","trace.span_id":"span-1","name":"parent","service.name":"api","duration_ms":100}
{"trace.trace_id":"trace-1","trace.span_id":"span-2","trace.parent_id":"span-1","name":"child","service.name":"api","duration_ms":50}"#;

        let traces = ingestor.ingest(data).unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].span_count, 2);
    }

    #[test]
    fn honeycomb_ingestor_ingest_multiple_traces() {
        let ingestor = HoneycombIngestor;
        let data = br#"{"trace.trace_id":"trace-1","trace.span_id":"span-1","name":"op1","service.name":"svc","duration_ms":100}
{"trace.trace_id":"trace-2","trace.span_id":"span-2","name":"op2","service.name":"svc","duration_ms":200}"#;

        let traces = ingestor.ingest(data).unwrap();
        assert_eq!(traces.len(), 2);
    }

    #[test]
    fn honeycomb_ingestor_error_detection() {
        let ingestor = HoneycombIngestor;
        let data = br#"{"trace.trace_id":"trace-1","trace.span_id":"span-1","name":"fail","service.name":"api","duration_ms":50,"error":true}"#;

        let traces = ingestor.ingest(data).unwrap();
        assert!(traces[0].is_error);
    }

    #[test]
    fn honeycomb_ingestor_http_status_error() {
        let ingestor = HoneycombIngestor;
        let data = br#"{"trace.trace_id":"trace-1","trace.span_id":"span-1","name":"req","service.name":"api","duration_ms":50,"http.status_code":503}"#;

        let traces = ingestor.ingest(data).unwrap();
        assert!(traces[0].is_error);
        assert_eq!(traces[0].status, Some(503));
    }

    #[test]
    fn honeycomb_ingestor_preserves_attributes() {
        let ingestor = HoneycombIngestor;
        let data = br#"{"trace.trace_id":"t","trace.span_id":"s","name":"op","service.name":"svc","duration_ms":1,"custom.field":"value","count":42}"#;

        let traces = ingestor.ingest(data).unwrap();
        let span = &traces[0].spans()[0];

        assert_eq!(
            span.get_attribute("custom.field"),
            Some(&AttributeValue::String("value".to_string()))
        );
        assert_eq!(
            span.get_attribute("count"),
            Some(&AttributeValue::Int(42))
        );
    }

    #[test]
    fn parse_span_with_alternative_field_names() {
        let line = r#"{"trace_id":"t1","span_id":"s1","parent_id":"p1","operation":"op","service":"svc","duration":100}"#;
        let span = parse_honeycomb_span(line).unwrap();

        assert_eq!(span.trace_id, "t1");
        assert_eq!(span.span_id, "s1");
        assert_eq!(span.parent_id, Some("p1".to_string()));
        assert_eq!(span.name, "op");
        assert_eq!(span.service, "svc");
        assert_eq!(span.duration_ms, 100.0);
    }
}
