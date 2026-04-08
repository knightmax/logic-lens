use crate::language::Language;
use crate::lint::{Finding, Severity};
use std::collections::HashSet;
use std::path::{Path, PathBuf};

/// Result of hallucination detection scan.
pub struct HallucinationResult {
    pub findings: Vec<Finding>,
    pub manifest_found: bool,
}

/// Extract import statements from source code based on language.
pub fn extract_imports(source: &str, lang: Language) -> Vec<ImportStatement> {
    match lang {
        Language::TypeScript | Language::JavaScript => extract_js_imports(source),
        Language::Python => extract_python_imports(source),
        Language::Rust => extract_rust_imports(source),
        Language::Java => extract_java_imports(source),
    }
}

#[derive(Debug, Clone)]
pub struct ImportStatement {
    pub module: String,
    pub line: usize,
    pub is_relative: bool,
}

// --- Import Extraction per Language ---

fn extract_js_imports(source: &str) -> Vec<ImportStatement> {
    let mut imports = Vec::new();
    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        // import ... from 'module' OR import 'module' (side-effect)
        if let Some(after_import) = trimmed.strip_prefix("import ") {
            if let Some(module) = extract_js_module_name(trimmed) {
                let is_relative = module.starts_with('.') || module.starts_with('/');
                imports.push(ImportStatement {
                    module,
                    line: line_idx + 1,
                    is_relative,
                });
            } else if let Some(module) = extract_quoted_string(after_import.trim()) {
                // Side-effect import: import 'module'
                let is_relative = module.starts_with('.') || module.starts_with('/');
                imports.push(ImportStatement {
                    module,
                    line: line_idx + 1,
                    is_relative,
                });
            }
        }
        // const x = require('module')
        if trimmed.contains("require(") {
            if let Some(module) = extract_require_module(trimmed) {
                let is_relative = module.starts_with('.') || module.starts_with('/');
                imports.push(ImportStatement {
                    module,
                    line: line_idx + 1,
                    is_relative,
                });
            }
        }
    }
    imports
}

fn extract_js_module_name(line: &str) -> Option<String> {
    // Match: from 'module' or from "module"
    let from_idx = line.find(" from ")?;
    let after = line[from_idx + 6..].trim();
    extract_quoted_string(after)
}

fn extract_require_module(line: &str) -> Option<String> {
    let req_idx = line.find("require(")?;
    let after = line[req_idx + 8..].trim();
    extract_quoted_string(after)
}

fn extract_quoted_string(s: &str) -> Option<String> {
    let quote = s.chars().next()?;
    if quote != '\'' && quote != '"' {
        return None;
    }
    let end = s[1..].find(quote)?;
    Some(s[1..1 + end].to_string())
}

fn extract_python_imports(source: &str) -> Vec<ImportStatement> {
    let mut imports = Vec::new();
    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(after_import) = trimmed.strip_prefix("import ") {
            // import module or import module as alias
            let module = after_import
                .split_whitespace()
                .next()
                .unwrap_or("")
                .trim_end_matches(',');
            let base = module.split('.').next().unwrap_or(module);
            let is_relative = false; // Python relative imports use "from ."
            imports.push(ImportStatement {
                module: base.to_string(),
                line: line_idx + 1,
                is_relative,
            });
        } else if let Some(rest) = trimmed.strip_prefix("from ") {
            // from module import ...
            let module = rest.split_whitespace().next().unwrap_or("");
            let is_relative = module.starts_with('.');
            let base = module
                .trim_start_matches('.')
                .split('.')
                .next()
                .unwrap_or(module);
            imports.push(ImportStatement {
                module: base.to_string(),
                line: line_idx + 1,
                is_relative,
            });
        }
    }
    imports
}

fn extract_rust_imports(source: &str) -> Vec<ImportStatement> {
    let mut imports = Vec::new();
    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(after_use) = trimmed.strip_prefix("use ") {
            let rest = after_use.trim_end_matches(';');
            let crate_name = rest.split("::").next().unwrap_or(rest);
            let is_relative =
                crate_name == "crate" || crate_name == "self" || crate_name == "super";
            imports.push(ImportStatement {
                module: crate_name.to_string(),
                line: line_idx + 1,
                is_relative,
            });
        }
    }
    imports
}

fn extract_java_imports(source: &str) -> Vec<ImportStatement> {
    let mut imports = Vec::new();
    for (line_idx, line) in source.lines().enumerate() {
        let trimmed = line.trim();
        if let Some(after_import) = trimmed.strip_prefix("import ") {
            let rest = after_import.trim_end_matches(';').trim();
            let rest = rest.strip_prefix("static ").unwrap_or(rest);
            imports.push(ImportStatement {
                module: rest.to_string(),
                line: line_idx + 1,
                is_relative: false,
            });
        }
    }
    imports
}

// --- Manifest Detection & Parsing ---

/// Discover the nearest manifest file by traversing parent directories.
pub fn find_manifest(start: &Path, lang: Language) -> Option<PathBuf> {
    let manifest_names = match lang {
        Language::TypeScript | Language::JavaScript => vec!["package.json"],
        Language::Python => vec!["pyproject.toml", "requirements.txt", "setup.py"],
        Language::Rust => vec!["Cargo.toml"],
        Language::Java => vec!["pom.xml", "build.gradle", "build.gradle.kts"],
    };

    let mut dir = if start.is_file() {
        start.parent().map(Path::to_path_buf)
    } else {
        Some(start.to_path_buf())
    };

    while let Some(d) = dir {
        for name in &manifest_names {
            let candidate = d.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
}

/// Parse known dependencies from a manifest file.
pub fn parse_manifest_deps(path: &Path) -> Result<HashSet<String>, String> {
    let filename = path.file_name().and_then(|n| n.to_str()).unwrap_or("");

    match filename {
        "package.json" => parse_package_json(path),
        "Cargo.toml" => parse_cargo_toml(path),
        "pyproject.toml" => parse_pyproject_toml(path),
        "pom.xml" => parse_pom_xml(path),
        _ => Ok(HashSet::new()),
    }
}

fn parse_package_json(path: &Path) -> Result<HashSet<String>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let pkg: serde_json::Value = serde_json::from_str(&content).map_err(|e| e.to_string())?;
    let mut deps = HashSet::new();
    for section in ["dependencies", "devDependencies", "peerDependencies"] {
        if let Some(obj) = pkg.get(section).and_then(|v| v.as_object()) {
            for key in obj.keys() {
                deps.insert(key.clone());
            }
        }
    }
    Ok(deps)
}

fn parse_cargo_toml(path: &Path) -> Result<HashSet<String>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let cargo: toml::Value = toml::from_str(&content).map_err(|e| e.to_string())?;
    let mut deps = HashSet::new();
    for section in ["dependencies", "dev-dependencies", "build-dependencies"] {
        if let Some(table) = cargo.get(section).and_then(|v| v.as_table()) {
            for key in table.keys() {
                deps.insert(key.clone());
            }
        }
    }
    // Also check workspace.dependencies
    if let Some(ws) = cargo.get("workspace") {
        if let Some(table) = ws.get("dependencies").and_then(|v| v.as_table()) {
            for key in table.keys() {
                deps.insert(key.clone());
            }
        }
    }
    Ok(deps)
}

fn parse_pyproject_toml(path: &Path) -> Result<HashSet<String>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let pyproject: toml::Value = toml::from_str(&content).map_err(|e| e.to_string())?;
    let mut deps = HashSet::new();
    // PEP 621: [project.dependencies]
    if let Some(project) = pyproject.get("project") {
        if let Some(dep_list) = project.get("dependencies").and_then(|v| v.as_array()) {
            for dep in dep_list {
                if let Some(s) = dep.as_str() {
                    // Parse "package>=1.0" → "package"
                    let name = s
                        .split(|c: char| !c.is_alphanumeric() && c != '-' && c != '_')
                        .next()
                        .unwrap_or(s);
                    deps.insert(name.to_lowercase().replace('-', "_"));
                }
            }
        }
    }
    // Poetry: [tool.poetry.dependencies]
    if let Some(tool) = pyproject.get("tool") {
        if let Some(poetry) = tool.get("poetry") {
            if let Some(table) = poetry.get("dependencies").and_then(|v| v.as_table()) {
                for key in table.keys() {
                    deps.insert(key.to_lowercase().replace('-', "_"));
                }
            }
        }
    }
    Ok(deps)
}

fn parse_pom_xml(path: &Path) -> Result<HashSet<String>, String> {
    let content = std::fs::read_to_string(path).map_err(|e| e.to_string())?;
    let mut deps = HashSet::new();

    // Simple regex-like extraction for groupId and artifactId from <dependency> blocks
    let mut in_dependency = false;
    let mut current_group_id = String::new();
    let mut current_artifact_id = String::new();

    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("<dependency>") {
            in_dependency = true;
            current_group_id.clear();
            current_artifact_id.clear();
        }
        if in_dependency {
            if let Some(val) = extract_xml_value(trimmed, "groupId") {
                current_group_id = val;
            }
            if let Some(val) = extract_xml_value(trimmed, "artifactId") {
                current_artifact_id = val;
            }
        }
        if trimmed.starts_with("</dependency>") && in_dependency {
            in_dependency = false;
            if !current_group_id.is_empty() {
                deps.insert(format!("{}:{}", current_group_id, current_artifact_id));
                deps.insert(current_group_id.clone());
                deps.insert(current_artifact_id.clone());
            }
        }
    }
    Ok(deps)
}

fn extract_xml_value(line: &str, tag: &str) -> Option<String> {
    let open = format!("<{}>", tag);
    let close = format!("</{}>", tag);
    if let Some(start) = line.find(&open) {
        if let Some(end) = line.find(&close) {
            return Some(line[start + open.len()..end].to_string());
        }
    }
    None
}

// --- Standard Library Detection ---

fn is_std_library(module: &str, lang: Language) -> bool {
    match lang {
        Language::Python => PYTHON_STDLIB.contains(&module),
        Language::Java => {
            module.starts_with("java.")
                || module.starts_with("javax.")
                || module.starts_with("sun.")
        }
        Language::Rust => {
            module == "std" || module == "core" || module == "alloc" || module == "proc_macro"
        }
        Language::TypeScript | Language::JavaScript => JS_BUILTINS.contains(&module),
    }
}

const PYTHON_STDLIB: &[&str] = &[
    "abc",
    "argparse",
    "ast",
    "asyncio",
    "base64",
    "bisect",
    "builtins",
    "calendar",
    "cgi",
    "codecs",
    "collections",
    "contextlib",
    "copy",
    "csv",
    "ctypes",
    "dataclasses",
    "datetime",
    "decimal",
    "difflib",
    "dis",
    "email",
    "enum",
    "errno",
    "fileinput",
    "fnmatch",
    "fractions",
    "functools",
    "gc",
    "getpass",
    "glob",
    "gzip",
    "hashlib",
    "heapq",
    "hmac",
    "html",
    "http",
    "importlib",
    "inspect",
    "io",
    "itertools",
    "json",
    "keyword",
    "linecache",
    "locale",
    "logging",
    "math",
    "mimetypes",
    "multiprocessing",
    "numbers",
    "operator",
    "os",
    "pathlib",
    "pickle",
    "platform",
    "pprint",
    "pdb",
    "queue",
    "random",
    "re",
    "readline",
    "reprlib",
    "secrets",
    "select",
    "shelve",
    "shlex",
    "shutil",
    "signal",
    "site",
    "smtplib",
    "socket",
    "sqlite3",
    "ssl",
    "statistics",
    "string",
    "struct",
    "subprocess",
    "sys",
    "syslog",
    "tempfile",
    "textwrap",
    "threading",
    "time",
    "timeit",
    "tkinter",
    "token",
    "tokenize",
    "tomllib",
    "traceback",
    "types",
    "typing",
    "unicodedata",
    "unittest",
    "urllib",
    "uuid",
    "venv",
    "warnings",
    "weakref",
    "webbrowser",
    "xml",
    "xmlrpc",
    "zipfile",
    "zipimport",
    "zlib",
];

const JS_BUILTINS: &[&str] = &[
    "assert",
    "buffer",
    "child_process",
    "cluster",
    "console",
    "constants",
    "crypto",
    "dgram",
    "dns",
    "domain",
    "events",
    "fs",
    "http",
    "http2",
    "https",
    "net",
    "os",
    "path",
    "perf_hooks",
    "process",
    "punycode",
    "querystring",
    "readline",
    "repl",
    "stream",
    "string_decoder",
    "sys",
    "timers",
    "tls",
    "tty",
    "url",
    "util",
    "v8",
    "vm",
    "worker_threads",
    "zlib",
    "node:assert",
    "node:buffer",
    "node:child_process",
    "node:crypto",
    "node:dns",
    "node:events",
    "node:fs",
    "node:http",
    "node:https",
    "node:net",
    "node:os",
    "node:path",
    "node:process",
    "node:querystring",
    "node:readline",
    "node:stream",
    "node:timers",
    "node:tls",
    "node:url",
    "node:util",
    "node:vm",
    "node:worker_threads",
    "node:zlib",
];

// --- Hallucination Check ---

/// Run hallucination detection: cross-reference imports against manifest dependencies.
pub fn check_hallucinated_imports(
    source: &str,
    lang: Language,
    file_path: &Path,
) -> HallucinationResult {
    let imports = extract_imports(source, lang);
    let manifest_path = find_manifest(file_path, lang);

    let manifest_found = manifest_path.is_some();
    if !manifest_found {
        return HallucinationResult {
            findings: vec![],
            manifest_found: false,
        };
    }

    let deps = match manifest_path.and_then(|p| parse_manifest_deps(&p).ok()) {
        Some(deps) => deps,
        None => {
            return HallucinationResult {
                findings: vec![],
                manifest_found: true,
            }
        }
    };

    let mut findings = Vec::new();
    for import in &imports {
        // Skip relative imports
        if import.is_relative {
            continue;
        }
        // Skip standard library
        if is_std_library(&import.module, lang) {
            continue;
        }
        // For JS/TS: extract the package name (handle scoped packages like @foo/bar)
        let pkg_name = normalize_package_name(&import.module, lang);

        if !deps.contains(&pkg_name) {
            findings.push(Finding {
                rule: "hallucinated-import".to_string(),
                severity: Severity::Warning,
                message: format!(
                    "Import `{}` not found in project dependencies — possibly hallucinated",
                    import.module
                ),
                file: file_path.display().to_string(),
                line: import.line,
                column: 1,
            });
        }
    }

    HallucinationResult {
        findings,
        manifest_found,
    }
}

fn normalize_package_name(module: &str, lang: Language) -> String {
    match lang {
        Language::TypeScript | Language::JavaScript => {
            // Handle scoped packages: @scope/package → @scope/package
            // Handle subpath imports: lodash/merge → lodash
            if module.starts_with('@') {
                // @scope/package/subpath → @scope/package
                let parts: Vec<&str> = module.splitn(3, '/').collect();
                if parts.len() >= 2 {
                    format!("{}/{}", parts[0], parts[1])
                } else {
                    module.to_string()
                }
            } else {
                // package/subpath → package
                module.split('/').next().unwrap_or(module).to_string()
            }
        }
        Language::Python => {
            // Normalize - to _ for Python package names
            module.to_lowercase().replace('-', "_")
        }
        _ => module.to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn test_extract_js_imports() {
        let source = r#"
import React from 'react';
import { useState } from 'react';
import './styles.css';
import lodash from 'lodash';
const fs = require('fs');
"#;
        let imports = extract_imports(source, Language::JavaScript);
        assert_eq!(imports.len(), 5);
        assert!(imports.iter().any(|i| i.module == "react"));
        assert!(imports.iter().any(|i| i.module == "lodash"));
        assert!(imports.iter().any(|i| i.module == "fs"));
        assert!(imports.iter().any(|i| i.is_relative)); // ./styles.css
    }

    #[test]
    fn test_extract_python_imports() {
        let source = r#"
import os
import json
from pathlib import Path
from requests import get
from . import local_module
"#;
        let imports = extract_imports(source, Language::Python);
        assert!(imports.iter().any(|i| i.module == "os"));
        assert!(imports.iter().any(|i| i.module == "json"));
        assert!(imports.iter().any(|i| i.module == "pathlib"));
        assert!(imports.iter().any(|i| i.module == "requests"));
        assert!(imports.iter().any(|i| i.is_relative)); // from .
    }

    #[test]
    fn test_extract_rust_imports() {
        let source = r#"
use std::collections::HashMap;
use serde::Serialize;
use crate::config::Config;
"#;
        let imports = extract_imports(source, Language::Rust);
        assert!(imports.iter().any(|i| i.module == "std"));
        assert!(imports.iter().any(|i| i.module == "serde"));
        assert!(imports.iter().any(|i| i.module == "crate" && i.is_relative));
    }

    #[test]
    fn test_extract_java_imports() {
        let source = r#"
import java.util.List;
import java.util.Map;
import com.example.MyClass;
"#;
        let imports = extract_imports(source, Language::Java);
        assert!(imports.iter().any(|i| i.module == "java.util.List"));
        assert!(imports.iter().any(|i| i.module == "com.example.MyClass"));
    }

    #[test]
    fn test_package_json_parsing() {
        let dir = TempDir::new().unwrap();
        let pkg_path = dir.path().join("package.json");
        let mut file = std::fs::File::create(&pkg_path).unwrap();
        write!(
            file,
            r#"{{"dependencies": {{"react": "^18.0.0", "lodash": "^4.0.0"}}, "devDependencies": {{"typescript": "^5.0.0"}}}}"#
        )
        .unwrap();

        let deps = parse_manifest_deps(&pkg_path).unwrap();
        assert!(deps.contains("react"));
        assert!(deps.contains("lodash"));
        assert!(deps.contains("typescript"));
    }

    #[test]
    fn test_cargo_toml_parsing() {
        let dir = TempDir::new().unwrap();
        let cargo_path = dir.path().join("Cargo.toml");
        let mut file = std::fs::File::create(&cargo_path).unwrap();
        write!(
            file,
            r#"[dependencies]
serde = "1"
toml = "0.8"

[dev-dependencies]
tempfile = "3"
"#
        )
        .unwrap();

        let deps = parse_manifest_deps(&cargo_path).unwrap();
        assert!(deps.contains("serde"));
        assert!(deps.contains("toml"));
        assert!(deps.contains("tempfile"));
    }

    #[test]
    fn test_hallucination_detection() {
        let dir = TempDir::new().unwrap();

        // Create package.json
        let pkg_path = dir.path().join("package.json");
        let mut file = std::fs::File::create(&pkg_path).unwrap();
        write!(file, r#"{{"dependencies": {{"react": "^18.0.0"}}}}"#).unwrap();

        // Create source file with a hallucinated import
        let src_path = dir.path().join("app.ts");
        let source = "import React from 'react';\nimport phantom from 'phantom-lib';\n";
        std::fs::write(&src_path, source).unwrap();

        let result = check_hallucinated_imports(source, Language::TypeScript, &src_path);
        assert!(result.manifest_found);
        assert_eq!(result.findings.len(), 1);
        assert!(result.findings[0].message.contains("phantom-lib"));
    }

    #[test]
    fn test_std_library_not_flagged() {
        let dir = TempDir::new().unwrap();

        let pkg_path = dir.path().join("package.json");
        let mut file = std::fs::File::create(&pkg_path).unwrap();
        write!(file, r#"{{"dependencies": {{}}}}"#).unwrap();

        let src_path = dir.path().join("app.ts");
        let source = "import fs from 'fs';\nimport path from 'path';\n";
        std::fs::write(&src_path, source).unwrap();

        let result = check_hallucinated_imports(source, Language::TypeScript, &src_path);
        assert!(result.findings.is_empty());
    }

    #[test]
    fn test_no_manifest_skips_detection() {
        let dir = TempDir::new().unwrap();
        let src_path = dir.path().join("app.ts");
        let source = "import phantom from 'phantom-lib';\n";
        std::fs::write(&src_path, source).unwrap();

        let result = check_hallucinated_imports(source, Language::TypeScript, &src_path);
        assert!(!result.manifest_found);
        assert!(result.findings.is_empty());
    }
}
