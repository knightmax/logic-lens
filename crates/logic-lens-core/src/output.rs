use crate::diff::{ChangeClassification, ChangeSet};
use crate::entity::Entity;
use crate::lint::{Finding, Severity};
use serde::Serialize;
use std::time::Duration;

/// Complete audit result as the primary output contract.
#[derive(Debug, Serialize)]
pub struct AuditResult {
    pub old_file: String,
    pub new_file: String,
    pub language: String,
    pub entities: EntitySummary,
    pub changes: ChangesSummary,
    pub findings: Vec<Finding>,
    pub metadata: AuditMetadata,
}

#[derive(Debug, Serialize)]
pub struct EntitySummary {
    pub old_count: usize,
    pub new_count: usize,
    pub old_entities: Vec<EntityOutput>,
    pub new_entities: Vec<EntityOutput>,
}

#[derive(Debug, Serialize)]
pub struct EntityOutput {
    pub name: String,
    pub kind: String,
    pub start_line: usize,
    pub end_line: usize,
    pub is_public: bool,
}

impl From<&Entity> for EntityOutput {
    fn from(e: &Entity) -> Self {
        EntityOutput {
            name: e.name.clone(),
            kind: format!("{}", e.kind),
            start_line: e.span.start_line,
            end_line: e.span.end_line,
            is_public: e.is_public,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ChangesSummary {
    pub total: usize,
    pub added: usize,
    pub removed: usize,
    pub modified: usize,
    pub renamed: usize,
    pub by_classification: ClassificationCounts,
    pub details: Vec<ChangeDetail>,
}

#[derive(Debug, Serialize)]
pub struct ClassificationCounts {
    pub cosmetic: usize,
    pub refactor: usize,
    pub logic: usize,
    pub api: usize,
    pub side_effect: usize,
}

#[derive(Debug, Serialize)]
pub struct ChangeDetail {
    pub entity_name: String,
    pub entity_kind: String,
    pub change_type: String,
    pub classification: String,
}

#[derive(Debug, Serialize)]
pub struct AuditMetadata {
    pub parse_duration_ms: f64,
    pub diff_duration_ms: f64,
    pub analyze_duration_ms: f64,
    pub total_duration_ms: f64,
}

impl AuditResult {
    #[allow(clippy::too_many_arguments)]
    pub fn build(
        old_file: &str,
        new_file: &str,
        language: &str,
        old_entities: &[Entity],
        new_entities: &[Entity],
        change_set: &ChangeSet,
        findings: Vec<Finding>,
        parse_duration: Duration,
        diff_duration: Duration,
        analyze_duration: Duration,
        total_duration: Duration,
    ) -> Self {
        let by_class = ClassificationCounts {
            cosmetic: change_set
                .changes
                .iter()
                .filter(|c| c.classification == ChangeClassification::Cosmetic)
                .count(),
            refactor: change_set
                .changes
                .iter()
                .filter(|c| c.classification == ChangeClassification::Refactor)
                .count(),
            logic: change_set
                .changes
                .iter()
                .filter(|c| c.classification == ChangeClassification::Logic)
                .count(),
            api: change_set
                .changes
                .iter()
                .filter(|c| c.classification == ChangeClassification::Api)
                .count(),
            side_effect: change_set
                .changes
                .iter()
                .filter(|c| c.classification == ChangeClassification::SideEffect)
                .count(),
        };

        AuditResult {
            old_file: old_file.to_string(),
            new_file: new_file.to_string(),
            language: language.to_string(),
            entities: EntitySummary {
                old_count: old_entities.len(),
                new_count: new_entities.len(),
                old_entities: old_entities.iter().map(EntityOutput::from).collect(),
                new_entities: new_entities.iter().map(EntityOutput::from).collect(),
            },
            changes: ChangesSummary {
                total: change_set.changes.len(),
                added: change_set.added_count(),
                removed: change_set.removed_count(),
                modified: change_set.modified_count(),
                renamed: change_set.renamed_count(),
                by_classification: by_class,
                details: change_set
                    .changes
                    .iter()
                    .map(|c| ChangeDetail {
                        entity_name: c.entity_name.clone(),
                        entity_kind: format!("{}", c.entity_kind),
                        change_type: format!("{:?}", c.change_type),
                        classification: format!("{:?}", c.classification),
                    })
                    .collect(),
            },
            findings,
            metadata: AuditMetadata {
                parse_duration_ms: parse_duration.as_secs_f64() * 1000.0,
                diff_duration_ms: diff_duration.as_secs_f64() * 1000.0,
                analyze_duration_ms: analyze_duration.as_secs_f64() * 1000.0,
                total_duration_ms: total_duration.as_secs_f64() * 1000.0,
            },
        }
    }

    /// Returns true if there are any error-severity findings.
    pub fn has_errors(&self) -> bool {
        self.findings.iter().any(|f| f.severity == Severity::Error)
    }

    /// Compute risk level based on findings and changes.
    pub fn risk_level(&self) -> &'static str {
        let error_count = self
            .findings
            .iter()
            .filter(|f| f.severity == Severity::Error)
            .count();
        let logic_or_side_effect =
            self.changes.by_classification.logic + self.changes.by_classification.side_effect;

        if error_count > 0 || logic_or_side_effect >= 3 {
            "High"
        } else if logic_or_side_effect >= 1 || self.findings.len() >= 3 {
            "Medium"
        } else {
            "Low"
        }
    }
}

/// Verbosity level for output.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    Quiet,
    Normal,
    Verbose,
}

// --- JSON Renderer ---

pub fn render_json(result: &AuditResult) -> String {
    serde_json::to_string_pretty(result).unwrap_or_else(|e| format!("{{\"error\": \"{}\"}}", e))
}

// --- Terminal Renderer ---

pub fn render_terminal(result: &AuditResult, verbosity: Verbosity, use_color: bool) -> String {
    let mut out = String::new();

    if verbosity != Verbosity::Quiet {
        // Summary line
        let summary = format!(
            "{} changes ({} logic, {} cosmetic, {} refactor, {} api, {} side-effect) | {} findings | Risk: {}",
            result.changes.total,
            result.changes.by_classification.logic,
            result.changes.by_classification.cosmetic,
            result.changes.by_classification.refactor,
            result.changes.by_classification.api,
            result.changes.by_classification.side_effect,
            result.findings.len(),
            result.risk_level(),
        );
        out.push_str(&summary);
        out.push('\n');
        out.push('\n');
    }

    // Findings
    for f in &result.findings {
        let severity_str = match f.severity {
            Severity::Error => {
                if use_color {
                    "\x1b[31mERROR\x1b[0m"
                } else {
                    "ERROR"
                }
            }
            Severity::Warning => {
                if use_color {
                    "\x1b[33mWARN \x1b[0m"
                } else {
                    "WARN "
                }
            }
        };
        out.push_str(&format!(
            "  {} {}:{}:{} {}\n",
            severity_str, f.file, f.line, f.column, f.message
        ));
    }

    if verbosity == Verbosity::Verbose {
        out.push('\n');
        out.push_str(&format!(
            "Timing: parse={:.1}ms diff={:.1}ms analyze={:.1}ms total={:.1}ms\n",
            result.metadata.parse_duration_ms,
            result.metadata.diff_duration_ms,
            result.metadata.analyze_duration_ms,
            result.metadata.total_duration_ms,
        ));
    }

    out
}

// --- Markdown Renderer ---

pub fn render_markdown(result: &AuditResult) -> String {
    let mut out = String::new();

    out.push_str(&format!(
        "# Audit: {} → {}\n\n",
        result.old_file, result.new_file
    ));
    out.push_str(&format!(
        "**Language:** {} | **Risk:** {}\n\n",
        result.language,
        result.risk_level()
    ));

    // Changes table
    out.push_str("## Changes\n\n");
    out.push_str("| Entity | Kind | Type | Classification |\n");
    out.push_str("|--------|------|------|----------------|\n");
    for c in &result.changes.details {
        out.push_str(&format!(
            "| {} | {} | {} | {} |\n",
            c.entity_name, c.entity_kind, c.change_type, c.classification
        ));
    }
    out.push('\n');

    // Findings
    if !result.findings.is_empty() {
        out.push_str("## Findings\n\n");
        for f in &result.findings {
            let icon = match f.severity {
                Severity::Error => "🔴",
                Severity::Warning => "🟡",
            };
            out.push_str(&format!(
                "- {} **{}** `{}:{}` — {}\n",
                icon, f.rule, f.file, f.line, f.message
            ));
        }
        out.push('\n');
    }

    // Summary
    out.push_str("## Summary\n\n");
    out.push_str(&format!(
        "- **Changes:** {} ({} added, {} removed, {} modified, {} renamed)\n",
        result.changes.total,
        result.changes.added,
        result.changes.removed,
        result.changes.modified,
        result.changes.renamed,
    ));
    out.push_str(&format!("- **Findings:** {}\n", result.findings.len()));

    out
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_result() -> AuditResult {
        AuditResult {
            old_file: "old.ts".to_string(),
            new_file: "new.ts".to_string(),
            language: "TypeScript".to_string(),
            entities: EntitySummary {
                old_count: 1,
                new_count: 1,
                old_entities: vec![],
                new_entities: vec![],
            },
            changes: ChangesSummary {
                total: 2,
                added: 0,
                removed: 0,
                modified: 2,
                renamed: 0,
                by_classification: ClassificationCounts {
                    cosmetic: 1,
                    refactor: 0,
                    logic: 1,
                    api: 0,
                    side_effect: 0,
                },
                details: vec![
                    ChangeDetail {
                        entity_name: "foo".to_string(),
                        entity_kind: "Function".to_string(),
                        change_type: "Modified".to_string(),
                        classification: "Logic".to_string(),
                    },
                    ChangeDetail {
                        entity_name: "bar".to_string(),
                        entity_kind: "Function".to_string(),
                        change_type: "Modified".to_string(),
                        classification: "Cosmetic".to_string(),
                    },
                ],
            },
            findings: vec![Finding {
                rule: "placeholder-detection".to_string(),
                severity: Severity::Warning,
                message: "AI placeholder detected".to_string(),
                file: "new.ts".to_string(),
                line: 5,
                column: 1,
            }],
            metadata: AuditMetadata {
                parse_duration_ms: 1.2,
                diff_duration_ms: 0.5,
                analyze_duration_ms: 0.3,
                total_duration_ms: 2.0,
            },
        }
    }

    #[test]
    fn test_json_output_parseable() {
        let result = sample_result();
        let json = render_json(&result);
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["language"], "TypeScript");
        assert_eq!(parsed["changes"]["total"], 2);
        assert_eq!(parsed["findings"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn test_terminal_normal_output() {
        let result = sample_result();
        let out = render_terminal(&result, Verbosity::Normal, false);
        assert!(out.contains("2 changes"));
        assert!(out.contains("Risk: Medium"));
        assert!(out.contains("WARN"));
        assert!(!out.contains("Timing:"));
    }

    #[test]
    fn test_terminal_quiet_output() {
        let result = sample_result();
        let out = render_terminal(&result, Verbosity::Quiet, false);
        assert!(!out.contains("changes"));
        assert!(out.contains("WARN"));
    }

    #[test]
    fn test_terminal_verbose_output() {
        let result = sample_result();
        let out = render_terminal(&result, Verbosity::Verbose, false);
        assert!(out.contains("Timing:"));
        assert!(out.contains("parse="));
    }

    #[test]
    fn test_terminal_no_color_when_not_tty() {
        let result = sample_result();
        let out = render_terminal(&result, Verbosity::Normal, false);
        assert!(!out.contains("\x1b["));
    }

    #[test]
    fn test_markdown_output() {
        let result = sample_result();
        let md = render_markdown(&result);
        assert!(md.contains("# Audit:"));
        assert!(md.contains("| foo |"));
        assert!(md.contains("## Findings"));
        assert!(md.contains("🟡"));
    }

    #[test]
    fn test_exit_code_no_errors() {
        let result = sample_result();
        assert!(!result.has_errors());
    }

    #[test]
    fn test_exit_code_with_errors() {
        let mut result = sample_result();
        result.findings[0].severity = Severity::Error;
        assert!(result.has_errors());
    }
}
