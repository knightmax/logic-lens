---
description: "Semantic auditor for AI-generated code — detects logic changes, placeholder code, hallucinated imports, and stub implementations by comparing old vs new file versions."
---

# LogicLens Audit Skill

## When to use this skill

Use this skill when:
- Reviewing AI-generated code changes (Copilot, Claude, ChatGPT suggestions)
- Comparing an original file with an AI-modified version
- Checking for placeholder comments, empty implementations, or hallucinated imports
- Classifying whether changes are cosmetic, refactoring, logic, API, or side-effect changes
- Assessing risk of AI-suggested code modifications

## Preflight Check

Before running the audit, verify the binary is available:

```bash
logic-lens --version
```

If not found, instruct the user:
> LogicLens is not installed. Install it with: `cargo install --path crates/logic-lens-cli` from the logic-lens repository.

## Workflow

### Step 1: Run the audit

```bash
logic-lens audit <old_file> <new_file> --format json
```

Replace `<old_file>` with the path to the original file and `<new_file>` with the AI-modified version.

### Step 2: Parse the JSON output

The command outputs structured JSON containing:
- **entities**: Functions, methods, classes extracted from both files
- **changes**: Semantic diff with classification (Cosmetic/Refactor/Logic/Api/SideEffect)
- **findings**: Lint warnings, hallucination alerts, rule violations
- **metadata**: Timing information

### Step 3: Render results for the user

#### For Logic/API/SideEffect changes:
Provide a contextual explanation of what changed and why it matters. Example:
> The AI modified the validation logic in `processOrder` — this changes the error handling contract for callers. The original checked for null before processing; the new version skips this check.

#### For findings:
List each finding with severity and actionable advice. Example:
> ⚠️ **placeholder-detection** `app.ts:15` — AI placeholder comment detected: `// TODO: implement`. This needs to be replaced with actual implementation.

#### For risk assessment:
Summarize the overall risk based on the number and severity of findings and change classifications:
- **High**: Error-severity findings or 3+ logic/side-effect changes
- **Medium**: 1+ logic/side-effect changes or 3+ findings
- **Low**: Only cosmetic/refactor changes with no findings

#### For clean audits:
> ✅ Clean audit — all changes are cosmetic/refactoring with no findings detected. The AI modifications appear safe.

## Output Schema Reference

See `references/output-schema.md` for the complete JSON schema documentation.

## Custom Rules

See `references/rules-guide.md` for creating custom YAML audit rules.

## Notes

- This skill operates via direct CLI invocation — no MCP server required
- Supports TypeScript, JavaScript, Python, Rust, and Java
- Add `--verify` flag to also run local build verification
- Add `--format terminal` for human-readable output instead of JSON
