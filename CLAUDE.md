# Nectar

AI-native sampling policy engine for Honeycomb Refinery. TOON is the wire format. Claude is the policy author. Deterministic verification gates all changes.

## Autonomous Operation

This project is configured for autonomous Claude Code operation:
- **Auto-approve**: All cargo commands, file reads, directory operations
- **Verify before commit**: Run `cargo test && cargo clippy` before any commit
- **No human intervention needed** for: building, testing, formatting, documentation

## Quick Reference

| Action | Command |
|--------|---------|
| Build | `cargo build` |
| Test | `cargo test` |
| Lint | `cargo clippy -- -D warnings` |
| Format | `cargo fmt` |
| Check | `cargo check` |
| Doc | `cargo doc --no-deps` |

## Architecture

```
nectar/
├── crates/
│   ├── toon_policy/       # TOON ↔ typed policy model (parse, validate, serialize)
│   ├── nectar_corpus/     # Trace exemplars → TOON encoding for Claude
│   ├── nectar_claude/     # Claude API client, prompt builder, TOON I/O
│   ├── nectar_prover/     # Replay simulation + safety gate (runs BEFORE compile)
│   └── nectar_compiler/   # policy.toon → rules.yaml (deterministic, pure)
└── cmd/
    └── nectar/            # CLI: ingest, propose, prove, compile, explain
```

## Core Invariants

1. **TOON is the only format Claude sees** — all I/O encoded as TOON
2. **Prover runs before compiler** — no rules.yaml until safety checks pass
3. **Compiler is pure** — no network, no randomness, fully deterministic
4. **Claude proposes, never executes** — human approves PR at merge time

## Artifact Chain

```
corpus (traces) ─→ TOON ─→ Claude ─→ policy.toon ─→ Prover ─→ Compiler ─→ rules.yaml
                                                      │
                                                      └─→ FAIL (blocks unsafe policies)
```

## Key Files

| File | Purpose |
|------|---------|
| `policy.toon` | Source of truth for sampling rules |
| `policy.lock` | Compiled + pinned output (deterministic hash) |
| `waggle.md` | Human-readable explanation of current policy |
| `rules.yaml` | Refinery-compatible output |

## Dependencies (Blessed Crates)

| Domain | Crate |
|--------|-------|
| Async runtime | `tokio` |
| HTTP client | `reqwest` |
| Serialization | `serde`, `serde_json` |
| CLI | `clap` |
| Errors | `thiserror`, `anyhow` |
| Observability | `tracing`, `tracing-subscriber` |
| Testing | `proptest` (property-based), `insta` (snapshots) |
| Schema | `schemars` |

## Code Standards

See `.claude/agents/rust-standards.md` for complete rules. Summary:

- Rust 2021 edition
- `#![deny(clippy::all, clippy::pedantic)]` in all crates
- No `.unwrap()` in library code — propagate errors
- All public items documented with `///`
- Property-based tests for all compiler/prover logic
- Snapshot tests for TOON serialization

## What Claude Should Do

When working on this codebase:

1. **Read the steering docs first**: `.claude/agents/rust-standards.md`
2. **Run tests after every change**: `cargo test`
3. **Run clippy before commits**: `cargo clippy -- -D warnings`
4. **Keep functions small**: <50 lines, single responsibility
5. **Prefer composition**: No deep inheritance hierarchies
6. **Make illegal states unrepresentable**: Use types to enforce invariants

## What Claude Should NOT Do

- Never commit code that fails `cargo test`
- Never use `.unwrap()` or `.expect()` in library code
- Never add dependencies without justification
- Never bypass the prover for "quick fixes"
- Never generate JSON for Claude I/O — always TOON
