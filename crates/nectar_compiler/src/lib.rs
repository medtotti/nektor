//! Policy to Refinery rules compiler for Nectar.
//!
//! This crate is **pure and deterministic**:
//! - No network calls
//! - No randomness
//! - Same input always produces same output
//!
//! # Example
//!
//! ```rust,ignore
//! use nectar_compiler::{Compiler, OutputFormat};
//!
//! let compiler = Compiler::new();
//! let output = compiler.compile(&policy, OutputFormat::Yaml)?;
//! ```

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![warn(clippy::nursery)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]
#![allow(clippy::format_push_string)]
#![allow(clippy::uninlined_format_args)]

pub mod compiler;
pub mod error;
pub mod lockfile;
pub mod match_expr;
pub mod refinery;
pub mod waggle;

pub use compiler::{CompileOptions, Compiler, OutputFormat};
pub use error::{Error, Result};
pub use lockfile::Lockfile;
