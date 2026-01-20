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
//!
//! # Traffic Pattern Simulation
//!
//! The prover can simulate policy behavior against real traffic patterns:
//!
//! ```rust,ignore
//! use nectar_prover::{Prover, ProverConfig, TrafficPattern};
//!
//! let prover = Prover::new(ProverConfig { max_budget: Some(10000), ..Default::default() });
//! let traffic = TrafficPattern::from_csv_file("traffic.csv")?;
//! let result = prover.simulate_traffic(&policy, &traffic)?;
//!
//! if !result.is_compliant() {
//!     for violation in &result.violations {
//!         println!("Budget exceeded at {}: {} events/sec", violation.timestamp, violation.actual_events);
//!     }
//! }
//! ```
//!
//! # Analysis Modes
//!
//! The prover supports different analysis modes for varying speed/accuracy tradeoffs:
//!
//! ```rust,ignore
//! use nectar_prover::{Prover, ProverConfig, AnalysisMode};
//!
//! // Static mode: Fast rule analysis only (O(rules))
//! let config = ProverConfig { analysis_mode: AnalysisMode::Static, ..Default::default() };
//!
//! // Dynamic mode: Full traffic simulation (O(rules × events))
//! let config = ProverConfig { analysis_mode: AnalysisMode::Dynamic, ..Default::default() };
//!
//! // Auto mode: Static for iterations, dynamic for final prove
//! let config = ProverConfig { analysis_mode: AnalysisMode::Auto, ..Default::default() };
//! ```

#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![warn(clippy::nursery)]
#![allow(clippy::module_name_repetitions)]
#![allow(clippy::must_use_candidate)]

pub mod analysis;
pub mod checks;
pub mod error;
pub mod prover;
pub mod replay;
pub mod result;
pub mod simulation;
pub mod traffic;

pub use analysis::{
    AnalysisMode, Confidence, CoverageAnalysis, RuleConflict, StaticAnalysisResult,
    StaticAnalyzer, StaticWarning,
};
pub use error::{Error, Result};
pub use prover::{AnalysisResult, Prover, ProverConfig};
pub use replay::{
    ReplayConfig, ReplayResult, ReplaySpeed, ReplaySummary, ReplayTimeRange, ReplayWindow,
    Replayer, TimeWindow,
};
pub use result::{ProverResult, Severity, Violation, Warning};
pub use simulation::{
    BudgetViolation, Recommendation, RecommendationKind, SimulationPoint, SimulationResult,
    SimulationSummary, Simulator,
};
pub use traffic::{TrafficPattern, TrafficPoint, TrafficStats};
