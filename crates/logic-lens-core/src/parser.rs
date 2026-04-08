use crate::language::Language;
use std::path::Path;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ParseError {
    #[error("failed to read file: {0}")]
    Io(#[from] std::io::Error),
    #[error("unsupported language for file: {0}")]
    UnsupportedLanguage(String),
    #[error("tree-sitter parse failed for {0}")]
    ParseFailed(String),
    #[error("failed to load grammar: {0}")]
    GrammarLoad(String),
}

/// Load the tree-sitter language grammar for the given language.
/// Each grammar is only loaded when needed (lazy per-language).
pub fn load_grammar(lang: Language) -> Result<tree_sitter::Language, ParseError> {
    let grammar = match lang {
        Language::TypeScript => tree_sitter_typescript::LANGUAGE_TYPESCRIPT.into(),
        Language::JavaScript => tree_sitter_javascript::LANGUAGE.into(),
        Language::Python => tree_sitter_python::LANGUAGE.into(),
        Language::Rust => tree_sitter_rust::LANGUAGE.into(),
        Language::Java => tree_sitter_java::LANGUAGE.into(),
    };
    Ok(grammar)
}

/// Parse a source string into a tree-sitter Tree.
pub fn parse_source(source: &str, lang: Language) -> Result<tree_sitter::Tree, ParseError> {
    let grammar = load_grammar(lang)?;
    let mut parser = tree_sitter::Parser::new();
    parser
        .set_language(&grammar)
        .map_err(|e| ParseError::GrammarLoad(e.to_string()))?;
    parser
        .parse(source, None)
        .ok_or_else(|| ParseError::ParseFailed(lang.name().to_string()))
}

/// Parse a file from disk, detecting language from extension.
/// Returns the source text, detected language, and parsed tree.
pub fn parse_file(path: &Path) -> Result<ParsedFile, ParseError> {
    let source = std::fs::read_to_string(path)?;
    let lang = Language::from_path(path)
        .ok_or_else(|| ParseError::UnsupportedLanguage(path.display().to_string()))?;
    let tree = parse_source(&source, lang)?;
    Ok(ParsedFile {
        source,
        language: lang,
        tree,
    })
}

/// A successfully parsed source file.
pub struct ParsedFile {
    pub source: String,
    pub language: Language,
    pub tree: tree_sitter::Tree,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_typescript() {
        let source = "function hello(): string { return 'world'; }";
        let tree = parse_source(source, Language::TypeScript).unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_javascript() {
        let source = "function hello() { return 'world'; }";
        let tree = parse_source(source, Language::JavaScript).unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_python() {
        let source = "def hello():\n    return 'world'\n";
        let tree = parse_source(source, Language::Python).unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_rust() {
        let source = "fn hello() -> &'static str { \"world\" }";
        let tree = parse_source(source, Language::Rust).unwrap();
        assert!(!tree.root_node().has_error());
    }

    #[test]
    fn test_parse_java() {
        let source = "public class Hello { public String greet() { return \"world\"; } }";
        let tree = parse_source(source, Language::Java).unwrap();
        assert!(!tree.root_node().has_error());
    }
}
