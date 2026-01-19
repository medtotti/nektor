//! Claude API client for Nectar policy generation.
//!
//! This crate provides:
//! - Claude API client with TOON I/O
//! - Prompt building for policy generation
//! - Response validation and parsing
//!
//! # Example
//!
//! ```rust,ignore
//! use nectar_claude::{Client, PolicyRequest};
//!
//! let client = Client::new(api_key)?;
//! let policy = client.generate_policy(request).await?;
//! ```

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![warn(clippy::nursery)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

pub mod client;
pub mod error;
pub mod prompt;
pub mod response;

pub use client::{Client, ClientConfig};
pub use error::{Error, Result};
