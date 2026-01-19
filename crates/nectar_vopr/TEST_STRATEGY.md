# VOPR Test Strategy

## Overview

VOPR (Vaguely Ordered Parallel Replayability) is a deterministic simulation testing methodology. All randomness is seeded, making failures reproducible.

## Core Principles

| Principle | Implementation |
|-----------|----------------|
| **Determinism** | ChaCha8 RNG with explicit seeds |
| **Reproducibility** | Same seed = same test run |
| **Time Compression** | Simulate hours in milliseconds |
| **Fault Injection** | Controlled chaos with configurable intensity |

## Test Layers

```
┌─────────────────────────────────────────────┐
│           Property-Based Tests              │  ← generators.rs
│         (proptest strategies)               │
├─────────────────────────────────────────────┤
│          Synthetic Workloads                │  ← synthetic.rs
│    (realistic corpus generation)            │
├─────────────────────────────────────────────┤
│         Simulation Harness                  │  ← harness.rs
│   (deterministic scenario execution)        │
├─────────────────────────────────────────────┤
│           Chaos Injection                   │  ← chaos.rs
│      (fault tolerance testing)              │
├─────────────────────────────────────────────┤
│          Replay Testing                     │  ← replay.rs
│    (time-compressed evolution)              │
└─────────────────────────────────────────────┘
```

## Components

### 1. Generators (`generators.rs`)

Proptest strategies for synthetic data:

```rust
service_name()    // "[a-z][a-z0-9-]{2,20}"
http_status()     // 80% success, 10% 4xx, 10% 5xx
duration_ms()     // 70% fast, 20% normal, 10% slow
match_expr()      // Valid policy expressions
policy()          // Complete policies with fallback
```

### 2. Synthetic Corpus (`synthetic.rs`)

Generates realistic trace corpora:

```rust
SyntheticConfig {
    seed: 42,              // Reproducible
    trace_count: 1000,
    error_rate: 0.05,      // 5% errors
    slow_rate: 0.10,       // 10% slow
}
```

Methods:
- `generate()` — Standard corpus
- `generate_edge_cases()` — Boundary conditions
- `generate_high_cardinality()` — Stress testing

### 3. Simulation Harness (`harness.rs`)

Executes test scenarios deterministically:

| Scenario | Validates |
|----------|-----------|
| `CompileDeterminism` | Same input → same output |
| `ProverConsistency` | Repeated verification = same result |
| `RoundTrip` | serialize → parse → serialize = identical |
| `ChaosResilience` | Graceful handling of corrupted input |
| `HighCardinality` | Performance under load |

### 4. Chaos Injection (`chaos.rs`)

Controlled fault injection:

| Corruption Type | Effect |
|-----------------|--------|
| `InvalidStatus` | HTTP status = 999 |
| `ZeroDuration` | duration = 0 |
| `EmptyServiceName` | service = "" |
| `ExtremeValues` | status = MAX, duration = MAX |
| `MalformedMatchExpr` | Syntax errors in rules |
| `RemoveFallback` | Delete fallback rule |

Intensity ramps from 0% to 50% corruption rate.

### 5. Replay Testing (`replay.rs`)

Time-compressed policy evolution:

```rust
TimeCompressor {
    ratio: 1000,  // 1ms real = 1s simulated
}

SimAction::AddRule { .. }
SimAction::RemoveRule { .. }
SimAction::Verify
SimAction::Compile
SimAction::Checkpoint
```

Checkpoints capture state hashes for regression detection.

## Test Matrix

| Component | Unit | Property | Chaos | Replay |
|-----------|:----:|:--------:|:-----:|:------:|
| Compiler  | ✓    | ✓        | ✓     | ✓      |
| Prover    | ✓    | ✓        | ✓     | ✓      |
| Parser    | ✓    | ✓        | —     | —      |
| Corpus    | ✓    | ✓        | ✓     | —      |

## Usage

```rust
// Deterministic simulation
let config = SimConfig::default()
    .with_seed(12345)
    .with_iterations(100);
let mut sim = Simulation::new(config);
let result = sim.run_scenario(&Scenario::CompileDeterminism { policy });

// Chaos campaign
let results = chaos_campaign(&policy, &corpus, 100);

// Time-compressed replay
let mut evo = PolicyEvolutionSim::new(policy, corpus);
evo.step(SimAction::AddRule { .. });
evo.step(SimAction::Checkpoint);
```

## Reproducing Failures

Every test failure includes a seed:

```
thread 'test' panicked at assertion failed
seed: 12345
```

Replay with:

```rust
let config = SimConfig::default().with_seed(12345);
```

## Key Invariants Tested

1. **Compiler determinism** — Same policy always produces identical output
2. **Prover consistency** — Verification results never vary
3. **Roundtrip integrity** — Serialization is lossless
4. **Graceful degradation** — Chaos doesn't cause panics
5. **Performance bounds** — High cardinality completes within timeout
