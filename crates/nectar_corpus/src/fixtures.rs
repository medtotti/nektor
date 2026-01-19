//! Production-grade fixture generators for testing.
//!
//! These generators create realistic trace corpora that mirror production
//! patterns seen at scale. Inspired by Honeycomb's observability patterns.
//!
//! # Scenarios
//!
//! - **Microservices topology**: Realistic service mesh with dependencies
//! - **Failure patterns**: Cascading failures, circuit breakers, retry storms
//! - **Latency distributions**: Bimodal, long-tail, cold starts, GC pauses
//! - **Observability patterns**: Wide events, deep traces, high cardinality

// Allow panics in fixture generators - arrays are never empty
#![allow(clippy::missing_panics_doc)]
#![allow(clippy::suboptimal_flops)]

use crate::{Corpus, Trace};
use rand::prelude::*;
use rand_chacha::ChaCha8Rng;
use std::time::Duration;

/// Configuration for fixture generation.
#[derive(Debug, Clone)]
pub struct FixtureConfig {
    /// Random seed for reproducibility.
    pub seed: u64,
    /// Number of traces to generate.
    pub trace_count: usize,
}

impl Default for FixtureConfig {
    fn default() -> Self {
        Self {
            seed: 42,
            trace_count: 100,
        }
    }
}

impl FixtureConfig {
    /// Creates a config with the given seed.
    #[must_use]
    pub const fn with_seed(mut self, seed: u64) -> Self {
        self.seed = seed;
        self
    }

    /// Sets the trace count.
    #[must_use]
    pub const fn with_count(mut self, count: usize) -> Self {
        self.trace_count = count;
        self
    }
}

/// Production fixture generator.
pub struct FixtureGenerator {
    rng: ChaCha8Rng,
    config: FixtureConfig,
}

impl FixtureGenerator {
    /// Creates a new fixture generator.
    #[must_use]
    pub fn new(config: FixtureConfig) -> Self {
        let rng = ChaCha8Rng::seed_from_u64(config.seed);
        Self { rng, config }
    }

    /// Generates a microservices topology corpus.
    ///
    /// Simulates a realistic e-commerce service mesh with:
    /// - API gateway as entry point
    /// - Domain services (user, order, product, payment)
    /// - Infrastructure services (cache, db, queue)
    /// - External integrations (payment processors, shipping)
    #[must_use]
    pub fn microservices_topology(&mut self) -> Corpus {
        let mut corpus = Corpus::new();
        let services = [
            ("api-gateway", vec!["/api/v2/users/:id", "/api/v2/orders", "/api/v2/products", "/api/v2/checkout", "/health"]),
            ("user-service", vec!["/internal/users/:id", "/internal/users/lookup", "/internal/auth/validate"]),
            ("order-service", vec!["/internal/orders", "/internal/orders/:id", "/internal/orders/history"]),
            ("product-service", vec!["/internal/products/:id", "/internal/products/search", "/internal/inventory"]),
            ("payment-service", vec!["/internal/charge", "/internal/refund", "/internal/verify"]),
            ("notification-service", vec!["/internal/send", "/internal/batch", "/internal/templates"]),
            ("cache-service", vec!["/internal/get", "/internal/set", "/internal/invalidate"]),
        ];

        for i in 0..self.config.trace_count {
            let (service, routes) = services.choose(&mut self.rng).unwrap();
            let route = routes.choose(&mut self.rng).unwrap();

            let is_error = self.rng.gen_bool(0.03);
            let is_slow = self.rng.gen_bool(0.05);

            let status = if is_error {
                *[500u16, 502, 503, 504].choose(&mut self.rng).unwrap()
            } else {
                *[200u16, 201, 204].choose(&mut self.rng).unwrap()
            };

            let base_latency = match *service {
                "api-gateway" => 50,
                "cache-service" => 5,
                "payment-service" => 400,
                _ => 30,
            };

            let duration_ms = if is_slow {
                base_latency * 50 + self.rng.gen_range(0..5000)
            } else {
                base_latency + self.rng.gen_range(0..base_latency * 2)
            };

            let span_count = match *service {
                "api-gateway" => self.rng.gen_range(5..25),
                "payment-service" => self.rng.gen_range(8..15),
                _ => self.rng.gen_range(2..8),
            };

            let mut trace = Trace::new(format!("topo-{i:08x}"))
                .with_service(*service)
                .with_endpoint(*route)
                .with_status(status)
                .with_duration(Duration::from_millis(duration_ms))
                .with_attribute("span_count", span_count.to_string())
                .with_attribute("scenario", "microservices_topology");

            if is_error {
                trace = trace.with_attribute("error.type", self.random_error_type());
            }

            corpus.add(trace);
        }

        corpus
    }

    /// Generates a failure patterns corpus.
    ///
    /// Simulates cascading failures through a service mesh:
    /// - Database connection pool exhaustion
    /// - Timeout cascades
    /// - Circuit breaker states (open, half-open, closed)
    /// - Retry storms
    /// - Graceful degradation
    #[must_use]
    pub fn failure_patterns(&mut self) -> Corpus {
        let mut corpus = Corpus::new();

        let phases = [
            ("pre-incident", 0.0, 45, 0.01),
            ("degradation-start", 0.1, 500, 0.05),
            ("pool-exhaustion", 0.3, 5000, 0.30),
            ("cascade-start", 0.5, 10000, 0.50),
            ("retry-storm", 0.7, 15000, 0.70),
            ("circuit-open", 0.8, 5, 0.90),
            ("recovery-start", 0.6, 500, 0.20),
            ("circuit-half-open", 0.4, 200, 0.10),
            ("recovered", 0.0, 50, 0.02),
        ];

        let services = ["api-gateway", "order-service", "payment-service", "inventory-service"];

        for i in 0..self.config.trace_count {
            let phase_idx = (i * phases.len()) / self.config.trace_count;
            let (phase, error_boost, base_latency, error_rate) = phases[phase_idx.min(phases.len() - 1)];

            let service = services.choose(&mut self.rng).unwrap();
            let combined_rate = f64::min(error_rate + error_boost * 0.3, 0.99);
            let is_error = self.rng.gen_bool(combined_rate);

            let status = if is_error {
                *[500u16, 502, 503, 504].choose(&mut self.rng).unwrap()
            } else {
                200
            };

            let duration_ms = base_latency + self.rng.gen_range(0..base_latency);

            let mut trace = Trace::new(format!("fail-{i:08x}"))
                .with_service(*service)
                .with_endpoint("/api/v2/orders")
                .with_status(status)
                .with_duration(Duration::from_millis(duration_ms))
                .with_attribute("incident.phase", phase)
                .with_attribute("scenario", "failure_patterns");

            match phase {
                "circuit-open" | "circuit-half-open" => {
                    trace = trace
                        .with_attribute("circuit_breaker.state", phase.replace("circuit-", ""))
                        .with_attribute("circuit_breaker.failures", self.rng.gen_range(10..30).to_string());
                }
                "retry-storm" => {
                    trace = trace
                        .with_attribute("retry.attempt", self.rng.gen_range(1..5).to_string())
                        .with_attribute("retry.exhausted", (self.rng.gen_range(1..5) >= 4).to_string());
                }
                "pool-exhaustion" => {
                    trace = trace
                        .with_attribute("db.connection_pool.active", "20")
                        .with_attribute("db.connection_pool.max", "20")
                        .with_attribute("db.connection_pool.waiting", self.rng.gen_range(10..100).to_string());
                }
                _ => {}
            }

            if is_error {
                trace = trace.with_attribute("error.message", self.failure_error_message(phase));
            }

            corpus.add(trace);
        }

        corpus
    }

    /// Generates a latency distribution corpus.
    ///
    /// Creates traces with realistic latency patterns:
    /// - Bimodal: Cache hits (fast) vs misses (slow)
    /// - Long-tail: P99/P999 outliers
    /// - Cold starts: Lambda/container initialization
    /// - GC pauses: JVM stop-the-world events
    #[must_use]
    pub fn latency_distributions(&mut self) -> Corpus {
        let mut corpus = Corpus::new();

        for i in 0..self.config.trace_count {
            let distribution = match i % 5 {
                0 => "bimodal",
                1 => "long_tail",
                2 => "cold_start",
                3 => "gc_pause",
                _ => "database",
            };

            let (duration_ms, attrs) = match distribution {
                "bimodal" => {
                    let cache_hit = self.rng.gen_bool(0.85);
                    let ms = if cache_hit {
                        self.rng.gen_range(1..5)
                    } else {
                        self.rng.gen_range(100..300)
                    };
                    (ms, vec![("cache.hit", cache_hit.to_string())])
                }
                "long_tail" => {
                    let percentile: f64 = self.rng.gen();
                    let ms = if percentile < 0.50 {
                        self.rng.gen_range(40..60)
                    } else if percentile < 0.90 {
                        self.rng.gen_range(60..150)
                    } else if percentile < 0.99 {
                        self.rng.gen_range(150..500)
                    } else if percentile < 0.999 {
                        self.rng.gen_range(500..2000)
                    } else {
                        self.rng.gen_range(2000..10000)
                    };
                    let p = if percentile < 0.50 { "p50" }
                        else if percentile < 0.90 { "p90" }
                        else if percentile < 0.99 { "p99" }
                        else { "p99.9" };
                    (ms, vec![("latency.percentile", p.to_string())])
                }
                "cold_start" => {
                    let is_cold = self.rng.gen_bool(0.05);
                    let ms = if is_cold {
                        self.rng.gen_range(5000..12000)
                    } else {
                        self.rng.gen_range(200..600)
                    };
                    (ms, vec![("faas.cold_start", is_cold.to_string())])
                }
                "gc_pause" => {
                    let gc_event = self.rng.gen_bool(0.02);
                    let (ms, gc_ms) = if gc_event {
                        let gc = self.rng.gen_range(100..15000);
                        (self.rng.gen_range(50..200) + gc, gc)
                    } else {
                        (self.rng.gen_range(50..200), 0)
                    };
                    (ms, vec![("jvm.gc_pause_ms", gc_ms.to_string())])
                }
                _ => {
                    let is_indexed = self.rng.gen_bool(0.90);
                    let ms = if is_indexed {
                        self.rng.gen_range(1..10)
                    } else {
                        self.rng.gen_range(100..2000)
                    };
                    (ms, vec![("db.index_used", is_indexed.to_string())])
                }
            };

            let mut trace = Trace::new(format!("lat-{i:08x}"))
                .with_service("latency-test-service")
                .with_endpoint("/api/v2/test")
                .with_status(200)
                .with_duration(Duration::from_millis(duration_ms))
                .with_attribute("latency.distribution", distribution)
                .with_attribute("scenario", "latency_distributions");

            for (key, value) in attrs {
                trace = trace.with_attribute(key, value);
            }

            corpus.add(trace);
        }

        corpus
    }

    /// Generates an observability patterns corpus.
    ///
    /// Creates traces demonstrating:
    /// - Wide events: Many attributes per trace
    /// - Deep traces: Many spans in call chain
    /// - High cardinality: Unique IDs per request
    /// - Sampling decisions: Keep/sample/drop patterns
    #[must_use]
    pub fn observability_patterns(&mut self) -> Corpus {
        let mut corpus = Corpus::new();

        for i in 0..self.config.trace_count {
            let pattern = match i % 6 {
                0 => "wide_event",
                1 => "deep_trace",
                2 => "high_cardinality",
                3 => "sampling_decision",
                4 => "multi_tenant",
                _ => "correlation",
            };

            let mut trace = Trace::new(format!("obs-{i:08x}"))
                .with_service("api-gateway")
                .with_endpoint("/api/v2/checkout")
                .with_status(200)
                .with_duration(Duration::from_millis(self.rng.gen_range(50..500)))
                .with_attribute("pattern", pattern)
                .with_attribute("scenario", "observability_patterns");

            match pattern {
                "wide_event" => {
                    trace = trace
                        .with_attribute("user.id", format!("usr_{:016x}", self.rng.gen::<u64>()))
                        .with_attribute("session.id", format!("sess_{:016x}", self.rng.gen::<u64>()))
                        .with_attribute("request.id", format!("req_{:016x}", self.rng.gen::<u64>()))
                        .with_attribute("cart.item_count", self.rng.gen_range(1..10).to_string())
                        .with_attribute("cart.total_cents", self.rng.gen_range(1000..50000).to_string())
                        .with_attribute("payment.method", *["card", "paypal", "apple_pay"].choose(&mut self.rng).unwrap())
                        .with_attribute("attribute_count", "50+");
                }
                "deep_trace" => {
                    let depth = self.rng.gen_range(5..15);
                    let span_count = self.rng.gen_range(50..500);
                    trace = trace
                        .with_attribute("trace.depth", depth.to_string())
                        .with_attribute("span_count", span_count.to_string())
                        .with_attribute("trace.services_involved", self.rng.gen_range(5..15).to_string());
                }
                "high_cardinality" => {
                    trace = trace
                        .with_attribute("user.id", format!("usr_{:032x}", self.rng.gen::<u128>()))
                        .with_attribute("request.id", format!("req_{:032x}", self.rng.gen::<u128>()))
                        .with_attribute("cardinality.warning", "true");
                }
                "sampling_decision" => {
                    let decision = ["keep", "sample", "drop"].choose(&mut self.rng).unwrap();
                    trace = trace
                        .with_attribute("sampling.decision", *decision)
                        .with_attribute("sampling.rule", format!("{decision}-rule"))
                        .with_attribute("sampling.rate", match *decision {
                            "keep" => "1.0",
                            "sample" => "0.10",
                            _ => "0.0",
                        });
                }
                "multi_tenant" => {
                    let tier = ["enterprise", "business", "starter"].choose(&mut self.rng).unwrap();
                    trace = trace
                        .with_attribute("tenant.id", format!("tenant_{:08x}", self.rng.gen::<u32>()))
                        .with_attribute("tenant.tier", *tier)
                        .with_attribute("tenant.isolation", if *tier == "enterprise" { "dedicated" } else { "shared" });
                }
                _ => {
                    trace = trace
                        .with_attribute("correlation.order_id", format!("ord_{:016x}", self.rng.gen::<u64>()))
                        .with_attribute("correlation.user_id", format!("usr_{:016x}", self.rng.gen::<u64>()));
                }
            }

            corpus.add(trace);
        }

        corpus
    }

    /// Generates a combined production corpus with all patterns.
    #[must_use]
    pub fn production_mix(&mut self) -> Corpus {
        let mut corpus = Corpus::new();
        let traces_per_scenario = self.config.trace_count / 4;

        self.rng = ChaCha8Rng::seed_from_u64(self.config.seed);

        let old_count = self.config.trace_count;
        self.config.trace_count = traces_per_scenario;

        for trace in self.microservices_topology().iter() {
            corpus.add(trace.clone());
        }
        for trace in self.failure_patterns().iter() {
            corpus.add(trace.clone());
        }
        for trace in self.latency_distributions().iter() {
            corpus.add(trace.clone());
        }
        for trace in self.observability_patterns().iter() {
            corpus.add(trace.clone());
        }

        self.config.trace_count = old_count;
        corpus
    }

    fn random_error_type(&mut self) -> &'static str {
        [
            "ConnectionTimeout",
            "ServiceUnavailable",
            "InternalServerError",
            "DatabaseError",
            "AuthenticationFailed",
            "RateLimitExceeded",
            "InvalidRequest",
            "ResourceNotFound",
        ]
        .choose(&mut self.rng)
        .unwrap()
    }

    fn failure_error_message(&mut self, phase: &str) -> String {
        match phase {
            "pool-exhaustion" => "Connection pool exhausted: timeout waiting for connection".to_string(),
            "cascade-start" => "Upstream service unavailable".to_string(),
            "retry-storm" => format!("Retry attempt {} failed", self.rng.gen_range(1..5)),
            "circuit-open" => "Circuit breaker OPEN - fast failing".to_string(),
            _ => "Internal server error".to_string(),
        }
    }
}

/// Convenience function to generate a deterministic production corpus.
#[must_use]
pub fn production_corpus(seed: u64, size: usize) -> Corpus {
    let config = FixtureConfig::default()
        .with_seed(seed)
        .with_count(size);
    let mut gen = FixtureGenerator::new(config);
    gen.production_mix()
}

/// Convenience function to generate a failure scenarios corpus.
#[must_use]
pub fn failure_corpus(seed: u64, size: usize) -> Corpus {
    let config = FixtureConfig::default()
        .with_seed(seed)
        .with_count(size);
    let mut gen = FixtureGenerator::new(config);
    gen.failure_patterns()
}

/// Convenience function to generate a latency distribution corpus.
#[must_use]
pub fn latency_corpus(seed: u64, size: usize) -> Corpus {
    let config = FixtureConfig::default()
        .with_seed(seed)
        .with_count(size);
    let mut gen = FixtureGenerator::new(config);
    gen.latency_distributions()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_generation_is_deterministic() {
        let corpus1 = production_corpus(42, 100);
        let corpus2 = production_corpus(42, 100);

        assert_eq!(corpus1.len(), corpus2.len());

        let traces1: Vec<_> = corpus1.iter().collect();
        let traces2: Vec<_> = corpus2.iter().collect();

        for (t1, t2) in traces1.iter().zip(traces2.iter()) {
            assert_eq!(t1.trace_id, t2.trace_id);
            assert_eq!(t1.service, t2.service);
            assert_eq!(t1.status, t2.status);
            assert_eq!(t1.duration, t2.duration);
        }
    }

    #[test]
    fn microservices_topology_has_variety() {
        let config = FixtureConfig::default().with_count(1000);
        let mut gen = FixtureGenerator::new(config);
        let corpus = gen.microservices_topology();

        // Should have multiple services
        let services: std::collections::HashSet<_> = corpus
            .iter()
            .filter_map(|t| t.service.as_ref())
            .collect();
        assert!(services.len() >= 5, "Should have at least 5 different services");

        // Should have some errors
        let errors = corpus.errors();
        assert!(!errors.is_empty(), "Should have some errors");
    }

    #[test]
    fn failure_patterns_shows_progression() {
        let config = FixtureConfig::default().with_count(100);
        let mut gen = FixtureGenerator::new(config);
        let corpus = gen.failure_patterns();

        // Should have phase progression
        let phases: Vec<_> = corpus
            .iter()
            .filter_map(|t| t.attributes.get("incident.phase"))
            .collect();

        assert!(!phases.is_empty());
        // Early traces should be pre-incident or degradation
        assert!(phases.first().is_some_and(|p| p.contains("pre") || p.contains("deg")));
    }

    #[test]
    fn latency_distributions_has_outliers() {
        let config = FixtureConfig::default().with_count(1000);
        let mut gen = FixtureGenerator::new(config);
        let corpus = gen.latency_distributions();

        let durations: Vec<_> = corpus.iter().map(|t| t.duration.as_millis()).collect();

        // Should have fast traces (< 10ms)
        let fast = durations.iter().filter(|&&d| d < 10).count();
        assert!(fast > 0, "Should have fast traces");

        // Should have slow traces (> 1000ms)
        let slow = durations.iter().filter(|&&d| d > 1000).count();
        assert!(slow > 0, "Should have slow outliers");
    }

    #[test]
    fn observability_patterns_has_all_patterns() {
        let config = FixtureConfig::default().with_count(100);
        let mut gen = FixtureGenerator::new(config);
        let corpus = gen.observability_patterns();

        let patterns: std::collections::HashSet<_> = corpus
            .iter()
            .filter_map(|t| t.attributes.get("pattern"))
            .map(String::as_str)
            .collect();

        assert!(patterns.contains("wide_event"));
        assert!(patterns.contains("deep_trace"));
        assert!(patterns.contains("high_cardinality"));
        assert!(patterns.contains("sampling_decision"));
    }

    #[test]
    fn production_mix_combines_all_scenarios() {
        let corpus = production_corpus(42, 400);

        let scenarios: std::collections::HashSet<_> = corpus
            .iter()
            .filter_map(|t| t.attributes.get("scenario"))
            .map(String::as_str)
            .collect();

        assert!(scenarios.contains("microservices_topology"));
        assert!(scenarios.contains("failure_patterns"));
        assert!(scenarios.contains("latency_distributions"));
        assert!(scenarios.contains("observability_patterns"));
    }
}
