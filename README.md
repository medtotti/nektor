# Nectar

[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org/)
[![License](https://img.shields.io/badge/license-MIT%2FApache--2.0-blue.svg)](LICENSE)

<!-- GitHub Topics: observability, sampling, honeycomb, refinery, telemetry, tracing, opentelemetry, rust, ai, llm, claude, anthropic, apm, monitoring, tail-sampling, distributed-tracing, policy-engine -->

AI-native sampling policy engine for [Honeycomb Refinery](https://docs.honeycomb.io/manage-data-volume/sample/honeycomb-refinery/).

Nectar uses Claude to generate and refine tail-based sampling policies from natural language intent and trace exemplars. Policies are expressed in TOON format, verified against historical data, and compiled to Refinery-compatible rules.

## Features

- **Natural language policy generation** - Describe what you want to keep in plain English
- **Trace-aware suggestions** - Learns from your actual traffic patterns
- **Safety verification** - Proves policies won't drop critical traces before deployment
- **Deterministic compilation** - Same input always produces same output, with lockfile verification
- **Human-readable explanations** - Generates "waggle" reports explaining policy behavior

## Installation

```bash
cargo install --path cmd/nectar
```

## Quick Start

```bash
# Initialize a new Nectar project
nectar init my-project
cd my-project

# Generate a policy from natural language
nectar propose "Keep all errors and slow requests over 5 seconds, sample everything else at 1%"

# Verify the policy against trace corpus
nectar prove --corpus corpus/

# Compile to Refinery rules
nectar compile -o rules.yaml

# Generate human-readable explanation
nectar explain
```

## Architecture

```
nectar/
├── crates/
│   ├── toon_policy/       # TOON format parser and policy model
│   ├── nectar_corpus/     # Trace exemplar storage and encoding
│   ├── nectar_claude/     # Claude API client for policy generation
│   ├── nectar_prover/     # Policy verification and safety checks
│   ├── nectar_compiler/   # Policy to Refinery rules compiler
│   └── nectar_vopr/       # VOPR deterministic simulation testing
└── cmd/
    └── nectar/            # CLI application
```

## TOON Format

TOON (Text Object Notation) is a human-readable format for sampling policies:

```toon
nectar_policy{version,name,budget_per_second,rules}:
  1
  production-sampling
  10000
  rules[3]{name,description,match,action,priority}:
    keep-errors,Retain all HTTP 5xx errors,http.status >= 500,keep,100
    keep-slow,Retain slow requests,duration > 5s,keep,90
    sample-rest,Sample remaining traffic,true,sample(0.01),0
```

## Workflow

1. **Ingest** - Load trace exemplars into the corpus
2. **Propose** - Generate or refine policy using Claude
3. **Prove** - Verify policy safety against historical data
4. **Compile** - Generate Refinery-compatible `rules.yaml`
5. **Deploy** - Ship rules to Refinery

## Policy Verification

The prover ensures policies meet safety requirements:

- **Fallback rule** - Every policy must have a catch-all rule
- **Error preservation** - Error traces (status >= 500) are never dropped
- **Must-keep coverage** - Critical traces identified in corpus are retained
- **Budget compliance** - Policies stay within configured throughput limits

## VOPR Testing

Nectar uses **VOPR (Vaguely Ordered Parallel Replayability)** for deterministic simulation testing, inspired by [TigerBeetle's testing methodology](https://github.com/tigerbeetle/tigerbeetle/blob/main/docs/internals/vopr.md).

VOPR enables:
- **Determinism** - ChaCha8 RNG with explicit seeds ensures identical results from identical inputs
- **Time compression** - Simulate years of policy evolution in seconds
- **Fault injection** - Systematic chaos testing with controlled corruption
- **Reproducibility** - Every failure includes a seed for exact replay

```bash
# Run VOPR simulation tests
cargo test --package nectar_vopr

# Example output:
# === VOPR Campaign Summary ===
# [PASS] chaos_campaign: 10000 iterations, 36000000s simulated
# [PASS] evolution_campaign: 365 days simulated
# [PASS] determinism_campaign: 5000 consistency checks
# Total: 20375 iterations, 2.2 years simulated in ~13s
```

See `crates/nectar_vopr/TEST_STRATEGY.md` for detailed documentation.

## Commands

| Command | Description |
|---------|-------------|
| `nectar init` | Initialize a new Nectar project |
| `nectar propose` | Generate policy from natural language |
| `nectar prove` | Verify policy against corpus |
| `nectar compile` | Compile policy to Refinery rules |
| `nectar explain` | Generate human-readable policy report |

## Configuration

Set your Anthropic API key for policy generation:

```bash
export ANTHROPIC_API_KEY=your-key-here
```

## Development

```bash
# Build
cargo build

# Test
cargo test

# Lint
cargo clippy -- -D warnings

# Format
cargo fmt
```

## License

MIT
