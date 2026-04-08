use crate::entity::{Entity, EntityKind, Span};
use crate::language::Language;
use serde::Serialize;
use std::collections::HashMap;

/// The type of change detected for an entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeType {
    Added,
    Removed,
    Modified,
    Renamed,
}

/// Classification of the nature of a change.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ChangeClassification {
    /// Whitespace/formatting/comment-only change.
    Cosmetic,
    /// Structural refactoring without logic change (rename, reorder).
    Refactor,
    /// Control flow or logic mutation.
    Logic,
    /// Public API signature change (params, return type, visibility).
    Api,
    /// New I/O, network, or external side-effect introduced.
    SideEffect,
}

impl std::fmt::Display for ChangeClassification {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ChangeClassification::Cosmetic => write!(f, "Cosmetic"),
            ChangeClassification::Refactor => write!(f, "Refactor"),
            ChangeClassification::Logic => write!(f, "Logic"),
            ChangeClassification::Api => write!(f, "API"),
            ChangeClassification::SideEffect => write!(f, "Side-effect"),
        }
    }
}

/// A single change entry in the change set.
#[derive(Debug, Clone, Serialize)]
pub struct Change {
    pub change_type: ChangeType,
    pub classification: ChangeClassification,
    pub entity_name: String,
    pub entity_kind: EntityKind,
    pub old_span: Option<Span>,
    pub new_span: Option<Span>,
    /// For renames: the old name.
    pub old_name: Option<String>,
}

/// The complete set of changes between two file versions.
#[derive(Debug, Clone, Serialize)]
pub struct ChangeSet {
    pub changes: Vec<Change>,
}

impl ChangeSet {
    pub fn added_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Added)
            .count()
    }

    pub fn removed_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Removed)
            .count()
    }

    pub fn modified_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Modified)
            .count()
    }

    pub fn renamed_count(&self) -> usize {
        self.changes
            .iter()
            .filter(|c| c.change_type == ChangeType::Renamed)
            .count()
    }
}

/// Perform 2-phase entity matching between old and new entity lists.
///
/// Phase 1: Exact ID match (same name + type).
/// Phase 2: Structural hash match for renames (same structure, different name).
pub fn match_entities(
    old_entities: &[Entity],
    new_entities: &[Entity],
    old_source: &str,
    new_source: &str,
    _lang: Language,
) -> ChangeSet {
    let mut changes = Vec::new();

    // Build lookup maps
    let old_by_id: HashMap<String, &Entity> = old_entities.iter().map(|e| (e.id(), e)).collect();
    let _new_by_id: HashMap<String, &Entity> = new_entities.iter().map(|e| (e.id(), e)).collect();

    let mut matched_old: Vec<bool> = vec![false; old_entities.len()];
    let mut matched_new: Vec<bool> = vec![false; new_entities.len()];

    // Phase 1: Exact ID match
    for (ni, new_entity) in new_entities.iter().enumerate() {
        let id = new_entity.id();
        if let Some(old_entity) = old_by_id.get(&id) {
            // Find index in old_entities
            if let Some(oi) = old_entities.iter().position(|e| e.id() == id) {
                matched_old[oi] = true;
                matched_new[ni] = true;

                if old_entity.structural_hash == new_entity.structural_hash {
                    // Identical structure — classify as cosmetic if text differs, skip if identical
                    if old_entity.body != new_entity.body {
                        changes.push(Change {
                            change_type: ChangeType::Modified,
                            classification: ChangeClassification::Cosmetic,
                            entity_name: new_entity.name.clone(),
                            entity_kind: new_entity.kind,
                            old_span: Some(old_entity.span.clone()),
                            new_span: Some(new_entity.span.clone()),
                            old_name: None,
                        });
                    }
                    // If bodies are identical, no change at all — skip
                } else {
                    // Structural hash differs — real change
                    let classification =
                        classify_change(old_entity, new_entity, old_source, new_source);
                    changes.push(Change {
                        change_type: ChangeType::Modified,
                        classification,
                        entity_name: new_entity.name.clone(),
                        entity_kind: new_entity.kind,
                        old_span: Some(old_entity.span.clone()),
                        new_span: Some(new_entity.span.clone()),
                        old_name: None,
                    });
                }
            }
        }
    }

    // Phase 2: Structural hash match for renames
    let unmatched_old: Vec<(usize, &Entity)> = old_entities
        .iter()
        .enumerate()
        .filter(|(i, _)| !matched_old[*i])
        .collect();
    let unmatched_new: Vec<(usize, &Entity)> = new_entities
        .iter()
        .enumerate()
        .filter(|(i, _)| !matched_new[*i])
        .collect();

    let mut hash_matched_old = vec![false; old_entities.len()];
    let mut hash_matched_new = vec![false; new_entities.len()];

    for &(ni, new_entity) in &unmatched_new {
        for &(oi, old_entity) in &unmatched_old {
            if hash_matched_old[oi] {
                continue;
            }
            if old_entity.kind == new_entity.kind && old_entity.body_hash == new_entity.body_hash {
                // Same structure, different name → rename
                hash_matched_old[oi] = true;
                hash_matched_new[ni] = true;
                matched_old[oi] = true;
                matched_new[ni] = true;

                changes.push(Change {
                    change_type: ChangeType::Renamed,
                    classification: ChangeClassification::Refactor,
                    entity_name: new_entity.name.clone(),
                    entity_kind: new_entity.kind,
                    old_span: Some(old_entity.span.clone()),
                    new_span: Some(new_entity.span.clone()),
                    old_name: Some(old_entity.name.clone()),
                });
                break;
            }
        }
    }

    // Remaining unmatched old entities → removed
    for (oi, old_entity) in old_entities.iter().enumerate() {
        if !matched_old[oi] {
            changes.push(Change {
                change_type: ChangeType::Removed,
                classification: ChangeClassification::Logic,
                entity_name: old_entity.name.clone(),
                entity_kind: old_entity.kind,
                old_span: Some(old_entity.span.clone()),
                new_span: None,
                old_name: None,
            });
        }
    }

    // Remaining unmatched new entities → added
    for (ni, new_entity) in new_entities.iter().enumerate() {
        if !matched_new[ni] {
            changes.push(Change {
                change_type: ChangeType::Added,
                classification: classify_added(new_entity, new_source),
                entity_name: new_entity.name.clone(),
                entity_kind: new_entity.kind,
                old_span: None,
                new_span: Some(new_entity.span.clone()),
                old_name: None,
            });
        }
    }

    ChangeSet { changes }
}

/// Classify a modification between two matched entities.
fn classify_change(
    old: &Entity,
    new: &Entity,
    old_source: &str,
    new_source: &str,
) -> ChangeClassification {
    // Check API changes: visibility or signature changes
    if old.is_public != new.is_public {
        return ChangeClassification::Api;
    }

    // Check for signature changes (return type, parameters)
    if has_signature_change(old, new, old_source, new_source) {
        return ChangeClassification::Api;
    }

    // Check for side-effect introduction
    if has_new_side_effects(old, new) {
        return ChangeClassification::SideEffect;
    }

    // Check for control flow changes
    if has_control_flow_change(old, new) {
        return ChangeClassification::Logic;
    }

    // Default: Logic change (the hash differs, so something meaningful changed)
    ChangeClassification::Logic
}

/// Classify an added entity.
fn classify_added(entity: &Entity, _source: &str) -> ChangeClassification {
    if has_side_effect_patterns(&entity.body) {
        return ChangeClassification::SideEffect;
    }
    if entity.is_public {
        return ChangeClassification::Api;
    }
    ChangeClassification::Logic
}

/// Check if the entity signature changed (simplified heuristic).
fn has_signature_change(old: &Entity, new: &Entity, old_source: &str, new_source: &str) -> bool {
    // Extract the signature line (first line / up to opening brace)
    let old_sig = extract_signature(&old.body, old_source);
    let new_sig = extract_signature(&new.body, new_source);
    old_sig != new_sig
        && old.is_public
        && (old.kind == EntityKind::Function
            || old.kind == EntityKind::Method
            || old.kind == EntityKind::Interface)
}

fn extract_signature(body: &str, _source: &str) -> String {
    // Take everything up to the first '{' or ':'
    if let Some(pos) = body.find('{') {
        body[..pos].trim().to_string()
    } else if let Some(pos) = body.find(':') {
        // Python: def foo(x): → signature is "def foo(x)"
        body[..pos].trim().to_string()
    } else {
        body.lines().next().unwrap_or("").trim().to_string()
    }
}

/// Check if new side-effect patterns were introduced.
fn has_new_side_effects(old: &Entity, new: &Entity) -> bool {
    let old_has = has_side_effect_patterns(&old.body);
    let new_has = has_side_effect_patterns(&new.body);
    !old_has && new_has
}

/// Detect common I/O / side-effect patterns.
fn has_side_effect_patterns(body: &str) -> bool {
    let patterns = [
        "fetch(",
        "http.",
        "XMLHttpRequest",
        "fs.",
        "writeFile",
        "readFile",
        "open(",
        "socket",
        "database",
        "query(",
        "execute(",
        "console.log",
        "print(",
        "println!",
        "System.out",
        "localStorage",
        "sessionStorage",
    ];
    patterns.iter().any(|p| body.contains(p))
}

/// Check for control flow changes between two entity bodies.
fn has_control_flow_change(old: &Entity, new: &Entity) -> bool {
    let old_cf = count_control_flow(&old.body);
    let new_cf = count_control_flow(&new.body);
    old_cf != new_cf
}

fn count_control_flow(body: &str) -> (usize, usize, usize, usize) {
    let ifs = body.matches("if ").count() + body.matches("if(").count();
    let loops = body.matches("for ").count()
        + body.matches("for(").count()
        + body.matches("while ").count()
        + body.matches("while(").count();
    let returns = body.matches("return ").count() + body.matches("return;").count();
    let throws = body.matches("throw ").count() + body.matches("raise ").count();
    (ifs, loops, returns, throws)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::entity::extract_entities;
    use crate::parser::parse_source;

    #[test]
    fn test_exact_id_match_no_change() {
        let source = "function greet() { return 'hello'; }\n";
        let tree = parse_source(source, Language::TypeScript).unwrap();
        let entities = extract_entities(source, &tree, Language::TypeScript);
        let changeset = match_entities(&entities, &entities, source, source, Language::TypeScript);
        assert!(changeset.changes.is_empty());
    }

    #[test]
    fn test_cosmetic_change() {
        let old = "function greet() { return 'hello'; }\n";
        let new = "function greet() {\n    return 'hello';\n}\n";
        let old_tree = parse_source(old, Language::TypeScript).unwrap();
        let new_tree = parse_source(new, Language::TypeScript).unwrap();
        let old_entities = extract_entities(old, &old_tree, Language::TypeScript);
        let new_entities = extract_entities(new, &new_tree, Language::TypeScript);
        let changeset =
            match_entities(&old_entities, &new_entities, old, new, Language::TypeScript);
        assert_eq!(changeset.changes.len(), 1);
        assert_eq!(changeset.changes[0].change_type, ChangeType::Modified);
        assert_eq!(
            changeset.changes[0].classification,
            ChangeClassification::Cosmetic
        );
    }

    #[test]
    fn test_logic_change() {
        let old = "function greet() { return 'hello'; }\n";
        let new = "function greet() { if (true) { return 'hello'; } return 'bye'; }\n";
        let old_tree = parse_source(old, Language::TypeScript).unwrap();
        let new_tree = parse_source(new, Language::TypeScript).unwrap();
        let old_entities = extract_entities(old, &old_tree, Language::TypeScript);
        let new_entities = extract_entities(new, &new_tree, Language::TypeScript);
        let changeset =
            match_entities(&old_entities, &new_entities, old, new, Language::TypeScript);
        assert_eq!(changeset.changes.len(), 1);
        assert_eq!(changeset.changes[0].change_type, ChangeType::Modified);
        assert_eq!(
            changeset.changes[0].classification,
            ChangeClassification::Logic
        );
    }

    #[test]
    fn test_added_entity() {
        let old = "function greet() { return 'hello'; }\n";
        let new = "function greet() { return 'hello'; }\nfunction farewell() { return 'bye'; }\n";
        let old_tree = parse_source(old, Language::TypeScript).unwrap();
        let new_tree = parse_source(new, Language::TypeScript).unwrap();
        let old_entities = extract_entities(old, &old_tree, Language::TypeScript);
        let new_entities = extract_entities(new, &new_tree, Language::TypeScript);
        let changeset =
            match_entities(&old_entities, &new_entities, old, new, Language::TypeScript);
        assert_eq!(changeset.added_count(), 1);
        let added = changeset
            .changes
            .iter()
            .find(|c| c.change_type == ChangeType::Added)
            .unwrap();
        assert_eq!(added.entity_name, "farewell");
    }

    #[test]
    fn test_removed_entity() {
        let old = "function greet() { return 'hello'; }\nfunction farewell() { return 'bye'; }\n";
        let new = "function greet() { return 'hello'; }\n";
        let old_tree = parse_source(old, Language::TypeScript).unwrap();
        let new_tree = parse_source(new, Language::TypeScript).unwrap();
        let old_entities = extract_entities(old, &old_tree, Language::TypeScript);
        let new_entities = extract_entities(new, &new_tree, Language::TypeScript);
        let changeset =
            match_entities(&old_entities, &new_entities, old, new, Language::TypeScript);
        assert_eq!(changeset.removed_count(), 1);
    }

    #[test]
    fn test_rename_detection() {
        let old = "function handleRequest() { return 42; }\n";
        let new = "function processRequest() { return 42; }\n";
        let old_tree = parse_source(old, Language::TypeScript).unwrap();
        let new_tree = parse_source(new, Language::TypeScript).unwrap();
        let old_entities = extract_entities(old, &old_tree, Language::TypeScript);
        let new_entities = extract_entities(new, &new_tree, Language::TypeScript);
        let changeset =
            match_entities(&old_entities, &new_entities, old, new, Language::TypeScript);
        assert_eq!(changeset.renamed_count(), 1);
        let renamed = changeset
            .changes
            .iter()
            .find(|c| c.change_type == ChangeType::Renamed)
            .unwrap();
        assert_eq!(renamed.entity_name, "processRequest");
        assert_eq!(renamed.old_name.as_deref(), Some("handleRequest"));
        assert_eq!(renamed.classification, ChangeClassification::Refactor);
    }

    #[test]
    fn test_side_effect_introduction() {
        let old = "function save() { return true; }\n";
        let new = "function save() { fetch('/api/save'); return true; }\n";
        let old_tree = parse_source(old, Language::TypeScript).unwrap();
        let new_tree = parse_source(new, Language::TypeScript).unwrap();
        let old_entities = extract_entities(old, &old_tree, Language::TypeScript);
        let new_entities = extract_entities(new, &new_tree, Language::TypeScript);
        let changeset =
            match_entities(&old_entities, &new_entities, old, new, Language::TypeScript);
        assert_eq!(changeset.changes.len(), 1);
        assert_eq!(
            changeset.changes[0].classification,
            ChangeClassification::SideEffect
        );
    }
}
