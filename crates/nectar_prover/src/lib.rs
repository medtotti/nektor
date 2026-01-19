//! Policy verification and safety checks for Nectar.
//!
//! The prover is the safety gate: no policy reaches production without approval.
//!
//! # Checks Performed
//!
//! - **Must-keep coverage**: Critical traces are never dropped
//! - **Budget compliance**: Expected volume within limits
//! - **Fallback rule**: Policy has a catch-all rule
//! - **No error dropping**: Errors are always kept
//!
//! # Example
//!
//! ```rust,ignore
//! use nectar_prover::{Prover, ProverConfig};
//!
//! let prover = Prover::new(config);
//! let result = prover.verify(&policy, &corpus)?;
//! assert!(result.is_approved());
//! ```

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![warn(clippy::nursery)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

pub mod error;
pub mod prover;
pub mod result;
pub mod checks;

pub use error::{Error, Result};
pub use prover::{Prover, ProverConfig};
pub use result::{ProverResult, Severity, Violation, Warning};
