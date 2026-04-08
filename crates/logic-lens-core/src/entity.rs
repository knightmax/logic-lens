use crate::language::Language;
use ahash::AHasher;
use serde::Serialize;
use std::hash::Hasher;

/// The type of an extracted code entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    Function,
    Method,
    Class,
    Struct,
    Interface,
    Enum,
}

impl std::fmt::Display for EntityKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EntityKind::Function => write!(f, "function"),
            EntityKind::Method => write!(f, "method"),
            EntityKind::Class => write!(f, "class"),
            EntityKind::Struct => write!(f, "struct"),
            EntityKind::Interface => write!(f, "interface"),
            EntityKind::Enum => write!(f, "enum"),
        }
    }
}

/// A span within source code (byte offsets + line/col).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Span {
    pub start_byte: usize,
    pub end_byte: usize,
    pub start_line: usize,
    pub start_col: usize,
    pub end_line: usize,
    pub end_col: usize,
}

impl Span {
    pub fn from_node(node: &tree_sitter::Node) -> Self {
        let start = node.start_position();
        let end = node.end_position();
        Span {
            start_byte: node.start_byte(),
            end_byte: node.end_byte(),
            start_line: start.row + 1,
            start_col: start.column + 1,
            end_line: end.row + 1,
            end_col: end.column + 1,
        }
    }
}

/// An extracted code entity (function, class, method, etc.).
#[derive(Debug, Clone, Serialize)]
pub struct Entity {
    pub name: String,
    pub kind: EntityKind,
    pub span: Span,
    /// Raw source text of the entity body.
    pub body: String,
    /// Structural hash (whitespace/comment-normalized) of the full entity.
    pub structural_hash: u64,
    /// Structural hash of just the inner body (excluding signature/name).
    /// Used for rename detection.
    pub body_hash: u64,
    /// Whether the entity is public/exported.
    pub is_public: bool,
}

impl Entity {
    /// Unique identifier for matching: "kind::name".
    pub fn id(&self) -> String {
        format!("{}::{}", self.kind, self.name)
    }
}

/// Compute a structural hash of source text, normalizing whitespace and stripping comments.
pub fn structural_hash(body: &str, lang: Language) -> u64 {
    let normalized = normalize_for_hash(body, lang);
    let mut hasher = AHasher::default();
    hasher.write(normalized.as_bytes());
    hasher.finish()
}

/// Compute a structural hash of just the inner body (excluding signature/name).
/// Used for rename detection — two functions with different names but same body
/// will have the same body_hash.
pub fn body_only_hash(body: &str, lang: Language) -> u64 {
    let inner = extract_inner_body(body);
    structural_hash(inner, lang)
}

/// Extract the inner body of a function/method, stripping the signature.
fn extract_inner_body(body: &str) -> &str {
    // Find the first '{' (most languages) or ':' for Python-style
    if let Some(pos) = body.find('{') {
        &body[pos..]
    } else if let Some(pos) = body.find(':') {
        // Python: def foo(x): body
        &body[pos + 1..]
    } else {
        body
    }
}

/// Normalize source text for structural comparison:
/// - Collapse all whitespace runs to single space
/// - Strip single-line and multi-line comments
fn normalize_for_hash(text: &str, lang: Language) -> String {
    let stripped = strip_comments(text, lang);
    // Collapse whitespace
    let mut result = String::with_capacity(stripped.len());
    let mut prev_ws = false;
    for ch in stripped.chars() {
        if ch.is_whitespace() {
            if !prev_ws {
                result.push(' ');
                prev_ws = true;
            }
        } else {
            result.push(ch);
            prev_ws = false;
        }
    }
    result.trim().to_string()
}

/// Strip comments from source text. Handles // and /* */ for most languages,
/// and # for Python.
fn strip_comments(text: &str, lang: Language) -> String {
    let mut result = String::with_capacity(text.len());
    let chars: Vec<char> = text.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // String literals — skip over them to avoid stripping "comments" inside strings
        if chars[i] == '"' || chars[i] == '\'' {
            let quote = chars[i];
            result.push(chars[i]);
            i += 1;
            while i < len && chars[i] != quote {
                if chars[i] == '\\' && i + 1 < len {
                    result.push(chars[i]);
                    result.push(chars[i + 1]);
                    i += 2;
                } else {
                    result.push(chars[i]);
                    i += 1;
                }
            }
            if i < len {
                result.push(chars[i]);
                i += 1;
            }
        }
        // Line comments: Python # or C-style //
        else if (lang == Language::Python && chars[i] == '#')
            || (chars[i] == '/' && i + 1 < len && chars[i + 1] == '/')
        {
            while i < len && chars[i] != '\n' {
                i += 1;
            }
        }
        // C-style: /* block comments */
        else if chars[i] == '/' && i + 1 < len && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            if i + 1 < len {
                i += 2; // skip */
            }
        } else {
            result.push(chars[i]);
            i += 1;
        }
    }
    result
}

/// Extract all entities from a parsed tree-sitter AST.
pub fn extract_entities(source: &str, tree: &tree_sitter::Tree, lang: Language) -> Vec<Entity> {
    let root = tree.root_node();
    let mut entities = Vec::new();
    extract_from_node(source, &root, lang, &mut entities, false);
    entities
}

fn extract_from_node(
    source: &str,
    node: &tree_sitter::Node,
    lang: Language,
    entities: &mut Vec<Entity>,
    inside_class: bool,
) {
    match lang {
        Language::TypeScript | Language::JavaScript => {
            extract_ts_js(source, node, lang, entities, inside_class)
        }
        Language::Python => extract_python(source, node, lang, entities, inside_class),
        Language::Rust => extract_rust(source, node, lang, entities, inside_class),
        Language::Java => extract_java(source, node, lang, entities, inside_class),
    }
}

// --- TypeScript / JavaScript ---

fn extract_ts_js(
    source: &str,
    node: &tree_sitter::Node,
    lang: Language,
    entities: &mut Vec<Entity>,
    inside_class: bool,
) {
    let kind = node.kind();

    match kind {
        "function_declaration" | "generator_function_declaration" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                let is_public = is_exported_ts(node, source);
                entities.push(Entity {
                    name,
                    kind: if inside_class {
                        EntityKind::Method
                    } else {
                        EntityKind::Function
                    },
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public,
                });
            }
        }
        "lexical_declaration" | "variable_declaration" => {
            // Arrow functions: const foo = () => {}
            extract_arrow_functions(node, source, lang, entities);
        }
        "method_definition" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                entities.push(Entity {
                    name,
                    kind: EntityKind::Method,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public: true,
                });
            }
        }
        "class_declaration" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                let is_public = is_exported_ts(node, source);
                entities.push(Entity {
                    name,
                    kind: EntityKind::Class,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public,
                });
            }
            // Recurse into class body for methods
            if let Some(body_node) = node.child_by_field_name("body") {
                let mut cursor = body_node.walk();
                for child in body_node.children(&mut cursor) {
                    extract_from_node(source, &child, lang, entities, true);
                }
            }
            return; // Don't recurse normally — we handled the body
        }
        "interface_declaration" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                entities.push(Entity {
                    name,
                    kind: EntityKind::Interface,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public: is_exported_ts(node, source),
                });
            }
        }
        "enum_declaration" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                entities.push(Entity {
                    name,
                    kind: EntityKind::Enum,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public: is_exported_ts(node, source),
                });
            }
        }
        "export_statement" => {
            // Recurse into export to find the actual declaration
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                extract_from_node(source, &child, lang, entities, inside_class);
            }
            return;
        }
        _ => {}
    }

    // Recurse into children
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_from_node(source, &child, lang, entities, inside_class);
    }
}

fn extract_arrow_functions(
    node: &tree_sitter::Node,
    source: &str,
    lang: Language,
    entities: &mut Vec<Entity>,
) {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "variable_declarator" {
            let name = child_by_field_text(&child, "name", source);
            let value = child.child_by_field_name("value");
            if let (Some(name), Some(value)) = (name, value) {
                if value.kind() == "arrow_function" || value.kind() == "function_expression" {
                    let body = node_text(&value, source);
                    entities.push(Entity {
                        name,
                        kind: EntityKind::Function,
                        span: Span::from_node(node),
                        structural_hash: structural_hash(&body, lang),
                        body_hash: body_only_hash(&body, lang),
                        body,
                        is_public: is_exported_ts(node, source),
                    });
                }
            }
        }
    }
}

fn is_exported_ts(node: &tree_sitter::Node, source: &str) -> bool {
    if let Some(parent) = node.parent() {
        if parent.kind() == "export_statement" {
            return true;
        }
    }
    // Check if previous sibling is "export"
    let text = node_text(node, source);
    text.starts_with("export ")
}

// --- Python ---

fn extract_python(
    source: &str,
    node: &tree_sitter::Node,
    lang: Language,
    entities: &mut Vec<Entity>,
    inside_class: bool,
) {
    let kind = node.kind();

    match kind {
        "function_definition" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                let is_public = !name.starts_with('_');
                let entity_kind = if inside_class {
                    EntityKind::Method
                } else {
                    EntityKind::Function
                };
                entities.push(Entity {
                    name,
                    kind: entity_kind,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public,
                });
            }
        }
        "class_definition" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                entities.push(Entity {
                    name,
                    kind: EntityKind::Class,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public: true,
                });
            }
            // Recurse into class body for methods
            if let Some(body_node) = node.child_by_field_name("body") {
                let mut cursor = body_node.walk();
                for child in body_node.children(&mut cursor) {
                    extract_from_node(source, &child, lang, entities, true);
                }
            }
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_from_node(source, &child, lang, entities, inside_class);
    }
}

// --- Rust ---

fn extract_rust(
    source: &str,
    node: &tree_sitter::Node,
    lang: Language,
    entities: &mut Vec<Entity>,
    inside_class: bool,
) {
    let kind = node.kind();

    match kind {
        "function_item" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                let is_public = has_pub_modifier(node, source);
                entities.push(Entity {
                    name,
                    kind: if inside_class {
                        EntityKind::Method
                    } else {
                        EntityKind::Function
                    },
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public,
                });
            }
        }
        "struct_item" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                entities.push(Entity {
                    name,
                    kind: EntityKind::Struct,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public: has_pub_modifier(node, source),
                });
            }
        }
        "enum_item" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                entities.push(Entity {
                    name,
                    kind: EntityKind::Enum,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public: has_pub_modifier(node, source),
                });
            }
        }
        "trait_item" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                entities.push(Entity {
                    name,
                    kind: EntityKind::Interface,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public: has_pub_modifier(node, source),
                });
            }
        }
        "impl_item" => {
            // Recurse into impl blocks for methods
            let mut cursor = node.walk();
            for child in node.children(&mut cursor) {
                if child.kind() == "declaration_list" {
                    let mut inner_cursor = child.walk();
                    for inner in child.children(&mut inner_cursor) {
                        extract_from_node(source, &inner, lang, entities, true);
                    }
                }
            }
            return;
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_from_node(source, &child, lang, entities, inside_class);
    }
}

fn has_pub_modifier(node: &tree_sitter::Node, source: &str) -> bool {
    let text = node_text(node, source);
    text.starts_with("pub ")
}

// --- Java ---

fn extract_java(
    source: &str,
    node: &tree_sitter::Node,
    lang: Language,
    entities: &mut Vec<Entity>,
    inside_class: bool,
) {
    let kind = node.kind();

    match kind {
        "method_declaration" | "constructor_declaration" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                let is_public = has_java_modifier(node, source, "public");
                entities.push(Entity {
                    name,
                    kind: if inside_class {
                        EntityKind::Method
                    } else {
                        EntityKind::Function
                    },
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public,
                });
            }
        }
        "class_declaration" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                entities.push(Entity {
                    name,
                    kind: EntityKind::Class,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public: has_java_modifier(node, source, "public"),
                });
            }
            // Recurse into class body
            if let Some(body_node) = node.child_by_field_name("body") {
                let mut cursor = body_node.walk();
                for child in body_node.children(&mut cursor) {
                    extract_from_node(source, &child, lang, entities, true);
                }
            }
            return;
        }
        "interface_declaration" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                entities.push(Entity {
                    name,
                    kind: EntityKind::Interface,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public: has_java_modifier(node, source, "public"),
                });
            }
        }
        "enum_declaration" => {
            if let Some(name) = child_by_field_text(node, "name", source) {
                let body = node_text(node, source);
                entities.push(Entity {
                    name,
                    kind: EntityKind::Enum,
                    span: Span::from_node(node),
                    structural_hash: structural_hash(&body, lang),
                    body_hash: body_only_hash(&body, lang),
                    body,
                    is_public: has_java_modifier(node, source, "public"),
                });
            }
        }
        _ => {}
    }

    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        extract_from_node(source, &child, lang, entities, inside_class);
    }
}

fn has_java_modifier(node: &tree_sitter::Node, source: &str, modifier: &str) -> bool {
    let mut cursor = node.walk();
    for child in node.children(&mut cursor) {
        if child.kind() == "modifiers" {
            let text = node_text(&child, source);
            return text.contains(modifier);
        }
    }
    false
}

// --- Helpers ---

fn node_text(node: &tree_sitter::Node, source: &str) -> String {
    source[node.start_byte()..node.end_byte()].to_string()
}

fn child_by_field_text(node: &tree_sitter::Node, field: &str, source: &str) -> Option<String> {
    node.child_by_field_name(field)
        .map(|n| node_text(&n, source))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parser::parse_source;

    #[test]
    fn test_structural_hash_whitespace_invariant() {
        let body1 = "function foo() { return 1; }";
        let body2 = "function  foo()  {\n  return  1;\n}";
        assert_eq!(
            structural_hash(body1, Language::TypeScript),
            structural_hash(body2, Language::TypeScript)
        );
    }

    #[test]
    fn test_structural_hash_comment_invariant() {
        let body1 = "function foo() { return 1; }";
        let body2 = "function foo() { /* comment */ return 1; // line comment\n}";
        assert_eq!(
            structural_hash(body1, Language::TypeScript),
            structural_hash(body2, Language::TypeScript)
        );
    }

    #[test]
    fn test_structural_hash_differs_on_logic() {
        let body1 = "function foo() { return 1; }";
        let body2 = "function foo() { return 2; }";
        assert_ne!(
            structural_hash(body1, Language::TypeScript),
            structural_hash(body2, Language::TypeScript)
        );
    }

    #[test]
    fn test_extract_typescript_entities() {
        let source = r#"
export function greet(name: string): string {
    return `Hello, ${name}`;
}

function helper() {
    return 42;
}

export class UserService {
    getUser(id: number) {
        return { id };
    }
    deleteUser(id: number) {
        // TODO
    }
}
"#;
        let tree = parse_source(source, Language::TypeScript).unwrap();
        let entities = extract_entities(source, &tree, Language::TypeScript);
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"greet"));
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"UserService"));
        assert!(names.contains(&"getUser"));
        assert!(names.contains(&"deleteUser"));
        assert_eq!(entities.len(), 5);
    }

    #[test]
    fn test_extract_python_entities() {
        let source = r#"
def greet(name):
    return f"Hello, {name}"

class UserService:
    def get_user(self, id):
        return {"id": id}

    def _private_method(self):
        pass
"#;
        let tree = parse_source(source, Language::Python).unwrap();
        let entities = extract_entities(source, &tree, Language::Python);
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"greet"));
        assert!(names.contains(&"UserService"));
        assert!(names.contains(&"get_user"));
        assert!(names.contains(&"_private_method"));

        let private = entities
            .iter()
            .find(|e| e.name == "_private_method")
            .unwrap();
        assert!(!private.is_public);
    }

    #[test]
    fn test_extract_rust_entities() {
        let source = r#"
pub fn greet(name: &str) -> String {
    format!("Hello, {}", name)
}

fn helper() -> i32 {
    42
}

pub struct User {
    pub name: String,
}

impl User {
    pub fn new(name: String) -> Self {
        User { name }
    }
}

pub enum Status {
    Active,
    Inactive,
}

pub trait Greeter {
    fn greet(&self) -> String;
}
"#;
        let tree = parse_source(source, Language::Rust).unwrap();
        let entities = extract_entities(source, &tree, Language::Rust);
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"greet"));
        assert!(names.contains(&"helper"));
        assert!(names.contains(&"User"));
        assert!(names.contains(&"new"));
        assert!(names.contains(&"Status"));
        assert!(names.contains(&"Greeter"));
    }

    #[test]
    fn test_extract_java_entities() {
        let source = r#"
public class UserService {
    public String getUser(int id) {
        return "user-" + id;
    }

    private void helper() {
        // internal
    }
}
"#;
        let tree = parse_source(source, Language::Java).unwrap();
        let entities = extract_entities(source, &tree, Language::Java);
        let names: Vec<&str> = entities.iter().map(|e| e.name.as_str()).collect();
        assert!(names.contains(&"UserService"));
        assert!(names.contains(&"getUser"));
        assert!(names.contains(&"helper"));

        let helper = entities.iter().find(|e| e.name == "helper").unwrap();
        assert!(!helper.is_public);
    }

    #[test]
    fn test_python_hash_comment_stripping() {
        let body1 = "def foo():\n    return 1";
        let body2 = "def foo():\n    # comment\n    return 1";
        assert_eq!(
            structural_hash(body1, Language::Python),
            structural_hash(body2, Language::Python)
        );
    }
}
