//! Synthetic corpus generation for testing.
//!
//! Generates realistic trace corpora with configurable:
//! - Error rates
//! - Latency distributions
//! - Service topologies
//! - Traffic patterns

use nectar_corpus::{Corpus, Trace};
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use std::collections::HashMap;
use std::time::Duration;

/// Configuration for synthetic corpus generation.
#[derive(Debug, Clone)]
pub struct SyntheticConfig {
    /// Random seed for reproducibility.
    pub seed: u64,
    /// Number of traces to generate.
    pub trace_count: usize,
    /// Error rate (0.0 - 1.0).
    pub error_rate: f64,
    /// Slow request rate (0.0 - 1.0).
    pub slow_rate: f64,
    /// Slow threshold in milliseconds.
    pub slow_threshold_ms: u64,
    /// Services to simulate.
    pub services: Vec<String>,
    /// Routes per service.
    pub routes_per_service: usize,
}

impl Default for SyntheticConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            trace_count: 1000,
            error_rate: 0.05,
            slow_rate: 0.10,
            slow_threshold_ms: 5000,
            services: vec![
                "api-gateway".to_string(),
                "user-service".to_string(),
                "order-service".to_string(),
                "payment-service".to_string(),
                "inventory-service".to_string(),
            ],
            routes_per_service: 5,
        }
    }
}

impl SyntheticConfig {
    /// Creates a new config with the given seed.
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Sets the trace count.
    #[must_use]
    pub const fn with_trace_count(mut self, count: usize) -> Self {
        self.trace_count = count;
        self
    }

    /// Sets the error rate.
    #[must_use]
    pub const fn with_error_rate(mut self, rate: f64) -> Self {
        self.error_rate = rate;
        self
    }

    /// Sets the slow request rate.
    #[must_use]
    pub const fn with_slow_rate(mut self, rate: f64) -> Self {
        self.slow_rate = rate;
        self
    }
}

/// Synthetic corpus generator.
pub struct SyntheticCorpus {
    config: SyntheticConfig,
    rng: ChaCha8Rng,
    routes: HashMap<String, Vec<String>>,
}

impl SyntheticCorpus {
    /// Creates a new synthetic corpus generator.
    #[must_use]
    pub fn new(config: SyntheticConfig) -> Self {
        let mut rng = ChaCha8Rng::seed_from_u64(config.seed);
        let routes = Self::generate_routes(&config, &mut rng);
        Self {
            config,
            rng,
            routes,
        }
    }

    fn generate_routes(
        config: &SyntheticConfig,
        rng: &mut ChaCha8Rng,
    ) -> HashMap<String, Vec<String>> {
        let route_templates = [
            "/api/v1/<resource>",
            "/api/v1/<resource>/<id>",
            "/api/v2/<resource>",
            "/health",
            "/metrics",
        ];

        let resources = ["users", "orders", "products", "payments", "inventory"];

        config
            .services
            .iter()
            .map(|service| {
                let routes: Vec<String> = (0..config.routes_per_service)
                    .map(|_| {
                        let template = route_templates.choose(rng).unwrap();
                        let resource = resources.choose(rng).unwrap();
                        template
                            .replace("<resource>", resource)
                            .replace("<id>", "123")
                    })
                    .collect();
                (service.clone(), routes)
            })
            .collect()
    }

    /// Generates a corpus with the configured parameters.
    #[must_use]
    pub fn generate(&mut self) -> Corpus {
        let mut corpus = Corpus::new();

        for i in 0..self.config.trace_count {
            let trace = self.generate_trace(i);
            corpus.add(trace);
        }

        corpus
    }

    fn generate_trace(&mut self, index: usize) -> Trace {
        let service = self.config.services.choose(&mut self.rng).unwrap().clone();
        let routes = self.routes.get(&service).unwrap();
        let route = routes.choose(&mut self.rng).unwrap().clone();

        // Determine if this is an error or slow request
        let is_error = self.rng.gen_bool(self.config.error_rate);
        let is_slow = self.rng.gen_bool(self.config.slow_rate);

        let status = if is_error {
            *[500u16, 502, 503, 504].choose(&mut self.rng).unwrap()
        } else {
            *[200u16, 201, 204].choose(&mut self.rng).unwrap()
        };

        let duration_ms = if is_slow {
            self.config.slow_threshold_ms + self.rng.gen_range(0..10000)
        } else {
            self.rng.gen_range(10..self.config.slow_threshold_ms)
        };

        Trace::new(format!("trace-{index:08x}"))
            .with_service(service)
            .with_endpoint(route)
            .with_status(status)
            .with_duration(Duration::from_millis(duration_ms))
    }

    /// Generates a corpus specifically designed to test edge cases.
    #[must_use]
    pub fn generate_edge_cases(&mut self) -> Corpus {
        let mut corpus = Corpus::new();

        // All errors
        for i in 0..10 {
            corpus.add(
                Trace::new(format!("error-{i}"))
                    .with_service("error-service")
                    .with_status(500)
                    .with_duration(Duration::from_millis(100)),
            );
        }

        // All slow
        for i in 0..10 {
            corpus.add(
                Trace::new(format!("slow-{i}"))
                    .with_service("slow-service")
                    .with_status(200)
                    .with_duration(Duration::from_millis(30000)),
            );
        }

        // Boundary cases
        corpus.add(
            Trace::new("boundary-499")
                .with_service("boundary-service")
                .with_status(499)
                .with_duration(Duration::from_millis(4999)),
        );

        corpus.add(
            Trace::new("boundary-500")
                .with_service("boundary-service")
                .with_status(500)
                .with_duration(Duration::from_millis(5000)),
        );

        corpus
    }

    /// Generates a high-cardinality corpus for stress testing.
    #[must_use]
    pub fn generate_high_cardinality(&mut self, unique_services: usize) -> Corpus {
        let mut corpus = Corpus::new();

        for i in 0..unique_services {
            corpus.add(
                Trace::new(format!("hc-{i:08x}"))
                    .with_service(format!("service-{i:08x}"))
                    .with_endpoint(format!("/api/v1/resource-{i}"))
                    .with_status(200)
                    .with_duration(Duration::from_millis(self.rng.gen_range(10..1000))),
            );
        }

        corpus
    }
}

/// Generates deterministic corpora for snapshot testing.
pub fn deterministic_corpus(seed: u64, size: usize) -> Corpus {
    let config = SyntheticConfig::default()
        .with_seed(seed)
        .with_trace_count(size);
    let mut gen = SyntheticCorpus::new(config);
    gen.generate()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn synthetic_corpus_is_deterministic() {
        let corpus1 = deterministic_corpus(42, 100);
        let corpus2 = deterministic_corpus(42, 100);

        // Same seed should produce identical corpora
        assert_eq!(corpus1.len(), corpus2.len());

        let traces1: Vec<_> = corpus1.iter().collect();
        let traces2: Vec<_> = corpus2.iter().collect();

        for (t1, t2) in traces1.iter().zip(traces2.iter()) {
            assert_eq!(t1.trace_id, t2.trace_id);
            assert_eq!(t1.service, t2.service);
            assert_eq!(t1.status, t2.status);
        }
    }

    #[test]
    fn different_seeds_produce_different_corpora() {
        let corpus1 = deterministic_corpus(42, 100);
        let corpus2 = deterministic_corpus(43, 100);

        let traces1: Vec<_> = corpus1.iter().collect();
        let traces2: Vec<_> = corpus2.iter().collect();

        // At least some traces should differ (comparing service names, not trace IDs)
        let differences = traces1
            .iter()
            .zip(traces2.iter())
            .filter(|(t1, t2)| t1.service != t2.service || t1.status != t2.status)
            .count();

        assert!(differences > 0);
    }

    #[test]
    fn edge_cases_corpus_has_expected_traces() {
        let mut gen = SyntheticCorpus::new(SyntheticConfig::default());
        let corpus = gen.generate_edge_cases();

        // Should have errors
        assert!(!corpus.errors().is_empty());

        // Should have traces
        assert!(corpus.len() > 20);
    }

    #[test]
    #[allow(clippy::cast_precision_loss)]
    fn error_rate_is_respected() {
        let config = SyntheticConfig::default()
            .with_seed(42)
            .with_trace_count(10000)
            .with_error_rate(0.10);

        let mut gen = SyntheticCorpus::new(config);
        let corpus = gen.generate();

        let error_count = corpus.errors().len();
        let error_rate = error_count as f64 / corpus.len() as f64;

        // Should be within 2% of target (statistical tolerance)
        assert!(
            (error_rate - 0.10).abs() < 0.02,
            "Error rate {error_rate} not within tolerance of 0.10"
        );
    }
}
