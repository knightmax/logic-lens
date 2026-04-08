use std::path::Path;

/// Supported programming languages for semantic analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Language {
    TypeScript,
    JavaScript,
    Python,
    Rust,
    Java,
}

impl Language {
    /// Detect language from a file extension.
    /// Returns `None` for unsupported languages.
    pub fn from_extension(ext: &str) -> Option<Self> {
        match ext {
            "ts" | "tsx" => Some(Language::TypeScript),
            "js" | "jsx" | "mjs" | "cjs" => Some(Language::JavaScript),
            "py" | "pyi" => Some(Language::Python),
            "rs" => Some(Language::Rust),
            "java" => Some(Language::Java),
            _ => None,
        }
    }

    /// Detect language from a file path.
    pub fn from_path(path: &Path) -> Option<Self> {
        path.extension()
            .and_then(|ext| ext.to_str())
            .and_then(Self::from_extension)
    }

    /// Human-readable name.
    pub fn name(&self) -> &'static str {
        match self {
            Language::TypeScript => "TypeScript",
            Language::JavaScript => "JavaScript",
            Language::Python => "Python",
            Language::Rust => "Rust",
            Language::Java => "Java",
        }
    }
}

impl std::fmt::Display for Language {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    #[test]
    fn test_extension_mapping() {
        assert_eq!(Language::from_extension("ts"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("tsx"), Some(Language::TypeScript));
        assert_eq!(Language::from_extension("js"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension("jsx"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension("mjs"), Some(Language::JavaScript));
        assert_eq!(Language::from_extension("py"), Some(Language::Python));
        assert_eq!(Language::from_extension("rs"), Some(Language::Rust));
        assert_eq!(Language::from_extension("java"), Some(Language::Java));
        assert_eq!(Language::from_extension("go"), None);
        assert_eq!(Language::from_extension("hs"), None);
    }

    #[test]
    fn test_from_path() {
        assert_eq!(
            Language::from_path(&PathBuf::from("src/main.ts")),
            Some(Language::TypeScript)
        );
        assert_eq!(
            Language::from_path(&PathBuf::from("handler.py")),
            Some(Language::Python)
        );
        assert_eq!(Language::from_path(&PathBuf::from("README.md")), None);
    }
}
