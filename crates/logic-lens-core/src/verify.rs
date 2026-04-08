use serde::Serialize;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

/// Result of a build verification.
#[derive(Debug, Serialize)]
pub struct VerifyResult {
    pub success: bool,
    pub command: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub error_lines: Vec<String>,
    pub timed_out: bool,
}

/// Detected project type.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ProjectType {
    Maven,
    Gradle,
    Npm,
    Cargo,
    Go,
    Dotnet,
}

impl ProjectType {
    /// Default build command for this project type.
    pub fn default_command(&self) -> &'static str {
        match self {
            ProjectType::Maven => "mvn compile -q",
            ProjectType::Gradle => "gradle build -q",
            ProjectType::Npm => "npm run build",
            ProjectType::Cargo => "cargo check",
            ProjectType::Go => "go build ./...",
            ProjectType::Dotnet => "dotnet build",
        }
    }
}

/// Detect project type from manifest files by traversing parent directories.
pub fn detect_project_type(start: &Path) -> Option<(ProjectType, PathBuf)> {
    let manifest_map = [
        ("Cargo.toml", ProjectType::Cargo),
        ("pom.xml", ProjectType::Maven),
        ("build.gradle", ProjectType::Gradle),
        ("build.gradle.kts", ProjectType::Gradle),
        ("package.json", ProjectType::Npm),
        ("go.mod", ProjectType::Go),
    ];

    let mut dir = if start.is_file() {
        start.parent().map(Path::to_path_buf)
    } else {
        Some(start.to_path_buf())
    };

    while let Some(d) = dir {
        for (manifest, ptype) in &manifest_map {
            let candidate = d.join(manifest);
            if candidate.is_file() {
                return Some((ptype.clone(), d));
            }
        }
        dir = d.parent().map(Path::to_path_buf);
    }
    None
}

/// Run a build verification command with timeout.
pub fn run_verify(
    command_override: Option<&str>,
    project_dir: &Path,
    timeout: Duration,
) -> VerifyResult {
    let project_type = detect_project_type(project_dir);
    let cmd_str = match command_override {
        Some(c) => c.to_string(),
        None => match &project_type {
            Some((pt, _)) => pt.default_command().to_string(),
            None => {
                return VerifyResult {
                    success: false,
                    command: String::new(),
                    exit_code: None,
                    stdout: String::new(),
                    stderr: "No project type detected".to_string(),
                    error_lines: vec!["No project type detected".to_string()],
                    timed_out: false,
                }
            }
        },
    };

    let working_dir = match &project_type {
        Some((_, dir)) => dir.clone(),
        None => project_dir.to_path_buf(),
    };

    // Split command into program and args
    let parts: Vec<&str> = cmd_str.split_whitespace().collect();
    if parts.is_empty() {
        return VerifyResult {
            success: false,
            command: cmd_str,
            exit_code: None,
            stdout: String::new(),
            stderr: "Empty command".to_string(),
            error_lines: vec!["Empty command".to_string()],
            timed_out: false,
        };
    }

    let program = parts[0];
    let args = &parts[1..];

    let child_result = Command::new(program)
        .args(args)
        .current_dir(&working_dir)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn();

    let mut child = match child_result {
        Ok(c) => c,
        Err(e) => {
            return VerifyResult {
                success: false,
                command: cmd_str,
                exit_code: None,
                stdout: String::new(),
                stderr: format!("Failed to spawn command: {}", e),
                error_lines: vec![format!("Failed to spawn command: {}", e)],
                timed_out: false,
            }
        }
    };

    // Wait with timeout
    let start = std::time::Instant::now();
    loop {
        match child.try_wait() {
            Ok(Some(status)) => {
                let mut stdout = String::new();
                let mut stderr = String::new();
                if let Some(mut out) = child.stdout.take() {
                    let _ = out.read_to_string(&mut stdout);
                }
                if let Some(mut err) = child.stderr.take() {
                    let _ = err.read_to_string(&mut stderr);
                }

                let error_lines = extract_error_lines(&stdout, &stderr);

                return VerifyResult {
                    success: status.success(),
                    command: cmd_str,
                    exit_code: status.code(),
                    stdout,
                    stderr,
                    error_lines,
                    timed_out: false,
                };
            }
            Ok(None) => {
                if start.elapsed() >= timeout {
                    let _ = child.kill();
                    return VerifyResult {
                        success: false,
                        command: cmd_str,
                        exit_code: None,
                        stdout: String::new(),
                        stderr: format!("Command timed out after {}s", timeout.as_secs()),
                        error_lines: vec![format!(
                            "Command timed out after {}s",
                            timeout.as_secs()
                        )],
                        timed_out: true,
                    };
                }
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(e) => {
                return VerifyResult {
                    success: false,
                    command: cmd_str,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: format!("Error waiting for process: {}", e),
                    error_lines: vec![format!("Error waiting for process: {}", e)],
                    timed_out: false,
                };
            }
        }
    }
}

/// Extract error-relevant lines from build output.
fn extract_error_lines(stdout: &str, stderr: &str) -> Vec<String> {
    let mut errors = Vec::new();
    let error_patterns = [
        "error",
        "Error",
        "ERROR",
        "FAILED",
        "failed",
        "FAILURE",
        "cannot find",
        "not found",
        "undefined",
        "unresolved",
    ];

    for line in stderr.lines().chain(stdout.lines()) {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        if error_patterns.iter().any(|p| trimmed.contains(p)) {
            errors.push(trimmed.to_string());
        }
    }
    errors
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_detect_cargo_project() {
        let dir = TempDir::new().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"test\"\n",
        )
        .unwrap();

        let result = detect_project_type(dir.path());
        assert!(result.is_some());
        let (ptype, _) = result.unwrap();
        assert_eq!(ptype, ProjectType::Cargo);
    }

    #[test]
    fn test_detect_npm_project() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();

        let result = detect_project_type(dir.path());
        assert!(result.is_some());
        let (ptype, _) = result.unwrap();
        assert_eq!(ptype, ProjectType::Npm);
    }

    #[test]
    fn test_detect_maven_project() {
        let dir = TempDir::new().unwrap();
        std::fs::write(dir.path().join("pom.xml"), "<project></project>").unwrap();

        let result = detect_project_type(dir.path());
        assert!(result.is_some());
        let (ptype, _) = result.unwrap();
        assert_eq!(ptype, ProjectType::Maven);
    }

    #[test]
    fn test_no_project_detected() {
        let dir = TempDir::new().unwrap();
        let result = detect_project_type(dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_run_verify_success() {
        let dir = TempDir::new().unwrap();
        let result = run_verify(Some("echo hello"), dir.path(), Duration::from_secs(10));
        assert!(result.success);
        assert!(result.stdout.contains("hello"));
        assert!(!result.timed_out);
    }

    #[test]
    fn test_run_verify_failure() {
        let dir = TempDir::new().unwrap();
        let result = run_verify(Some("false"), dir.path(), Duration::from_secs(10));
        assert!(!result.success);
    }

    #[test]
    fn test_extract_error_lines() {
        let stdout = "compiling...\nerror[E0308]: mismatched types\n  --> src/main.rs:5:5\n";
        let stderr = "";
        let errors = extract_error_lines(stdout, stderr);
        assert!(errors.iter().any(|l| l.contains("error[E0308]")));
    }
}
