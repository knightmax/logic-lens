use crate::config::{Config, RuleSeverity};
use crate::diff::ChangeSet;
use crate::entity::Entity;
use crate::language::Language;
use serde::Serialize;

/// Severity of a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Severity {
    Error,
    Warning,
}

impl From<RuleSeverity> for Option<Severity> {
    fn from(rs: RuleSeverity) -> Self {
        match rs {
            RuleSeverity::Error => Some(Severity::Error),
            RuleSeverity::Warning => Some(Severity::Warning),
            RuleSeverity::Off => None,
        }
    }
}

/// A single finding produced by an audit lens.
#[derive(Debug, Clone, Serialize)]
pub struct Finding {
    pub rule: String,
    pub severity: Severity,
    pub message: String,
    pub file: String,
    pub line: usize,
    pub column: usize,
}

/// Context provided to each audit lens for evaluation.
pub struct ChangeContext<'a> {
    pub old_source: &'a str,
    pub new_source: &'a str,
    pub old_entities: &'a [Entity],
    pub new_entities: &'a [Entity],
    pub change_set: &'a ChangeSet,
    pub language: Language,
    pub new_file_path: &'a str,
}

/// The trait that all audit lenses (built-in and custom) must implement.
pub trait AuditLens {
    /// The rule name identifier.
    fn name(&self) -> &str;

    /// Evaluate the change context and return findings.
    fn evaluate(&self, ctx: &ChangeContext) -> Vec<Finding>;
}

// --- Built-in Lenses ---

/// Detects placeholder comments left by AI code generators.
pub struct PlaceholderDetectionLens;

impl AuditLens for PlaceholderDetectionLens {
    fn name(&self) -> &str {
        "placeholder-detection"
    }

    fn evaluate(&self, ctx: &ChangeContext) -> Vec<Finding> {
        let mut findings = Vec::new();
        let patterns = [
            "// implement here",
            "// todo: implement",
            "// todo implement",
            "/* implement here */",
            "/* add code */",
            "/* add code here */",
            "// ...",
            "/* ... */",
            "# implement here",
            "# todo: implement",
            "# todo implement",
            "// add implementation",
            "// placeholder",
            "// stub",
            "// fixme: implement",
            "// your code here",
            "# your code here",
            "// todo: add",
            "// fill in",
        ];

        for (line_idx, line) in ctx.new_source.lines().enumerate() {
            let trimmed = line.trim().to_lowercase();
            for pattern in &patterns {
                if trimmed == *pattern || trimmed.starts_with(pattern) {
                    // Skip legitimate TODO comments with substantive description
                    if is_substantive_todo(&trimmed) {
                        continue;
                    }
                    findings.push(Finding {
                        rule: self.name().to_string(),
                        severity: Severity::Warning,
                        message: format!("AI placeholder comment detected: {}", line.trim()),
                        file: ctx.new_file_path.to_string(),
                        line: line_idx + 1,
                        column: 1,
                    });
                    break;
                }
            }
        }
        findings
    }
}

/// Check if a TODO comment has substantive description (not just "implement").
fn is_substantive_todo(trimmed: &str) -> bool {
    if !trimmed.contains("todo") {
        return false;
    }
    // Remove the TODO prefix and check remaining content
    let after_todo = if let Some(pos) = trimmed.find("todo") {
        let rest = &trimmed[pos + 4..];
        // Strip ":" and whitespace
        rest.trim_start_matches(':').trim()
    } else {
        return false;
    };

    // Short or generic descriptions are not substantive
    let generic_words = [
        "implement",
        "add",
        "fix",
        "do",
        "here",
        "this",
        "later",
        "stub",
        "",
    ];
    let words: Vec<&str> = after_todo.split_whitespace().collect();
    if words.is_empty() {
        return false;
    }

    // If more than 3 words, it's likely substantive
    if words.len() > 3 {
        return true;
    }

    // If it's a single generic word, it's not substantive
    !generic_words.contains(&words[0])
}

/// Detects missing error handling in new code.
pub struct MissingErrorHandlingLens;

impl AuditLens for MissingErrorHandlingLens {
    fn name(&self) -> &str {
        "missing-error-handling"
    }

    fn evaluate(&self, ctx: &ChangeContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        for entity in ctx.new_entities {
            match ctx.language {
                Language::TypeScript | Language::JavaScript => {
                    check_js_error_handling(entity, ctx.new_file_path, &mut findings);
                }
                Language::Python => {
                    check_python_error_handling(entity, ctx.new_file_path, &mut findings);
                }
                Language::Java => {
                    check_java_error_handling(entity, ctx.new_file_path, &mut findings);
                }
                Language::Rust => {
                    // Rust's Result type makes this less of an issue;
                    // we check for unwrap() chains
                    check_rust_error_handling(entity, ctx.new_file_path, &mut findings);
                }
            }
        }
        findings
    }
}

fn check_js_error_handling(entity: &Entity, file: &str, findings: &mut Vec<Finding>) {
    let body = &entity.body;
    let has_await = body.contains("await ");
    let has_try = body.contains("try {") || body.contains("try{");
    let has_catch_handler = body.contains(".catch(");

    if has_await && !has_try && !has_catch_handler {
        findings.push(Finding {
            rule: "missing-error-handling".to_string(),
            severity: Severity::Warning,
            message: format!(
                "Function `{}` has unhandled await without try/catch or .catch()",
                entity.name
            ),
            file: file.to_string(),
            line: entity.span.start_line,
            column: entity.span.start_col,
        });
    }

    // Detect bare catch blocks
    if body.contains("catch {}")
        || body.contains("catch (e) {}")
        || body.contains("catch(e){}")
        || body.contains("catch (_) {}")
    {
        findings.push(Finding {
            rule: "missing-error-handling".to_string(),
            severity: Severity::Warning,
            message: format!(
                "Function `{}` has an empty catch block — errors are silently swallowed",
                entity.name
            ),
            file: file.to_string(),
            line: entity.span.start_line,
            column: entity.span.start_col,
        });
    }
}

fn check_python_error_handling(entity: &Entity, file: &str, findings: &mut Vec<Finding>) {
    let body = &entity.body;

    // Detect bare except
    if body.contains("except:") && !body.contains("except Exception") {
        findings.push(Finding {
            rule: "missing-error-handling".to_string(),
            severity: Severity::Warning,
            message: format!(
                "Function `{}` has a bare `except:` clause — too broad, may hide errors",
                entity.name
            ),
            file: file.to_string(),
            line: entity.span.start_line,
            column: entity.span.start_col,
        });
    }

    // Detect except with pass
    if (body.contains("except:") || body.contains("except "))
        && body.contains("pass")
        && !body.contains("logging")
        && !body.contains("logger")
    {
        // Check if the except block only contains pass
        for line in body.lines() {
            let trimmed = line.trim();
            if trimmed == "pass" {
                // Check if previous line is an except
                // Simplified: just flag it
                findings.push(Finding {
                    rule: "missing-error-handling".to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "Function `{}` has an except block that only contains `pass`",
                        entity.name
                    ),
                    file: file.to_string(),
                    line: entity.span.start_line,
                    column: entity.span.start_col,
                });
                break;
            }
        }
    }
}

fn check_java_error_handling(entity: &Entity, file: &str, findings: &mut Vec<Finding>) {
    let body = &entity.body;

    // Detect empty catch blocks
    if body.contains("catch (")
        && (body.contains("catch (Exception e) {}") || body.contains(") {}"))
    {
        findings.push(Finding {
            rule: "missing-error-handling".to_string(),
            severity: Severity::Warning,
            message: format!("Method `{}` has an empty catch block", entity.name),
            file: file.to_string(),
            line: entity.span.start_line,
            column: entity.span.start_col,
        });
    }
}

fn check_rust_error_handling(entity: &Entity, file: &str, findings: &mut Vec<Finding>) {
    let body = &entity.body;

    // Detect excessive unwrap() usage
    let unwrap_count = body.matches(".unwrap()").count();
    if unwrap_count >= 3 {
        findings.push(Finding {
            rule: "missing-error-handling".to_string(),
            severity: Severity::Warning,
            message: format!(
                "Function `{}` has {} .unwrap() calls — consider proper error handling",
                entity.name, unwrap_count
            ),
            file: file.to_string(),
            line: entity.span.start_line,
            column: entity.span.start_col,
        });
    }
}

/// Detects empty or stub function implementations.
pub struct EmptyImplementationLens;

impl AuditLens for EmptyImplementationLens {
    fn name(&self) -> &str {
        "empty-implementation"
    }

    fn evaluate(&self, ctx: &ChangeContext) -> Vec<Finding> {
        let mut findings = Vec::new();

        for entity in ctx.new_entities {
            let body = &entity.body;
            let inner = extract_function_body(body);

            if is_empty_body(inner, ctx.language) {
                findings.push(Finding {
                    rule: self.name().to_string(),
                    severity: Severity::Error,
                    message: format!("{} `{}` has an empty body", entity.kind, entity.name),
                    file: ctx.new_file_path.to_string(),
                    line: entity.span.start_line,
                    column: entity.span.start_col,
                });
            } else if is_throw_only(inner, ctx.language) {
                findings.push(Finding {
                    rule: self.name().to_string(),
                    severity: Severity::Warning,
                    message: format!(
                        "{} `{}` only throws/raises an error — stub implementation",
                        entity.kind, entity.name
                    ),
                    file: ctx.new_file_path.to_string(),
                    line: entity.span.start_line,
                    column: entity.span.start_col,
                });
            }
        }
        findings
    }
}

/// Extract the inner body content (between braces or after colon).
fn extract_function_body(body: &str) -> &str {
    if let Some(start) = body.find('{') {
        let inner = &body[start + 1..];
        if let Some(end) = inner.rfind('}') {
            return inner[..end].trim();
        }
    }
    // Python: after the colon
    if let Some(pos) = body.find(":\n") {
        return body[pos + 2..].trim();
    }
    body.trim()
}

fn is_empty_body(inner: &str, lang: Language) -> bool {
    if inner.is_empty() {
        return true;
    }
    match lang {
        Language::Python => inner == "pass" || inner == "...",
        _ => false,
    }
}

fn is_throw_only(inner: &str, _lang: Language) -> bool {
    let trimmed = inner.trim();
    let throw_patterns = [
        "throw new Error(",
        "throw new NotImplementedError(",
        "throw new UnsupportedOperationException(",
        "raise NotImplementedError(",
        "raise NotImplementedError",
        "todo!()",
        "unimplemented!()",
        "panic!(\"not implemented",
    ];
    throw_patterns
        .iter()
        .any(|p| trimmed.starts_with(p) && trimmed.lines().count() == 1)
}

/// Run all built-in lenses against a change context, applying severity config.
pub fn run_builtin_lenses(ctx: &ChangeContext, config: &Config) -> Vec<Finding> {
    let lenses: Vec<Box<dyn AuditLens>> = vec![
        Box::new(PlaceholderDetectionLens),
        Box::new(MissingErrorHandlingLens),
        Box::new(EmptyImplementationLens),
    ];

    let mut all_findings = Vec::new();
    for lens in &lenses {
        let severity_override = config.rules.get(lens.name());
        match severity_override {
            Some(RuleSeverity::Off) => continue,
            _ => {
                let mut findings = lens.evaluate(ctx);
                // Apply severity override
                if let Some(rs) = severity_override {
                    if let Some(sev) = Option::<Severity>::from(*rs) {
                        for f in &mut findings {
                            f.severity = sev;
                        }
                    }
                }
                all_findings.extend(findings);
            }
        }
    }
    all_findings
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::extract_entities;
    use crate::parser::parse_source;

    fn make_context<'a>(
        old_source: &'a str,
        new_source: &'a str,
        lang: Language,
        old_entities: &'a [Entity],
        new_entities: &'a [Entity],
        change_set: &'a ChangeSet,
    ) -> ChangeContext<'a> {
        ChangeContext {
            old_source,
            new_source,
            old_entities,
            new_entities,
            change_set,
            language: lang,
            new_file_path: "test.ts",
        }
    }

    // --- Placeholder Detection Tests ---

    #[test]
    fn test_placeholder_detected() {
        let old = "";
        let new = "function foo() {\n    // TODO: implement\n    return null;\n}\n";
        let tree = parse_source(new, Language::TypeScript).unwrap();
        let new_entities = extract_entities(new, &tree, Language::TypeScript);
        let change_set = ChangeSet { changes: vec![] };
        let ctx = make_context(
            old,
            new,
            Language::TypeScript,
            &[],
            &new_entities,
            &change_set,
        );
        let lens = PlaceholderDetectionLens;
        let findings = lens.evaluate(&ctx);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("placeholder"));
    }

    #[test]
    fn test_substantive_todo_not_flagged() {
        let new = "function foo() {\n    // TODO: migrate to v2 API after Q3 release\n    return null;\n}\n";
        let tree = parse_source(new, Language::TypeScript).unwrap();
        let new_entities = extract_entities(new, &tree, Language::TypeScript);
        let change_set = ChangeSet { changes: vec![] };
        let ctx = make_context(
            "",
            new,
            Language::TypeScript,
            &[],
            &new_entities,
            &change_set,
        );
        let lens = PlaceholderDetectionLens;
        let findings = lens.evaluate(&ctx);
        assert!(findings.is_empty());
    }

    #[test]
    fn test_ellipsis_comment_detected() {
        let new = "function foo() {\n    // ...\n}\n";
        let tree = parse_source(new, Language::TypeScript).unwrap();
        let new_entities = extract_entities(new, &tree, Language::TypeScript);
        let change_set = ChangeSet { changes: vec![] };
        let ctx = make_context(
            "",
            new,
            Language::TypeScript,
            &[],
            &new_entities,
            &change_set,
        );
        let lens = PlaceholderDetectionLens;
        let findings = lens.evaluate(&ctx);
        assert_eq!(findings.len(), 1);
    }

    // --- Missing Error Handling Tests ---

    #[test]
    fn test_unhandled_await_detected() {
        let new = "async function fetchData() {\n    const data = await fetch('/api');\n    return data;\n}\n";
        let tree = parse_source(new, Language::TypeScript).unwrap();
        let new_entities = extract_entities(new, &tree, Language::TypeScript);
        let change_set = ChangeSet { changes: vec![] };
        let ctx = make_context(
            "",
            new,
            Language::TypeScript,
            &[],
            &new_entities,
            &change_set,
        );
        let lens = MissingErrorHandlingLens;
        let findings = lens.evaluate(&ctx);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("unhandled await"));
    }

    #[test]
    fn test_handled_await_not_flagged() {
        let new = "async function fetchData() {\n    try {\n        const data = await fetch('/api');\n        return data;\n    } catch (e) {\n        console.error(e);\n    }\n}\n";
        let tree = parse_source(new, Language::TypeScript).unwrap();
        let new_entities = extract_entities(new, &tree, Language::TypeScript);
        let change_set = ChangeSet { changes: vec![] };
        let ctx = make_context(
            "",
            new,
            Language::TypeScript,
            &[],
            &new_entities,
            &change_set,
        );
        let lens = MissingErrorHandlingLens;
        let findings = lens.evaluate(&ctx);
        assert!(findings.is_empty());
    }

    // --- Empty Implementation Tests ---

    #[test]
    fn test_empty_function_detected() {
        let new = "function foo() {}\n";
        let tree = parse_source(new, Language::TypeScript).unwrap();
        let new_entities = extract_entities(new, &tree, Language::TypeScript);
        let change_set = ChangeSet { changes: vec![] };
        let ctx = make_context(
            "",
            new,
            Language::TypeScript,
            &[],
            &new_entities,
            &change_set,
        );
        let lens = EmptyImplementationLens;
        let findings = lens.evaluate(&ctx);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn test_python_pass_only_detected() {
        let new = "def foo():\n    pass\n";
        let tree = parse_source(new, Language::Python).unwrap();
        let new_entities = extract_entities(new, &tree, Language::Python);
        let change_set = ChangeSet { changes: vec![] };
        let ctx = ChangeContext {
            old_source: "",
            new_source: new,
            old_entities: &[],
            new_entities: &new_entities,
            change_set: &change_set,
            language: Language::Python,
            new_file_path: "test.py",
        };
        let lens = EmptyImplementationLens;
        let findings = lens.evaluate(&ctx);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Error);
    }

    #[test]
    fn test_throw_only_detected() {
        let new = "function foo() { throw new Error(\"not implemented\"); }\n";
        let tree = parse_source(new, Language::TypeScript).unwrap();
        let new_entities = extract_entities(new, &tree, Language::TypeScript);
        let change_set = ChangeSet { changes: vec![] };
        let ctx = make_context(
            "",
            new,
            Language::TypeScript,
            &[],
            &new_entities,
            &change_set,
        );
        let lens = EmptyImplementationLens;
        let findings = lens.evaluate(&ctx);
        assert_eq!(findings.len(), 1);
        assert_eq!(findings[0].severity, Severity::Warning);
    }

    #[test]
    fn test_non_empty_function_not_flagged() {
        let new = "function foo() { return 42; }\n";
        let tree = parse_source(new, Language::TypeScript).unwrap();
        let new_entities = extract_entities(new, &tree, Language::TypeScript);
        let change_set = ChangeSet { changes: vec![] };
        let ctx = make_context(
            "",
            new,
            Language::TypeScript,
            &[],
            &new_entities,
            &change_set,
        );
        let lens = EmptyImplementationLens;
        let findings = lens.evaluate(&ctx);
        assert!(findings.is_empty());
    }

    // --- Configurable Severity Tests ---

    #[test]
    fn test_rule_disabled_by_config() {
        let new = "function foo() {\n    // TODO: implement\n}\n";
        let tree = parse_source(new, Language::TypeScript).unwrap();
        let new_entities = extract_entities(new, &tree, Language::TypeScript);
        let change_set = ChangeSet { changes: vec![] };
        let ctx = make_context(
            "",
            new,
            Language::TypeScript,
            &[],
            &new_entities,
            &change_set,
        );

        let mut config = Config::default();
        config
            .rules
            .insert("placeholder-detection".to_string(), RuleSeverity::Off);

        let findings = run_builtin_lenses(&ctx, &config);
        // Placeholder should be suppressed, only empty impl should fire
        assert!(findings.iter().all(|f| f.rule != "placeholder-detection"));
    }

    #[test]
    fn test_severity_elevated_to_error() {
        let new = "async function fetchData() {\n    const data = await fetch('/api');\n    return data;\n}\n";
        let tree = parse_source(new, Language::TypeScript).unwrap();
        let new_entities = extract_entities(new, &tree, Language::TypeScript);
        let change_set = ChangeSet { changes: vec![] };
        let ctx = make_context(
            "",
            new,
            Language::TypeScript,
            &[],
            &new_entities,
            &change_set,
        );

        let mut config = Config::default();
        config
            .rules
            .insert("missing-error-handling".to_string(), RuleSeverity::Error);

        let findings = run_builtin_lenses(&ctx, &config);
        let error_handling: Vec<_> = findings
            .iter()
            .filter(|f| f.rule == "missing-error-handling")
            .collect();
        assert!(!error_handling.is_empty());
        assert!(error_handling.iter().all(|f| f.severity == Severity::Error));
    }
}
