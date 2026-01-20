//! Trace corpus management for Nectar.
//!
//! This crate provides:
//! - Trace exemplar storage and retrieval
//! - TOON encoding of trace data for Claude
//! - Corpus filtering and sampling
//! - Pluggable trace ingestion (OTLP, Honeycomb, JSON)
//!
//! # Example
//!
//! ```rust,ignore
//! use nectar_corpus::{Corpus, Trace};
//!
//! let mut corpus = Corpus::new();
//! corpus.add(trace);
//! let toon = corpus.encode_toon()?;
//! ```
//!
//! # Format Ingestion
//!
//! The crate supports multiple trace formats through the ingestor framework:
//!
//! ```rust,ignore
//! use nectar_corpus::Corpus;
//!
//! // Auto-detect format from bytes
//! let corpus = Corpus::ingest(&data)?;
//!
//! // Ingest with content-type hint
//! let corpus = Corpus::ingest_with_content_type(&data, Some("application/json"))?;
//!
//! // Ingest from a file
//! let corpus = Corpus::ingest_file("traces.json")?;
//! ```

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![warn(clippy::nursery)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::items_after_statements)]

pub mod corpus;
pub mod encoder;
pub mod error;
pub mod fixtures;
pub mod ingestor;
pub mod loader;
pub mod reservoir;
pub mod span;
pub mod trace;

pub use corpus::Corpus;
pub use error::{Error, Result};
pub use ingestor::{IngestorRegistry, TraceIngestor};
pub use reservoir::{
    EvictionEvent, EvictionReason, Reservoir, ReservoirConfig, ReservoirStats, SamplingStrategy,
};
pub use span::{AttributeValue, Span, SpanKind, SpanStatus, StatusCode};
pub use trace::Trace;
