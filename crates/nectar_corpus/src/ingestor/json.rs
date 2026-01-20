//! JSON trace ingestor.
//!
//! Handles plain JSON trace data in the format expected by the existing loader.

use crate::corpus::Corpus;
use crate::error::{Error, Result};
use crate::ingestor::TraceIngestor;
use crate::trace::Trace;

/// Ingestor for plain JSON trace data.
///
/// Supports two formats:
/// - A JSON array of trace objects
/// - A JSON object with a "traces" field containing the array
pub struct JsonIngestor;

impl TraceIngestor for JsonIngestor {
    fn format_name(&self) -> &'static str {
        "json"
    }

    fn can_handle(&self, header: &[u8], content_type: Option<&str>) -> bool {
        // Check content-type first
        if let Some(ct) = content_type {
            if ct.contains("application/json") {
                return true;
            }
        }

        // Check if it looks like JSON (starts with [ or {)
        let trimmed = trim_leading_whitespace(header);
        if trimmed.is_empty() {
            return false;
        }

        let first_byte = trimmed[0];
        first_byte == b'[' || first_byte == b'{'
    }

    fn ingest(&self, data: &[u8]) -> Result<Vec<Trace>> {
        let json_str = std::str::from_utf8(data)
            .map_err(|e| Error::parse("json", format!("invalid UTF-8: {e}")))?;

        // Use the existing Corpus::parse_json which handles the conversion
        let corpus = Corpus::parse_json(json_str)?;
        Ok(corpus.into_traces())
    }
}

/// Trims leading whitespace bytes from a slice.
fn trim_leading_whitespace(data: &[u8]) -> &[u8] {
    let mut start = 0;
    for (i, &byte) in data.iter().enumerate() {
        if !byte.is_ascii_whitespace() {
            start = i;
            break;
        }
    }
    &data[start..]
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn json_ingestor_format_name() {
        let ingestor = JsonIngestor;
        assert_eq!(ingestor.format_name(), "json");
    }

    #[test]
    fn json_ingestor_can_handle_array() {
        let ingestor = JsonIngestor;
        assert!(ingestor.can_handle(b"[{\"trace_id\": \"abc\"}]", None));
    }

    #[test]
    fn json_ingestor_can_handle_object() {
        let ingestor = JsonIngestor;
        assert!(ingestor.can_handle(b"{\"traces\": []}", None));
    }

    #[test]
    fn json_ingestor_can_handle_content_type() {
        let ingestor = JsonIngestor;
        assert!(ingestor.can_handle(b"anything", Some("application/json")));
        assert!(ingestor.can_handle(b"anything", Some("application/json; charset=utf-8")));
    }

    #[test]
    fn json_ingestor_rejects_protobuf() {
        let ingestor = JsonIngestor;
        assert!(!ingestor.can_handle(b"\x0a\x0b", Some("application/x-protobuf")));
    }

    #[test]
    fn json_ingestor_ingest_array() {
        let ingestor = JsonIngestor;
        let data = br#"[
            {"trace_id": "abc", "duration_ms": 100, "status": 200, "service": "api"},
            {"trace_id": "def", "duration_ms": 200, "status": 500, "service": "db"}
        ]"#;

        let traces = ingestor.ingest(data).unwrap();
        assert_eq!(traces.len(), 2);
        assert_eq!(traces[0].trace_id, "abc");
        assert_eq!(traces[1].trace_id, "def");
        assert!(traces[1].is_error);
    }

    #[test]
    fn json_ingestor_ingest_object() {
        let ingestor = JsonIngestor;
        let data = br#"{"traces": [{"trace_id": "xyz"}]}"#;

        let traces = ingestor.ingest(data).unwrap();
        assert_eq!(traces.len(), 1);
        assert_eq!(traces[0].trace_id, "xyz");
    }

    #[test]
    fn json_ingestor_handles_whitespace() {
        let ingestor = JsonIngestor;
        assert!(ingestor.can_handle(b"  \n  [{\"trace_id\": \"abc\"}]", None));
    }
}
