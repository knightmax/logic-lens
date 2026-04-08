# LogicLens

A semantic auditor for AI-generated code. LogicLens uses tree-sitter AST diffing to detect logic changes, placeholder code, hallucinated imports, and stub implementations — then exposes structured findings via CLI, MCP server, and Copilot Agent Plugin.

## Features

- **Semantic Diffing** — AST-level entity matching with structural hashing, classifying changes as Cosmetic / Refactor / Logic / API / Side-effect
- **AI Linter** — Built-in lenses for LLM-specific anti-patterns: placeholder comments (`// TODO`, `// ...`), missing error handling, empty/stub implementations
- **Hallucination Detection** — Cross-references imports against project manifests (`package.json`, `Cargo.toml`, `pyproject.toml`, `pom.xml`)
- **Rules Engine** — Declarative YAML rules with `contains`, `regex`, and `node_type` patterns
- **Shell Integration** — Optional local build verification (`npm run check`, `cargo build`, `mvn compile`, etc.)
- **MCP Server** — JSON-RPC over stdio for AI agent integration (Claude Code, Copilot, Cursor)
- **Agent Plugin** — Ready-to-install VS Code Copilot plugin with SKILL.md orchestration

### Supported Languages

TypeScript, JavaScript, Python, Rust, Java

## Installation

### From source

```sh
cargo install --path crates/logic-lens-cli
cargo install --path crates/logic-lens-mcp
```

### From releases

Download pre-built binaries from [GitHub Releases](https://github.com/knightmax/logic-lens/releases).

## Usage

### CLI

```sh
# Audit a file pair (old → new)
logic-lens audit --old src/old.ts --new src/new.ts

# JSON output (default)
logic-lens audit --old old.py --new new.py --format json

# Terminal output
logic-lens audit --old old.rs --new new.rs --format terminal

# Markdown output
logic-lens audit --old old.java --new new.java --format markdown

# With build verification
logic-lens audit --old old.ts --new new.ts --verify

# Custom rules directory
logic-lens audit --old old.ts --new new.ts --rules-dir ./my-rules

# Quiet / verbose
logic-lens audit --old old.ts --new new.ts --format terminal --quiet
logic-lens audit --old old.ts --new new.ts --format terminal --verbose
```

### Exit Codes

| Code | Meaning |
|------|---------|
| 0 | Clean — no error-level findings |
| 1 | Errors detected |

### MCP Server

```sh
# Start the MCP server (JSON-RPC over stdio)
logic-lens-mcp
```

Available tools:
- `ll_audit` — Full semantic audit of a file pair
- `ll_findings` — Findings only (filtered by severity)
- `ll_verify` — Run build verification

### Copilot Agent Plugin

Install as a VS Code Copilot plugin from source:

1. Open VS Code Command Palette
2. Run `Chat: Install Plugin From Source`
3. Point to this repository

## Configuration

Create `.logic-lens.toml` in your project root:

```toml
[rules]
placeholder-detection = "error"    # off | warning | error
missing-error-handling = "warning"
empty-implementation = "error"

[output]
format = "json"  # json | terminal | markdown

[verify]
enabled = false
timeout_secs = 30

rules_dir = ".logic-lens/rules"
```

### Custom YAML Rules

```yaml
name: no-console-log
description: "Disallow console.log in production code"
language: typescript
severity: warning
message: "console.log found in new code"
pattern:
  type: contains
  value: "console.log"
priority: 10
```

Place `.yaml` / `.yml` files in your rules directory (default: `.logic-lens/rules/`).

## Architecture

```
┌─────────────┐    ┌─────────────┐    ┌─────────────┐    ┌─────────────┐
│   Parse     │───▶│    Diff      │───▶│   Analyze   │───▶│   Output    │
│ tree-sitter │    │  2-phase     │    │ lint+hallu  │    │ json/term/md│
│  + extract  │    │  matching    │    │  +rules     │    │  + verify   │
└─────────────┘    └─────────────┘    └─────────────┘    └─────────────┘
```

**Crates:**
- `logic-lens-core` — Library: parsing, diffing, linting, hallucination detection, rules engine, output rendering
- `logic-lens-cli` — Binary: `logic-lens` CLI with full audit pipeline
- `logic-lens-mcp` — Binary: `logic-lens-mcp` MCP server (JSON-RPC/stdio)

## Development

```sh
# Run all tests
cargo test --all

# Format
cargo fmt --all

# Lint
cargo clippy --all-targets --all-features
```

## License

[MIT](LICENSE)
