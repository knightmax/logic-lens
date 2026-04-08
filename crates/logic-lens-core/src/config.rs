use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// Top-level configuration loaded from `logic-lens.toml`.
#[derive(Debug, Deserialize, Default, Clone)]
#[serde(default)]
pub struct Config {
    /// Rule severity overrides: rule name → severity.
    pub rules: HashMap<String, RuleSeverity>,

    /// Output configuration.
    pub output: OutputConfig,

    /// Shell integration (verify) configuration.
    pub verify: VerifyConfig,

    /// Custom rules directory (default: `.logic-lens/rules/`).
    pub rules_dir: Option<PathBuf>,
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum RuleSeverity {
    Error,
    Warning,
    Off,
}

#[derive(Debug, Deserialize, Clone)]
#[serde(default)]
pub struct OutputConfig {
    /// Default output format.
    pub format: OutputFormat,
}

impl Default for OutputConfig {
    fn default() -> Self {
        Self {
            format: OutputFormat::Json,
        }
    }
}

#[derive(Debug, Deserialize, Clone, Copy, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum OutputFormat {
    Json,
    Terminal,
    Markdown,
}

#[derive(Debug, Deserialize, Clone, Default)]
#[serde(default)]
pub struct VerifyConfig {
    /// Custom build command override.
    pub command: Option<String>,

    /// Timeout in seconds (default: 120).
    pub timeout: Option<u64>,
}

impl Config {
    /// Load configuration from a TOML file at the given path.
    pub fn from_file(path: &Path) -> Result<Self, ConfigError> {
        let content = std::fs::read_to_string(path).map_err(ConfigError::Io)?;
        toml::from_str(&content).map_err(ConfigError::Parse)
    }

    /// Discover and load `logic-lens.toml` by traversing parent directories
    /// from the given starting path. Returns default config if not found.
    pub fn discover(start: &Path) -> Self {
        let mut dir = if start.is_file() {
            start.parent().map(Path::to_path_buf)
        } else {
            Some(start.to_path_buf())
        };

        while let Some(d) = dir {
            let candidate = d.join("logic-lens.toml");
            if candidate.is_file() {
                return Self::from_file(&candidate).unwrap_or_default();
            }
            dir = d.parent().map(Path::to_path_buf);
        }

        Self::default()
    }

    /// Get the effective rules directory.
    pub fn rules_directory(&self) -> PathBuf {
        self.rules_dir
            .clone()
            .unwrap_or_else(|| PathBuf::from(".logic-lens/rules"))
    }

    /// Get the effective verify timeout in seconds.
    pub fn verify_timeout(&self) -> u64 {
        self.verify.timeout.unwrap_or(120)
    }
}

#[derive(Debug)]
pub enum ConfigError {
    Io(std::io::Error),
    Parse(toml::de::Error),
}

impl std::fmt::Display for ConfigError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ConfigError::Io(e) => write!(f, "failed to read config file: {}", e),
            ConfigError::Parse(e) => write!(f, "failed to parse config file: {}", e),
        }
    }
}

impl std::error::Error for ConfigError {}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_config() {
        let config = Config::default();
        assert_eq!(config.output.format, OutputFormat::Json);
        assert!(config.rules.is_empty());
        assert_eq!(config.verify_timeout(), 120);
        assert_eq!(config.rules_directory(), PathBuf::from(".logic-lens/rules"));
    }

    #[test]
    fn test_parse_config() {
        let toml_str = r#"
rules_dir = "custom/rules"

[rules]
placeholder-detection = "off"
missing-error-handling = "error"

[output]
format = "terminal"

[verify]
command = "mvnd compile -T4"
timeout = 60
"#;
        let config: Config = toml::from_str(toml_str).unwrap();
        assert_eq!(config.output.format, OutputFormat::Terminal);
        assert_eq!(
            config.rules.get("placeholder-detection"),
            Some(&RuleSeverity::Off)
        );
        assert_eq!(
            config.rules.get("missing-error-handling"),
            Some(&RuleSeverity::Error)
        );
        assert_eq!(config.verify.command.as_deref(), Some("mvnd compile -T4"));
        assert_eq!(config.verify_timeout(), 60);
        assert_eq!(config.rules_directory(), PathBuf::from("custom/rules"));
    }
}
