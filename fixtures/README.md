# Nectar Test Fixtures

Production-grade trace fixtures for developing and testing Nectar sampling policies. These fixtures mirror real-world observability patterns seen at scale.

## Fixture Philosophy

Good sampling policies need good test data. These fixtures provide:

1. **Realistic patterns** - Actual production scenarios, not synthetic noise
2. **Edge cases** - Failure modes, latency outliers, cardinality explosions
3. **Reproducibility** - Deterministic generation via seeded RNG
4. **Scale testing** - From 10 traces to 100k+ service endpoints

## Corpus Fixtures

Located in `corpus/`, these JSON files represent sample trace data.

### Core Fixtures

| File | Traces | Description | Use Case |
|------|--------|-------------|----------|
| `happy_path.json` | 10 | Normal successful requests | Baseline testing |
| `errors.json` | 15 | HTTP 5xx, exceptions | Error retention rules |
| `slow_requests.json` | 12 | Latency > 1s | Performance-based sampling |
| `high_cardinality.json` | 20 | Unique IDs per request | Cardinality safety |
| `mixed.json` | 10 | Production traffic mix | End-to-end testing |

### Production Scenarios

| File | Traces | Description | Key Patterns |
|------|--------|-------------|--------------|
| `microservices_topology.json` | 50 | E-commerce service mesh | Service dependencies, gRPC, Kafka, AWS |
| `failure_patterns.json` | 60 | Cascading failure incident | Circuit breakers, retry storms, recovery |
| `latency_distributions.json` | 100 | Realistic latency shapes | Bimodal, long-tail, cold starts, GC |
| `observability_patterns.json` | 50 | Honeycomb-style telemetry | Wide events, deep traces, SLOs |

## Scenario Details

### Microservices Topology

Simulates a production e-commerce platform:

```
                    ┌─────────────────┐
                    │   api-gateway   │
                    └────────┬────────┘
           ┌─────────────────┼─────────────────┐
           │                 │                 │
    ┌──────▼──────┐   ┌──────▼──────┐   ┌──────▼──────┐
    │user-service │   │order-service│   │product-svc  │
    └──────┬──────┘   └──────┬──────┘   └──────┬──────┘
           │                 │                 │
    ┌──────▼──────┐   ┌──────▼──────┐   ┌──────▼──────┐
    │ auth-service│   │payment-svc  │   │inventory-svc│
    └─────────────┘   └─────────────┘   └─────────────┘
```

**Patterns included:**
- API gateway routing
- gRPC internal calls
- Kafka event streaming
- Redis caching
- PostgreSQL queries
- AWS S3/SQS/Lambda
- External API calls (Stripe, FedEx)

### Failure Patterns

Simulates a 15-minute production incident:

```
Timeline:
├─ 0:00  Pre-incident (normal operation)
├─ 0:30  Database connection pool pressure
├─ 1:00  Pool exhaustion, queries timing out
├─ 1:30  Upstream timeouts cascade
├─ 2:00  Retry storm amplifies load
├─ 3:00  Circuit breakers trip (fast-fail)
├─ 5:00  Graceful degradation active
├─ 10:00 Database recovers
├─ 11:00 Circuit breakers half-open (testing)
├─ 12:00 Circuits close (recovery confirmed)
└─ 15:00 Full recovery verified
```

**Failure types:**
- Connection pool exhaustion
- Timeout cascades (multi-layer)
- Circuit breaker states (open/half-open/closed)
- Retry storms with exponential backoff
- Rate limiting (429)
- Load shedding (503)
- DNS/TLS failures
- Database deadlocks
- Memory pressure / GC pauses
- Network partitions

### Latency Distributions

Real-world latency patterns, not uniform random:

| Distribution | Pattern | Example |
|--------------|---------|---------|
| **Bimodal** | Two peaks | Cache hit (2ms) vs miss (150ms) |
| **Long-tail** | P99 outliers | P50=45ms, P99=500ms, P99.9=5s |
| **Cold start** | Initialization spike | Lambda: cold=8s, warm=400ms |
| **GC pause** | Stop-the-world | Normal + 2s GC pause |
| **Lock contention** | Database waits | Query + 1.2s lock wait |

### Observability Patterns

Patterns that stress sampling policy decisions:

| Pattern | Attribute Count | Challenge |
|---------|-----------------|-----------|
| **Wide events** | 50-120 attrs | Many fields per trace |
| **Deep traces** | 250+ spans | Long call chains |
| **High cardinality** | Unique per request | user.id, request.id explosion |
| **Multi-tenant** | Tenant isolation | Different sampling per tenant |
| **Sampling decisions** | Explicit rules | keep/sample/drop metadata |

## Trace Schema

All trace objects follow this schema:

```json
{
  "trace_id": "string (required, unique)",
  "service.name": "string (required)",
  "http.method": "GET|POST|PUT|DELETE|...",
  "http.route": "string with :param placeholders",
  "http.status_code": 200,
  "duration_ms": 45,
  "span_count": 3,
  "error": false,
  "error.message": "string (when error=true)"
}
```

### Extended Attributes

Fixtures include rich telemetry attributes:

**Request context:**
- `user.id`, `user.tier`, `session.id`
- `request.id`, `idempotency.key`
- `tenant.id`, `tenant.tier`

**Infrastructure:**
- `k8s.pod.name`, `k8s.namespace`
- `deployment.version`, `deployment.canary`
- `db.system`, `db.operation`, `db.connection_pool.*`
- `cache.hit`, `cache.backend`

**Observability:**
- `circuit_breaker.state`, `circuit_breaker.failures`
- `retry.attempt`, `retry.exhausted`
- `rate_limit.remaining`, `rate_limit.limit`
- `sampling.decision`, `sampling.rule`

**Performance:**
- `latency.distribution`, `latency.percentile`
- `jvm.gc_pause_ms`, `jvm.gc_type`
- `faas.cold_start`, `faas.init_duration_ms`

## Programmatic Generation

For VOPR testing, use the `fixtures` module:

```rust
use nectar_corpus::fixtures::{
    FixtureGenerator, FixtureConfig,
    production_corpus, failure_corpus, latency_corpus
};

// Quick generation with defaults
let corpus = production_corpus(42, 1000);  // seed, size

// Custom configuration
let config = FixtureConfig::default()
    .with_seed(12345)
    .with_count(10000);
let mut gen = FixtureGenerator::new(config);

// Generate specific scenarios
let topology = gen.microservices_topology();
let failures = gen.failure_patterns();
let latencies = gen.latency_distributions();
let observability = gen.observability_patterns();
let mixed = gen.production_mix();  // All combined
```

### Determinism

All generation is deterministic:

```rust
let corpus1 = production_corpus(42, 100);
let corpus2 = production_corpus(42, 100);
assert_eq!(corpus1, corpus2);  // Identical!
```

### VOPR Integration

These fixtures integrate with `nectar_vopr` campaigns:

```rust
use nectar_vopr::campaigns::run_chaos_campaign;
use nectar_corpus::fixtures::failure_corpus;

// Generate 10k failure scenario traces
let corpus = failure_corpus(42, 10000);

// Run chaos campaign against sampling policy
let results = run_chaos_campaign(&policy, &corpus, 10000);
assert!(results.all_passed());
```

## Loading Fixtures

```rust
use nectar_corpus::Corpus;

// Load all fixtures from directory
let corpus = Corpus::load_from_directory("fixtures/corpus")?;

// Load specific scenario
let failures = Corpus::load_from_file("fixtures/corpus/failure_patterns.json")?;

// Access traces
for trace in corpus.iter() {
    println!("{}: {} {}ms",
        trace.service.as_deref().unwrap_or("unknown"),
        trace.status.unwrap_or(0),
        trace.duration.as_millis()
    );
}

// Filter by error status
let errors = corpus.errors();
println!("Error traces: {}", errors.len());
```

## Adding New Fixtures

1. Follow the JSON schema above
2. Include realistic, production-like values
3. Add scenario metadata (`_meta` object)
4. Document patterns in this README
5. Add corresponding tests

### Metadata Convention

Each fixture file should include metadata:

```json
{
  "_meta": {
    "description": "Brief description of the scenario",
    "scenario": "What production situation this models",
    "patterns": ["pattern1", "pattern2"],
    "traces": 50
  },
  "traces": [...]
}
```

## Test Matrix

| Scenario | Compiler | Prover | Chaos | VOPR |
|----------|:--------:|:------:|:-----:|:----:|
| Happy path | ✓ | ✓ | — | — |
| Errors | ✓ | ✓ | ✓ | ✓ |
| Slow requests | ✓ | ✓ | ✓ | ✓ |
| High cardinality | ✓ | ✓ | ✓ | ✓ |
| Microservices | ✓ | ✓ | ✓ | ✓ |
| Failure patterns | ✓ | ✓ | ✓ | ✓ |
| Latency distributions | ✓ | ✓ | ✓ | ✓ |
| Observability | ✓ | ✓ | ✓ | ✓ |
