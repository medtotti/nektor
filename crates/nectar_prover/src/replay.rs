//! Historical traffic replay from corpus timestamps.
//!
//! Replays traces in timestamp order to simulate real traffic flow,
//! enabling validation of time-based rules and budget compliance.

use crate::error::{Error, Result};
use nectar_corpus::{Corpus, Trace};
use serde::{Deserialize, Serialize};
use std::time::Duration;
use toon_policy::Policy;

/// Replay speed multiplier.
#[derive(Debug, Clone, Copy, PartialEq, Default, Serialize, Deserialize)]
pub enum ReplaySpeed {
    /// Real-time replay (1x).
    RealTime,
    /// 10x speed.
    Fast,
    /// 100x speed.
    VeryFast,
    /// Maximum speed (no delays, process as fast as possible).
    #[default]
    Max,
    /// Custom multiplier.
    Custom(f64),
}

impl ReplaySpeed {
    /// Returns the speed multiplier.
    #[must_use]
    pub const fn multiplier(&self) -> f64 {
        match self {
            Self::RealTime => 1.0,
            Self::Fast => 10.0,
            Self::VeryFast => 100.0,
            Self::Max => f64::INFINITY,
            Self::Custom(m) => *m,
        }
    }

    /// Converts virtual time delta to real time delta.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn virtual_to_real(&self, virtual_delta_ns: u64) -> Duration {
        let multiplier = self.multiplier();
        if multiplier.is_infinite() || multiplier == 0.0 {
            Duration::ZERO
        } else {
            #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
            let real_ns = (virtual_delta_ns as f64 / multiplier) as u64;
            Duration::from_nanos(real_ns)
        }
    }
}

/// Time window for aggregation.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct TimeWindow {
    /// Window duration in nanoseconds.
    duration_ns: u64,
}

impl TimeWindow {
    /// Creates a time window from a duration.
    #[must_use]
    #[allow(clippy::cast_possible_truncation)]
    pub const fn from_duration(duration: Duration) -> Self {
        Self {
            duration_ns: duration.as_nanos() as u64,
        }
    }

    /// Creates a 1-second window.
    #[must_use]
    pub const fn one_second() -> Self {
        Self::from_duration(Duration::from_secs(1))
    }

    /// Creates a 1-minute window.
    #[must_use]
    pub const fn one_minute() -> Self {
        Self::from_duration(Duration::from_secs(60))
    }

    /// Creates a 5-minute window.
    #[must_use]
    pub const fn five_minutes() -> Self {
        Self::from_duration(Duration::from_secs(300))
    }

    /// Returns the window duration in nanoseconds.
    #[must_use]
    pub const fn duration_ns(&self) -> u64 {
        self.duration_ns
    }

    /// Returns the window duration.
    #[must_use]
    pub const fn duration(&self) -> Duration {
        Duration::from_nanos(self.duration_ns)
    }

    /// Calculates which window a timestamp belongs to.
    #[must_use]
    pub const fn window_index(&self, timestamp_ns: u64) -> u64 {
        timestamp_ns / self.duration_ns
    }
}

impl Default for TimeWindow {
    fn default() -> Self {
        Self::one_second()
    }
}

/// Configuration for replay.
#[derive(Debug, Clone, Default)]
pub struct ReplayConfig {
    /// Replay speed.
    pub speed: ReplaySpeed,
    /// Time window for aggregation.
    pub window: TimeWindow,
    /// Budget limit (events per second).
    pub budget_per_second: Option<f64>,
}

impl ReplayConfig {
    /// Creates a new replay config with default settings.
    #[must_use]
    pub const fn new() -> Self {
        Self {
            speed: ReplaySpeed::Max,
            window: TimeWindow::one_second(),
            budget_per_second: None,
        }
    }

    /// Sets the replay speed.
    #[must_use]
    pub const fn with_speed(mut self, speed: ReplaySpeed) -> Self {
        self.speed = speed;
        self
    }

    /// Sets the time window.
    #[must_use]
    pub const fn with_window(mut self, window: TimeWindow) -> Self {
        self.window = window;
        self
    }

    /// Sets the budget limit.
    #[must_use]
    pub const fn with_budget(mut self, budget_per_second: f64) -> Self {
        self.budget_per_second = Some(budget_per_second);
        self
    }
}

/// A single window of aggregated replay data.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayWindow {
    /// Window index (0-based).
    pub index: u64,
    /// Window start time (nanoseconds since corpus start).
    pub start_ns: u64,
    /// Window end time (nanoseconds since corpus start).
    pub end_ns: u64,
    /// Number of traces in this window.
    pub trace_count: usize,
    /// Number of error traces.
    pub error_count: usize,
    /// Traces kept after policy application.
    pub kept_count: usize,
    /// Traces dropped after policy application.
    pub dropped_count: usize,
    /// Effective throughput (traces per second).
    pub throughput: f64,
    /// Whether this window exceeds budget.
    pub exceeds_budget: bool,
    /// Amount over budget (if exceeding).
    pub over_budget_by: f64,
}

impl ReplayWindow {
    /// Creates a new empty window.
    #[must_use]
    pub const fn new(index: u64, window_duration_ns: u64) -> Self {
        let start_ns = index * window_duration_ns;
        let end_ns = start_ns + window_duration_ns;
        Self {
            index,
            start_ns,
            end_ns,
            trace_count: 0,
            error_count: 0,
            kept_count: 0,
            dropped_count: 0,
            throughput: 0.0,
            exceeds_budget: false,
            over_budget_by: 0.0,
        }
    }

    /// Calculates throughput based on window duration.
    #[allow(clippy::cast_precision_loss)]
    pub fn calculate_throughput(&mut self, window_duration_ns: u64) {
        let window_seconds = window_duration_ns as f64 / 1_000_000_000.0;
        self.throughput = self.kept_count as f64 / window_seconds;
    }

    /// Checks if this window exceeds budget.
    pub fn check_budget(&mut self, budget_per_second: Option<f64>) {
        if let Some(budget) = budget_per_second {
            if self.throughput > budget {
                self.exceeds_budget = true;
                self.over_budget_by = self.throughput - budget;
            }
        }
    }
}

/// Result of replaying a corpus.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayResult {
    /// Whether replay completed successfully.
    pub success: bool,
    /// Total traces processed.
    pub total_traces: usize,
    /// Total traces kept.
    pub total_kept: usize,
    /// Total traces dropped.
    pub total_dropped: usize,
    /// Total error traces.
    pub total_errors: usize,
    /// Per-window results.
    pub windows: Vec<ReplayWindow>,
    /// Windows that exceeded budget.
    pub violations: Vec<ReplayWindow>,
    /// Summary statistics.
    pub summary: ReplaySummary,
    /// Time range of the replay.
    pub time_range: Option<ReplayTimeRange>,
}

impl ReplayResult {
    /// Returns true if no budget violations occurred.
    #[must_use]
    pub fn is_compliant(&self) -> bool {
        self.violations.is_empty()
    }

    /// Returns the number of violations.
    #[must_use]
    pub fn violation_count(&self) -> usize {
        self.violations.len()
    }

    /// Returns the peak throughput.
    #[must_use]
    pub fn peak_throughput(&self) -> f64 {
        self.windows
            .iter()
            .map(|w| w.throughput)
            .fold(0.0, f64::max)
    }
}

/// Summary statistics for replay.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct ReplaySummary {
    /// Average throughput (traces per second).
    pub avg_throughput: f64,
    /// Peak throughput (traces per second).
    pub peak_throughput: f64,
    /// Minimum throughput (traces per second).
    pub min_throughput: f64,
    /// Standard deviation of throughput.
    pub throughput_std_dev: f64,
    /// Overall sample rate.
    pub overall_sample_rate: f64,
    /// Error rate.
    pub error_rate: f64,
    /// Number of windows.
    pub window_count: usize,
    /// Number of windows over budget.
    pub windows_over_budget: usize,
    /// Percentage of time over budget.
    pub percent_time_over_budget: f64,
}

/// Time range information for replay.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ReplayTimeRange {
    /// Start time (nanoseconds).
    pub start_ns: u64,
    /// End time (nanoseconds).
    pub end_ns: u64,
    /// Total duration (nanoseconds).
    pub duration_ns: u64,
    /// Total duration as a human-readable string.
    pub duration_str: String,
}

impl ReplayTimeRange {
    /// Creates a new time range.
    #[must_use]
    pub fn new(start_ns: u64, end_ns: u64) -> Self {
        let duration_ns = end_ns.saturating_sub(start_ns);
        let duration = Duration::from_nanos(duration_ns);
        Self {
            start_ns,
            end_ns,
            duration_ns,
            duration_str: format_duration(duration),
        }
    }
}

/// Replayer for corpus traces.
#[derive(Debug, Clone)]
pub struct Replayer {
    config: ReplayConfig,
}

impl Default for Replayer {
    fn default() -> Self {
        Self::new(ReplayConfig::new())
    }
}

impl Replayer {
    /// Creates a new replayer with the given configuration.
    #[must_use]
    pub const fn new(config: ReplayConfig) -> Self {
        Self { config }
    }

    /// Replays a corpus against a policy.
    ///
    /// # Errors
    ///
    /// Returns an error if the corpus is empty or has no timestamps.
    pub fn replay(&self, policy: &Policy, corpus: &Corpus) -> Result<ReplayResult> {
        if corpus.is_empty() {
            return Err(Error::InvalidCorpus("corpus is empty".to_string()));
        }

        // Get sorted traces
        let traces = corpus.sorted_by_time();

        // Get time range
        let time_range = corpus.time_range_ns().map(|(start, end)| {
            // Include the duration of traces that start at the end
            ReplayTimeRange::new(start, end)
        });

        // If no timestamps, process all in one window
        let base_time = time_range.as_ref().map_or(0, |r| r.start_ns);

        // Process traces into windows
        let mut windows = self.process_traces(&traces, policy, base_time);

        // Calculate statistics for each window
        let window_duration_ns = self.config.window.duration_ns();
        for window in &mut windows {
            window.calculate_throughput(window_duration_ns);
            window.check_budget(self.config.budget_per_second);
        }

        // Collect violations
        let violations: Vec<_> = windows
            .iter()
            .filter(|w| w.exceeds_budget)
            .cloned()
            .collect();

        // Calculate summary
        let summary = self.calculate_summary(&windows, &traces);

        // Calculate totals
        let total_traces = traces.len();
        let total_kept: usize = windows.iter().map(|w| w.kept_count).sum();
        let total_dropped: usize = windows.iter().map(|w| w.dropped_count).sum();
        let total_errors: usize = windows.iter().map(|w| w.error_count).sum();

        Ok(ReplayResult {
            success: true,
            total_traces,
            total_kept,
            total_dropped,
            total_errors,
            windows,
            violations,
            summary,
            time_range,
        })
    }

    /// Processes traces into time windows.
    fn process_traces(
        &self,
        traces: &[&Trace],
        policy: &Policy,
        base_time: u64,
    ) -> Vec<ReplayWindow> {
        let window_duration = self.config.window.duration_ns();
        let mut windows: std::collections::HashMap<u64, ReplayWindow> =
            std::collections::HashMap::new();

        for trace in traces {
            let timestamp = trace.start_time_ns().unwrap_or(base_time);
            let relative_time = timestamp.saturating_sub(base_time);
            let window_index = self.config.window.window_index(relative_time);

            let window = windows
                .entry(window_index)
                .or_insert_with(|| ReplayWindow::new(window_index, window_duration));

            window.trace_count += 1;

            if trace.is_error {
                window.error_count += 1;
            }

            // Apply policy to determine if trace is kept
            if self.should_keep_trace(policy, trace) {
                window.kept_count += 1;
            } else {
                window.dropped_count += 1;
            }
        }

        // Convert to sorted vec
        let mut window_vec: Vec<_> = windows.into_values().collect();
        window_vec.sort_by_key(|w| w.index);
        window_vec
    }

    /// Determines if a trace should be kept based on policy.
    #[allow(clippy::unused_self)]
    fn should_keep_trace(&self, policy: &Policy, trace: &Trace) -> bool {
        // Apply policy rules in priority order
        for rule in &policy.rules {
            if self.matches_rule(&rule.match_expr, trace) {
                return match rule.action {
                    toon_policy::Action::Keep => true,
                    toon_policy::Action::Drop => false,
                    toon_policy::Action::Sample(rate) => {
                        // Deterministic sampling based on trace_id
                        let hash = simple_hash(&trace.trace_id);
                        #[allow(clippy::cast_precision_loss)]
                        let normalized = (hash as f64) / (u64::MAX as f64);
                        normalized < rate
                    }
                };
            }
        }

        // Default: keep if no rule matches
        true
    }

    /// Checks if a trace matches a rule expression.
    ///
    /// This is a simplified matcher for common patterns.
    #[allow(clippy::unused_self)]
    fn matches_rule(&self, expr: &str, trace: &Trace) -> bool {
        // Handle "true" fallback
        if expr == "true" {
            return true;
        }

        // Handle error conditions
        let lower_expr = expr.to_lowercase();
        if lower_expr.contains("error") || lower_expr.contains("is_error") {
            return trace.is_error;
        }

        // Handle status code comparisons
        if lower_expr.contains("status") {
            if let Some(status) = trace.status {
                if lower_expr.contains(">=") {
                    if let Some(threshold) = extract_number(&lower_expr, ">=") {
                        return status >= threshold;
                    }
                }
                if lower_expr.contains("==") || lower_expr.contains('=') {
                    if let Some(threshold) = extract_number(&lower_expr, "=") {
                        return status == threshold;
                    }
                }
            }
        }

        // Default: no match
        false
    }

    /// Calculates summary statistics.
    #[allow(clippy::cast_precision_loss, clippy::unused_self)]
    fn calculate_summary(&self, windows: &[ReplayWindow], traces: &[&Trace]) -> ReplaySummary {
        if windows.is_empty() {
            return ReplaySummary::default();
        }

        let throughputs: Vec<f64> = windows.iter().map(|w| w.throughput).collect();

        let avg_throughput = throughputs.iter().sum::<f64>() / throughputs.len() as f64;
        let peak_throughput = throughputs.iter().copied().fold(0.0, f64::max);
        let min_throughput = throughputs.iter().copied().fold(f64::INFINITY, f64::min);

        // Calculate standard deviation
        let variance = throughputs
            .iter()
            .map(|&t| (t - avg_throughput).powi(2))
            .sum::<f64>()
            / throughputs.len() as f64;
        let throughput_std_dev = variance.sqrt();

        let total_kept: usize = windows.iter().map(|w| w.kept_count).sum();
        let total_errors: usize = windows.iter().map(|w| w.error_count).sum();
        let total_traces = traces.len();

        let overall_sample_rate = if total_traces > 0 {
            total_kept as f64 / total_traces as f64
        } else {
            0.0
        };

        let error_rate = if total_traces > 0 {
            total_errors as f64 / total_traces as f64
        } else {
            0.0
        };

        let windows_over_budget = windows.iter().filter(|w| w.exceeds_budget).count();
        let percent_time_over_budget = if windows.is_empty() {
            0.0
        } else {
            (windows_over_budget as f64 / windows.len() as f64) * 100.0
        };

        ReplaySummary {
            avg_throughput,
            peak_throughput,
            min_throughput,
            throughput_std_dev,
            overall_sample_rate,
            error_rate,
            window_count: windows.len(),
            windows_over_budget,
            percent_time_over_budget,
        }
    }
}

/// Formats a duration as a human-readable string.
fn format_duration(duration: Duration) -> String {
    let total_secs = duration.as_secs();
    let hours = total_secs / 3600;
    let minutes = (total_secs % 3600) / 60;
    let seconds = total_secs % 60;

    if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

/// Simple hash function for deterministic sampling.
fn simple_hash(s: &str) -> u64 {
    let mut hash = 0u64;
    for byte in s.bytes() {
        hash = hash.wrapping_mul(31).wrapping_add(u64::from(byte));
    }
    hash
}

/// Extracts a number after an operator in an expression.
fn extract_number(expr: &str, op: &str) -> Option<u16> {
    let parts: Vec<&str> = expr.split(op).collect();
    if parts.len() >= 2 {
        parts[1].split_whitespace().next()?.parse().ok()
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use nectar_corpus::Span;
    use toon_policy::{Action, Rule};

    fn sample_policy() -> Policy {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("keep-errors", "status >= 500", Action::Keep, 100));
        policy.add_rule(Rule::new("sample-normal", "true", Action::Sample(0.1), 0));
        policy
    }

    fn sample_corpus() -> Corpus {
        let base_ns = 1_000_000_000_000u64; // 1000 seconds
        let traces = vec![
            create_trace("t1", base_ns, 200, false),
            create_trace("t2", base_ns + 500_000_000, 200, false), // +0.5s
            create_trace("t3", base_ns + 1_000_000_000, 500, true), // +1s
            create_trace("t4", base_ns + 1_500_000_000, 200, false), // +1.5s
            create_trace("t5", base_ns + 2_000_000_000, 503, true), // +2s
        ];
        traces.into_iter().collect()
    }

    fn create_trace(id: &str, start_ns: u64, status: u16, is_error: bool) -> Trace {
        let span = Span::new(format!("{id}-span"), "operation")
            .with_service("api")
            .with_start_time_ns(start_ns)
            .with_duration(Duration::from_millis(50));

        let mut trace = Trace::from_spans(id, vec![span]);
        trace.status = Some(status);
        trace.is_error = is_error;
        trace
    }

    #[test]
    fn replay_speed_multipliers() {
        assert!((ReplaySpeed::RealTime.multiplier() - 1.0).abs() < f64::EPSILON);
        assert!((ReplaySpeed::Fast.multiplier() - 10.0).abs() < f64::EPSILON);
        assert!((ReplaySpeed::VeryFast.multiplier() - 100.0).abs() < f64::EPSILON);
        assert!(ReplaySpeed::Max.multiplier().is_infinite());
        assert!((ReplaySpeed::Custom(50.0).multiplier() - 50.0).abs() < f64::EPSILON);
    }

    #[test]
    fn time_window_index() {
        let window = TimeWindow::one_second();
        assert_eq!(window.window_index(0), 0);
        assert_eq!(window.window_index(500_000_000), 0); // 0.5s
        assert_eq!(window.window_index(1_000_000_000), 1); // 1s
        assert_eq!(window.window_index(2_500_000_000), 2); // 2.5s
    }

    #[test]
    fn replay_basic() {
        let policy = sample_policy();
        let corpus = sample_corpus();
        let replayer = Replayer::default();

        let result = replayer.replay(&policy, &corpus).unwrap();

        assert!(result.success);
        assert_eq!(result.total_traces, 5);
        // Errors (2) are always kept, others are sampled at 10%
        assert!(result.total_kept >= 2);
    }

    #[test]
    fn replay_with_budget() {
        let policy = sample_policy();
        let corpus = sample_corpus();

        let config = ReplayConfig::new()
            .with_window(TimeWindow::one_second())
            .with_budget(1.0); // Very low budget

        let replayer = Replayer::new(config);
        let result = replayer.replay(&policy, &corpus).unwrap();

        // With such a low budget, some windows should exceed it
        assert!(!result.windows.is_empty());
    }

    #[test]
    fn replay_empty_corpus_fails() {
        let policy = sample_policy();
        let corpus = Corpus::new();
        let replayer = Replayer::default();

        let result = replayer.replay(&policy, &corpus);
        assert!(result.is_err());
    }

    #[test]
    fn replay_time_range() {
        let policy = sample_policy();
        let corpus = sample_corpus();
        let replayer = Replayer::default();

        let result = replayer.replay(&policy, &corpus).unwrap();

        assert!(result.time_range.is_some());
        let range = result.time_range.unwrap();
        assert!(range.duration_ns > 0);
    }

    #[test]
    fn replay_summary_stats() {
        let policy = sample_policy();
        let corpus = sample_corpus();
        let replayer = Replayer::default();

        let result = replayer.replay(&policy, &corpus).unwrap();

        assert!(result.summary.window_count > 0);
        assert!(result.summary.overall_sample_rate > 0.0);
        assert!(result.summary.overall_sample_rate <= 1.0);
    }

    #[test]
    fn replay_window_aggregation() {
        let policy = sample_policy();
        let corpus = sample_corpus();

        let config = ReplayConfig::new().with_window(TimeWindow::one_minute());
        let replayer = Replayer::new(config);

        let result = replayer.replay(&policy, &corpus).unwrap();

        // With 1-minute windows, all traces (spanning ~2s) should be in one window
        assert_eq!(result.windows.len(), 1);
        assert_eq!(result.windows[0].trace_count, 5);
    }

    #[test]
    fn replay_keeps_errors() {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("keep-errors", "status >= 500", Action::Keep, 100));
        policy.add_rule(Rule::new("drop-all", "true", Action::Drop, 0));

        let corpus = sample_corpus();
        let replayer = Replayer::default();

        let result = replayer.replay(&policy, &corpus).unwrap();

        // Only the 2 error traces should be kept
        assert_eq!(result.total_kept, 2);
        assert_eq!(result.total_errors, 2);
    }

    #[test]
    fn deterministic_sampling() {
        let policy = sample_policy();
        let corpus = sample_corpus();
        let replayer = Replayer::default();

        // Run twice and verify same results
        let result1 = replayer.replay(&policy, &corpus).unwrap();
        let result2 = replayer.replay(&policy, &corpus).unwrap();

        assert_eq!(result1.total_kept, result2.total_kept);
        assert_eq!(result1.total_dropped, result2.total_dropped);
    }
}
