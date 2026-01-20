//! OTLP protobuf trace ingestor.
//!
//! Handles OpenTelemetry Protocol (OTLP) trace data in protobuf format.
//! This module is only available when the `otlp` feature is enabled.

use crate::error::{Error, Result};
use crate::ingestor::TraceIngestor;
use crate::span::{AttributeValue, Span, SpanKind, SpanStatus, StatusCode};
use crate::trace::Trace;
use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
use opentelemetry_proto::tonic::common::v1::any_value::Value as OtlpValue;
use opentelemetry_proto::tonic::common::v1::AnyValue;
use prost::Message;
use std::collections::HashMap;
use std::time::Duration;

/// Ingestor for OTLP protobuf trace data.
///
/// Decodes `ExportTraceServiceRequest` protobuf messages and converts
/// them to the internal trace representation.
pub struct OtlpIngestor;

impl TraceIngestor for OtlpIngestor {
    fn format_name(&self) -> &'static str {
        "otlp"
    }

    fn can_handle(&self, header: &[u8], content_type: Option<&str>) -> bool {
        // Check content-type first
        if let Some(ct) = content_type {
            if ct.contains("application/x-protobuf")
                || ct.contains("application/protobuf")
                || ct.contains("application/grpc")
            {
                return true;
            }
        }

        // Check for protobuf wire format markers
        // First byte 0x0A indicates a length-delimited field with field number 1
        if header.is_empty() {
            return false;
        }

        // Protobuf messages typically start with field tags
        // Field 1 with wire type 2 (length-delimited) = 0x0A
        header[0] == 0x0A
    }

    fn ingest(&self, data: &[u8]) -> Result<Vec<Trace>> {
        let request = ExportTraceServiceRequest::decode(data)
            .map_err(|e| Error::parse("otlp", format!("protobuf decode error: {e}")))?;

        let mut traces_map: HashMap<String, Vec<Span>> = HashMap::new();

        for resource_spans in request.resource_spans {
            // Extract service name from resource attributes
            let service_name = resource_spans
                .resource
                .as_ref()
                .and_then(|r| {
                    r.attributes.iter().find_map(|attr| {
                        if attr.key == "service.name" {
                            attr.value.as_ref().and_then(extract_string_value)
                        } else {
                            None
                        }
                    })
                })
                .unwrap_or_default();

            for scope_spans in resource_spans.scope_spans {
                for otlp_span in scope_spans.spans {
                    // Convert trace_id and span_id from bytes to hex strings
                    let trace_id = hex::encode(&otlp_span.trace_id);
                    let span_id = hex::encode(&otlp_span.span_id);
                    let parent_span_id = if otlp_span.parent_span_id.is_empty() {
                        None
                    } else {
                        Some(hex::encode(&otlp_span.parent_span_id))
                    };

                    // Calculate duration from start and end times (nanoseconds)
                    let duration_ns = otlp_span
                        .end_time_unix_nano
                        .saturating_sub(otlp_span.start_time_unix_nano);
                    let duration = Duration::from_nanos(duration_ns);

                    // Build span
                    let mut span = Span::new(&span_id, &otlp_span.name)
                        .with_service(&service_name)
                        .with_duration(duration)
                        .with_start_time_ns(otlp_span.start_time_unix_nano)
                        .with_kind(SpanKind::from_otlp(otlp_span.kind));

                    if let Some(parent) = parent_span_id {
                        span = span.with_parent(parent);
                    }

                    // Convert status
                    if let Some(status) = &otlp_span.status {
                        let code = StatusCode::from_otlp(status.code);
                        let span_status = if status.message.is_empty() {
                            SpanStatus::new(code)
                        } else {
                            SpanStatus {
                                code,
                                message: Some(status.message.clone()),
                            }
                        };
                        span = span.with_status(span_status);
                    }

                    // Convert attributes
                    for attr in &otlp_span.attributes {
                        if let Some(value) = &attr.value {
                            if let Some(attr_value) = convert_any_value(value) {
                                span = span.with_attribute(&attr.key, attr_value);
                            }
                        }
                    }

                    traces_map.entry(trace_id).or_default().push(span);
                }
            }
        }

        // Convert grouped spans to traces
        let traces = traces_map
            .into_iter()
            .map(|(trace_id, spans)| Trace::from_spans(trace_id, spans))
            .collect();

        Ok(traces)
    }
}

/// Extracts a string value from an OTLP `AnyValue`.
fn extract_string_value(value: &AnyValue) -> Option<String> {
    value.value.as_ref().and_then(|v| match v {
        OtlpValue::StringValue(s) => Some(s.clone()),
        _ => None,
    })
}

/// Converts an OTLP `AnyValue` to an `AttributeValue`.
fn convert_any_value(value: &AnyValue) -> Option<AttributeValue> {
    value.value.as_ref().and_then(|v| match v {
        OtlpValue::StringValue(s) => Some(AttributeValue::String(s.clone())),
        OtlpValue::BoolValue(b) => Some(AttributeValue::Bool(*b)),
        OtlpValue::IntValue(i) => Some(AttributeValue::Int(*i)),
        OtlpValue::DoubleValue(d) => Some(AttributeValue::Float(*d)),
        OtlpValue::ArrayValue(arr) => {
            let strings: Vec<String> = arr
                .values
                .iter()
                .filter_map(extract_string_value)
                .collect();
            if strings.is_empty() {
                None
            } else {
                Some(AttributeValue::StringArray(strings))
            }
        }
        OtlpValue::BytesValue(bytes) => Some(AttributeValue::String(hex::encode(bytes))),
        OtlpValue::KvlistValue(_) => None, // Skip nested key-value lists
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use opentelemetry_proto::tonic::collector::trace::v1::ExportTraceServiceRequest;
    use opentelemetry_proto::tonic::common::v1::{any_value, AnyValue, KeyValue};
    use opentelemetry_proto::tonic::resource::v1::Resource;
    use opentelemetry_proto::tonic::trace::v1::{
        ResourceSpans, ScopeSpans, Span as OtlpSpan, Status,
    };

    fn create_test_request() -> Vec<u8> {
        let request = ExportTraceServiceRequest {
            resource_spans: vec![ResourceSpans {
                resource: Some(Resource {
                    attributes: vec![KeyValue {
                        key: "service.name".to_string(),
                        value: Some(AnyValue {
                            value: Some(any_value::Value::StringValue("test-service".to_string())),
                        }),
                    }],
                    dropped_attributes_count: 0,
                    entity_refs: vec![],
                }),
                scope_spans: vec![ScopeSpans {
                    scope: None,
                    spans: vec![OtlpSpan {
                        trace_id: vec![0x01, 0x02, 0x03, 0x04, 0x05, 0x06, 0x07, 0x08,
                                       0x09, 0x0a, 0x0b, 0x0c, 0x0d, 0x0e, 0x0f, 0x10],
                        span_id: vec![0x11, 0x12, 0x13, 0x14, 0x15, 0x16, 0x17, 0x18],
                        parent_span_id: vec![],
                        name: "test-operation".to_string(),
                        kind: 2, // Server
                        start_time_unix_nano: 1_000_000_000,
                        end_time_unix_nano: 1_100_000_000, // 100ms duration
                        attributes: vec![KeyValue {
                            key: "http.status_code".to_string(),
                            value: Some(AnyValue {
                                value: Some(any_value::Value::IntValue(200)),
                            }),
                        }],
                        status: Some(Status {
                            code: 1, // Ok
                            message: String::new(),
                        }),
                        dropped_attributes_count: 0,
                        events: vec![],
                        dropped_events_count: 0,
                        links: vec![],
                        dropped_links_count: 0,
                        trace_state: String::new(),
                        flags: 0,
                    }],
                    schema_url: String::new(),
                }],
                schema_url: String::new(),
            }],
        };

        request.encode_to_vec()
    }

    #[test]
    fn otlp_ingestor_format_name() {
        let ingestor = OtlpIngestor;
        assert_eq!(ingestor.format_name(), "otlp");
    }

    #[test]
    fn otlp_ingestor_can_handle_content_type() {
        let ingestor = OtlpIngestor;
        assert!(ingestor.can_handle(&[], Some("application/x-protobuf")));
        assert!(ingestor.can_handle(&[], Some("application/protobuf")));
        assert!(ingestor.can_handle(&[], Some("application/grpc")));
    }

    #[test]
    fn otlp_ingestor_can_handle_protobuf_header() {
        let ingestor = OtlpIngestor;
        assert!(ingestor.can_handle(&[0x0A, 0x10], None));
    }

    #[test]
    fn otlp_ingestor_rejects_json() {
        let ingestor = OtlpIngestor;
        assert!(!ingestor.can_handle(b"[{\"trace_id\":\"abc\"}]", Some("application/json")));
    }

    #[test]
    fn otlp_ingestor_ingest() {
        let ingestor = OtlpIngestor;
        let data = create_test_request();

        let traces = ingestor.ingest(&data).unwrap();
        assert_eq!(traces.len(), 1);

        let trace = &traces[0];
        assert_eq!(trace.trace_id, "0102030405060708090a0b0c0d0e0f10");
        assert_eq!(trace.span_count, 1);
        assert_eq!(trace.service, Some("test-service".to_string()));

        let span = &trace.spans()[0];
        assert_eq!(span.span_id, "1112131415161718");
        assert_eq!(span.name, "test-operation");
        assert_eq!(span.kind, SpanKind::Server);
        assert_eq!(span.duration, Duration::from_nanos(100_000_000)); // 100ms
        assert!(span.is_root());
    }

    #[test]
    fn otlp_ingestor_extracts_attributes() {
        let ingestor = OtlpIngestor;
        let data = create_test_request();

        let traces = ingestor.ingest(&data).unwrap();
        let span = &traces[0].spans()[0];

        assert_eq!(
            span.get_attribute("http.status_code"),
            Some(&AttributeValue::Int(200))
        );
    }

    #[test]
    fn convert_any_value_string() {
        let value = AnyValue {
            value: Some(any_value::Value::StringValue("test".to_string())),
        };
        assert_eq!(
            convert_any_value(&value),
            Some(AttributeValue::String("test".to_string()))
        );
    }

    #[test]
    fn convert_any_value_int() {
        let value = AnyValue {
            value: Some(any_value::Value::IntValue(42)),
        };
        assert_eq!(convert_any_value(&value), Some(AttributeValue::Int(42)));
    }

    #[test]
    fn convert_any_value_bool() {
        let value = AnyValue {
            value: Some(any_value::Value::BoolValue(true)),
        };
        assert_eq!(convert_any_value(&value), Some(AttributeValue::Bool(true)));
    }
}
