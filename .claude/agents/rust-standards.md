# Rust Engineering Standards

This document defines the non-negotiable quality standards for Nectar. Every line of code must meet these criteria. No exceptions. No "we'll fix it later."

## Philosophy: Zero Slop

"Slop" is AI-generated code that:
- Works but isn't understood
- Has unnecessary complexity
- Lacks proper error handling
- Uses magic values
- Has poor naming
- Skips edge cases

We reject slop. Every function should be explainable in one sentence. Every type should have a reason. Every error should be actionable.

---

## Module Structure

### File Organization

```rust
// 1. Module docs (required for all public modules)
//! Brief description of what this module does.
//!
//! # Examples
//!
//! ```rust
//! // Show how to use the main types
//! ```

// 2. Imports (grouped, sorted)
use std::collections::HashMap;  // std first
use std::io::{self, Read};

use serde::{Deserialize, Serialize};  // external crates
use tokio::sync::mpsc;

use crate::error::Result;  // crate imports last
use crate::types::Policy;

// 3. Constants (if any)
const MAX_RETRIES: u32 = 3;

// 4. Type definitions
// 5. Trait definitions
// 6. Implementations
// 7. Functions
// 8. Tests (in same file or tests/ submodule)
```

### Naming Conventions

| Item | Convention | Example |
|------|------------|---------|
| Types | `PascalCase` | `PolicyRule`, `TraceCorpus` |
| Functions | `snake_case` | `parse_policy`, `validate_rule` |
| Constants | `SCREAMING_SNAKE_CASE` | `MAX_TRACE_SIZE` |
| Modules | `snake_case` | `toon_parser`, `rule_compiler` |
| Type parameters | Single uppercase or descriptive | `T`, `E`, `Item` |
| Lifetimes | Short, descriptive | `'a`, `'src`, `'input` |

---

## Error Handling

### The Error Contract

```rust
// Every crate has ONE error type in src/error.rs
use thiserror::Error;

#[derive(Debug, Error)]
pub enum Error {
    #[error("failed to parse TOON at line {line}: {reason}")]
    ParseError { line: usize, reason: String },

    #[error("policy validation failed: {0}")]
    ValidationError(String),

    #[error("prover rejected policy: {violations:?}")]
    ProverRejection { violations: Vec<String> },

    #[error(transparent)]
    Io(#[from] std::io::Error),

    #[error(transparent)]
    Serde(#[from] serde_json::Error),
}

pub type Result<T> = std::result::Result<T, Error>;
```

### Error Rules

1. **Never `.unwrap()` in library code** — always propagate
2. **Never `.expect()` in library code** — use proper errors
3. **`.unwrap()` is allowed in**:
   - Tests
   - Examples
   - CLI `main()` (after user-facing error display)
4. **Errors must be actionable** — tell the user what went wrong and what to do
5. **Use `?` for propagation** — not manual `match` unless adding context

```rust
// BAD
let value = map.get("key").unwrap();

// BAD
let value = map.get("key").expect("key should exist");

// GOOD
let value = map.get("key").ok_or_else(|| Error::MissingKey("key"))?;

// GOOD (adding context)
let value = map.get("key")
    .ok_or_else(|| Error::MissingKey("key"))
    .map_err(|e| e.with_context(|| format!("while processing {}", item)))?;
```

---

## Function Design

### Size Limits

- **Maximum 50 lines** per function (excluding docs/tests)
- **Maximum 4 parameters** — use a config struct if more needed
- **Maximum 3 levels of nesting** — extract helpers if deeper

### Single Responsibility

Each function does ONE thing. If you use "and" to describe it, split it.

```rust
// BAD: Does two things
fn parse_and_validate(input: &str) -> Result<Policy> { ... }

// GOOD: Separate concerns
fn parse(input: &str) -> Result<Ast> { ... }
fn validate(ast: &Ast) -> Result<Policy> { ... }
```

### Pure Functions Preferred

When possible, functions should be pure (no side effects, deterministic output):

```rust
// PURE: Same input always gives same output
fn compile_rule(rule: &Rule) -> CompiledRule { ... }

// IMPURE: Has side effects (mark clearly)
fn compile_rule_with_metrics(rule: &Rule, metrics: &mut Metrics) -> CompiledRule { ... }
```

---

## Type Design

### Make Illegal States Unrepresentable

```rust
// BAD: Can have invalid state
struct Policy {
    rules: Vec<Rule>,
    is_validated: bool,  // Can forget to set this
}

// GOOD: Type system enforces validity
struct UnvalidatedPolicy { rules: Vec<Rule> }
struct ValidatedPolicy { rules: Vec<Rule> }  // Can only be created via validate()

impl UnvalidatedPolicy {
    fn validate(self) -> Result<ValidatedPolicy> {
        // Validation logic here
        Ok(ValidatedPolicy { rules: self.rules })
    }
}
```

### Newtypes for Semantic Meaning

```rust
// BAD: Easy to mix up
fn process(trace_id: String, span_id: String) { ... }

// GOOD: Types prevent mistakes
struct TraceId(String);
struct SpanId(String);
fn process(trace_id: TraceId, span_id: SpanId) { ... }
```

### Enums Over Booleans

```rust
// BAD: What does `true` mean?
fn sample(trace: &Trace, keep: bool) { ... }

// GOOD: Self-documenting
enum SampleDecision { Keep, Drop }
fn sample(trace: &Trace, decision: SampleDecision) { ... }
```

---

## Documentation

### Every Public Item Gets Docs

```rust
/// Compiles a TOON policy into Refinery rules.
///
/// # Arguments
///
/// * `policy` - The validated policy to compile
/// * `options` - Compilation options (output format, optimization level)
///
/// # Returns
///
/// The compiled rules, ready to be serialized to YAML.
///
/// # Errors
///
/// Returns an error if:
/// - The policy contains unsupported constructs
/// - Budget calculations overflow
///
/// # Examples
///
/// ```rust
/// let policy = Policy::parse(input)?;
/// let rules = compile(&policy, CompileOptions::default())?;
/// ```
pub fn compile(policy: &Policy, options: CompileOptions) -> Result<Rules> {
    // ...
}
```

### Doc Test Everything

Examples in docs MUST compile and run:

```rust
/// ```rust
/// # use nectar_compiler::compile;
/// # fn main() -> Result<(), Box<dyn std::error::Error>> {
/// let rules = compile("policy.toon")?;
/// assert!(!rules.is_empty());
/// # Ok(())
/// # }
/// ```
```

---

## Testing

### Test Structure

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    // Unit tests: test one function in isolation
    #[test]
    fn parse_empty_policy_returns_empty_rules() {
        let result = parse("");
        assert!(result.unwrap().rules.is_empty());
    }

    // Property tests: test invariants hold for all inputs
    proptest! {
        #[test]
        fn roundtrip_preserves_policy(policy in arb_policy()) {
            let encoded = encode(&policy);
            let decoded = decode(&encoded).unwrap();
            prop_assert_eq!(policy, decoded);
        }
    }

    // Snapshot tests: for complex outputs
    #[test]
    fn compile_fixture_policy() {
        let policy = include_str!("../fixtures/sample.toon");
        let rules = compile(policy).unwrap();
        insta::assert_yaml_snapshot!(rules);
    }
}
```

### Test Naming

Tests describe behavior, not implementation:

```rust
// BAD
#[test]
fn test_parse() { ... }

// GOOD
#[test]
fn parse_rejects_invalid_utf8() { ... }

#[test]
fn parse_handles_empty_input() { ... }

#[test]
fn parse_extracts_all_rules_from_valid_policy() { ... }
```

### Coverage Targets

- **Line coverage**: >80%
- **Branch coverage**: >70%
- **All error paths tested**

---

## Concurrency

### Prefer Message Passing

```rust
// Use channels over shared state
use tokio::sync::mpsc;

let (tx, mut rx) = mpsc::channel(100);

// Producer
tx.send(item).await?;

// Consumer
while let Some(item) = rx.recv().await {
    process(item);
}
```

### When Sharing State

```rust
// Use Arc<RwLock<T>> for read-heavy workloads
// Use Arc<Mutex<T>> for write-heavy workloads
// Document WHY sharing is necessary

/// Shared cache for compiled policies.
///
/// We use RwLock because policies are read frequently but updated rarely.
type PolicyCache = Arc<RwLock<HashMap<PolicyId, CompiledPolicy>>>;
```

---

## Performance

### Measure Before Optimizing

```rust
// Add benchmarks for hot paths
#[bench]
fn bench_compile_large_policy(b: &mut Bencher) {
    let policy = load_fixture("large.toon");
    b.iter(|| compile(&policy));
}
```

### Avoid Premature Allocation

```rust
// BAD: Allocates even if not needed
fn process(items: &[Item]) -> Vec<Result> {
    items.iter().map(process_one).collect()
}

// GOOD: Returns iterator, caller decides when to collect
fn process(items: &[Item]) -> impl Iterator<Item = Result> + '_ {
    items.iter().map(process_one)
}
```

---

## Clippy Configuration

Every crate's `lib.rs` or `main.rs` starts with:

```rust
#![deny(clippy::all)]
#![deny(clippy::pedantic)]
#![deny(clippy::nursery)]
#![allow(clippy::module_name_repetitions)]  // We use clear module prefixes
#![allow(clippy::must_use_candidate)]       // Not everything needs #[must_use]
```

### Clippy Must Pass

```bash
cargo clippy -- -D warnings
```

No warnings allowed. Fix them or explicitly allow with justification:

```rust
#[allow(clippy::too_many_arguments)]  // Builder pattern not suitable here because X
fn complex_function(...) { ... }
```

---

## Git Hygiene

### Commit Messages

```
<type>(<scope>): <subject>

<body>

<footer>
```

Types: `feat`, `fix`, `refactor`, `test`, `docs`, `chore`

Examples:
- `feat(compiler): add budget overflow detection`
- `fix(prover): handle empty trace corpus`
- `refactor(toon_policy): extract parser into submodule`

### Pre-Commit Checks

Before every commit:
1. `cargo fmt --check`
2. `cargo clippy -- -D warnings`
3. `cargo test`

---

## Dependencies

### Before Adding a Dependency

Ask:
1. Is it actively maintained?
2. Does it have acceptable security history?
3. Is the API stable?
4. Can we vendor it if needed?
5. What's the compile time impact?

### Blessed List

Only these crates are pre-approved:
- `tokio`, `reqwest`, `hyper`
- `serde`, `serde_json`, `serde_yaml`
- `clap`, `tracing`, `tracing-subscriber`
- `thiserror`, `anyhow`
- `proptest`, `insta`, `criterion`
- `schemars`

Anything else requires explicit justification in the commit message.

---

## Summary Checklist

Before marking code complete:

- [ ] All public items have doc comments
- [ ] All error cases return proper `Error` variants
- [ ] No `.unwrap()` or `.expect()` in library code
- [ ] Functions are <50 lines
- [ ] Tests cover success, failure, and edge cases
- [ ] `cargo clippy -- -D warnings` passes
- [ ] `cargo fmt --check` passes
- [ ] `cargo test` passes
