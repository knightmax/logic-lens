# LogicLens JSON Output Schema

The `logic-lens audit --format json` command produces a structured JSON document with the following schema.

## Top-Level Fields

| Field | Type | Description |
|-------|------|-------------|
| `old_file` | string | Path to the original file |
| `new_file` | string | Path to the modified file |
| `language` | string | Detected language (TypeScript, JavaScript, Python, Rust, Java) |
| `entities` | object | Entity summary for both files |
| `changes` | object | Semantic diff results |
| `findings` | array | Lint and analysis findings |
| `metadata` | object | Timing and performance data |

## `entities` Object

| Field | Type | Description |
|-------|------|-------------|
| `old_count` | integer | Number of entities in old file |
| `new_count` | integer | Number of entities in new file |
| `old_entities` | array | Entity details from old file |
| `new_entities` | array | Entity details from new file |

### Entity Object

| Field | Type | Description |
|-------|------|-------------|
| `name` | string | Entity name (function/class/method name) |
| `kind` | string | Entity type: Function, Method, Class, Struct, Interface, Enum |
| `start_line` | integer | Start line number (1-based) |
| `end_line` | integer | End line number (1-based) |
| `is_public` | boolean | Whether the entity is exported/public |

## `changes` Object

| Field | Type | Description |
|-------|------|-------------|
| `total` | integer | Total number of changes |
| `added` | integer | New entities count |
| `removed` | integer | Deleted entities count |
| `modified` | integer | Modified entities count |
| `renamed` | integer | Renamed entities count |
| `by_classification` | object | Counts per classification |
| `details` | array | Individual change details |

### Classification Counts

| Field | Type | Description |
|-------|------|-------------|
| `cosmetic` | integer | Whitespace/comment-only changes |
| `refactor` | integer | Structural changes preserving behavior |
| `logic` | integer | Changes to control flow or business logic |
| `api` | integer | Changes to public interface/signature |
| `side_effect` | integer | New I/O, network, or filesystem operations |

### Change Detail

| Field | Type | Description |
|-------|------|-------------|
| `entity_name` | string | Name of the changed entity |
| `entity_kind` | string | Kind of entity |
| `change_type` | string | Added, Removed, Modified, or Renamed |
| `classification` | string | Cosmetic, Refactor, Logic, Api, or SideEffect |

## `findings` Array

Each finding object:

| Field | Type | Description |
|-------|------|-------------|
| `rule` | string | Rule identifier (e.g., `placeholder-detection`) |
| `severity` | string | `error` or `warning` |
| `message` | string | Human-readable description |
| `file` | string | File path |
| `line` | integer | Line number (1-based) |
| `column` | integer | Column number (1-based) |

### Built-in Rules

| Rule | Description |
|------|-------------|
| `placeholder-detection` | AI placeholder comments (TODO: implement, // ..., etc.) |
| `missing-error-handling` | Unhandled await, empty catch blocks, bare except |
| `empty-implementation` | Empty function bodies, pass-only, throw-only stubs |
| `hallucinated-import` | Imports not found in project manifest dependencies |

## `metadata` Object

| Field | Type | Description |
|-------|------|-------------|
| `parse_duration_ms` | float | Time to parse both files (ms) |
| `diff_duration_ms` | float | Time to compute semantic diff (ms) |
| `analyze_duration_ms` | float | Time for lint + hallucination analysis (ms) |
| `total_duration_ms` | float | Total audit duration (ms) |
