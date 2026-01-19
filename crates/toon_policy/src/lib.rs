//! TOON policy parsing and typed model for Nectar.
//!
//! This crate provides:
//! - TOON format parsing with strict validation
//! - Typed policy model with compile-time guarantees
//! - Serialization/deserialization of policies
//!
//! # Example
//!
//! ```rust,ignore
//! use toon_policy::{Policy, parse};
//!
//! let input = r#"
//! nectar_policy{version,name,rules}:
//!   1
//!   my-policy
//!   rules[1]{name,match,action,priority}:
//!     keep-errors,status >= 500,keep,100
//! "#;
//!
//! let policy = parse(input)?;
//! assert_eq!(policy.name, "my-policy");
//! ```

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![warn(clippy::nursery)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

pub mod error;
pub mod model;
pub mod parser;

pub use error::{Error, Result};
pub use model::{Action, Policy, Rule};
pub use parser::{parse, parse_action, serialize};
