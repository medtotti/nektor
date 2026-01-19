//! VOPR-style deterministic simulation testing for Nectar.
//!
//! This crate provides:
//! - Synthetic trace generation with configurable distributions
//! - Deterministic simulation harness for reproducible testing
//! - Time-compressed replay for policy evolution testing
//! - Chaos/fault injection for robustness verification
//! - VOPR-style parallel deterministic execution
//!
//! # VOPR Testing Philosophy
//!
//! VOPR (Vaguely Ordered Parallel Replayability) testing ensures:
//! 1. **Determinism**: Same seed produces identical results
//! 2. **Time compression**: Hours of behavior in seconds
//! 3. **Fault injection**: Systematic chaos testing
//! 4. **Parallel safety**: Concurrent execution correctness
//!
//! # Example
//!
//! ```rust,ignore
//! use nectar_vopr::{Simulation, SimConfig};
//!
//! let sim = Simulation::new(SimConfig::default().with_seed(42));
//! let results = sim.run_scenario(scenario)?;
//! assert!(results.all_invariants_held());
//! ```

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![warn(clippy::nursery)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

pub mod chaos;
pub mod generators;
pub mod harness;
pub mod replay;
pub mod simulation;
pub mod synthetic;

pub use harness::{SimConfig, Simulation};
pub use replay::{ReplayLog, TimeCompressor};
pub use simulation::{Scenario, SimResult};
pub use synthetic::SyntheticCorpus;
