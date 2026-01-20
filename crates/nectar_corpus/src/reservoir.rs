//! Reservoir sampling for bounded corpus growth.
//!
//! Implements Algorithm R (Vitter) for uniform reservoir sampling with
//! extensions for stratified sampling and time-decay weighting.
//!
//! # Example
//!
//! ```rust,ignore
//! use nectar_corpus::{Reservoir, ReservoirConfig, SamplingStrategy, Trace};
//!
//! let config = ReservoirConfig::new(1000)
//!     .with_strategy(SamplingStrategy::Stratified)
//!     .with_preserve_errors(true);
//!
//! let mut reservoir = Reservoir::new(config);
//! reservoir.add(trace);
//! ```

use crate::trace::Trace;
use rand::Rng;
use rand_chacha::ChaCha8Rng;
use rand::SeedableRng;
use std::time::Duration;

/// Sampling strategy for the reservoir.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SamplingStrategy {
    /// Uniform random sampling (Algorithm R).
    #[default]
    Uniform,
    /// Stratified sampling preserving error and slow traces.
    Stratified,
    /// Time-decay sampling favoring recent traces.
    TimeDecay,
}

/// Configuration for reservoir sampling.
#[derive(Debug, Clone)]
pub struct ReservoirConfig {
    /// Maximum number of traces to keep.
    pub max_size: usize,
    /// Sampling strategy.
    pub strategy: SamplingStrategy,
    /// Whether to always preserve error traces.
    pub preserve_errors: bool,
    /// Threshold duration for "slow" traces to preserve.
    pub slow_threshold: Option<Duration>,
    /// Half-life for time-decay sampling (in nanoseconds).
    pub decay_half_life_ns: Option<u64>,
    /// Random seed for deterministic sampling.
    pub seed: u64,
}

impl Default for ReservoirConfig {
    fn default() -> Self {
        Self {
            max_size: 10_000,
            strategy: SamplingStrategy::Uniform,
            preserve_errors: false,
            slow_threshold: None,
            decay_half_life_ns: None,
            seed: 0,
        }
    }
}

impl ReservoirConfig {
    /// Creates a new configuration with the given maximum size.
    #[must_use]
    pub const fn new(max_size: usize) -> Self {
        Self {
            max_size,
            strategy: SamplingStrategy::Uniform,
            preserve_errors: false,
            slow_threshold: None,
            decay_half_life_ns: None,
            seed: 0,
        }
    }

    /// Sets the sampling strategy.
    #[must_use]
    pub const fn with_strategy(mut self, strategy: SamplingStrategy) -> Self {
        self.strategy = strategy;
        self
    }

    /// Enables error trace preservation.
    #[must_use]
    pub const fn with_preserve_errors(mut self, preserve: bool) -> Self {
        self.preserve_errors = preserve;
        self
    }

    /// Sets the slow trace threshold.
    #[must_use]
    pub const fn with_slow_threshold(mut self, threshold: Duration) -> Self {
        self.slow_threshold = Some(threshold);
        self
    }

    /// Sets the time-decay half-life.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn with_decay_half_life(mut self, half_life: Duration) -> Self {
        // Truncation is acceptable: half-life > 584 years overflows, which is fine
        self.decay_half_life_ns = Some(half_life.as_nanos() as u64);
        self
    }

    /// Sets the random seed for deterministic sampling.
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }
}

/// Event emitted when a trace is evicted from the reservoir.
#[derive(Debug, Clone)]
pub struct EvictionEvent {
    /// The trace that was evicted.
    pub evicted_trace_id: String,
    /// The trace that replaced it.
    pub replacement_trace_id: String,
    /// Reason for eviction.
    pub reason: EvictionReason,
    /// Current reservoir size.
    pub reservoir_size: usize,
    /// Total traces seen so far.
    pub total_seen: u64,
}

/// Reason for trace eviction.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EvictionReason {
    /// Random eviction (Algorithm R).
    RandomSample,
    /// Time-decay priority eviction.
    TimeDecay,
    /// Stratified rebalancing.
    StratifiedRebalance,
}

/// Statistics about the reservoir.
#[derive(Debug, Clone, Default)]
pub struct ReservoirStats {
    /// Total traces seen.
    pub total_seen: u64,
    /// Current reservoir size.
    pub current_size: usize,
    /// Number of error traces in reservoir.
    pub error_count: usize,
    /// Number of slow traces in reservoir.
    pub slow_count: usize,
    /// Number of evictions performed.
    pub eviction_count: u64,
}

/// A reservoir for bounded trace sampling.
///
/// Uses Algorithm R (Vitter) for uniform sampling with optional
/// stratified and time-decay extensions.
pub struct Reservoir {
    config: ReservoirConfig,
    traces: Vec<Trace>,
    /// Separate stratum for error traces (stratified mode).
    error_stratum: Vec<Trace>,
    /// Separate stratum for slow traces (stratified mode).
    slow_stratum: Vec<Trace>,
    /// Total number of traces seen.
    total_seen: u64,
    /// Total number of evictions.
    eviction_count: u64,
    /// Random number generator.
    rng: ChaCha8Rng,
    /// Callback for eviction events.
    eviction_callback: Option<Box<dyn Fn(EvictionEvent) + Send + Sync>>,
}

impl std::fmt::Debug for Reservoir {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Reservoir")
            .field("config", &self.config)
            .field("traces", &self.traces.len())
            .field("error_stratum", &self.error_stratum.len())
            .field("slow_stratum", &self.slow_stratum.len())
            .field("total_seen", &self.total_seen)
            .field("eviction_count", &self.eviction_count)
            .field("has_callback", &self.eviction_callback.is_some())
            .finish_non_exhaustive()
    }
}

impl Reservoir {
    /// Creates a new reservoir with the given configuration.
    #[must_use]
    pub fn new(config: ReservoirConfig) -> Self {
        let rng = ChaCha8Rng::seed_from_u64(config.seed);
        Self {
            config,
            traces: Vec::new(),
            error_stratum: Vec::new(),
            slow_stratum: Vec::new(),
            total_seen: 0,
            eviction_count: 0,
            rng,
            eviction_callback: None,
        }
    }

    /// Creates a new reservoir with default configuration.
    #[must_use]
    pub fn with_capacity(max_size: usize) -> Self {
        Self::new(ReservoirConfig::new(max_size))
    }

    /// Sets a callback for eviction events.
    pub fn on_eviction<F>(&mut self, callback: F)
    where
        F: Fn(EvictionEvent) + Send + Sync + 'static,
    {
        self.eviction_callback = Some(Box::new(callback));
    }

    /// Returns the current number of traces in the reservoir.
    #[must_use]
    pub fn len(&self) -> usize {
        match self.config.strategy {
            SamplingStrategy::Stratified => {
                self.traces.len() + self.error_stratum.len() + self.slow_stratum.len()
            }
            SamplingStrategy::Uniform | SamplingStrategy::TimeDecay => self.traces.len(),
        }
    }

    /// Returns true if the reservoir is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.len() == 0
    }

    /// Returns the maximum capacity.
    #[must_use]
    pub const fn capacity(&self) -> usize {
        self.config.max_size
    }

    /// Returns statistics about the reservoir.
    #[must_use]
    pub fn stats(&self) -> ReservoirStats {
        let (error_count, slow_count) = match self.config.strategy {
            SamplingStrategy::Stratified => (self.error_stratum.len(), self.slow_stratum.len()),
            SamplingStrategy::Uniform | SamplingStrategy::TimeDecay => {
                let errors = self.traces.iter().filter(|t| t.is_error).count();
                let slow = self.config.slow_threshold.map_or(0, |thresh| {
                    self.traces.iter().filter(|t| t.duration >= thresh).count()
                });
                (errors, slow)
            }
        };

        ReservoirStats {
            total_seen: self.total_seen,
            current_size: self.len(),
            error_count,
            slow_count,
            eviction_count: self.eviction_count,
        }
    }

    /// Adds a trace to the reservoir using the configured sampling strategy.
    ///
    /// Returns `Some(EvictionEvent)` if a trace was evicted, `None` otherwise.
    pub fn add(&mut self, trace: Trace) -> Option<EvictionEvent> {
        self.total_seen += 1;

        match self.config.strategy {
            SamplingStrategy::Uniform => self.add_uniform(trace),
            SamplingStrategy::Stratified => self.add_stratified(trace),
            SamplingStrategy::TimeDecay => self.add_time_decay(trace),
        }
    }

    /// Uniform reservoir sampling (Algorithm R).
    #[allow(clippy::cast_possible_truncation)]
    fn add_uniform(&mut self, trace: Trace) -> Option<EvictionEvent> {
        if self.traces.len() < self.config.max_size {
            // Reservoir not full, just add
            self.traces.push(trace);
            None
        } else {
            // Reservoir full, use Algorithm R
            // Truncation is safe: max_size is usize, so j < max_size fits in usize
            let j = self.rng.gen_range(0..self.total_seen) as usize;
            if j < self.config.max_size {
                let evicted = std::mem::replace(&mut self.traces[j], trace.clone());
                self.eviction_count += 1;
                let event = EvictionEvent {
                    evicted_trace_id: evicted.trace_id,
                    replacement_trace_id: trace.trace_id,
                    reason: EvictionReason::RandomSample,
                    reservoir_size: self.traces.len(),
                    total_seen: self.total_seen,
                };
                self.emit_eviction(&event);
                Some(event)
            } else {
                None
            }
        }
    }

    /// Stratified reservoir sampling.
    ///
    /// Maintains separate strata for error traces and slow traces,
    /// with the main reservoir for normal traces.
    fn add_stratified(&mut self, trace: Trace) -> Option<EvictionEvent> {
        let is_error = trace.is_error;
        let is_slow = self
            .config
            .slow_threshold
            .is_some_and(|thresh| trace.duration >= thresh);

        // Determine stratum allocation (e.g., 20% errors, 10% slow, 70% normal)
        let error_capacity = self.config.max_size / 5; // 20%
        let slow_capacity = self.config.max_size / 10; // 10%
        let normal_capacity = self.config.max_size - error_capacity - slow_capacity;

        if is_error && self.config.preserve_errors {
            self.add_to_error_stratum(trace, error_capacity)
        } else if is_slow {
            self.add_to_slow_stratum(trace, slow_capacity)
        } else {
            self.add_to_normal_stratum(trace, normal_capacity)
        }
    }

    /// Adds a trace to the error stratum.
    #[allow(clippy::cast_possible_truncation)]
    fn add_to_error_stratum(&mut self, trace: Trace, capacity: usize) -> Option<EvictionEvent> {
        if self.error_stratum.len() < capacity {
            self.error_stratum.push(trace);
            None
        } else {
            let j = self.rng.gen_range(0..self.total_seen) as usize;
            if j < capacity && j < self.error_stratum.len() {
                let evicted = std::mem::replace(&mut self.error_stratum[j], trace.clone());
                self.eviction_count += 1;
                let event = EvictionEvent {
                    evicted_trace_id: evicted.trace_id,
                    replacement_trace_id: trace.trace_id,
                    reason: EvictionReason::StratifiedRebalance,
                    reservoir_size: self.len(),
                    total_seen: self.total_seen,
                };
                self.emit_eviction(&event);
                Some(event)
            } else {
                None
            }
        }
    }

    /// Adds a trace to the slow stratum.
    #[allow(clippy::cast_possible_truncation)]
    fn add_to_slow_stratum(&mut self, trace: Trace, capacity: usize) -> Option<EvictionEvent> {
        if self.slow_stratum.len() < capacity {
            self.slow_stratum.push(trace);
            None
        } else {
            let j = self.rng.gen_range(0..self.total_seen) as usize;
            if j < capacity && j < self.slow_stratum.len() {
                let evicted = std::mem::replace(&mut self.slow_stratum[j], trace.clone());
                self.eviction_count += 1;
                let event = EvictionEvent {
                    evicted_trace_id: evicted.trace_id,
                    replacement_trace_id: trace.trace_id,
                    reason: EvictionReason::StratifiedRebalance,
                    reservoir_size: self.len(),
                    total_seen: self.total_seen,
                };
                self.emit_eviction(&event);
                Some(event)
            } else {
                None
            }
        }
    }

    /// Adds a trace to the normal stratum.
    #[allow(clippy::cast_possible_truncation)]
    fn add_to_normal_stratum(&mut self, trace: Trace, capacity: usize) -> Option<EvictionEvent> {
        if self.traces.len() < capacity {
            self.traces.push(trace);
            None
        } else {
            let j = self.rng.gen_range(0..self.total_seen) as usize;
            if j < capacity && j < self.traces.len() {
                let evicted = std::mem::replace(&mut self.traces[j], trace.clone());
                self.eviction_count += 1;
                let event = EvictionEvent {
                    evicted_trace_id: evicted.trace_id,
                    replacement_trace_id: trace.trace_id,
                    reason: EvictionReason::RandomSample,
                    reservoir_size: self.len(),
                    total_seen: self.total_seen,
                };
                self.emit_eviction(&event);
                Some(event)
            } else {
                None
            }
        }
    }

    /// Time-decay reservoir sampling.
    ///
    /// Traces are weighted by recency, with newer traces having higher
    /// probability of being kept.
    #[allow(clippy::cast_precision_loss)]
    fn add_time_decay(&mut self, trace: Trace) -> Option<EvictionEvent> {
        if self.traces.len() < self.config.max_size {
            self.traces.push(trace);
            return None;
        }

        // Calculate weight based on time decay
        let current_time = trace.start_time_ns().unwrap_or(self.total_seen * 1_000_000);
        let half_life = self.config.decay_half_life_ns.unwrap_or(24 * 60 * 60 * 1_000_000_000); // 24h default

        // Find trace with lowest weight (oldest adjusted for decay)
        let mut min_weight = f64::MAX;
        let mut min_idx = 0;

        for (i, t) in self.traces.iter().enumerate() {
            let t_time = t.start_time_ns().unwrap_or(0);
            let age = current_time.saturating_sub(t_time);
            // Precision loss is acceptable for exponential decay calculation
            let weight = (-0.693 * age as f64 / half_life as f64).exp();
            if weight < min_weight {
                min_weight = weight;
                min_idx = i;
            }
        }

        // Probabilistically replace based on relative weights
        let new_weight = 1.0; // New trace has weight 1
        let replace_prob = new_weight / (new_weight + min_weight);

        if self.rng.gen::<f64>() < replace_prob {
            let evicted = std::mem::replace(&mut self.traces[min_idx], trace.clone());
            self.eviction_count += 1;
            let event = EvictionEvent {
                evicted_trace_id: evicted.trace_id,
                replacement_trace_id: trace.trace_id,
                reason: EvictionReason::TimeDecay,
                reservoir_size: self.traces.len(),
                total_seen: self.total_seen,
            };
            self.emit_eviction(&event);
            Some(event)
        } else {
            None
        }
    }

    /// Emits an eviction event to the callback if set.
    fn emit_eviction(&self, event: &EvictionEvent) {
        if let Some(ref callback) = self.eviction_callback {
            callback(event.clone());
        }
    }

    /// Returns an iterator over all traces in the reservoir.
    pub fn iter(&self) -> impl Iterator<Item = &Trace> {
        match self.config.strategy {
            SamplingStrategy::Stratified => {
                Box::new(
                    self.traces
                        .iter()
                        .chain(self.error_stratum.iter())
                        .chain(self.slow_stratum.iter()),
                ) as Box<dyn Iterator<Item = &Trace>>
            }
            SamplingStrategy::Uniform | SamplingStrategy::TimeDecay => {
                Box::new(self.traces.iter())
            }
        }
    }

    /// Consumes the reservoir and returns all traces.
    #[must_use]
    pub fn into_traces(self) -> Vec<Trace> {
        match self.config.strategy {
            SamplingStrategy::Stratified => {
                let mut all = self.traces;
                all.extend(self.error_stratum);
                all.extend(self.slow_stratum);
                all
            }
            SamplingStrategy::Uniform | SamplingStrategy::TimeDecay => self.traces,
        }
    }

    /// Returns the configuration.
    #[must_use]
    pub const fn config(&self) -> &ReservoirConfig {
        &self.config
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reservoir_add_under_capacity() {
        let mut reservoir = Reservoir::with_capacity(100);

        for i in 0..50 {
            let event = reservoir.add(Trace::new(format!("trace-{i}")));
            assert!(event.is_none(), "No eviction should occur under capacity");
        }

        assert_eq!(reservoir.len(), 50);
        assert_eq!(reservoir.stats().total_seen, 50);
    }

    #[test]
    fn reservoir_uniform_sampling() {
        let config = ReservoirConfig::new(10).with_seed(42);
        let mut reservoir = Reservoir::new(config);

        // Add 100 traces to a reservoir of size 10
        let mut eviction_count = 0;
        for i in 0..100 {
            if reservoir.add(Trace::new(format!("trace-{i}"))).is_some() {
                eviction_count += 1;
            }
        }

        assert_eq!(reservoir.len(), 10);
        assert!(eviction_count > 0, "Some evictions should have occurred");
        assert_eq!(reservoir.stats().eviction_count, eviction_count);
    }

    #[test]
    fn reservoir_deterministic_sampling() {
        // Two reservoirs with same seed should produce same results
        let config1 = ReservoirConfig::new(10).with_seed(12345);
        let config2 = ReservoirConfig::new(10).with_seed(12345);

        let mut reservoir1 = Reservoir::new(config1);
        let mut reservoir2 = Reservoir::new(config2);

        for i in 0..100 {
            reservoir1.add(Trace::new(format!("trace-{i}")));
            reservoir2.add(Trace::new(format!("trace-{i}")));
        }

        let ids1: Vec<_> = reservoir1.iter().map(|t| &t.trace_id).collect();
        let ids2: Vec<_> = reservoir2.iter().map(|t| &t.trace_id).collect();

        assert_eq!(ids1, ids2, "Deterministic sampling should produce same results");
    }

    #[test]
    fn reservoir_stratified_preserves_errors() {
        let config = ReservoirConfig::new(100)
            .with_strategy(SamplingStrategy::Stratified)
            .with_preserve_errors(true)
            .with_seed(42);

        let mut reservoir = Reservoir::new(config);

        // Add 80 normal traces and 20 error traces
        for i in 0..80 {
            reservoir.add(Trace::new(format!("normal-{i}")).with_status(200));
        }
        for i in 0..20 {
            reservoir.add(Trace::new(format!("error-{i}")).with_status(500));
        }

        let stats = reservoir.stats();
        assert_eq!(stats.error_count, 20, "All errors should be preserved");
    }

    #[test]
    fn reservoir_stratified_preserves_slow() {
        let config = ReservoirConfig::new(100)
            .with_strategy(SamplingStrategy::Stratified)
            .with_slow_threshold(Duration::from_secs(5))
            .with_seed(42);

        let mut reservoir = Reservoir::new(config);

        // Add 90 fast traces and 10 slow traces
        for i in 0..90 {
            reservoir.add(Trace::new(format!("fast-{i}")).with_duration(Duration::from_millis(100)));
        }
        for i in 0..10 {
            reservoir.add(Trace::new(format!("slow-{i}")).with_duration(Duration::from_secs(10)));
        }

        let stats = reservoir.stats();
        assert_eq!(stats.slow_count, 10, "All slow traces should be preserved");
    }

    #[test]
    fn reservoir_eviction_callback() {
        use std::sync::atomic::{AtomicU64, Ordering};
        use std::sync::Arc;

        let eviction_count = Arc::new(AtomicU64::new(0));
        let count_clone = Arc::clone(&eviction_count);

        let config = ReservoirConfig::new(10).with_seed(42);
        let mut reservoir = Reservoir::new(config);
        reservoir.on_eviction(move |_event| {
            count_clone.fetch_add(1, Ordering::SeqCst);
        });

        for i in 0..100 {
            reservoir.add(Trace::new(format!("trace-{i}")));
        }

        assert!(
            eviction_count.load(Ordering::SeqCst) > 0,
            "Eviction callback should have been called"
        );
    }

    #[test]
    fn reservoir_time_decay_favors_recent() {
        let config = ReservoirConfig::new(10)
            .with_strategy(SamplingStrategy::TimeDecay)
            .with_decay_half_life(Duration::from_secs(1))
            .with_seed(42);

        let mut reservoir = Reservoir::new(config);

        // Add old traces first
        for i in 0..10 {
            let mut trace = Trace::new(format!("old-{i}"));
            trace.spans.push(crate::span::Span {
                span_id: format!("span-{i}"),
                parent_span_id: None,
                name: "test".to_string(),
                service: "test".to_string(),
                duration: Duration::from_millis(100),
                start_time_ns: 1_000_000_000, // 1 second
                kind: crate::span::SpanKind::Internal,
                status: crate::span::SpanStatus::default(),
                attributes: std::collections::HashMap::new(),
            });
            reservoir.add(trace);
        }

        // Add new traces
        for i in 0..50 {
            let mut trace = Trace::new(format!("new-{i}"));
            trace.spans.push(crate::span::Span {
                span_id: format!("span-new-{i}"),
                parent_span_id: None,
                name: "test".to_string(),
                service: "test".to_string(),
                duration: Duration::from_millis(100),
                start_time_ns: 100_000_000_000, // 100 seconds (much more recent)
                kind: crate::span::SpanKind::Internal,
                status: crate::span::SpanStatus::default(),
                attributes: std::collections::HashMap::new(),
            });
            reservoir.add(trace);
        }

        // Count how many "new" traces are in the reservoir
        let new_count = reservoir.iter().filter(|t| t.trace_id.starts_with("new-")).count();

        // With time decay, more recent traces should dominate
        assert!(new_count > 5, "Time decay should favor recent traces, got {new_count}");
    }

    #[test]
    fn reservoir_stats() {
        let config = ReservoirConfig::new(100)
            .with_slow_threshold(Duration::from_secs(5))
            .with_seed(42);

        let mut reservoir = Reservoir::new(config);

        // Add mixed traces
        for i in 0..30 {
            reservoir.add(Trace::new(format!("normal-{i}")).with_status(200).with_duration(Duration::from_millis(100)));
        }
        for i in 0..10 {
            reservoir.add(Trace::new(format!("error-{i}")).with_status(500).with_duration(Duration::from_millis(100)));
        }
        for i in 0..5 {
            reservoir.add(Trace::new(format!("slow-{i}")).with_status(200).with_duration(Duration::from_secs(10)));
        }

        let stats = reservoir.stats();
        assert_eq!(stats.total_seen, 45);
        assert_eq!(stats.current_size, 45);
        assert_eq!(stats.error_count, 10);
        assert_eq!(stats.slow_count, 5);
    }

    #[test]
    fn reservoir_into_traces() {
        let mut reservoir = Reservoir::with_capacity(100);

        for i in 0..10 {
            reservoir.add(Trace::new(format!("trace-{i}")));
        }

        let traces = reservoir.into_traces();
        assert_eq!(traces.len(), 10);
    }
}
