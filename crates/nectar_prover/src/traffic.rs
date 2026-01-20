//! Traffic pattern model for time-series simulation.
//!
//! Traffic patterns represent real-world event rates over time,
//! enabling realistic budget compliance verification.

use crate::error::{Error, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::io::Read;
use std::path::Path;

/// A single data point in a traffic pattern.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct TrafficPoint {
    /// Timestamp for this data point.
    pub timestamp: DateTime<Utc>,
    /// Events per second at this time.
    pub events_per_second: f64,
    /// Error rate (0.0 to 1.0) at this time.
    #[serde(default)]
    pub error_rate: f64,
    /// P99 latency in seconds at this time.
    #[serde(default)]
    pub p99_latency: f64,
}

impl TrafficPoint {
    /// Creates a new traffic point.
    #[must_use]
    pub const fn new(timestamp: DateTime<Utc>, events_per_second: f64) -> Self {
        Self {
            timestamp,
            events_per_second,
            error_rate: 0.0,
            p99_latency: 0.0,
        }
    }

    /// Sets the error rate.
    #[must_use]
    pub const fn with_error_rate(mut self, error_rate: f64) -> Self {
        self.error_rate = error_rate;
        self
    }

    /// Sets the P99 latency.
    #[must_use]
    pub const fn with_p99_latency(mut self, p99_latency: f64) -> Self {
        self.p99_latency = p99_latency;
        self
    }
}

/// A time-series traffic pattern.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrafficPattern {
    /// Data points in chronological order.
    points: Vec<TrafficPoint>,
    /// Optional name/description.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl TrafficPattern {
    /// Creates an empty traffic pattern.
    #[must_use]
    pub fn new() -> Self {
        Self::default()
    }

    /// Creates a traffic pattern with a name.
    #[must_use]
    pub fn with_name(name: impl Into<String>) -> Self {
        Self {
            points: Vec::new(),
            name: Some(name.into()),
        }
    }

    /// Adds a data point to the pattern.
    pub fn add_point(&mut self, point: TrafficPoint) {
        self.points.push(point);
    }

    /// Returns the data points.
    #[must_use]
    pub fn points(&self) -> &[TrafficPoint] {
        &self.points
    }

    /// Returns the number of data points.
    #[must_use]
    pub fn len(&self) -> usize {
        self.points.len()
    }

    /// Returns true if the pattern is empty.
    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.points.is_empty()
    }

    /// Returns the time range of the pattern.
    #[must_use]
    pub fn time_range(&self) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
        if self.points.is_empty() {
            return None;
        }
        let start = self.points.first()?.timestamp;
        let end = self.points.last()?.timestamp;
        Some((start, end))
    }

    /// Returns the peak events per second.
    #[must_use]
    pub fn peak_eps(&self) -> f64 {
        self.points
            .iter()
            .map(|p| p.events_per_second)
            .fold(0.0, f64::max)
    }

    /// Returns the average events per second.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn average_eps(&self) -> f64 {
        if self.points.is_empty() {
            return 0.0;
        }
        let sum: f64 = self.points.iter().map(|p| p.events_per_second).sum();
        sum / self.points.len() as f64
    }

    /// Returns the total events across all points.
    ///
    /// Assumes each point represents one second of traffic.
    #[must_use]
    pub fn total_events(&self) -> f64 {
        self.points.iter().map(|p| p.events_per_second).sum()
    }

    /// Loads a traffic pattern from a CSV file.
    ///
    /// Expected format:
    /// ```csv
    /// timestamp,events_per_second,error_rate,p99_latency
    /// 2024-01-15T09:00:00Z,5000,0.02,1.2
    /// ```
    ///
    /// # Errors
    ///
    /// Returns an error if the file cannot be read or parsed.
    pub fn from_csv_file(path: impl AsRef<Path>) -> Result<Self> {
        let file = std::fs::File::open(path.as_ref()).map_err(|e| {
            Error::InvalidTraffic(format!("failed to open file: {e}"))
        })?;
        Self::from_csv_reader(file)
    }

    /// Loads a traffic pattern from a CSV reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the CSV cannot be parsed.
    pub fn from_csv_reader<R: Read>(reader: R) -> Result<Self> {
        let mut csv_reader = csv::ReaderBuilder::new()
            .has_headers(true)
            .flexible(true)
            .from_reader(reader);

        let mut pattern = Self::new();

        for result in csv_reader.deserialize() {
            let record: CsvRecord = result.map_err(|e| {
                Error::InvalidTraffic(format!("CSV parse error: {e}"))
            })?;
            pattern.add_point(record.into_point()?);
        }

        if pattern.is_empty() {
            return Err(Error::InvalidTraffic("traffic pattern is empty".to_string()));
        }

        // Sort by timestamp
        pattern.points.sort_by_key(|p| p.timestamp);

        Ok(pattern)
    }

    /// Creates a traffic pattern from raw data points.
    #[must_use]
    pub fn from_points(points: Vec<TrafficPoint>) -> Self {
        let mut pattern = Self { points, name: None };
        pattern.points.sort_by_key(|p| p.timestamp);
        pattern
    }

    /// Finds the peak period (highest traffic window).
    ///
    /// Returns the index of the peak point.
    #[must_use]
    pub fn find_peak_index(&self) -> Option<usize> {
        self.points
            .iter()
            .enumerate()
            .max_by(|(_, a), (_, b)| {
                a.events_per_second
                    .partial_cmp(&b.events_per_second)
                    .unwrap_or(std::cmp::Ordering::Equal)
            })
            .map(|(i, _)| i)
    }

    /// Returns statistics about the traffic pattern.
    #[must_use]
    #[allow(clippy::cast_precision_loss)]
    pub fn stats(&self) -> TrafficStats {
        if self.is_empty() {
            return TrafficStats::default();
        }

        let eps_values: Vec<f64> = self.points.iter().map(|p| p.events_per_second).collect();
        let error_rates: Vec<f64> = self.points.iter().map(|p| p.error_rate).collect();

        let peak_eps = eps_values.iter().copied().fold(0.0, f64::max);
        let min_eps = eps_values.iter().copied().fold(f64::INFINITY, f64::min);
        let avg_eps = eps_values.iter().sum::<f64>() / eps_values.len() as f64;

        let avg_error_rate = error_rates.iter().sum::<f64>() / error_rates.len() as f64;
        let max_error_rate = error_rates.iter().copied().fold(0.0, f64::max);

        // Calculate variance and standard deviation
        let variance = eps_values
            .iter()
            .map(|&x| (x - avg_eps).powi(2))
            .sum::<f64>()
            / eps_values.len() as f64;
        let std_dev = variance.sqrt();

        TrafficStats {
            point_count: self.points.len(),
            peak_eps,
            min_eps,
            avg_eps,
            std_dev_eps: std_dev,
            avg_error_rate,
            max_error_rate,
            total_events: self.total_events(),
        }
    }
}

/// Statistics about a traffic pattern.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct TrafficStats {
    /// Number of data points.
    pub point_count: usize,
    /// Peak events per second.
    pub peak_eps: f64,
    /// Minimum events per second.
    pub min_eps: f64,
    /// Average events per second.
    pub avg_eps: f64,
    /// Standard deviation of events per second.
    pub std_dev_eps: f64,
    /// Average error rate.
    pub avg_error_rate: f64,
    /// Maximum error rate.
    pub max_error_rate: f64,
    /// Total events in the pattern.
    pub total_events: f64,
}

/// CSV record for deserializing traffic data.
#[derive(Debug, Deserialize)]
struct CsvRecord {
    timestamp: String,
    events_per_second: f64,
    #[serde(default)]
    error_rate: Option<f64>,
    #[serde(default)]
    p99_latency: Option<f64>,
}

impl CsvRecord {
    fn into_point(self) -> Result<TrafficPoint> {
        let timestamp = DateTime::parse_from_rfc3339(&self.timestamp)
            .map_err(|e| Error::InvalidTraffic(format!("invalid timestamp '{}': {e}", self.timestamp)))?
            .with_timezone(&Utc);

        Ok(TrafficPoint {
            timestamp,
            events_per_second: self.events_per_second,
            error_rate: self.error_rate.unwrap_or(0.0),
            p99_latency: self.p99_latency.unwrap_or(0.0),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::{TimeZone, Timelike};

    fn sample_pattern() -> TrafficPattern {
        let base = Utc.with_ymd_and_hms(2024, 1, 15, 9, 0, 0).unwrap();
        TrafficPattern::from_points(vec![
            TrafficPoint::new(base, 5000.0).with_error_rate(0.02),
            TrafficPoint::new(base + chrono::Duration::minutes(1), 8500.0).with_error_rate(0.01),
            TrafficPoint::new(base + chrono::Duration::minutes(2), 12000.0).with_error_rate(0.03),
            TrafficPoint::new(base + chrono::Duration::minutes(3), 7000.0).with_error_rate(0.01),
            TrafficPoint::new(base + chrono::Duration::minutes(4), 4000.0).with_error_rate(0.02),
        ])
    }

    #[test]
    fn traffic_pattern_stats() {
        let pattern = sample_pattern();
        let stats = pattern.stats();

        assert_eq!(stats.point_count, 5);
        assert!((stats.peak_eps - 12000.0).abs() < f64::EPSILON);
        assert!((stats.min_eps - 4000.0).abs() < f64::EPSILON);
        assert!((stats.avg_eps - 7300.0).abs() < f64::EPSILON);
    }

    #[test]
    fn traffic_pattern_peak() {
        let pattern = sample_pattern();
        assert_eq!(pattern.peak_eps(), 12000.0);
        assert_eq!(pattern.find_peak_index(), Some(2));
    }

    #[test]
    fn traffic_pattern_time_range() {
        let pattern = sample_pattern();
        let (start, end) = pattern.time_range().unwrap();

        assert_eq!(start.hour(), 9);
        assert_eq!(start.minute(), 0);
        assert_eq!(end.minute(), 4);
    }

    #[test]
    fn traffic_pattern_from_csv() {
        let csv_data = r#"timestamp,events_per_second,error_rate,p99_latency
2024-01-15T09:00:00Z,5000,0.02,1.2
2024-01-15T09:01:00Z,8500,0.01,0.8
2024-01-15T09:02:00Z,6000,0.015,1.0
"#;

        let pattern = TrafficPattern::from_csv_reader(csv_data.as_bytes()).unwrap();
        assert_eq!(pattern.len(), 3);
        assert!((pattern.peak_eps() - 8500.0).abs() < f64::EPSILON);
    }

    #[test]
    fn traffic_pattern_from_csv_minimal() {
        // Only required columns
        let csv_data = r#"timestamp,events_per_second
2024-01-15T09:00:00Z,5000
2024-01-15T09:01:00Z,8500
"#;

        let pattern = TrafficPattern::from_csv_reader(csv_data.as_bytes()).unwrap();
        assert_eq!(pattern.len(), 2);
    }

    #[test]
    fn traffic_pattern_empty_fails() {
        let csv_data = "timestamp,events_per_second\n";
        let result = TrafficPattern::from_csv_reader(csv_data.as_bytes());
        assert!(result.is_err());
    }

    #[test]
    fn traffic_point_builder() {
        let ts = Utc::now();
        let point = TrafficPoint::new(ts, 1000.0)
            .with_error_rate(0.05)
            .with_p99_latency(2.5);

        assert_eq!(point.events_per_second, 1000.0);
        assert!((point.error_rate - 0.05).abs() < f64::EPSILON);
        assert!((point.p99_latency - 2.5).abs() < f64::EPSILON);
    }
}
