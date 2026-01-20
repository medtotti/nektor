//! Traffic pattern simulation against policies.
//!
//! Simulates policy behavior over time-series traffic data to
//! verify budget compliance under realistic conditions.

use crate::traffic::{TrafficPattern, TrafficPoint};
use chrono::{DateTime, Timelike, Utc};
use serde::{Deserialize, Serialize};
use toon_policy::Policy;

/// Result of simulating a policy against a traffic pattern.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationResult {
    /// Whether the policy stays within budget.
    pub budget_compliant: bool,
    /// Budget violations found.
    pub violations: Vec<BudgetViolation>,
    /// Per-point simulation data.
    pub timeline: Vec<SimulationPoint>,
    /// Summary statistics.
    pub summary: SimulationSummary,
    /// Recommendations for fixing violations.
    pub recommendations: Vec<Recommendation>,
}

impl SimulationResult {
    /// Returns true if no budget violations occurred.
    #[must_use]
    pub const fn is_compliant(&self) -> bool {
        self.budget_compliant
    }

    /// Returns the number of violations.
    #[must_use]
    pub fn violation_count(&self) -> usize {
        self.violations.len()
    }

    /// Returns the peak violation (highest over-budget amount).
    #[must_use]
    pub fn peak_violation(&self) -> Option<&BudgetViolation> {
        self.violations
            .iter()
            .max_by(|a, b| {
                a.excess_events
                    .partial_cmp(&b.excess_events)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
    }
}

/// A budget violation at a specific time.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BudgetViolation {
    /// When the violation occurred.
    pub timestamp: DateTime<Utc>,
    /// The budget limit.
    pub budget_limit: f64,
    /// Actual events that would be kept.
    pub actual_events: f64,
    /// Events over budget.
    pub excess_events: f64,
    /// Percentage over budget.
    pub excess_percent: f64,
    /// Traffic point index.
    pub point_index: usize,
}

impl BudgetViolation {
    /// Creates a new budget violation.
    #[must_use]
    pub fn new(
        timestamp: DateTime<Utc>,
        budget_limit: f64,
        actual_events: f64,
        point_index: usize,
    ) -> Self {
        let excess_events = (actual_events - budget_limit).max(0.0);
        let excess_percent = if budget_limit > 0.0 {
            (excess_events / budget_limit) * 100.0
        } else {
            0.0
        };

        Self {
            timestamp,
            budget_limit,
            actual_events,
            excess_events,
            excess_percent,
            point_index,
        }
    }
}

/// Simulation data for a single time point.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SimulationPoint {
    /// Timestamp of this point.
    pub timestamp: DateTime<Utc>,
    /// Incoming events per second.
    pub incoming_eps: f64,
    /// Events kept after sampling.
    pub kept_eps: f64,
    /// Events dropped.
    pub dropped_eps: f64,
    /// Effective sample rate.
    pub sample_rate: f64,
    /// Whether this point exceeds budget.
    pub exceeds_budget: bool,
    /// Error events kept.
    pub error_events_kept: f64,
}

/// Summary statistics for a simulation.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct SimulationSummary {
    /// Total incoming events.
    pub total_incoming: f64,
    /// Total events kept.
    pub total_kept: f64,
    /// Total events dropped.
    pub total_dropped: f64,
    /// Overall sample rate.
    pub overall_sample_rate: f64,
    /// Number of points over budget.
    pub points_over_budget: usize,
    /// Total points simulated.
    pub total_points: usize,
    /// Percentage of time over budget.
    pub percent_time_over_budget: f64,
    /// Peak kept events per second.
    pub peak_kept_eps: f64,
    /// Average kept events per second.
    pub avg_kept_eps: f64,
}

/// A recommendation for addressing budget issues.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Recommendation {
    /// Type of recommendation.
    pub kind: RecommendationKind,
    /// Human-readable description.
    pub message: String,
    /// Suggested value (if applicable).
    pub suggested_value: Option<f64>,
}

/// Types of recommendations.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum RecommendationKind {
    /// Reduce sample rate.
    ReduceSampleRate,
    /// Increase budget.
    IncreaseBudget,
    /// Add rate limiting.
    AddRateLimit,
    /// Adjust during peak hours.
    PeakHourAdjustment,
}

impl Recommendation {
    /// Creates a recommendation to reduce sample rate.
    #[must_use]
    pub fn reduce_sample_rate(current: f64, suggested: f64) -> Self {
        Self {
            kind: RecommendationKind::ReduceSampleRate,
            message: format!(
                "Reduce sample rate from {:.1}% to {:.1}% to stay within budget",
                current * 100.0,
                suggested * 100.0
            ),
            suggested_value: Some(suggested),
        }
    }

    /// Creates a recommendation to increase budget.
    #[must_use]
    pub fn increase_budget(current: f64, required: f64) -> Self {
        Self {
            kind: RecommendationKind::IncreaseBudget,
            message: format!(
                "Increase budget from {current:.0} to {required:.0} events/sec to handle peak traffic"
            ),
            suggested_value: Some(required),
        }
    }

    /// Creates a recommendation for peak hour adjustment.
    #[must_use]
    pub fn peak_hour_adjustment(peak_hour: u32, suggested_rate: f64) -> Self {
        Self {
            kind: RecommendationKind::PeakHourAdjustment,
            message: format!(
                "Consider reducing sample rate to {:.1}% during peak hour ({}:00)",
                suggested_rate * 100.0,
                peak_hour
            ),
            suggested_value: Some(suggested_rate),
        }
    }
}

/// Simulator for running policies against traffic patterns.
#[derive(Debug, Clone)]
pub struct Simulator {
    /// Budget limit (events per second).
    budget: f64,
}

impl Simulator {
    /// Creates a new simulator with the given budget.
    #[must_use]
    pub const fn new(budget: f64) -> Self {
        Self { budget }
    }

    /// Simulates a policy against a traffic pattern.
    #[must_use]
    pub fn simulate(&self, policy: &Policy, traffic: &TrafficPattern) -> SimulationResult {
        let mut timeline = Vec::with_capacity(traffic.len());
        let mut violations = Vec::new();

        let mut total_incoming = 0.0;
        let mut total_kept = 0.0;
        let mut peak_kept = 0.0f64;

        for (index, point) in traffic.points().iter().enumerate() {
            let sim_point = self.simulate_point(policy, point, index);

            if sim_point.exceeds_budget {
                violations.push(BudgetViolation::new(
                    point.timestamp,
                    self.budget,
                    sim_point.kept_eps,
                    index,
                ));
            }

            total_incoming += sim_point.incoming_eps;
            total_kept += sim_point.kept_eps;
            peak_kept = peak_kept.max(sim_point.kept_eps);

            timeline.push(sim_point);
        }

        let total_dropped = total_incoming - total_kept;
        let overall_sample_rate = if total_incoming > 0.0 {
            total_kept / total_incoming
        } else {
            0.0
        };

        let points_over_budget = violations.len();
        let total_points = traffic.len();
        #[allow(clippy::cast_precision_loss)]
        let percent_time_over_budget = if total_points > 0 {
            (points_over_budget as f64 / total_points as f64) * 100.0
        } else {
            0.0
        };

        #[allow(clippy::cast_precision_loss)]
        let avg_kept = if total_points > 0 {
            total_kept / total_points as f64
        } else {
            0.0
        };

        let summary = SimulationSummary {
            total_incoming,
            total_kept,
            total_dropped,
            overall_sample_rate,
            points_over_budget,
            total_points,
            percent_time_over_budget,
            peak_kept_eps: peak_kept,
            avg_kept_eps: avg_kept,
        };

        let recommendations = self.generate_recommendations(
            policy,
            traffic,
            &violations,
            &summary,
        );

        SimulationResult {
            budget_compliant: violations.is_empty(),
            violations,
            timeline,
            summary,
            recommendations,
        }
    }

    /// Simulates a single traffic point.
    fn simulate_point(
        &self,
        policy: &Policy,
        point: &TrafficPoint,
        _index: usize,
    ) -> SimulationPoint {
        let incoming_eps = point.events_per_second;
        let error_rate = point.error_rate;

        // Calculate error events (always kept)
        let error_events = incoming_eps * error_rate;

        // Calculate non-error events
        let non_error_events = incoming_eps * (1.0 - error_rate);

        // Apply policy sample rate to non-error events
        let sample_rate = self.effective_sample_rate(policy);
        let sampled_non_error = non_error_events * sample_rate;

        // Total kept events
        let kept_eps = error_events + sampled_non_error;
        let dropped_eps = incoming_eps - kept_eps;

        let effective_rate = if incoming_eps > 0.0 {
            kept_eps / incoming_eps
        } else {
            0.0
        };

        SimulationPoint {
            timestamp: point.timestamp,
            incoming_eps,
            kept_eps,
            dropped_eps,
            sample_rate: effective_rate,
            exceeds_budget: kept_eps > self.budget,
            error_events_kept: error_events,
        }
    }

    /// Extracts the effective sample rate from a policy.
    #[allow(clippy::unused_self)]
    fn effective_sample_rate(&self, policy: &Policy) -> f64 {
        // Find the fallback rule's sample rate
        for rule in &policy.rules {
            if rule.match_expr == "true" {
                if let toon_policy::Action::Sample(rate) = rule.action {
                    return rate;
                }
            }
        }
        // Default to 100% if no fallback sample rule
        1.0
    }

    /// Generates recommendations based on violations.
    fn generate_recommendations(
        &self,
        policy: &Policy,
        traffic: &TrafficPattern,
        violations: &[BudgetViolation],
        summary: &SimulationSummary,
    ) -> Vec<Recommendation> {
        let mut recommendations = Vec::new();

        if violations.is_empty() {
            return recommendations;
        }

        let current_rate = self.effective_sample_rate(policy);

        // Calculate required sample rate to stay within budget
        if summary.peak_kept_eps > self.budget {
            let required_rate = self.budget / traffic.peak_eps();
            let suggested_rate = (required_rate * 0.9).max(0.001); // 10% safety margin

            if suggested_rate < current_rate {
                recommendations.push(Recommendation::reduce_sample_rate(
                    current_rate,
                    suggested_rate,
                ));
            }
        }

        // Calculate required budget increase
        let required_budget = summary.peak_kept_eps * 1.1; // 10% headroom
        if required_budget > self.budget {
            recommendations.push(Recommendation::increase_budget(
                self.budget,
                required_budget,
            ));
        }

        // Check for peak hour patterns
        if let Some(peak_violation) = violations.iter().max_by(|a, b| {
            a.excess_events
                .partial_cmp(&b.excess_events)
                .unwrap_or(std::cmp::Ordering::Equal)
        }) {
            let peak_hour = peak_violation.timestamp.hour();
            let peak_rate = self.budget / peak_violation.actual_events;
            recommendations.push(Recommendation::peak_hour_adjustment(
                peak_hour,
                peak_rate.max(0.001),
            ));
        }

        recommendations
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;
    use toon_policy::{Action, Rule};

    fn sample_policy() -> Policy {
        let mut policy = Policy::new("test");
        policy.add_rule(Rule::new("keep-errors", "error", Action::Keep, 100));
        policy.add_rule(Rule::new("fallback", "true", Action::Sample(0.1), 0));
        policy
    }

    fn sample_traffic() -> TrafficPattern {
        let base = Utc.with_ymd_and_hms(2024, 1, 15, 9, 0, 0).unwrap();
        TrafficPattern::from_points(vec![
            TrafficPoint::new(base, 5000.0).with_error_rate(0.02),
            TrafficPoint::new(base + chrono::Duration::minutes(1), 8000.0).with_error_rate(0.01),
            TrafficPoint::new(base + chrono::Duration::minutes(2), 15000.0).with_error_rate(0.02),
            TrafficPoint::new(base + chrono::Duration::minutes(3), 6000.0).with_error_rate(0.01),
        ])
    }

    #[test]
    fn simulation_within_budget() {
        let policy = sample_policy();
        let traffic = sample_traffic();
        let simulator = Simulator::new(10000.0); // High budget

        let result = simulator.simulate(&policy, &traffic);

        assert!(result.is_compliant());
        assert!(result.violations.is_empty());
        assert_eq!(result.timeline.len(), 4);
    }

    #[test]
    fn simulation_exceeds_budget() {
        let policy = sample_policy();
        let traffic = sample_traffic();
        let simulator = Simulator::new(1000.0); // Low budget

        let result = simulator.simulate(&policy, &traffic);

        assert!(!result.is_compliant());
        assert!(!result.violations.is_empty());
    }

    #[test]
    fn simulation_generates_recommendations() {
        let policy = sample_policy();
        let traffic = sample_traffic();
        let simulator = Simulator::new(500.0); // Very low budget

        let result = simulator.simulate(&policy, &traffic);

        assert!(!result.recommendations.is_empty());
        // Should have sample rate and budget recommendations
        assert!(result.recommendations.iter().any(|r| {
            matches!(r.kind, RecommendationKind::ReduceSampleRate)
        }));
    }

    #[test]
    fn simulation_summary_stats() {
        let policy = sample_policy();
        let traffic = sample_traffic();
        let simulator = Simulator::new(10000.0);

        let result = simulator.simulate(&policy, &traffic);

        assert!(result.summary.total_incoming > 0.0);
        assert!(result.summary.total_kept > 0.0);
        assert!(result.summary.overall_sample_rate > 0.0);
        assert!(result.summary.overall_sample_rate <= 1.0);
    }

    #[test]
    fn simulation_keeps_errors() {
        let policy = sample_policy();
        let base = Utc.with_ymd_and_hms(2024, 1, 15, 9, 0, 0).unwrap();
        let traffic = TrafficPattern::from_points(vec![
            TrafficPoint::new(base, 1000.0).with_error_rate(0.5), // 50% errors
        ]);
        let simulator = Simulator::new(10000.0);

        let result = simulator.simulate(&policy, &traffic);
        let point = &result.timeline[0];

        // 500 error events should be kept
        assert!((point.error_events_kept - 500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn budget_violation_calculation() {
        let ts = Utc::now();
        let violation = BudgetViolation::new(ts, 1000.0, 1500.0, 0);

        assert!((violation.excess_events - 500.0).abs() < f64::EPSILON);
        assert!((violation.excess_percent - 50.0).abs() < f64::EPSILON);
    }
}
