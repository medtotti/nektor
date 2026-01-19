//! Corpus loading from files and directories.

use crate::corpus::Corpus;
use crate::error::{Error, Result};
use crate::trace::Trace;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::Path;
use std::time::Duration;
use tracing::{debug, info, warn};

/// Raw trace format for JSON input.
#[derive(Debug, Deserialize)]
struct RawTrace {
    trace_id: String,
    #[serde(default)]
    duration_ms: Option<u64>,
    #[serde(default)]
    duration: Option<String>,
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    service: Option<String>,
    #[serde(default)]
    endpoint: Option<String>,
    #[serde(default)]
    is_error: Option<bool>,
    #[serde(default)]
    error: Option<bool>,
    #[serde(flatten)]
    extra: HashMap<String, serde_json::Value>,
}

impl RawTrace {
    fn into_trace(self) -> Result<Trace> {
        let mut trace = Trace::new(&self.trace_id);

        // Parse duration
        if let Some(ms) = self.duration_ms {
            trace = trace.with_duration(Duration::from_millis(ms));
        } else if let Some(dur_str) = self.duration {
            trace = trace.with_duration(parse_duration(&dur_str)?);
        }

        // Parse status (check extra fields for OpenTelemetry-style names)
        let status_code = self.status.or_else(|| {
            self.extra
                .get("http.status_code")
                .and_then(serde_json::Value::as_u64)
                .and_then(|n| u16::try_from(n).ok())
        });
        if let Some(s) = status_code {
            trace = trace.with_status(s);
        }

        // Parse service (check extra fields for OpenTelemetry-style names)
        let service = self.service.or_else(|| {
            self.extra
                .get("service.name")
                .and_then(|v| v.as_str())
                .map(String::from)
        });
        if let Some(svc) = service {
            trace = trace.with_service(svc);
        }

        // Parse endpoint (check extra fields for OpenTelemetry-style names)
        let endpoint = self.endpoint.or_else(|| {
            self.extra
                .get("http.route")
                .and_then(|v| v.as_str())
                .map(String::from)
        });
        if let Some(ep) = endpoint {
            trace = trace.with_endpoint(ep);
        }

        // Handle explicit error flag
        if let Some(err) = self.is_error.or(self.error) {
            trace.is_error = err;
        }

        // Add remaining attributes
        for (key, value) in self.extra {
            // Skip internal fields we've already processed
            if matches!(
                key.as_str(),
                "http.status_code" | "service.name" | "http.route"
            ) {
                continue;
            }
            let value_str = match value {
                serde_json::Value::String(s) => s,
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => continue,
            };
            trace = trace.with_attribute(key, value_str);
        }

        Ok(trace)
    }
}

/// Parses a duration string like "150ms", "2.5s", "100".
fn parse_duration(s: &str) -> Result<Duration> {
    let s = s.trim();

    // Try parsing as milliseconds if just a number
    if let Ok(ms) = s.parse::<u64>() {
        return Ok(Duration::from_millis(ms));
    }

    // Parse with suffix
    if let Some(ms_str) = s.strip_suffix("ms") {
        let ms: u64 = ms_str
            .trim()
            .parse()
            .map_err(|_| Error::InvalidTrace(format!("invalid duration: {s}")))?;
        return Ok(Duration::from_millis(ms));
    }

    if let Some(s_str) = s.strip_suffix('s') {
        let secs: f64 = s_str
            .trim()
            .parse()
            .map_err(|_| Error::InvalidTrace(format!("invalid duration: {s}")))?;
        return Ok(Duration::from_secs_f64(secs));
    }

    Err(Error::InvalidTrace(format!("invalid duration format: {s}")))
}

impl Corpus {
    /// Loads a corpus from a JSON file.
    ///
    /// The file can contain:
    /// - A JSON array of trace objects
    /// - A JSON object with a "traces" field containing the array
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn load_file(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        info!("Loading corpus from {}", path.display());

        let content = std::fs::read_to_string(path)?;
        Self::parse_json(&content)
    }

    /// Loads a corpus from a directory containing JSON files.
    ///
    /// All `.json` files in the directory are loaded and merged.
    ///
    /// # Errors
    ///
    /// Returns an error if any file cannot be read or parsed.
    pub fn load_directory(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        info!("Loading corpus from directory {}", path.display());

        let mut corpus = Self::new();

        for entry in std::fs::read_dir(path)? {
            let entry = entry?;
            let file_path = entry.path();

            if file_path.extension().is_some_and(|e| e == "json") {
                debug!("Loading {}", file_path.display());
                match Self::load_file(&file_path) {
                    Ok(file_corpus) => {
                        for trace in file_corpus.iter() {
                            corpus.add(trace.clone());
                        }
                    }
                    Err(e) => {
                        warn!("Failed to load {}: {}", file_path.display(), e);
                    }
                }
            }
        }

        info!("Loaded {} traces from directory", corpus.len());
        Ok(corpus)
    }

    /// Parses a corpus from a JSON string.
    ///
    /// # Errors
    ///
    /// Returns an error if the JSON is invalid.
    pub fn parse_json(json: &str) -> Result<Self> {
        // Try parsing as an array first
        if let Ok(traces) = serde_json::from_str::<Vec<RawTrace>>(json) {
            return Ok(Self::from_raw_traces(traces));
        }

        // Try parsing as an object with a "traces" field
        #[derive(Deserialize)]
        struct CorpusWrapper {
            traces: Vec<RawTrace>,
        }

        if let Ok(wrapper) = serde_json::from_str::<CorpusWrapper>(json) {
            return Ok(Self::from_raw_traces(wrapper.traces));
        }

        Err(Error::LoadError(
            "JSON must be an array of traces or an object with a 'traces' field".to_string(),
        ))
    }

    fn from_raw_traces(raw_traces: Vec<RawTrace>) -> Self {
        let mut corpus = Self::new();

        for raw in raw_traces {
            match raw.into_trace() {
                Ok(trace) => corpus.add(trace),
                Err(e) => {
                    warn!("Skipping invalid trace: {}", e);
                }
            }
        }

        corpus
    }

    /// Creates an example corpus for testing/demos.
    #[must_use]
    pub fn example() -> Self {
        let traces = vec![
            Trace::new("trace-001")
                .with_duration(Duration::from_millis(45))
                .with_status(200)
                .with_service("api-gateway")
                .with_endpoint("GET /users"),
            Trace::new("trace-002")
                .with_duration(Duration::from_millis(1250))
                .with_status(200)
                .with_service("checkout")
                .with_endpoint("POST /orders"),
            Trace::new("trace-003")
                .with_duration(Duration::from_millis(89))
                .with_status(500)
                .with_service("inventory")
                .with_endpoint("GET /stock"),
            Trace::new("trace-004")
                .with_duration(Duration::from_millis(5200))
                .with_status(200)
                .with_service("search")
                .with_endpoint("GET /search"),
            Trace::new("trace-005")
                .with_duration(Duration::from_millis(32))
                .with_status(204)
                .with_service("auth")
                .with_endpoint("POST /login"),
            Trace::new("trace-006")
                .with_duration(Duration::from_millis(78))
                .with_status(503)
                .with_service("payments")
                .with_endpoint("POST /charge"),
        ];

        traces.into_iter().collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_duration_milliseconds() {
        assert_eq!(parse_duration("150ms").unwrap(), Duration::from_millis(150));
        assert_eq!(parse_duration("150").unwrap(), Duration::from_millis(150));
    }

    #[test]
    fn parse_duration_seconds() {
        assert_eq!(parse_duration("2s").unwrap(), Duration::from_secs(2));
        assert_eq!(
            parse_duration("2.5s").unwrap(),
            Duration::from_secs_f64(2.5)
        );
    }

    #[test]
    fn load_json_array() {
        let json = r#"[
            {"trace_id": "abc", "duration_ms": 100, "status": 200},
            {"trace_id": "def", "duration_ms": 200, "status": 500}
        ]"#;

        let corpus = Corpus::parse_json(json).unwrap();
        assert_eq!(corpus.len(), 2);
        assert_eq!(corpus.errors().len(), 1);
    }

    #[test]
    fn load_json_object_with_traces() {
        let json = r#"{
            "traces": [
                {"trace_id": "abc", "duration_ms": 100, "status": 200}
            ]
        }"#;

        let corpus = Corpus::parse_json(json).unwrap();
        assert_eq!(corpus.len(), 1);
    }

    #[test]
    fn load_json_with_otel_style_fields() {
        let json = r#"[
            {
                "trace_id": "abc",
                "duration_ms": 100,
                "http.status_code": 200,
                "service.name": "my-service",
                "http.route": "/api/v1/users"
            }
        ]"#;

        let corpus = Corpus::parse_json(json).unwrap();
        let trace = corpus.iter().next().unwrap();

        assert_eq!(trace.status, Some(200));
        assert_eq!(trace.service, Some("my-service".to_string()));
        assert_eq!(trace.endpoint, Some("/api/v1/users".to_string()));
    }

    #[test]
    fn example_corpus_has_errors() {
        let corpus = Corpus::example();
        assert!(!corpus.is_empty());
        assert!(!corpus.errors().is_empty());
    }
}
