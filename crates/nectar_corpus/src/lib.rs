//! Trace corpus management for Nectar.
//!
//! This crate provides:
//! - Trace exemplar storage and retrieval
//! - TOON encoding of trace data for Claude
//! - Corpus filtering and sampling
//!
//! # Example
//!
//! ```rust,ignore
//! use nectar_corpus::{Corpus, Trace};
//!
//! let mut corpus = Corpus::new();
//! corpus.add_trace(trace);
//! let toon = corpus.encode_toon()?;
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
pub mod loader;
pub mod trace;

pub use corpus::Corpus;
pub use error::{Error, Result};
pub use trace::Trace;
