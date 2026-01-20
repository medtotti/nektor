//! Corpus container and operations.

use crate::error::Result;
use crate::ingestor::IngestorRegistry;
use crate::trace::Trace;
use std::path::Path;

/// A collection of trace exemplars.
#[derive(Debug, Clone, Default)]
pub struct Corpus {
    traces: Vec<Trace>,
}

impl Corpus {
    /// Creates an empty corpus.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a trace to the corpus.
    pub fn add(&mut self, trace: Trace) {
        self.traces.push(trace);
    }

    /// Returns the number of traces.
    #[must_use]
    pub fn len(&self) -> usize {
        self.traces.len()
    }

    /// Returns true if the corpus is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.traces.is_empty()
    }

    /// Returns an iterator over traces.
    pub fn iter(&self) -> impl Iterator<Item = &Trace> {
        self.traces.iter()
    }

    /// Filters traces by a predicate.
    #[must_use]
    pub fn filter<F>(&self, predicate: F) -> Self
    where
        F: Fn(&Trace) -> bool,
    {
        Self {
            traces: self
                .traces
                .iter()
                .filter(|t| predicate(t))
                .cloned()
                .collect(),
        }
    }

    /// Returns only error traces.
    #[must_use]
    pub fn errors(&self) -> Self {
        self.filter(|t| t.is_error)
    }

    /// Encodes the corpus to TOON format.
    ///
    /// # Errors
    ///
    /// Returns an error if encoding fails.
    pub fn encode_toon(&self) -> Result<String> {
        // TODO: Implement TOON encoding
        Ok(format!(
            "corpus[{}]{{trace_id,duration_ms,status,service,endpoint,is_error}}:\n{}",
            self.traces.len(),
            self.traces
                .iter()
                .map(|t| format!(
                    "  {},{},{},{},{},{}",
                    t.trace_id,
                    t.duration.as_millis(),
                    t.status.map_or_else(|| "-".to_string(), |s| s.to_string()),
                    t.service.as_deref().unwrap_or("-"),
                    t.endpoint.as_deref().unwrap_or("-"),
                    t.is_error
                ))
                .collect::<Vec<_>>()
                .join("\n")
        ))
    }

    /// Consumes the corpus and returns the traces.
    #[must_use]
    pub fn into_traces(self) -> Vec<Trace> {
        self.traces
    }

    /// Returns traces sorted by their start time.
    ///
    /// Traces with spans are sorted by the earliest span start time.
    /// Traces without spans are placed at the beginning (start time = 0).
    #[must_use]
    pub fn sorted_by_time(&self) -> Vec<&Trace> {
        let mut traces: Vec<_> = self.traces.iter().collect();
        traces.sort_by_key(|t| t.start_time_ns().unwrap_or(0));
        traces
    }

    /// Consumes the corpus and returns traces sorted by their start time.
    #[must_use]
    pub fn into_sorted_by_time(mut self) -> Vec<Trace> {
        self.traces
            .sort_by_key(|t| t.start_time_ns().unwrap_or(0));
        self.traces
    }

    /// Returns the time range of the corpus in nanoseconds.
    ///
    /// Returns `(min_start_time, max_start_time)` or `None` if no traces have timestamps.
    #[must_use]
    pub fn time_range_ns(&self) -> Option<(u64, u64)> {
        let mut min_time = u64::MAX;
        let mut max_time = 0u64;
        let mut found = false;

        for trace in &self.traces {
            if let Some(start) = trace.start_time_ns() {
                min_time = min_time.min(start);
                max_time = max_time.max(start);
                found = true;
            }
        }

        if found {
            Some((min_time, max_time))
        } else {
            None
        }
    }

    /// Ingests trace data from bytes with auto-detection.
    ///
    /// Uses the ingestor registry to auto-detect the format and parse traces.
    ///
    /// # Errors
    ///
    /// Returns an error if the format is unknown or parsing fails.
    pub fn ingest(data: &[u8]) -> Result<Self> {
        Self::ingest_with_content_type(data, None)
    }

    /// Ingests trace data with a content-type hint.
    ///
    /// The content-type is used to help identify the format.
    ///
    /// # Errors
    ///
    /// Returns an error if parsing fails.
    pub fn ingest_with_content_type(data: &[u8], content_type: Option<&str>) -> Result<Self> {
        let registry = IngestorRegistry::new();
        let traces = registry.ingest(data, content_type)?;
        Ok(Self { traces })
    }

    /// Ingests trace data from a file with auto-detection.
    ///
    /// The format is auto-detected from the file contents.
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsing fails.
    pub fn ingest_file(path: impl AsRef<Path>) -> Result<Self> {
        let data = std::fs::read(path)?;
        Self::ingest(&data)
    }
}

impl FromIterator<Trace> for Corpus {
    fn from_iter<I: IntoIterator<Item = Trace>>(iter: I) -> Self {
        Self {
            traces: iter.into_iter().collect(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn corpus_add_and_iterate() {
        let mut corpus = Corpus::new();
        corpus.add(Trace::new("a"));
        corpus.add(Trace::new("b"));

        assert_eq!(corpus.len(), 2);
        let ids: Vec<_> = corpus.iter().map(|t| &t.trace_id).collect();
        assert_eq!(ids, vec!["a", "b"]);
    }

    #[test]
    fn corpus_filter_errors() {
        let corpus: Corpus = vec![
            Trace::new("ok").with_status(200),
            Trace::new("err1").with_status(500),
            Trace::new("err2").with_status(503),
        ]
        .into_iter()
        .collect();

        let errors = corpus.errors();
        assert_eq!(errors.len(), 2);
    }

    #[test]
    fn corpus_encode_toon() {
        let corpus: Corpus = vec![Trace::new("abc")
            .with_duration(Duration::from_millis(150))
            .with_status(200)
            .with_service("api")
            .with_endpoint("/users")]
        .into_iter()
        .collect();

        let toon = corpus.encode_toon().unwrap();
        assert!(toon.contains("corpus[1]"));
        assert!(toon.contains("abc,150,200,api,/users,false"));
    }
}
