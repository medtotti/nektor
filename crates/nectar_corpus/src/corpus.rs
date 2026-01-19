//! Corpus container and operations.

use crate::error::Result;
use crate::trace::Trace;

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
