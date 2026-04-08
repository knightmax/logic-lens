use crate::language::Language;
use crate::lint::{AuditLens, ChangeContext, Finding, Severity};
use serde::Deserialize;
use std::path::{Path, PathBuf};

/// A declarative rule defined in YAML.
#[derive(Debug, Clone, Deserialize)]
pub struct YamlRule {
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub language: Vec<String>,
    pub severity: YamlSeverity,
    pub message: String,
    pub pattern: RulePattern,
    #[serde(default)]
    pub priority: Option<i32>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum YamlSeverity {
    Error,
    Warning,
}

impl From<&YamlSeverity> for Severity {
    fn from(s: &YamlSeverity) -> Self {
        match s {
            YamlSeverity::Error => Severity::Error,
            YamlSeverity::Warning => Severity::Warning,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(tag = "type")]
pub enum RulePattern {
    #[serde(rename = "contains")]
    Contains { value: String },
    #[serde(rename = "regex")]
    Regex { value: String },
    #[serde(rename = "node_type")]
    NodeType {
        node_type: String,
        #[serde(default)]
        contains: Option<String>,
    },
}

/// A YAML rule wrapped as an AuditLens.
pub struct YamlRuleLens {
    pub rule: YamlRule,
    compiled_regex: Option<regex::Regex>,
}

impl YamlRuleLens {
    pub fn new(rule: YamlRule) -> Result<Self, String> {
        let compiled_regex = match &rule.pattern {
            RulePattern::Regex { value } => {
                let re = regex::Regex::new(value)
                    .map_err(|e| format!("Invalid regex in rule '{}': {}", rule.name, e))?;
                Some(re)
            }
            _ => None,
        };
        Ok(YamlRuleLens {
            rule,
            compiled_regex,
        })
    }

    fn applies_to_language(&self, lang: Language) -> bool {
        if self.rule.language.is_empty() {
            return true;
        }
        let lang_str = match lang {
            Language::TypeScript => "typescript",
            Language::JavaScript => "javascript",
            Language::Python => "python",
            Language::Rust => "rust",
            Language::Java => "java",
        };
        self.rule.language.iter().any(|l| l == lang_str)
    }
}

impl AuditLens for YamlRuleLens {
    fn name(&self) -> &str {
        &self.rule.name
    }

    fn evaluate(&self, ctx: &ChangeContext) -> Vec<Finding> {
        if !self.applies_to_language(ctx.language) {
            return vec![];
        }

        let mut findings = Vec::new();
        match &self.rule.pattern {
            RulePattern::Contains { value } => {
                for (line_idx, line) in ctx.new_source.lines().enumerate() {
                    if line.contains(value.as_str()) {
                        findings.push(Finding {
                            rule: self.rule.name.clone(),
                            severity: Severity::from(&self.rule.severity),
                            message: self.rule.message.clone(),
                            file: ctx.new_file_path.to_string(),
                            line: line_idx + 1,
                            column: 1,
                        });
                    }
                }
            }
            RulePattern::Regex { .. } => {
                if let Some(re) = &self.compiled_regex {
                    for (line_idx, line) in ctx.new_source.lines().enumerate() {
                        if re.is_match(line) {
                            findings.push(Finding {
                                rule: self.rule.name.clone(),
                                severity: Severity::from(&self.rule.severity),
                                message: self.rule.message.clone(),
                                file: ctx.new_file_path.to_string(),
                                line: line_idx + 1,
                                column: 1,
                            });
                        }
                    }
                }
            }
            RulePattern::NodeType {
                node_type,
                contains,
            } => {
                for entity in ctx.new_entities {
                    let kind_str = format!("{}", entity.kind);
                    if kind_str.to_lowercase() == node_type.to_lowercase() {
                        if let Some(text) = contains {
                            if entity.body.contains(text.as_str()) {
                                findings.push(Finding {
                                    rule: self.rule.name.clone(),
                                    severity: Severity::from(&self.rule.severity),
                                    message: self.rule.message.clone(),
                                    file: ctx.new_file_path.to_string(),
                                    line: entity.span.start_line,
                                    column: entity.span.start_col,
                                });
                            }
                        } else {
                            findings.push(Finding {
                                rule: self.rule.name.clone(),
                                severity: Severity::from(&self.rule.severity),
                                message: self.rule.message.clone(),
                                file: ctx.new_file_path.to_string(),
                                line: entity.span.start_line,
                                column: entity.span.start_col,
                            });
                        }
                    }
                }
            }
        }
        findings
    }
}

/// Discover YAML rule files from a directory.
pub fn discover_rules(dir: &Path) -> Vec<PathBuf> {
    if !dir.is_dir() {
        return vec![];
    }
    let mut files: Vec<PathBuf> = Vec::new();
    if let Ok(entries) = std::fs::read_dir(dir) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_file() {
                if let Some(ext) = path.extension().and_then(|e| e.to_str()) {
                    if ext == "yaml" || ext == "yml" {
                        files.push(path);
                    }
                }
            }
        }
    }
    // Sort alphabetically for deterministic order
    files.sort();
    files
}

/// Load a YAML rule from a file.
pub fn load_rule(path: &Path) -> Result<YamlRule, String> {
    let content = std::fs::read_to_string(path)
        .map_err(|e| format!("Failed to read rule file {}: {}", path.display(), e))?;
    let rule: YamlRule = serde_yaml::from_str(&content)
        .map_err(|e| format!("Invalid YAML in rule file {}: {}", path.display(), e))?;
    Ok(rule)
}

/// Load all rules from a directory, returning loaded lenses and errors.
pub fn load_all_rules(dir: &Path) -> (Vec<YamlRuleLens>, Vec<String>) {
    let files = discover_rules(dir);
    let mut lenses = Vec::new();
    let mut errors = Vec::new();

    for file in files {
        match load_rule(&file) {
            Ok(rule) => match YamlRuleLens::new(rule) {
                Ok(lens) => lenses.push(lens),
                Err(e) => errors.push(e),
            },
            Err(e) => errors.push(e),
        }
    }

    // Sort by priority (lower = first), then alphabetically
    lenses.sort_by(|a, b| {
        let pa = a.rule.priority.unwrap_or(100);
        let pb = b.rule.priority.unwrap_or(100);
        pa.cmp(&pb).then_with(|| a.rule.name.cmp(&b.rule.name))
    });

    (lenses, errors)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::diff::ChangeSet;
    use crate::entity::extract_entities;
    use crate::parser::parse_source;
    use std::io::Write;
    use tempfile::TempDir;

    fn make_ctx<'a>(
        source: &'a str,
        lang: Language,
        entities: &'a [crate::entity::Entity],
        change_set: &'a ChangeSet,
    ) -> ChangeContext<'a> {
        ChangeContext {
            old_source: "",
            new_source: source,
            old_entities: &[],
            new_entities: entities,
            change_set,
            language: lang,
            new_file_path: "test.ts",
        }
    }

    #[test]
    fn test_contains_pattern_rule() {
        let yaml = r#"
name: no-console-log
description: Disallow console.log
severity: warning
message: "console.log should not be used in production code"
pattern:
  type: contains
  value: "console.log"
"#;
        let rule: YamlRule = serde_yaml::from_str(yaml).unwrap();
        let lens = YamlRuleLens::new(rule).unwrap();

        let source = "function foo() {\n    console.log('debug');\n    return 42;\n}\n";
        let tree = parse_source(source, Language::TypeScript).unwrap();
        let entities = extract_entities(source, &tree, Language::TypeScript);
        let cs = ChangeSet { changes: vec![] };
        let ctx = make_ctx(source, Language::TypeScript, &entities, &cs);

        let findings = lens.evaluate(&ctx);
        assert_eq!(findings.len(), 1);
        assert!(findings[0].message.contains("console.log"));
    }

    #[test]
    fn test_regex_pattern_rule() {
        let yaml = r#"
name: no-magic-numbers
severity: error
message: "Avoid magic numbers > 100"
pattern:
  type: regex
  value: '\b\d{3,}\b'
"#;
        let rule: YamlRule = serde_yaml::from_str(yaml).unwrap();
        let lens = YamlRuleLens::new(rule).unwrap();

        let source = "const timeout = 5000;\nconst retries = 3;\n";
        let tree = parse_source(source, Language::TypeScript).unwrap();
        let entities = extract_entities(source, &tree, Language::TypeScript);
        let cs = ChangeSet { changes: vec![] };
        let ctx = make_ctx(source, Language::TypeScript, &entities, &cs);

        let findings = lens.evaluate(&ctx);
        assert_eq!(findings.len(), 1); // 5000 matches \d{3,}, 3 does not
    }

    #[test]
    fn test_language_scoped_rule() {
        let yaml = r#"
name: no-console-log
language: [typescript, javascript]
severity: warning
message: "No console.log"
pattern:
  type: contains
  value: "console.log"
"#;
        let rule: YamlRule = serde_yaml::from_str(yaml).unwrap();
        let lens = YamlRuleLens::new(rule).unwrap();

        let source = "console.log('test');";
        let cs = ChangeSet { changes: vec![] };

        // Should match TypeScript
        let ctx = make_ctx(source, Language::TypeScript, &[], &cs);
        assert_eq!(lens.evaluate(&ctx).len(), 1);

        // Should NOT match Python
        let ctx = ChangeContext {
            old_source: "",
            new_source: source,
            old_entities: &[],
            new_entities: &[],
            change_set: &cs,
            language: Language::Python,
            new_file_path: "test.py",
        };
        assert!(lens.evaluate(&ctx).is_empty());
    }

    #[test]
    fn test_discover_and_load_rules() {
        let dir = TempDir::new().unwrap();
        let rules_dir = dir.path().join(".logic-lens").join("rules");
        std::fs::create_dir_all(&rules_dir).unwrap();

        // Create a valid rule
        let mut f = std::fs::File::create(rules_dir.join("no-console-log.yaml")).unwrap();
        write!(
            f,
            r#"name: no-console-log
severity: warning
message: "No console.log"
pattern:
  type: contains
  value: "console.log"
"#
        )
        .unwrap();

        // Create an invalid rule
        let mut f2 = std::fs::File::create(rules_dir.join("broken.yaml")).unwrap();
        write!(f2, "this is not valid yaml: [[[").unwrap();

        let (lenses, errors) = load_all_rules(&rules_dir);
        assert_eq!(lenses.len(), 1);
        assert_eq!(lenses[0].rule.name, "no-console-log");
        assert_eq!(errors.len(), 1);
    }

    #[test]
    fn test_rule_ordering_by_priority() {
        let dir = TempDir::new().unwrap();
        let rules_dir = dir.path().join("rules");
        std::fs::create_dir_all(&rules_dir).unwrap();

        let write_rule = |name: &str, priority: i32| {
            let path = rules_dir.join(format!("{}.yaml", name));
            let mut f = std::fs::File::create(path).unwrap();
            write!(
                f,
                "name: {}\nseverity: warning\nmessage: test\npriority: {}\npattern:\n  type: contains\n  value: x\n",
                name, priority
            )
            .unwrap();
        };

        write_rule("c-rule", 10);
        write_rule("a-rule", 50);
        write_rule("b-rule", 10);

        let (lenses, errors) = load_all_rules(&rules_dir);
        assert!(errors.is_empty());
        assert_eq!(lenses.len(), 3);
        // Priority 10 first, alphabetically: b-rule, c-rule — then priority 50: a-rule
        assert_eq!(lenses[0].rule.name, "b-rule");
        assert_eq!(lenses[1].rule.name, "c-rule");
        assert_eq!(lenses[2].rule.name, "a-rule");
    }
}
