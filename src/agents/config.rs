//! Agent configuration types and TOML parsing.
//!
//! This module provides types for loading and managing agent configurations
//! from TOML files, including support for global and per-project configs.

use super::fallback::FallbackConfig;
use super::parser::JsonParserType;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Default agents.toml template embedded at compile time.
pub const DEFAULT_AGENTS_TOML: &str = include_str!("../../examples/agents.toml");

/// Get the global config directory for Ralph.
///
/// Returns `~/.config/ralph` on Unix and `%APPDATA%\ralph` on Windows.
/// Returns None if the home directory cannot be determined.
pub fn global_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|d| d.join("ralph"))
}

/// Get the global agents.toml path.
///
/// Returns `~/.config/ralph/agents.toml` on Unix.
pub fn global_agents_config_path() -> Option<PathBuf> {
    global_config_dir().map(|d| d.join("agents.toml"))
}

/// Config source for tracking where config was loaded from.
#[derive(Debug, Clone)]
pub struct ConfigSource {
    pub path: PathBuf,
    pub agents_loaded: usize,
}

/// Agent capabilities.
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Base command to run the agent.
    pub cmd: String,
    /// Output-format flag (JSON streaming, text mode, etc.).
    pub output_flag: String,
    /// Flag for autonomous mode (no prompts).
    pub yolo_flag: String,
    /// Flag for verbose output.
    pub verbose_flag: String,
    /// Whether the agent can run git commit.
    pub can_commit: bool,
    /// Which JSON parser to use for this agent's output.
    pub json_parser: JsonParserType,
    /// Model/provider flag for agents that support model selection.
    pub model_flag: Option<String>,
    /// Print/non-interactive mode flag (e.g., "-p" for Claude/CCS).
    pub print_flag: String,
    /// Display name for UI/logging (e.g., "ccs-glm" instead of raw agent name).
    /// If None, the agent name from the registry is used.
    pub display_name: Option<String>,
}

impl AgentConfig {
    /// Build full command string with specified flags.
    pub fn build_cmd(&self, output: bool, yolo: bool, verbose: bool) -> String {
        self.build_cmd_with_model(output, yolo, verbose, None)
    }

    /// Build full command string with specified flags and optional model override.
    pub fn build_cmd_with_model(
        &self,
        output: bool,
        yolo: bool,
        verbose: bool,
        model_override: Option<&str>,
    ) -> String {
        let mut parts = vec![self.cmd.clone()];

        // Add print flag first (for CCS that needs -p after the profile name)
        if !self.print_flag.is_empty() {
            parts.push(self.print_flag.clone());
        }

        if output && !self.output_flag.is_empty() {
            parts.push(self.output_flag.clone());
        }
        if yolo && !self.yolo_flag.is_empty() {
            parts.push(self.yolo_flag.clone());
        }

        // Claude CLI requires --verbose when using --output-format=stream-json
        let needs_verbose = verbose || self.requires_verbose_for_json(output);

        if needs_verbose && !self.verbose_flag.is_empty() {
            parts.push(self.verbose_flag.clone());
        }

        // Add model flag: runtime override takes precedence over config
        let effective_model = model_override.or(self.model_flag.as_deref());
        if let Some(model) = effective_model {
            if !model.is_empty() {
                parts.push(model.to_string());
            }
        }

        parts.join(" ")
    }

    /// Check if this agent requires --verbose when JSON output is enabled.
    fn requires_verbose_for_json(&self, json_enabled: bool) -> bool {
        if !json_enabled || !self.output_flag.contains("stream-json") {
            return false;
        }

        // Both `claude` and CCS (`ccs ...`) require verbose mode when using stream-json output.
        // CCS is a wrapper around the Claude CLI and inherits its stream-json quirks.
        let base = self.cmd.split_whitespace().next().unwrap_or("");
        matches!(base, "claude" | "ccs")
    }
}

/// TOML configuration for an agent (for deserialization).
#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfigToml {
    /// Base command to run the agent.
    pub cmd: String,
    /// Output-format flag (optional, defaults to empty).
    #[serde(default)]
    pub output_flag: String,
    /// Flag for autonomous mode (optional, defaults to empty).
    #[serde(default)]
    pub yolo_flag: String,
    /// Flag for verbose output (optional, defaults to empty).
    #[serde(default)]
    pub verbose_flag: String,
    /// Whether the agent can run git commit (optional, defaults to true).
    #[serde(default = "default_can_commit")]
    pub can_commit: bool,
    /// Which JSON parser to use (optional, defaults to "generic").
    #[serde(default)]
    pub json_parser: String,
    /// Model/provider flag for model selection.
    #[serde(default)]
    pub model_flag: Option<String>,
    /// Print/non-interactive mode flag (optional, defaults to empty).
    #[serde(default)]
    pub print_flag: String,
    /// Display name for UI/logging (optional, e.g., "My Custom Agent" instead of registry name).
    #[serde(default)]
    pub display_name: Option<String>,
}

fn default_can_commit() -> bool {
    true
}

impl From<AgentConfigToml> for AgentConfig {
    fn from(toml: AgentConfigToml) -> Self {
        AgentConfig {
            cmd: toml.cmd,
            output_flag: toml.output_flag,
            yolo_flag: toml.yolo_flag,
            verbose_flag: toml.verbose_flag,
            can_commit: toml.can_commit,
            json_parser: JsonParserType::parse(&toml.json_parser),
            model_flag: toml.model_flag,
            print_flag: toml.print_flag,
            display_name: toml.display_name,
        }
    }
}

/// Root TOML configuration structure.
#[derive(Debug, Clone, Deserialize)]
pub struct AgentsConfigFile {
    /// Map of agent name to configuration.
    #[serde(default)]
    pub agents: HashMap<String, AgentConfigToml>,
    /// Agent chain configuration (preferred agents + fallbacks).
    #[serde(default, rename = "agent_chain")]
    pub fallback: FallbackConfig,
}

/// Error type for agent configuration loading.
#[derive(Debug, thiserror::Error)]
pub enum AgentConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] io::Error),
    #[error("Failed to parse TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Built-in agents.toml template is invalid TOML: {0}")]
    DefaultTemplateToml(toml::de::Error),
}

/// Result of checking/initializing the agents config file.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigInitResult {
    /// Config file already exists, no action taken.
    AlreadyExists,
    /// Config file was just created from template.
    Created,
}

impl AgentsConfigFile {
    /// Load agents config from a file, returning None if file doesn't exist.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Option<Self>, AgentConfigError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(path)?;
        let config: AgentsConfigFile = toml::from_str(&contents)?;
        Ok(Some(config))
    }

    /// Ensure agents config file exists, creating it from template if needed.
    pub fn ensure_config_exists<P: AsRef<Path>>(path: P) -> io::Result<ConfigInitResult> {
        let path = path.as_ref();

        if path.exists() {
            return Ok(ConfigInitResult::AlreadyExists);
        }

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }

        // Write the default template
        fs::write(path, DEFAULT_AGENTS_TOML)?;

        Ok(ConfigInitResult::Created)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_build_cmd() {
        let agent = AgentConfig {
            cmd: "testbot run".to_string(),
            output_flag: "--json".to_string(),
            yolo_flag: "--yes".to_string(),
            verbose_flag: "--verbose".to_string(),
            can_commit: true,
            json_parser: JsonParserType::Generic,
            model_flag: None,
            print_flag: String::new(),
            display_name: None,
        };

        let cmd = agent.build_cmd(true, true, true);
        assert!(cmd.contains("testbot run"));
        assert!(cmd.contains("--json"));
        assert!(cmd.contains("--yes"));
        assert!(cmd.contains("--verbose"));
    }

    #[test]
    fn test_agent_config_from_toml() {
        let toml = AgentConfigToml {
            cmd: "myagent run".to_string(),
            output_flag: "--json".to_string(),
            yolo_flag: "--auto".to_string(),
            verbose_flag: "--verbose".to_string(),
            can_commit: false,
            json_parser: "claude".to_string(),
            model_flag: Some("-m provider/model".to_string()),
            print_flag: String::new(),
            display_name: Some("My Custom Agent".to_string()),
        };

        let config: AgentConfig = toml.into();
        assert_eq!(config.cmd, "myagent run");
        assert!(!config.can_commit);
        assert_eq!(config.json_parser, JsonParserType::Claude);
        assert_eq!(config.model_flag, Some("-m provider/model".to_string()));
        assert_eq!(config.display_name, Some("My Custom Agent".to_string()));
    }

    #[test]
    fn test_agent_config_toml_defaults() {
        let toml_str = r#"cmd = "myagent""#;
        let config: AgentConfigToml = toml::from_str(toml_str).unwrap();

        assert_eq!(config.cmd, "myagent");
        assert_eq!(config.output_flag, "");
        assert!(config.can_commit); // default is true
    }

    #[test]
    fn test_agent_config_with_print_flag() {
        let agent = AgentConfig {
            cmd: "ccs glm".to_string(),
            output_flag: "--output-format=stream-json".to_string(),
            yolo_flag: "--dangerously-skip-permissions".to_string(),
            verbose_flag: "--verbose".to_string(),
            can_commit: true,
            json_parser: JsonParserType::Claude,
            model_flag: None,
            print_flag: "-p".to_string(),
            display_name: None,
        };

        let cmd = agent.build_cmd(true, true, true);
        assert!(cmd.contains("ccs glm -p"));
        assert!(cmd.contains("--output-format=stream-json"));
    }

    #[test]
    fn test_default_agents_toml_is_valid() {
        let config: AgentsConfigFile = toml::from_str(DEFAULT_AGENTS_TOML).unwrap();
        assert!(config.agents.contains_key("claude"));
        assert!(config.agents.contains_key("codex"));
    }

    #[test]
    fn test_global_config_path() {
        if let Some(path) = global_agents_config_path() {
            assert!(path.ends_with("agents.toml"));
        }
    }
}
