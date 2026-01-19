# Nectar

AI-native sampling policy engine for [Honeycomb Refinery](https://docs.honeycomb.io/manage-data-volume/refinery/).

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
│   └── nectar_compiler/   # Policy to Refinery rules compiler
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
