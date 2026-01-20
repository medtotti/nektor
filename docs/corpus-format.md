# Corpus Format Specification

The corpus is Nectar's trace exemplar storage—a collection of representative traces used for policy simulation, validation, and Claude-powered recommendations.

## Overview

### What is the Corpus?

The corpus stores trace data that the prover uses to:

- **Simulate** sampling decisions before deploying rules
- **Validate** that must-keep traces won't be dropped
- **Verify** budget compliance under real traffic patterns
- **Train** Claude to understand your system's trace characteristics

### Why It Matters

Without representative trace data, you're flying blind:

- Rules that look correct might drop critical incidents
- Budget estimates are guesses, not simulations
- Claude can't give meaningful recommendations

The corpus bridges the gap between "this rule looks right" and "this rule works on real data."

## Data Model

### Trace (Summary Level)

Each trace in the corpus has these fields:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `trace_id` | string | Yes | Unique trace identifier |
| `duration` | Duration | No | Total trace duration |
| `status` | u16 | No | HTTP status code (root span) |
| `service` | string | No | Service name (root span) |
| `endpoint` | string | No | HTTP route or operation name |
| `is_error` | bool | No | Whether any span errored |
| `attributes` | map | No | Additional key-value pairs |
| `spans` | Span[] | No | Individual spans (if available) |
| `span_count` | usize | No | Number of spans in trace |

### Span (Detail Level)

When full span data is ingested, each span has:

| Field | Type | Required | Description |
|-------|------|----------|-------------|
| `span_id` | string | Yes | Unique span identifier |
| `parent_span_id` | string | No | Parent span (none = root) |
| `name` | string | Yes | Operation name |
| `service` | string | Yes | Service that emitted span |
| `duration` | Duration | Yes | Span duration |
| `start_time_ns` | u64 | Yes | Start time (Unix nanos) |
| `kind` | SpanKind | No | Server, Client, Internal, etc. |
| `status` | SpanStatus | No | Ok, Error, or Unset |
| `attributes` | map | No | Span attributes |

### Span Kind

```
Unspecified | Internal | Server | Client | Producer | Consumer
```

### Status Code

```
Unset | Ok | Error
```

### Attribute Values

Attributes support these types:

```
String | Int (i64) | Float (f64) | Bool | StringArray
```

## Input Formats

Nectar auto-detects the input format from content. Three formats are supported:

### 1. Plain JSON

The simplest format—a JSON array of trace objects.

```json
[
  {
    "trace_id": "abc123",
    "duration_ms": 150,
    "status": 200,
    "service": "api-gateway",
    "endpoint": "GET /users",
    "is_error": false
  },
  {
    "trace_id": "def456",
    "duration_ms": 2500,
    "status": 500,
    "service": "checkout",
    "endpoint": "POST /orders",
    "is_error": true
  }
]
```

Or wrapped in an object:

```json
{
  "traces": [
    {"trace_id": "abc123", "duration_ms": 150, "status": 200}
  ]
}
```

**Field Mappings:**

| JSON Field | Alternative Names | Maps To |
|------------|-------------------|---------|
| `trace_id` | — | `trace_id` |
| `duration_ms` | `duration` | `duration` |
| `status` | `http.status_code` | `status` |
| `service` | `service.name` | `service` |
| `endpoint` | `http.route` | `endpoint` |
| `is_error` | `error` | `is_error` |

**Detection:** First non-whitespace byte is `[` or `{`, content-type contains `application/json`.

### 2. Honeycomb NDJSON

Newline-delimited JSON with Honeycomb trace fields. Each line is a span.

```json
{"trace.trace_id":"t1","trace.span_id":"s1","name":"GET /api","service.name":"api","duration_ms":100}
{"trace.trace_id":"t1","trace.span_id":"s2","trace.parent_id":"s1","name":"db.query","service.name":"api","duration_ms":45}
{"trace.trace_id":"t2","trace.span_id":"s3","name":"POST /login","service.name":"auth","duration_ms":200,"error":true}
```

**Field Mappings:**

| Honeycomb Field | Alternative Names | Maps To |
|-----------------|-------------------|---------|
| `trace.trace_id` | `trace_id` | Trace grouping key |
| `trace.span_id` | `span_id` | `span_id` |
| `trace.parent_id` | `parent_id` | `parent_span_id` |
| `service.name` | `service_name`, `service` | `service` |
| `name` | `operation`, `span.name` | `name` |
| `duration_ms` | `duration` | `duration` |
| `timestamp_ms` | `start_time_ms`, `time` | `start_time_ns` |
| `error` | `is_error` | Error detection |
| `http.status_code` | `status_code` | `status` (>=500 = error) |
| `status.code` | — | `status.code` |
| `span.kind` | `kind` | `kind` |

**Span Reconstruction:**

- Spans are grouped by `trace.trace_id`
- Parent-child relationships built from `trace.parent_id`
- Root span (no parent) provides trace-level summary
- `is_error` is true if any span has error status or HTTP 5xx

**Detection:** First line is JSON object containing `trace.trace_id` or `trace.span_id`, or content-type is `application/x-ndjson`.

### 3. OTLP Protobuf

OpenTelemetry Protocol trace data in protobuf format.

```
ExportTraceServiceRequest (protobuf binary)
├── resource_spans[]
│   ├── resource
│   │   └── attributes[] (includes service.name)
│   └── scope_spans[]
│       └── spans[]
│           ├── trace_id (16 bytes → hex string)
│           ├── span_id (8 bytes → hex string)
│           ├── parent_span_id (8 bytes → hex string)
│           ├── name
│           ├── kind (1-5 → SpanKind enum)
│           ├── start_time_unix_nano
│           ├── end_time_unix_nano
│           ├── status {code, message}
│           └── attributes[]
```

**Requirements:**

Enable the `otlp` feature:

```toml
[dependencies]
nectar_corpus = { version = "0.1", features = ["otlp"] }
```

**Detection:** Content-type is `application/x-protobuf`, `application/protobuf`, or `application/grpc`, or first byte is `0x0A` (protobuf field marker).

## Storage Format

Internally, traces are stored as Rust structs serializable with serde:

```rust
pub struct Trace {
    pub trace_id: String,
    pub duration: Duration,
    pub status: Option<u16>,
    pub service: Option<String>,
    pub endpoint: Option<String>,
    pub is_error: bool,
    pub attributes: HashMap<String, String>,
    pub spans: Vec<Span>,
    pub span_count: usize,
}

pub struct Span {
    pub span_id: String,
    pub parent_span_id: Option<String>,
    pub name: String,
    pub service: String,
    pub duration: Duration,
    pub start_time_ns: u64,
    pub kind: SpanKind,
    pub status: SpanStatus,
    pub attributes: HashMap<String, AttributeValue>,
}
```

### Summary Computation

When traces are created from spans, summary fields are computed automatically:

- **duration**: Earliest span start to latest span end
- **service**: Root span's service name
- **endpoint**: Root span's `http.route` attribute, or span name
- **status**: Root span's `http.status_code` attribute
- **is_error**: True if any span has error status OR HTTP status >= 500

## API Usage

### Ingesting Data

```rust
use nectar_corpus::Corpus;

// Auto-detect format from bytes
let corpus = Corpus::ingest(&data)?;

// With content-type hint
let corpus = Corpus::ingest_with_content_type(&data, Some("application/json"))?;

// From a file (auto-detect)
let corpus = Corpus::ingest_file("traces.json")?;

// Existing JSON loader (still works)
let corpus = Corpus::load_file("traces.json")?;
```

### Using the Registry Directly

```rust
use nectar_corpus::IngestorRegistry;

let registry = IngestorRegistry::new();

// Check available formats
println!("{:?}", registry.formats()); // ["otlp", "honeycomb", "json"]

// Ingest with explicit format
let traces = registry.ingest_as(&data, "honeycomb")?;
```

### Building Traces Programmatically

```rust
use nectar_corpus::{Trace, Span, SpanKind, SpanStatus};
use std::time::Duration;

// From spans
let spans = vec![
    Span::new("span-1", "GET /users")
        .with_service("api")
        .with_duration(Duration::from_millis(100))
        .with_kind(SpanKind::Server)
        .with_attribute("http.status_code", 200i64),
    Span::new("span-2", "db.query")
        .with_parent("span-1")
        .with_service("api")
        .with_duration(Duration::from_millis(45)),
];

let trace = Trace::from_spans("trace-001", spans);
// trace.service == Some("api")
// trace.endpoint == Some("GET /users")
// trace.is_error == false

// Or build summary directly
let trace = Trace::new("trace-002")
    .with_duration(Duration::from_millis(150))
    .with_status(500)
    .with_service("payments")
    .with_endpoint("POST /charge");
// trace.is_error == true (status >= 500)
```

## CLI Examples

### Ingest and Encode to TOON

```bash
# From JSON file
nectar corpus ingest traces.json --output corpus.toon

# From Honeycomb export
nectar corpus ingest honeycomb-export.ndjson --output corpus.toon

# From OTLP (requires --features otlp)
nectar corpus ingest traces.pb --format otlp --output corpus.toon

# Pipe from stdin
cat traces.json | nectar corpus ingest - --output corpus.toon
```

### Validate Corpus

```bash
# Check corpus is valid
nectar corpus validate corpus.toon

# Show summary statistics
nectar corpus stats corpus.toon
```

### Filter Traces

```bash
# Only errors
nectar corpus filter corpus.toon --errors-only --output errors.toon

# By service
nectar corpus filter corpus.toon --service checkout --output checkout.toon
```

## Troubleshooting

### Common Errors

#### "unknown format"

**Cause:** No ingestor recognized the input format.

**Fix:**
- Check the file is valid JSON, NDJSON, or protobuf
- Use `--format` flag to specify explicitly
- Ensure file isn't empty or corrupted

#### "missing trace_id"

**Cause:** JSON/NDJSON line lacks a trace identifier.

**Fix:** Ensure every trace/span has `trace_id` or `trace.trace_id` field.

#### "parse error (honeycomb): invalid JSON"

**Cause:** NDJSON file has malformed lines.

**Fix:**
- Check each line is valid JSON
- Remove empty lines or comments
- Validate with `jq -c . < file.ndjson`

#### "protobuf decode error"

**Cause:** File is not valid OTLP protobuf or wrong message type.

**Fix:**
- Ensure file is `ExportTraceServiceRequest` message
- Check protobuf version compatibility
- Verify the `otlp` feature is enabled

### Format Detection Issues

If auto-detection picks the wrong format:

```bash
# Force specific format
nectar corpus ingest data.bin --format otlp
nectar corpus ingest export.txt --format honeycomb
nectar corpus ingest traces.txt --format json
```

### Missing Span Data

If traces have no spans (only summary):

- This is valid—summary-only traces work for policy simulation
- Span data provides more detail for analysis
- Use `--require-spans` flag if spans are mandatory

### Empty Corpus

If ingestion produces zero traces:

1. Check input file isn't empty
2. Verify format is correct
3. Look for warnings about skipped invalid traces
4. Use `--verbose` flag to see parsing details

## Format Conversion

### Honeycomb → JSON

```bash
# Convert NDJSON spans to JSON trace summaries
nectar corpus convert honeycomb-export.ndjson --to json --output traces.json
```

### JSON → TOON

```bash
# Encode for Claude consumption
nectar corpus encode traces.json --output corpus.toon
```

### Export Spans

```bash
# Export with full span detail
nectar corpus export corpus.toon --include-spans --output full-traces.json
```

## Best Practices

1. **Keep corpus representative** — Include errors, slow traces, and edge cases
2. **Update regularly** — Stale corpus leads to stale rules
3. **Include metadata** — Service names and endpoints enable smarter rules
4. **Size appropriately** — Enough diversity, not too large for simulation
5. **Version control** — Corpus changes affect rule validation

## See Also

- [Vision Document](vision.md) — Project overview and architecture
- [TOON Format](https://github.com/toon-format/toon-rust) — Wire format specification
- [Honeycomb Docs](https://docs.honeycomb.io/) — Honeycomb query exports
- [OTLP Spec](https://opentelemetry.io/docs/specs/otlp/) — OpenTelemetry Protocol
