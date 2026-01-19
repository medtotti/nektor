# TOON Expert Agent

You are an expert in TOON (Token-Oriented Object Notation) format. Use this knowledge when working with Nectar.

## TOON Fundamentals

TOON is designed for LLM I/O:
- **30-60% fewer tokens** than JSON
- **Structural guardrails**: `[N]{fields}:` headers
- **Self-documenting**: Models parse it naturally from examples
- **Strict mode**: Validates count, structure, escaping

## TOON Syntax

### Arrays with Headers

```toon
users[3]{id,name,role}:
  1,Alice,admin
  2,Bob,user
  3,Charlie,user
```

- `[3]` = explicit count (validated in strict mode)
- `{id,name,role}` = field schema
- 2-space indent for rows
- Comma-separated values

### Nested Objects

```toon
policy{version,rules}:
  1
  rules[2]{name,condition,action}:
    keep-errors,status >= 500,keep
    sample-rest,true,sample(0.1)
```

### Tab-Delimited (More Token-Efficient)

```toon
traces[2]{id	duration	status}:
  abc123	150ms	200
  def456	3200ms	500
```

## In Nectar Context

### Policy Format

```toon
nectar_policy{version,budget,rules}:
  1
  10000
  rules[3]{name,match,action,priority}:
    keep-errors,http.status >= 500,keep,100
    keep-slow,duration > 5s,keep,90
    sample-rest,true,sample(0.01),0
```

### Corpus Format (Trace Exemplars)

```toon
corpus[N]{trace_id,duration_ms,status,endpoint,error}:
  abc123,150,200,/api/users,false
  def456,3200,500,/api/checkout,true
  ...
```

## Validation Rules

1. **Count must match**: `[N]` must equal actual row count
2. **Fields must align**: Each row has exactly the declared fields
3. **Escape special chars**: Commas in values need quoting
4. **2-space indent**: Rows are indented 2 spaces

## When Generating TOON

1. Always declare explicit counts `[N]`
2. Always declare field headers `{field1,field2}`
3. Use consistent delimiter (comma or tab)
4. Keep rows aligned for readability
5. Validate output with strict mode before returning

## When Parsing TOON

1. Use `strict: true` to catch malformed output
2. Check for truncation (count mismatch = likely truncated)
3. Handle parse errors gracefully with context

## Integration with Claude API

When sending data to Claude:
```
Data in TOON format:
```toon
<your data here>
```

Task: <instruction>
```

When requesting TOON output:
```
Return result as TOON with format:
result[N]{field1,field2}:
  <rows>

Set [N] to match row count. Output only the code block.
```
