//! Agent configuration types and TOML parsing.
//!
//! This module provides types for loading and managing agent configurations
//! from TOML files, including support for global and per-project configs.

use super::ccs_env::{load_ccs_env_vars, CcsEnvVarsError};
use super::fallback::FallbackConfig;
use super::parser::JsonParserType;
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Default agents.toml template embedded at compile time.
pub const DEFAULT_AGENTS_TOML: &str = include_str!("../../examples/agents.toml");

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
    /// Include partial messages flag for streaming with -p (e.g., "--include-partial-messages").
    /// Required for Claude/CCS to stream JSON output when using -p mode.
    pub streaming_flag: String,
    /// Session continuation flag template (e.g., "--session {}" for OpenCode).
    /// The `{}` placeholder is replaced with the session ID at runtime.
    /// If empty, session continuation is not supported for this agent.
    pub session_flag: String,
    /// Environment variables to set when running this agent.
    /// Used for providers that need env vars (e.g., loaded from CCS settings).
    pub env_vars: std::collections::HashMap<String, String>,
    /// Display name for UI/logging (e.g., "ccs-glm" instead of raw agent name).
    /// If None, the agent name from the registry is used.
    pub display_name: Option<String>,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self {
            cmd: String::new(),
            output_flag: String::new(),
            yolo_flag: String::new(),
            verbose_flag: String::new(),
            can_commit: true,
            json_parser: JsonParserType::Generic,
            model_flag: None,
            print_flag: String::new(),
            streaming_flag: String::new(),
            session_flag: String::new(),
            env_vars: std::collections::HashMap::new(),
            display_name: None,
        }
    }
}

impl AgentConfig {
    /// Create a new AgentConfig builder.
    pub fn builder() -> AgentConfigBuilder {
        AgentConfigBuilder::default()
    }

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

        // Add streaming flag when using stream-json output with -p
        // Claude/CCS require --include-partial-messages to stream JSON in -p mode
        if output
            && !self.output_flag.is_empty()
            && self.output_flag.contains("stream-json")
            && !self.print_flag.is_empty()
            && !self.streaming_flag.is_empty()
        {
            parts.push(self.streaming_flag.clone());
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

    /// Build full command string with session continuation.
    ///
    /// This is used for XSD retries where we want to continue an existing session
    /// so the AI retains memory of its previous reasoning.
    ///
    /// # Arguments
    ///
    /// * `output` - Enable JSON output format
    /// * `yolo` - Enable autonomous mode
    /// * `verbose` - Enable verbose output
    /// * `model_override` - Optional model override
    /// * `session_id` - Session ID to continue (if supported by this agent)
    ///
    /// # Returns
    ///
    /// The command string with session continuation flag if supported
    pub fn build_cmd_with_session(
        &self,
        output: bool,
        yolo: bool,
        verbose: bool,
        model_override: Option<&str>,
        session_id: Option<&str>,
    ) -> String {
        let mut cmd = self.build_cmd_with_model(output, yolo, verbose, model_override);

        // Add session continuation flag if we have a session ID and the agent supports it
        if let Some(sid) = session_id {
            if !self.session_flag.is_empty() {
                let session_arg = self.session_flag.replace("{}", sid);
                cmd.push(' ');
                cmd.push_str(&session_arg);
            }
        }

        cmd
    }

    /// Check if this agent supports session continuation.
    pub fn supports_session_continuation(&self) -> bool {
        !self.session_flag.is_empty()
    }

    /// Check if this agent requires --verbose when JSON output is enabled.
    fn requires_verbose_for_json(&self, json_enabled: bool) -> bool {
        if !json_enabled || !self.output_flag.contains("stream-json") {
            return false;
        }

        // Both `claude` and CCS (`ccs ...`) require verbose mode when using stream-json output.
        // CCS is a wrapper around the Claude CLI and inherits its stream-json quirks.
        let base = self.cmd.split_whitespace().next().unwrap_or("");
        // Extract just the file name from the path to handle cases like "/usr/local/bin/claude"
        let exe_name = Path::new(base)
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(base);
        matches!(exe_name, "claude" | "ccs")
    }
}

/// Builder for AgentConfig.
///
/// Provides a fluent API for constructing AgentConfig instances
/// without needing to specify all 12 fields.
///
/// # Example
///
/// ```
/// use ralph_workflow::agents::AgentConfig;
///
/// let config = AgentConfig::builder()
///     .cmd("claude")
///     .output_flag("--output-format=stream-json")
///     .yolo_flag("--dangerously-skip-permissions")
///     .build();
/// ```
#[derive(Default, Debug, Clone)]
pub struct AgentConfigBuilder {
    cmd: Option<String>,
    output_flag: Option<String>,
    yolo_flag: Option<String>,
    verbose_flag: Option<String>,
    can_commit: Option<bool>,
    json_parser: Option<JsonParserType>,
    model_flag: Option<String>,
    print_flag: Option<String>,
    streaming_flag: Option<String>,
    session_flag: Option<String>,
    env_vars: Option<std::collections::HashMap<String, String>>,
    display_name: Option<String>,
}

impl AgentConfigBuilder {
    /// Set the base command to run the agent.
    pub fn cmd(mut self, cmd: impl Into<String>) -> Self {
        self.cmd = Some(cmd.into());
        self
    }

    /// Set the output-format flag.
    pub fn output_flag(mut self, flag: impl Into<String>) -> Self {
        self.output_flag = Some(flag.into());
        self
    }

    /// Set the autonomous mode flag.
    pub fn yolo_flag(mut self, flag: impl Into<String>) -> Self {
        self.yolo_flag = Some(flag.into());
        self
    }

    /// Set the verbose output flag.
    pub fn verbose_flag(mut self, flag: impl Into<String>) -> Self {
        self.verbose_flag = Some(flag.into());
        self
    }

    /// Set whether the agent can run git commit.
    pub fn can_commit(mut self, can_commit: bool) -> Self {
        self.can_commit = Some(can_commit);
        self
    }

    /// Set the JSON parser type.
    pub fn json_parser(mut self, parser: JsonParserType) -> Self {
        self.json_parser = Some(parser);
        self
    }

    /// Set the model/provider flag.
    pub fn model_flag(mut self, flag: impl Into<String>) -> Self {
        self.model_flag = Some(flag.into());
        self
    }

    /// Set the print/non-interactive mode flag.
    pub fn print_flag(mut self, flag: impl Into<String>) -> Self {
        self.print_flag = Some(flag.into());
        self
    }

    /// Set the streaming flag.
    pub fn streaming_flag(mut self, flag: impl Into<String>) -> Self {
        self.streaming_flag = Some(flag.into());
        self
    }

    /// Set the session continuation flag template.
    pub fn session_flag(mut self, flag: impl Into<String>) -> Self {
        self.session_flag = Some(flag.into());
        self
    }

    /// Set environment variables.
    pub fn env_vars(mut self, env_vars: std::collections::HashMap<String, String>) -> Self {
        self.env_vars = Some(env_vars);
        self
    }

    /// Set the display name.
    pub fn display_name(mut self, name: impl Into<String>) -> Self {
        self.display_name = Some(name.into());
        self
    }

    /// Build the AgentConfig.
    ///
    /// Uses defaults for any unset fields.
    pub fn build(self) -> AgentConfig {
        AgentConfig {
            cmd: self.cmd.unwrap_or_default(),
            output_flag: self.output_flag.unwrap_or_default(),
            yolo_flag: self.yolo_flag.unwrap_or_default(),
            verbose_flag: self.verbose_flag.unwrap_or_default(),
            can_commit: self.can_commit.unwrap_or(true),
            json_parser: self.json_parser.unwrap_or(JsonParserType::Generic),
            model_flag: self.model_flag,
            print_flag: self.print_flag.unwrap_or_default(),
            streaming_flag: self.streaming_flag.unwrap_or_default(),
            session_flag: self.session_flag.unwrap_or_default(),
            env_vars: self.env_vars.unwrap_or_default(),
            display_name: self.display_name,
        }
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
    /// Include partial messages flag for streaming with -p (optional, defaults to "--include-partial-messages").
    #[serde(default = "default_streaming_flag")]
    pub streaming_flag: String,
    /// Session continuation flag template (optional, e.g., "--session {}" for OpenCode).
    /// The `{}` placeholder is replaced with the session ID at runtime.
    /// If empty, session continuation is not supported for this agent.
    #[serde(default)]
    pub session_flag: String,
    /// CCS profile to load env vars from (e.g., "glm").
    ///
    /// Ralph resolves the CCS profile to a settings file using CCS config mappings
    /// (`~/.ccs/config.json` and/or `~/.ccs/config.yaml`) and common settings file
    /// naming (`~/.ccs/{profile}.settings.json` / `~/.ccs/{profile}.setting.json`).
    ///
    /// The resulting values are injected into the agent process only (they are not
    /// persisted).
    #[serde(default)]
    pub ccs_profile: Option<String>,
    /// Environment variables to set when running this agent (optional).
    /// If `ccs_profile` is set, these are merged with CCS env vars (CCS takes precedence).
    #[serde(default)]
    pub env_vars: std::collections::HashMap<String, String>,
    /// Display name for UI/logging (optional, e.g., "My Custom Agent" instead of registry name).
    #[serde(default)]
    pub display_name: Option<String>,
}

const fn default_can_commit() -> bool {
    true
}

fn default_streaming_flag() -> String {
    "--include-partial-messages".to_string()
}

impl From<AgentConfigToml> for AgentConfig {
    fn from(toml: AgentConfigToml) -> Self {
        // Loading CCS env vars is best-effort: registry initialization should not fail
        // just because a CCS profile is missing or misconfigured.
        let ccs_env_vars = toml
            .ccs_profile
            .as_deref()
            .map_or_else(HashMap::new, |profile| match load_ccs_env_vars(profile) {
                Ok(vars) => vars,
                Err(err) => {
                    eprintln!(
                        "Warning: failed to load CCS env vars for profile '{profile}': {err}"
                    );
                    HashMap::new()
                }
            });

        // Merge manually specified env vars with CCS env vars
        // CCS env vars take precedence (as documented in ccs_profile field)
        let mut merged_env_vars = toml.env_vars;
        for (key, value) in ccs_env_vars {
            merged_env_vars.insert(key, value);
        }

        Self {
            cmd: toml.cmd,
            output_flag: toml.output_flag,
            yolo_flag: toml.yolo_flag,
            verbose_flag: toml.verbose_flag,
            can_commit: toml.can_commit,
            json_parser: JsonParserType::parse(&toml.json_parser),
            model_flag: toml.model_flag,
            print_flag: toml.print_flag,
            streaming_flag: toml.streaming_flag,
            session_flag: toml.session_flag,
            env_vars: merged_env_vars,
            display_name: toml.display_name,
        }
    }
}

// Note: Legacy global config directory functions (global_config_dir, global_agents_config_path)
// have been removed. Use unified config path from the config module instead.

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
    #[error("{0}")]
    CcsEnvVars(#[from] CcsEnvVarsError),
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
        let config: Self = toml::from_str(&contents)?;
        Ok(Some(config))
    }

    /// Load agents config from a file using workspace abstraction.
    ///
    /// This is the architecture-conformant version that uses the Workspace trait
    /// instead of direct filesystem access, allowing for proper testing with
    /// MemoryWorkspace.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the config file (relative to workspace root)
    /// * `workspace` - The workspace to use for filesystem operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(Some(config))` if file exists and parses successfully,
    /// `Ok(None)` if file doesn't exist, or an error if parsing fails.
    pub fn load_from_file_with_workspace(
        path: &Path,
        workspace: &dyn crate::workspace::Workspace,
    ) -> Result<Option<Self>, AgentConfigError> {
        if !workspace.exists(path) {
            return Ok(None);
        }

        let contents = workspace
            .read(path)
            .map_err(|e| AgentConfigError::Io(io::Error::other(e)))?;
        let config: Self = toml::from_str(&contents)?;
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

    /// Ensure agents config file exists using workspace abstraction.
    ///
    /// This is the architecture-conformant version that uses the Workspace trait
    /// instead of direct filesystem access, allowing for proper testing with
    /// MemoryWorkspace.
    ///
    /// # Arguments
    ///
    /// * `path` - Path to the config file (relative to workspace root)
    /// * `workspace` - The workspace to use for filesystem operations
    ///
    /// # Returns
    ///
    /// Returns `Ok(ConfigInitResult::AlreadyExists)` if file exists,
    /// `Ok(ConfigInitResult::Created)` if file was created from template,
    /// or an error if creation fails.
    pub fn ensure_config_exists_with_workspace(
        path: &Path,
        workspace: &dyn crate::workspace::Workspace,
    ) -> io::Result<ConfigInitResult> {
        if workspace.exists(path) {
            return Ok(ConfigInitResult::AlreadyExists);
        }

        // Create parent directories if they don't exist
        if let Some(parent) = path.parent() {
            workspace.create_dir_all(parent)?;
        }

        // Write the default template
        workspace.write(path, DEFAULT_AGENTS_TOML)?;

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
            streaming_flag: String::new(),
            session_flag: String::new(),
            env_vars: std::collections::HashMap::new(),
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
            streaming_flag: String::new(),
            session_flag: "--session {}".to_string(),
            ccs_profile: None,
            env_vars: std::collections::HashMap::new(),
            display_name: Some("My Custom Agent".to_string()),
        };

        let config: AgentConfig = AgentConfig::from(toml);
        assert_eq!(config.cmd, "myagent run");
        assert!(!config.can_commit);
        assert_eq!(config.json_parser, JsonParserType::Claude);
        assert_eq!(config.model_flag, Some("-m provider/model".to_string()));
        assert_eq!(config.display_name, Some("My Custom Agent".to_string()));
        assert_eq!(config.session_flag, "--session {}");
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
            streaming_flag: "--include-partial-messages".to_string(),
            session_flag: String::new(),
            env_vars: std::collections::HashMap::new(),
            display_name: None,
        };

        let cmd = agent.build_cmd(true, true, true);
        assert!(cmd.contains("ccs glm -p"));
        assert!(cmd.contains("--output-format=stream-json"));
        assert!(cmd.contains("--include-partial-messages"));
    }

    #[test]
    fn test_default_agents_toml_is_valid() {
        let config: AgentsConfigFile = toml::from_str(DEFAULT_AGENTS_TOML).unwrap();
        assert!(config.agents.contains_key("claude"));
        assert!(config.agents.contains_key("codex"));
    }

    // Note: test_global_config_path was removed along with global_agents_config_path function.
    // Legacy agent config paths are no longer supported.

    #[test]
    fn test_build_cmd_with_session() {
        // Test with OpenCode agent (uses -s flag per `opencode run --help`)
        let agent = AgentConfig {
            cmd: "opencode run".to_string(),
            output_flag: "--json".to_string(),
            yolo_flag: "--yes".to_string(),
            verbose_flag: "--verbose".to_string(),
            can_commit: true,
            json_parser: JsonParserType::OpenCode,
            model_flag: None,
            print_flag: String::new(),
            streaming_flag: String::new(),
            session_flag: "-s {}".to_string(), // From `opencode run --help`
            env_vars: std::collections::HashMap::new(),
            display_name: None,
        };

        // Without session ID
        let cmd = agent.build_cmd_with_session(true, true, true, None, None);
        assert!(!cmd.contains("-s "));

        // With session ID
        let cmd = agent.build_cmd_with_session(true, true, true, None, Some("ses_abc123"));
        assert!(cmd.contains("-s ses_abc123"));
    }

    #[test]
    fn test_build_cmd_with_session_claude() {
        // Test with Claude agent (uses --resume flag per `claude --help`)
        let agent = AgentConfig {
            cmd: "claude -p".to_string(),
            output_flag: "--output-format=stream-json".to_string(),
            yolo_flag: "--dangerously-skip-permissions".to_string(),
            verbose_flag: "--verbose".to_string(),
            can_commit: true,
            json_parser: JsonParserType::Claude,
            model_flag: None,
            print_flag: String::new(),
            streaming_flag: String::new(),
            session_flag: "--resume {}".to_string(), // From `claude --help`
            env_vars: std::collections::HashMap::new(),
            display_name: None,
        };

        // With session ID
        let cmd = agent.build_cmd_with_session(true, true, true, None, Some("abc123"));
        assert!(cmd.contains("--resume abc123"));
    }

    #[test]
    fn test_build_cmd_with_session_no_support() {
        let agent = AgentConfig {
            cmd: "generic-agent".to_string(),
            output_flag: String::new(),
            yolo_flag: String::new(),
            verbose_flag: String::new(),
            can_commit: true,
            json_parser: JsonParserType::Generic,
            model_flag: None,
            print_flag: String::new(),
            streaming_flag: String::new(),
            session_flag: String::new(), // No session support
            env_vars: std::collections::HashMap::new(),
            display_name: None,
        };

        // Session ID should be ignored when agent doesn't support it
        let cmd = agent.build_cmd_with_session(false, false, false, None, Some("ses_abc123"));
        assert!(!cmd.contains("ses_abc123"));
        assert!(!agent.supports_session_continuation());
    }

    #[test]
    fn test_supports_session_continuation() {
        let with_support = AgentConfig {
            cmd: "opencode run".to_string(),
            output_flag: String::new(),
            yolo_flag: String::new(),
            verbose_flag: String::new(),
            can_commit: true,
            json_parser: JsonParserType::OpenCode,
            model_flag: None,
            print_flag: String::new(),
            streaming_flag: String::new(),
            session_flag: "--session {}".to_string(),
            env_vars: std::collections::HashMap::new(),
            display_name: None,
        };
        assert!(with_support.supports_session_continuation());

        let without_support = AgentConfig {
            cmd: "generic-agent".to_string(),
            output_flag: String::new(),
            yolo_flag: String::new(),
            verbose_flag: String::new(),
            can_commit: true,
            json_parser: JsonParserType::Generic,
            model_flag: None,
            print_flag: String::new(),
            streaming_flag: String::new(),
            session_flag: String::new(),
            env_vars: std::collections::HashMap::new(),
            display_name: None,
        };
        assert!(!without_support.supports_session_continuation());
    }

    // =========================================================================
    // Workspace-aware function tests (architecture-conformant)
    // =========================================================================

    #[test]
    fn test_load_from_file_with_workspace_nonexistent() {
        use crate::workspace::MemoryWorkspace;
        let workspace = MemoryWorkspace::new_test();
        let path = Path::new(".agent/agents.toml");

        let result = AgentsConfigFile::load_from_file_with_workspace(path, &workspace).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_load_from_file_with_workspace_valid_config() {
        use crate::workspace::MemoryWorkspace;
        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/agents.toml", DEFAULT_AGENTS_TOML);
        let path = Path::new(".agent/agents.toml");

        let result = AgentsConfigFile::load_from_file_with_workspace(path, &workspace).unwrap();
        assert!(result.is_some());
        let config = result.unwrap();
        assert!(config.agents.contains_key("claude"));
    }

    #[test]
    fn test_ensure_config_exists_with_workspace_creates_file() {
        use crate::workspace::{MemoryWorkspace, Workspace};
        let workspace = MemoryWorkspace::new_test();
        let path = Path::new(".agent/agents.toml");

        let result =
            AgentsConfigFile::ensure_config_exists_with_workspace(path, &workspace).unwrap();
        assert!(matches!(result, ConfigInitResult::Created));
        assert!(workspace.exists(path));

        // Verify the content is the default template
        let content = workspace.read(path).unwrap();
        assert_eq!(content, DEFAULT_AGENTS_TOML);
    }

    #[test]
    fn test_ensure_config_exists_with_workspace_already_exists() {
        use crate::workspace::{MemoryWorkspace, Workspace};
        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/agents.toml", "# custom config");
        let path = Path::new(".agent/agents.toml");

        let result =
            AgentsConfigFile::ensure_config_exists_with_workspace(path, &workspace).unwrap();
        assert!(matches!(result, ConfigInitResult::AlreadyExists));

        // Verify the content was not overwritten
        let content = workspace.read(path).unwrap();
        assert_eq!(content, "# custom config");
    }
}
