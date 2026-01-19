# Nectar Tasks — Road to Done Done

## Phase 1: Core Parsing & Models

### 1.1 TOON Parser Implementation
- [ ] Implement full TOON lexer in `toon_policy/src/parser.rs`
- [ ] Parse headers with count validation (strict mode)
- [ ] Parse nested objects and arrays
- [ ] Parse policy schema: `name`, `version`, `rules[]`
- [ ] Parse rule schema: `name`, `match`, `action`, `priority`
- [ ] Parse match expressions: field operators (`=`, `!=`, `~`, `contains`, `exists`)
- [ ] Parse compound matches: `AND`, `OR`, `NOT`
- [ ] Add span tracking for error messages
- [ ] Property tests: roundtrip `parse(serialize(policy)) == policy`

### 1.2 Match Expression Engine
- [ ] Define `MatchExpr` AST in `toon_policy/src/model.rs`
- [ ] Implement `MatchExpr::evaluate(&self, trace: &Trace) -> bool`
- [ ] Support field access: `service.name`, `http.status_code`
- [ ] Support wildcards: `service.name ~ "api-*"`
- [ ] Support numeric comparisons: `duration_ms > 1000`
- [ ] Unit tests for each operator
- [ ] Property tests for evaluation consistency

---

## Phase 2: Corpus Management

### 2.1 Trace Corpus Loading
- [ ] Implement `Corpus::load_from_directory(path)` in `nectar_corpus`
- [ ] Support JSON trace format (Honeycomb export)
- [ ] Support JSONL streaming format
- [ ] Support TOON trace format (native)
- [ ] Add deduplication by trace ID
- [ ] Add sampling for large corpora (configurable max)

### 2.2 Corpus Fixtures
- [ ] Create `fixtures/corpus/` directory
- [ ] Add `happy_path.json` — normal successful traces
- [ ] Add `errors.json` — traces with `error=true`, status 5xx
- [ ] Add `high_cardinality.json` — traces with unique user IDs
- [ ] Add `slow_requests.json` — traces with high `duration_ms`
- [ ] Add `mixed.json` — realistic production mix
- [ ] Document fixture format in `fixtures/README.md`

### 2.3 Corpus Analytics
- [ ] Implement `Corpus::field_cardinality(field) -> usize`
- [ ] Implement `Corpus::field_distribution(field) -> HashMap<String, usize>`
- [ ] Implement `Corpus::error_rate() -> f64`
- [ ] Implement `Corpus::summary() -> CorpusSummary`

---

## Phase 3: Prover Implementation

### 3.1 Core Verification Checks
- [ ] `check_fallback_rule` — policy must have `*` fallback
- [ ] `check_error_coverage` — errors must not be dropped
- [ ] `check_cardinality_safety` — warn on keep rules for high-cardinality fields
- [ ] `check_budget_compliance` — estimated sample rate vs target
- [ ] `check_rule_overlap` — warn on redundant/shadowed rules
- [ ] `check_priority_gaps` — warn on non-contiguous priorities

### 3.2 Simulation Engine
- [ ] Implement `Prover::simulate(policy, corpus) -> SimulationResult`
- [ ] Calculate per-rule hit counts
- [ ] Calculate effective sample rate
- [ ] Calculate estimated cost (spans kept / total)
- [ ] Generate coverage report: which traces hit which rules
- [ ] Shadow comparison: diff two policies on same corpus

### 3.3 Must-Keep Validation
- [ ] Define `must_keep` annotations in policy schema
- [ ] Verify must-keep traces are never dropped
- [ ] Verify must-keep traces hit expected rules
- [ ] Report must-keep violations with trace IDs

---

## Phase 4: Compiler Implementation

### 4.1 Refinery Output
- [ ] Complete `compile_rule` in `nectar_compiler/src/compiler.rs`
- [ ] Convert `MatchExpr` to Refinery conditions
- [ ] Handle `AND`/`OR` compound conditions
- [ ] Handle field existence checks
- [ ] Handle regex patterns
- [ ] Generate deterministic output (sorted, canonical)
- [ ] Add content hash to output for cache invalidation

### 4.2 Waggle Report
- [ ] Implement full `generate_waggle_report` in `nectar_compiler/src/waggle.rs`
- [ ] Human-readable rule explanations
- [ ] Estimated impact per rule
- [ ] Budget analysis section
- [ ] Diff mode: explain changes between two policies

### 4.3 Output Formats
- [ ] YAML output (Refinery native)
- [ ] JSON output (API consumption)
- [ ] TOML output (alternative config)
- [ ] Dry-run mode (validate without writing)

---

## Phase 5: Claude Integration

### 5.1 API Client
- [ ] Complete `Client::generate_policy` in `nectar_claude/src/client.rs`
- [ ] Implement retry with exponential backoff
- [ ] Handle rate limiting (429 responses)
- [ ] Stream response for progress indication
- [ ] Token usage tracking and logging

### 5.2 Prompt Engineering
- [ ] Finalize system prompt in `nectar_claude/src/prompt.rs`
- [ ] Include TOON format examples in prompt
- [ ] Include policy schema in prompt
- [ ] Include corpus summary in context
- [ ] Include current policy for iteration
- [ ] Add intent parsing for natural language goals

### 5.3 Response Processing
- [ ] Extract TOON from markdown code blocks
- [ ] Validate extracted TOON syntax
- [ ] Parse into typed Policy
- [ ] Handle partial/malformed responses
- [ ] Retry on validation failure with error context

---

## Phase 6: CLI Commands

### 6.1 `nectar init`
- [ ] Create `policy.toon` with sensible defaults
- [ ] Create `corpus/` directory structure
- [ ] Create `.nectar.toml` config file
- [ ] Generate README with quickstart

### 6.2 `nectar compile`
- [ ] Load policy from file or stdin
- [ ] Validate policy structure
- [ ] Compile to Refinery rules
- [ ] Write output to file or stdout
- [ ] Support `--format yaml|json|toml`
- [ ] Support `--dry-run`

### 6.3 `nectar prove`
- [ ] Load policy and corpus
- [ ] Run all verification checks
- [ ] Output structured result (JSON for CI)
- [ ] Human-readable summary
- [ ] Exit code: 0=approved, 1=rejected, 2=warnings
- [ ] Support `--strict` (warnings are errors)

### 6.4 `nectar propose`
- [ ] Load corpus and optional current policy
- [ ] Accept intent from argument or file
- [ ] Call Claude API to generate policy
- [ ] Auto-verify with prover
- [ ] Write to output or stdout
- [ ] Support `--verify` (fail if prover rejects)

### 6.5 `nectar explain`
- [ ] Load policy
- [ ] Generate waggle report
- [ ] Optional: load corpus for impact analysis
- [ ] Support `--diff <other-policy>`

### 6.6 `nectar simulate`
- [ ] Load policy and corpus
- [ ] Run simulation engine
- [ ] Output per-rule statistics
- [ ] Output cost estimate
- [ ] Support `--compare <other-policy>`

---

## Phase 7: Testing & Quality

### 7.1 Unit Tests
- [ ] 80%+ coverage on all crates
- [ ] Property tests for parser roundtrip
- [ ] Property tests for compiler determinism
- [ ] Property tests for prover consistency
- [ ] Snapshot tests for CLI output

### 7.2 Integration Tests
- [ ] End-to-end: `init` → `propose` → `prove` → `compile`
- [ ] Regression tests with fixture corpus
- [ ] Golden file tests for compiler output
- [ ] CLI exit code tests

### 7.3 Documentation
- [ ] Rustdoc for all public APIs
- [ ] `docs/architecture.md` — system design
- [ ] `docs/toon-schema.md` — policy format spec
- [ ] `docs/cli.md` — command reference
- [ ] `docs/integration.md` — CI/CD setup guide

---

## Phase 8: Production Readiness

### 8.1 Error Handling
- [ ] All errors have context (file, line, field)
- [ ] No panics in library code
- [ ] Graceful degradation on partial failures
- [ ] Structured error output for tooling

### 8.2 Observability
- [ ] Tracing spans for all major operations
- [ ] Metrics hooks for API latency, token usage
- [ ] Debug logging behind feature flag
- [ ] Performance benchmarks

### 8.3 Configuration
- [ ] `.nectar.toml` for project config
- [ ] Environment variable overrides
- [ ] XDG-compliant config paths
- [ ] Config validation on load

### 8.4 Release
- [ ] GitHub Actions CI pipeline
- [ ] Release binaries for linux/macos/windows
- [ ] Cargo publish to crates.io
- [ ] Changelog generation
- [ ] Semantic versioning

---

## Quick Reference

| Command | What it does |
|---------|--------------|
| `nectar init` | Bootstrap new project |
| `nectar propose "reduce costs by 50%"` | AI generates policy |
| `nectar prove policy.toon` | Verify safety invariants |
| `nectar compile policy.toon -o rules.yaml` | Generate Refinery config |
| `nectar explain policy.toon` | Human-readable summary |
| `nectar simulate policy.toon --corpus ./corpus` | Test against traces |
