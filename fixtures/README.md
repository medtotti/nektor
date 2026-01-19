# Nectar Test Fixtures

This directory contains test fixtures for developing and testing Nectar.

## Corpus Fixtures

Located in `corpus/`, these JSON files represent sample trace data for testing the prover and simulation engine.

### Files

| File | Description | Use Case |
|------|-------------|----------|
| `happy_path.json` | Normal successful traces | Baseline testing |
| `errors.json` | Traces with errors (5xx, exceptions) | Error handling rules |
| `high_cardinality.json` | Traces with unique IDs per request | Cardinality safety checks |
| `slow_requests.json` | Long-running traces (>1s) | Performance-based sampling |
| `mixed.json` | Realistic production mix | End-to-end testing |

### Trace Schema

Each trace object contains:

```json
{
  "trace_id": "string (required)",
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

Additional fields may be present for specific test scenarios (e.g., `user.id`, `cache.hit`).

### Loading Fixtures

```rust
use nectar_corpus::Corpus;

let corpus = Corpus::load_from_directory("fixtures/corpus")?;

// Or load a specific file
let corpus = Corpus::load_from_file("fixtures/corpus/errors.json")?;
```

## Policy Fixtures

Located in `policies/` (when created), these contain sample TOON policies for testing.

### Planned Files

- `minimal.toon` — Smallest valid policy (fallback only)
- `production.toon` — Full-featured production policy
- `aggressive.toon` — High sampling reduction policy
- `debug.toon` — Keep everything policy

## Adding New Fixtures

1. Follow the existing JSON schema
2. Include realistic field values
3. Document the purpose in this README
4. Add corresponding test cases
