# Custom Rules Guide

LogicLens supports declarative YAML rules that run alongside built-in lenses. Place rule files in `.logic-lens/rules/` (or a custom path via `rules_dir` in config).

## Rule File Format

```yaml
name: no-console-log
description: Disallow console.log in production code
language: [typescript, javascript]
severity: warning
message: "console.log should not be used in production code"
priority: 50
pattern:
  type: contains
  value: "console.log"
```

## Fields

| Field | Required | Description |
|-------|----------|-------------|
| `name` | Yes | Unique rule identifier |
| `description` | No | Human-readable description |
| `language` | No | Languages to apply to (empty = all). Values: typescript, javascript, python, rust, java |
| `severity` | Yes | `error` or `warning` |
| `message` | Yes | Message shown when the rule matches |
| `priority` | No | Execution order (lower = earlier, default: 100) |
| `pattern` | Yes | Match pattern (see below) |

## Pattern Types

### `contains` — Simple text match

Matches lines containing the literal text.

```yaml
pattern:
  type: contains
  value: "console.log"
```

### `regex` — Regular expression match

Matches lines against a regex pattern.

```yaml
pattern:
  type: regex
  value: '\b\d{3,}\b'
```

### `node_type` — Entity-level match

Matches entities by kind, optionally checking body content.

```yaml
pattern:
  type: node_type
  node_type: function
  contains: "eval("
```

Valid `node_type` values: function, method, class, struct, interface, enum.

## Examples

### Detect `eval()` usage

```yaml
name: no-eval
language: [typescript, javascript]
severity: error
message: "eval() is a security risk — avoid dynamic code execution"
pattern:
  type: contains
  value: "eval("
```

### Detect hardcoded secrets

```yaml
name: no-hardcoded-secrets
severity: error
message: "Possible hardcoded secret detected"
pattern:
  type: regex
  value: '(password|secret|api_key|token)\s*=\s*["\x27][^"\x27]{8,}'
```

### Detect functions over 50 lines

This is not directly supported in v1. Use the regex pattern to detect very long function bodies or wait for tree-sitter query support in a future version.

## Rule Discovery

Rules are loaded from:
1. `.logic-lens/rules/*.yaml` (default)
2. Custom path via `rules_dir` in `logic-lens.toml`

```toml
# logic-lens.toml
rules_dir = "audit/rules"
```

## Execution Order

1. Built-in rules (placeholder-detection, missing-error-handling, empty-implementation)
2. User-defined rules, sorted by:
   - `priority` (ascending, default 100)
   - Then alphabetical by `name`
