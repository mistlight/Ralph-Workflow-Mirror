//! Agent Abstraction Module
//!
//! Provides a pluggable agent system for different
//! AI coding assistants (Claude, Codex, OpenCode, Goose, Cline, etc.)
//!
//! ## Configuration
//!
//! Agents can be configured via (in order of increasing priority):
//! 1. Built-in defaults (claude, codex, opencode, aider, goose, cline, continue, amazon-q, gemini)
//! 2. Global config file (`~/.config/ralph/agents.toml`)
//! 3. Project config file (default: `.agent/agents.toml`, overridable via `RALPH_AGENTS_CONFIG`)
//! 4. Environment variables (`RALPH_DEVELOPER_CMD`, `RALPH_REVIEWER_CMD`)
//! 5. Programmatic registration via `AgentRegistry::register()`
//!
//! Config files are merged, with later sources overriding earlier ones.
//! This allows setting global defaults while customizing per-project.
//!
//! ## Agent Switching / Fallback
//!
//! Configure fallback agents for automatic switching when primary agent fails:
//! ```toml
//! [fallback]
//! developer = ["claude", "codex", "goose"]
//! reviewer = ["codex", "claude"]
//! max_retries = 3
//! retry_delay_ms = 1000
//! ```
//!
//! ## Example TOML Configuration
//!
//! ```toml
//! [agents.myagent]
//! cmd = "my-ai-tool run"
//! json_flag = "--json-stream"
//! yolo_flag = "--auto-fix"
//! verbose_flag = "--verbose"
//! can_commit = true
//! json_parser = "claude"  # Use Claude's JSON parser
//! ```

use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Get the global config directory for Ralph
///
/// Returns `~/.config/ralph` on Unix and `%APPDATA%\ralph` on Windows.
/// Returns None if the home directory cannot be determined.
pub(crate) fn global_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("ralph"))
}

/// Get the global agents.toml path
///
/// Returns `~/.config/ralph/agents.toml` on Unix.
pub(crate) fn global_agents_config_path() -> Option<PathBuf> {
    global_config_dir().map(|p| p.join("agents.toml"))
}

/// Config source for tracking where config was loaded from
#[derive(Debug, Clone)]
pub(crate) struct ConfigSource {
    pub(crate) path: PathBuf,
    pub(crate) agents_loaded: usize,
}

/// Default agents.toml template embedded at compile time
pub(crate) const DEFAULT_AGENTS_TOML: &str = include_str!("../examples/agents.toml");

/// JSON parser type for agent output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub(crate) enum JsonParserType {
    /// Claude's stream-json format
    #[default]
    Claude,
    /// Codex's JSON format
    Codex,
    /// Gemini's stream-json format
    Gemini,
    /// Generic line-based output (no parsing)
    Generic,
}

impl JsonParserType {
    /// Parse parser type from string
    pub(crate) fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "claude" => JsonParserType::Claude,
            "codex" => JsonParserType::Codex,
            "gemini" => JsonParserType::Gemini,
            "generic" | "none" | "raw" => JsonParserType::Generic,
            _ => JsonParserType::Generic,
        }
    }
}

impl std::fmt::Display for JsonParserType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            JsonParserType::Claude => write!(f, "claude"),
            JsonParserType::Codex => write!(f, "codex"),
            JsonParserType::Gemini => write!(f, "gemini"),
            JsonParserType::Generic => write!(f, "generic"),
        }
    }
}

/// Agent capabilities
#[derive(Debug, Clone)]
pub(crate) struct AgentConfig {
    /// Base command to run the agent
    pub(crate) cmd: String,
    /// Flag to enable JSON output
    pub(crate) json_flag: String,
    /// Flag for autonomous mode (no prompts)
    pub(crate) yolo_flag: String,
    /// Flag for verbose output
    pub(crate) verbose_flag: String,
    /// Whether the agent can run git commit
    pub(crate) can_commit: bool,
    /// Which JSON parser to use for this agent's output
    pub(crate) json_parser: JsonParserType,
}

/// TOML configuration for an agent (for deserialization)
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AgentConfigToml {
    /// Base command to run the agent
    pub(crate) cmd: String,
    /// Flag to enable JSON output (optional, defaults to empty)
    #[serde(default)]
    pub(crate) json_flag: String,
    /// Flag for autonomous mode (optional, defaults to empty)
    #[serde(default)]
    pub(crate) yolo_flag: String,
    /// Flag for verbose output (optional, defaults to empty)
    #[serde(default)]
    pub(crate) verbose_flag: String,
    /// Whether the agent can run git commit (optional, defaults to true)
    #[serde(default = "default_can_commit")]
    pub(crate) can_commit: bool,
    /// Which JSON parser to use: "claude", "codex", or "generic" (optional, defaults to "generic")
    #[serde(default)]
    pub(crate) json_parser: String,
}

fn default_can_commit() -> bool {
    true
}

impl From<AgentConfigToml> for AgentConfig {
    fn from(toml: AgentConfigToml) -> Self {
        AgentConfig {
            cmd: toml.cmd,
            json_flag: toml.json_flag,
            yolo_flag: toml.yolo_flag,
            verbose_flag: toml.verbose_flag,
            can_commit: toml.can_commit,
            json_parser: JsonParserType::parse(&toml.json_parser),
        }
    }
}

/// Root TOML configuration structure
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct AgentsConfigFile {
    /// Map of agent name to configuration
    #[serde(default)]
    pub(crate) agents: HashMap<String, AgentConfigToml>,
    /// Agent chain configuration (preferred agents + fallbacks)
    /// Supports both `[fallback]` (legacy) and `[agent_chain]` section names
    #[serde(default, alias = "agent_chain")]
    pub(crate) fallback: FallbackConfig,
}

/// Error type for agent configuration loading
#[derive(Debug, thiserror::Error)]
pub(crate) enum AgentConfigError {
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),
    #[error("Failed to parse TOML: {0}")]
    Toml(#[from] toml::de::Error),
    #[error("Built-in agents.toml template is invalid TOML: {0}")]
    DefaultTemplateToml(toml::de::Error),
}

/// Result of checking/initializing the agents config file
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum ConfigInitResult {
    /// Config file already exists, no action taken
    AlreadyExists,
    /// Config file was just created from template
    Created,
}

impl AgentsConfigFile {
    /// Load agents configuration from a TOML file
    ///
    /// Returns Ok(None) if the file doesn't exist.
    /// Returns Err if the file exists but can't be parsed.
    pub(crate) fn load_from_file<P: AsRef<Path>>(
        path: P,
    ) -> Result<Option<Self>, AgentConfigError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(path)?;
        let config: AgentsConfigFile = toml::from_str(&contents)?;
        Ok(Some(config))
    }

    /// Ensure agents config file exists, creating it from template if needed.
    ///
    /// Returns:
    /// - `Ok(ConfigInitResult::AlreadyExists)` if the file already exists
    /// - `Ok(ConfigInitResult::Created)` if the file was just created from the default template
    /// - `Err` if there was an error creating the file or parent directories
    pub(crate) fn ensure_config_exists<P: AsRef<Path>>(path: P) -> io::Result<ConfigInitResult> {
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

impl AgentConfig {
    /// Build full command string with specified flags
    ///
    /// Note: For Claude CLI, when using `--output-format=stream-json` (the json_flag),
    /// the `--verbose` flag is always required. This method automatically adds verbose
    /// when using Claude's stream-json format, regardless of the `verbose` parameter.
    pub(crate) fn build_cmd(&self, json: bool, yolo: bool, verbose: bool) -> String {
        let mut parts = vec![self.cmd.clone()];

        if json && !self.json_flag.is_empty() {
            parts.push(self.json_flag.clone());
        }
        if yolo && !self.yolo_flag.is_empty() {
            parts.push(self.yolo_flag.clone());
        }

        // Claude CLI requires --verbose when using --output-format=stream-json
        // See: https://github.com/anthropics/claude-code
        let needs_verbose = verbose || self.requires_verbose_for_json(json);

        if needs_verbose && !self.verbose_flag.is_empty() {
            parts.push(self.verbose_flag.clone());
        }

        parts.join(" ")
    }

    /// Check if this agent requires --verbose when JSON output is enabled.
    ///
    /// Claude CLI specifically requires --verbose when using --output-format=stream-json
    /// in print mode (-p). Without it, the command fails with:
    /// "Error: When using --print, --output-format=stream-json requires --verbose"
    fn requires_verbose_for_json(&self, json_enabled: bool) -> bool {
        json_enabled
            && self.json_parser == JsonParserType::Claude
            && self.json_flag.contains("stream-json")
    }
}

/// Agent chain configuration for preferred agents and fallback switching
///
/// The agent chain defines both:
/// 1. The **preferred agent** (first in the list) for each role
/// 2. The **fallback agents** (remaining in the list) to try if the preferred fails
///
/// This provides a unified way to configure which agents to use and in what order.
/// Ralph automatically switches to the next agent in the chain when encountering
/// errors like rate limits or auth failures.
///
/// Note: For backward compatibility, this section can be named either `[fallback]`
/// or `[agent_chain]` in the TOML config file.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct FallbackConfig {
    /// Ordered list of agents for developer role (first = preferred, rest = fallbacks)
    #[serde(default)]
    pub(crate) developer: Vec<String>,
    /// Ordered list of agents for reviewer role (first = preferred, rest = fallbacks)
    #[serde(default)]
    pub(crate) reviewer: Vec<String>,
    /// Maximum number of retries per agent before moving to next
    #[serde(default = "default_max_retries")]
    pub(crate) max_retries: u32,
    /// Delay between retries in milliseconds
    #[serde(default = "default_retry_delay_ms")]
    pub(crate) retry_delay_ms: u64,
}

fn default_max_retries() -> u32 {
    3
}

fn default_retry_delay_ms() -> u64 {
    1000
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            developer: Vec::new(),
            reviewer: Vec::new(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
        }
    }
}

impl FallbackConfig {
    /// Get fallback agents for a role
    pub(crate) fn get_fallbacks(&self, role: AgentRole) -> &[String] {
        match role {
            AgentRole::Developer => &self.developer,
            AgentRole::Reviewer => &self.reviewer,
        }
    }

    /// Check if fallback is configured for a role
    pub(crate) fn has_fallbacks(&self, role: AgentRole) -> bool {
        !self.get_fallbacks(role).is_empty()
    }
}

/// Agent role (developer or reviewer)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentRole {
    Developer,
    Reviewer,
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentRole::Developer => write!(f, "developer"),
            AgentRole::Reviewer => write!(f, "reviewer"),
        }
    }
}

/// Error classification for agent failures (to determine if retry is appropriate)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum AgentErrorKind {
    /// API rate limit exceeded - retry after delay
    RateLimited,
    /// Token/context limit exceeded - may need different agent
    TokenExhausted,
    /// API temporarily unavailable - retry
    ApiUnavailable,
    /// Authentication failure - switch agent
    AuthFailure,
    /// Command not found - switch agent
    CommandNotFound,
    /// Other transient error - retry
    Transient,
    /// Permanent failure - do not retry
    Permanent,
}

impl AgentErrorKind {
    /// Determine if this error should trigger a retry
    pub(crate) fn should_retry(&self) -> bool {
        matches!(
            self,
            AgentErrorKind::RateLimited
                | AgentErrorKind::ApiUnavailable
                | AgentErrorKind::Transient
        )
    }

    /// Determine if this error should trigger a fallback to another agent
    pub(crate) fn should_fallback(&self) -> bool {
        matches!(
            self,
            AgentErrorKind::TokenExhausted
                | AgentErrorKind::AuthFailure
                | AgentErrorKind::CommandNotFound
        )
    }

    /// Classify an error from exit code and output
    pub(crate) fn classify(exit_code: i32, stderr: &str) -> Self {
        let stderr_lower = stderr.to_lowercase();

        // Rate limiting indicators
        if stderr_lower.contains("rate limit")
            || stderr_lower.contains("too many requests")
            || stderr_lower.contains("429")
        {
            return AgentErrorKind::RateLimited;
        }

        // Token/context exhaustion
        if stderr_lower.contains("token")
            || stderr_lower.contains("context length")
            || stderr_lower.contains("maximum context")
            || stderr_lower.contains("too long")
        {
            return AgentErrorKind::TokenExhausted;
        }

        // API unavailable
        if stderr_lower.contains("service unavailable")
            || stderr_lower.contains("503")
            || stderr_lower.contains("502")
            || stderr_lower.contains("timeout")
            || stderr_lower.contains("connection refused")
        {
            return AgentErrorKind::ApiUnavailable;
        }

        // Auth failures
        if stderr_lower.contains("unauthorized")
            || stderr_lower.contains("authentication")
            || stderr_lower.contains("401")
            || stderr_lower.contains("api key")
            || stderr_lower.contains("invalid token")
        {
            return AgentErrorKind::AuthFailure;
        }

        // Command not found
        if exit_code == 127
            || stderr_lower.contains("command not found")
            || stderr_lower.contains("not found")
            || stderr_lower.contains("no such file")
        {
            return AgentErrorKind::CommandNotFound;
        }

        // Transient errors (exit codes that might succeed on retry)
        if exit_code == 1 && stderr_lower.contains("error") {
            return AgentErrorKind::Transient;
        }

        AgentErrorKind::Permanent
    }
}

/// Agent registry
pub(crate) struct AgentRegistry {
    agents: HashMap<String, AgentConfig>,
    fallback: FallbackConfig,
}

impl AgentRegistry {
    /// Create a new registry with default agents
    pub(crate) fn new() -> Result<Self, AgentConfigError> {
        let AgentsConfigFile { agents, fallback } =
            toml::from_str(DEFAULT_AGENTS_TOML).map_err(AgentConfigError::DefaultTemplateToml)?;

        let mut registry = Self {
            agents: HashMap::new(),
            fallback,
        };

        for (name, agent_toml) in agents {
            registry.register(&name, agent_toml.into());
        }

        Ok(registry)
    }

    /// Register a new agent
    pub(crate) fn register(&mut self, name: &str, config: AgentConfig) {
        self.agents.insert(name.to_string(), config);
    }

    /// Get agent configuration
    pub(crate) fn get(&self, name: &str) -> Option<&AgentConfig> {
        self.agents.get(name)
    }

    /// Check if agent exists
    pub(crate) fn is_known(&self, name: &str) -> bool {
        self.agents.contains_key(name)
    }

    /// List all registered agents
    pub(crate) fn list(&self) -> Vec<(&str, &AgentConfig)> {
        self.agents.iter().map(|(k, v)| (k.as_str(), v)).collect()
    }

    /// Get command for developer role
    pub(crate) fn developer_cmd(&self, agent_name: &str) -> Option<String> {
        self.get(agent_name).map(|c| c.build_cmd(true, true, true))
    }

    /// Get command for reviewer role
    pub(crate) fn reviewer_cmd(&self, agent_name: &str) -> Option<String> {
        self.get(agent_name).map(|c| c.build_cmd(true, true, false))
    }

    /// Get the JSON parser type for an agent
    #[allow(dead_code)]
    pub(crate) fn parser_type(&self, agent_name: &str) -> JsonParserType {
        self.get(agent_name)
            .map(|c| c.json_parser)
            .unwrap_or(JsonParserType::Generic)
    }

    /// Load custom agents from a TOML configuration file
    ///
    /// Custom agents override built-in defaults if they have the same name.
    /// Returns the number of agents loaded, or an error if the file can't be parsed.
    pub(crate) fn load_from_file<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<usize, AgentConfigError> {
        match AgentsConfigFile::load_from_file(path)? {
            Some(config) => {
                let count = config.agents.len();
                for (name, agent_toml) in config.agents {
                    self.register(&name, agent_toml.into());
                }
                // Load fallback configuration
                self.fallback = config.fallback;
                Ok(count)
            }
            None => Ok(0),
        }
    }

    /// Create a new registry with default agents, then load custom agents from file
    ///
    /// This is the recommended way to create a registry for production use.
    /// Custom agents in the file will override built-in defaults.
    #[allow(dead_code)]
    pub(crate) fn with_config_file<P: AsRef<Path>>(path: P) -> Result<Self, AgentConfigError> {
        let mut registry = Self::new()?;
        registry.load_from_file(path)?;
        Ok(registry)
    }

    /// Create a new registry with merged config from multiple sources
    ///
    /// Loads config in order of increasing priority:
    /// 1. Built-in defaults
    /// 2. Global config (`~/.config/ralph/agents.toml`)
    /// 3. Per-repository config (`.agent/agents.toml` or `local_path`)
    ///
    /// Later sources override earlier ones. Returns a list of loaded config sources.
    pub(crate) fn with_merged_configs<P: AsRef<Path>>(
        local_path: P,
    ) -> Result<(Self, Vec<ConfigSource>, Vec<String>), AgentConfigError> {
        let mut registry = Self::new()?;
        let mut sources = Vec::new();
        let mut warnings = Vec::new();

        // 1. Try global config
        if let Some(global_path) = global_agents_config_path() {
            if global_path.exists() {
                match registry.load_from_file(&global_path) {
                    Ok(count) => {
                        sources.push(ConfigSource {
                            path: global_path,
                            agents_loaded: count,
                        });
                    }
                    Err(e) => {
                        // Global config is optional: continue, but return a warning for the caller
                        warnings.push(format!(
                            "Failed to load global config from {}: {}",
                            global_path.display(),
                            e
                        ));
                    }
                }
            }
        }

        // 2. Try local (per-repo) config
        let local_path = local_path.as_ref();
        if local_path.exists() {
            let count = registry.load_from_file(local_path)?;
            sources.push(ConfigSource {
                path: local_path.to_path_buf(),
                agents_loaded: count,
            });
        }

        Ok((registry, sources, warnings))
    }

    /// Merge another config file into this registry
    ///
    /// Agents from the new file override existing agents with the same name.
    /// If the new file has fallback config, it replaces the existing fallback config.
    #[allow(dead_code)]
    pub(crate) fn merge_from_file<P: AsRef<Path>>(
        &mut self,
        path: P,
    ) -> Result<usize, AgentConfigError> {
        match AgentsConfigFile::load_from_file(path)? {
            Some(config) => {
                let count = config.agents.len();
                for (name, agent_toml) in config.agents {
                    self.register(&name, agent_toml.into());
                }
                // Only update fallback if new config has non-empty fallback
                if config.fallback.has_fallbacks(AgentRole::Developer)
                    || config.fallback.has_fallbacks(AgentRole::Reviewer)
                {
                    self.fallback = config.fallback;
                }
                Ok(count)
            }
            None => Ok(0),
        }
    }

    /// Get the fallback configuration
    pub(crate) fn fallback_config(&self) -> &FallbackConfig {
        &self.fallback
    }

    /// Set the fallback configuration
    #[allow(dead_code)]
    pub(crate) fn set_fallback(&mut self, fallback: FallbackConfig) {
        self.fallback = fallback;
    }

    /// Get all fallback agents for a role that are registered in this registry
    pub(crate) fn available_fallbacks(&self, role: AgentRole) -> Vec<&str> {
        self.fallback
            .get_fallbacks(role)
            .iter()
            .filter(|name| self.is_known(name))
            .map(|s| s.as_str())
            .collect()
    }

    /// Validate that agent chains are configured for both roles.
    ///
    /// Returns Ok(()) if both developer and reviewer chains are configured,
    /// or an Err with a helpful error message if not.
    pub(crate) fn validate_agent_chains(&self) -> Result<(), String> {
        let has_developer = self.fallback.has_fallbacks(AgentRole::Developer);
        let has_reviewer = self.fallback.has_fallbacks(AgentRole::Reviewer);

        if !has_developer && !has_reviewer {
            return Err("No agent chain configured.\n\
                Please add an [agent_chain] section to your agents.toml file.\n\
                Run 'ralph --init' to create a default configuration."
                .to_string());
        }

        if !has_developer {
            return Err("No developer agent chain configured.\n\
                Add 'developer = [\"claude\", ...]' to your [agent_chain] section."
                .to_string());
        }

        if !has_reviewer {
            return Err("No reviewer agent chain configured.\n\
                Add 'reviewer = [\"codex\", ...]' to your [agent_chain] section."
                .to_string());
        }

        Ok(())
    }

    /// Check if an agent is available (command exists and is executable)
    pub(crate) fn is_agent_available(&self, name: &str) -> bool {
        if let Some(config) = self.get(name) {
            let Ok(parts) = crate::utils::split_command(&config.cmd) else {
                return false;
            };
            let Some(base_cmd) = parts.first() else {
                return false;
            };

            // Check if the command exists in PATH (portable; avoids shelling out)
            which::which(base_cmd).is_ok()
        } else {
            false
        }
    }

    /// List all available (installed) agents
    pub(crate) fn list_available(&self) -> Vec<&str> {
        self.agents
            .keys()
            .filter(|name| self.is_agent_available(name))
            .map(|s| s.as_str())
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_registry_defaults() {
        let registry = AgentRegistry::new().unwrap();

        // Original agents
        assert!(registry.is_known("claude"));
        assert!(registry.is_known("codex"));
        assert!(registry.is_known("driver"));
        assert!(registry.is_known("reviewer"));
        assert!(registry.is_known("opencode"));
        assert!(registry.is_known("aider"));

        // New agents
        assert!(registry.is_known("goose"));
        assert!(registry.is_known("cline"));
        assert!(registry.is_known("continue"));
        assert!(registry.is_known("amazon-q"));
        assert!(registry.is_known("gemini"));

        assert!(!registry.is_known("unknown_agent"));
    }

    #[test]
    fn test_agent_get_cmd() {
        let registry = AgentRegistry::new().unwrap();

        let claude = registry.get("claude").unwrap();
        assert!(claude.cmd.contains("claude"));

        let codex = registry.get("codex").unwrap();
        assert!(codex.cmd.contains("codex"));
    }

    #[test]
    fn test_agent_build_cmd() {
        let registry = AgentRegistry::new().unwrap();
        let codex = registry.get("codex").unwrap();

        // Codex doesn't require verbose with JSON, so verbose=false should exclude it
        let cmd = codex.build_cmd(true, true, false);
        assert!(cmd.contains("codex"));
        assert!(cmd.contains("json"));
        assert!(cmd.contains("full-auto")); // Codex uses --full-auto for automatic execution
        assert!(!cmd.contains("verbose"));

        // With verbose=true, it should be included
        let cmd_verbose = codex.build_cmd(true, true, true);
        // Codex has empty verbose_flag, so still no verbose in output
        assert!(!cmd_verbose.contains("verbose"));
    }

    #[test]
    fn test_claude_requires_verbose_with_stream_json() {
        let registry = AgentRegistry::new().unwrap();
        let claude = registry.get("claude").unwrap();

        // Claude requires --verbose when using --output-format=stream-json
        // Even when verbose=false is passed, it should include --verbose
        let cmd = claude.build_cmd(true, true, false);
        assert!(cmd.contains("claude"));
        assert!(cmd.contains("stream-json"));
        assert!(cmd.contains("skip-permissions"));
        assert!(
            cmd.contains("verbose"),
            "Claude should always include --verbose with stream-json"
        );

        // With verbose=true, it should also be included
        let cmd_verbose = claude.build_cmd(true, true, true);
        assert!(cmd_verbose.contains("verbose"));

        // Without JSON, verbose should follow the parameter
        let cmd_no_json = claude.build_cmd(false, true, false);
        assert!(!cmd_no_json.contains("verbose"));
        assert!(!cmd_no_json.contains("stream-json"));

        let cmd_no_json_verbose = claude.build_cmd(false, true, true);
        assert!(cmd_no_json_verbose.contains("verbose"));
    }

    #[test]
    fn test_agent_developer_cmd() {
        let registry = AgentRegistry::new().unwrap();
        let cmd = registry.developer_cmd("claude").unwrap();
        assert!(cmd.contains("claude"));
        assert!(cmd.contains("json"));
    }

    #[test]
    fn test_agent_reviewer_cmd() {
        let registry = AgentRegistry::new().unwrap();
        let cmd = registry.reviewer_cmd("codex").unwrap();
        assert!(cmd.contains("codex"));
        assert!(cmd.contains("json"));
    }

    #[test]
    fn test_claude_reviewer_cmd_includes_verbose() {
        // Regression test: Claude as reviewer must include --verbose with stream-json
        // See: "Error: When using --print, --output-format=stream-json requires --verbose"
        let registry = AgentRegistry::new().unwrap();
        let cmd = registry.reviewer_cmd("claude").unwrap();
        assert!(cmd.contains("claude"));
        assert!(cmd.contains("stream-json"));
        assert!(
            cmd.contains("verbose"),
            "Claude reviewer must include --verbose for stream-json to work"
        );
    }

    #[test]
    fn test_register_custom_agent() {
        let mut registry = AgentRegistry::new().unwrap();

        registry.register(
            "testbot",
            AgentConfig {
                cmd: "testbot run".to_string(),
                json_flag: "--output-json".to_string(),
                yolo_flag: "--auto".to_string(),
                verbose_flag: String::new(),
                can_commit: true,
                json_parser: JsonParserType::Claude,
            },
        );

        assert!(registry.is_known("testbot"));
        let config = registry.get("testbot").unwrap();
        assert_eq!(config.cmd, "testbot run");
        assert_eq!(config.json_parser, JsonParserType::Claude);
    }

    #[test]
    fn test_can_commit() {
        let registry = AgentRegistry::new().unwrap();

        let claude = registry.get("claude").unwrap();
        assert!(claude.can_commit);

        let codex = registry.get("codex").unwrap();
        assert!(codex.can_commit);
    }

    #[test]
    fn test_json_parser_type_parse() {
        assert_eq!(JsonParserType::parse("claude"), JsonParserType::Claude);
        assert_eq!(JsonParserType::parse("CLAUDE"), JsonParserType::Claude);
        assert_eq!(JsonParserType::parse("codex"), JsonParserType::Codex);
        assert_eq!(JsonParserType::parse("gemini"), JsonParserType::Gemini);
        assert_eq!(JsonParserType::parse("GEMINI"), JsonParserType::Gemini);
        assert_eq!(JsonParserType::parse("generic"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("none"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("raw"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("unknown"), JsonParserType::Generic);
    }

    #[test]
    fn test_json_parser_type_display() {
        assert_eq!(format!("{}", JsonParserType::Claude), "claude");
        assert_eq!(format!("{}", JsonParserType::Codex), "codex");
        assert_eq!(format!("{}", JsonParserType::Gemini), "gemini");
        assert_eq!(format!("{}", JsonParserType::Generic), "generic");
    }

    #[test]
    fn test_default_agent_parser_types() {
        let registry = AgentRegistry::new().unwrap();

        assert_eq!(registry.parser_type("claude"), JsonParserType::Claude);
        assert_eq!(registry.parser_type("codex"), JsonParserType::Codex);
        assert_eq!(registry.parser_type("gemini"), JsonParserType::Gemini);
        assert_eq!(registry.parser_type("opencode"), JsonParserType::Generic);
        assert_eq!(registry.parser_type("aider"), JsonParserType::Generic);
        assert_eq!(registry.parser_type("unknown"), JsonParserType::Generic);
    }

    #[test]
    fn test_agent_config_from_toml() {
        let toml = AgentConfigToml {
            cmd: "myagent run".to_string(),
            json_flag: "--json".to_string(),
            yolo_flag: "--auto".to_string(),
            verbose_flag: "--verbose".to_string(),
            can_commit: false,
            json_parser: "claude".to_string(),
        };

        let config: AgentConfig = toml.into();
        assert_eq!(config.cmd, "myagent run");
        assert_eq!(config.json_flag, "--json");
        assert_eq!(config.yolo_flag, "--auto");
        assert_eq!(config.verbose_flag, "--verbose");
        assert!(!config.can_commit);
        assert_eq!(config.json_parser, JsonParserType::Claude);
    }

    #[test]
    fn test_agent_config_toml_defaults() {
        // Test that serde defaults work correctly
        let toml_str = r#"cmd = "myagent""#;
        let config: AgentConfigToml = toml::from_str(toml_str).unwrap();

        assert_eq!(config.cmd, "myagent");
        assert_eq!(config.json_flag, "");
        assert_eq!(config.yolo_flag, "");
        assert_eq!(config.verbose_flag, "");
        assert!(config.can_commit); // default is true
        assert_eq!(config.json_parser, "");
    }

    #[test]
    fn test_agents_config_file_parse() {
        let toml_str = r#"
[agents.custom1]
cmd = "custom1-cli"
json_flag = "--json"
yolo_flag = "--yes"
can_commit = true
json_parser = "codex"

[agents.custom2]
cmd = "custom2-tool run"
json_parser = "claude"
"#;
        let config: AgentsConfigFile = toml::from_str(toml_str).unwrap();

        assert_eq!(config.agents.len(), 2);
        assert!(config.agents.contains_key("custom1"));
        assert!(config.agents.contains_key("custom2"));

        let custom1 = &config.agents["custom1"];
        assert_eq!(custom1.cmd, "custom1-cli");
        assert_eq!(custom1.json_flag, "--json");
        assert_eq!(custom1.json_parser, "codex");

        let custom2 = &config.agents["custom2"];
        assert_eq!(custom2.cmd, "custom2-tool run");
        assert!(custom2.can_commit); // default
        assert_eq!(custom2.json_parser, "claude");
    }

    #[test]
    fn test_load_from_file_nonexistent() {
        let mut registry = AgentRegistry::new().unwrap();
        let result = registry.load_from_file("/nonexistent/path/agents.toml");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), 0);
    }

    #[test]
    fn test_load_from_file_with_temp() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("agents.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[agents.testbot]
cmd = "testbot exec"
json_flag = "--output-json"
yolo_flag = "--auto"
json_parser = "codex"
"#
        )
        .unwrap();

        let mut registry = AgentRegistry::new().unwrap();
        let loaded = registry.load_from_file(&config_path).unwrap();

        assert_eq!(loaded, 1);
        assert!(registry.is_known("testbot"));

        let config = registry.get("testbot").unwrap();
        assert_eq!(config.cmd, "testbot exec");
        assert_eq!(config.json_flag, "--output-json");
        assert_eq!(config.json_parser, JsonParserType::Codex);
    }

    #[test]
    fn test_with_config_file_overrides_defaults() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("agents.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[agents.claude]
cmd = "claude-custom -p"
json_flag = "--custom-json"
yolo_flag = "--skip"
json_parser = "codex"
"#
        )
        .unwrap();

        let registry = AgentRegistry::with_config_file(&config_path).unwrap();

        let config = registry.get("claude").unwrap();
        assert_eq!(config.cmd, "claude-custom -p");
        assert_eq!(config.json_flag, "--custom-json");
        assert_eq!(config.json_parser, JsonParserType::Codex);
    }

    #[test]
    fn test_new_agent_configs() {
        let registry = AgentRegistry::new().unwrap();

        // Test Goose config
        let goose = registry.get("goose").unwrap();
        assert!(goose.cmd.contains("goose"));
        assert_eq!(goose.json_parser, JsonParserType::Generic);

        // Test Cline config
        let cline = registry.get("cline").unwrap();
        assert!(cline.cmd.contains("cline"));

        // Test Continue config
        let cont = registry.get("continue").unwrap();
        assert!(cont.cmd.contains("cn"));

        // Test Amazon Q config
        let q = registry.get("amazon-q").unwrap();
        assert!(q.cmd.contains("q"));
        assert!(q.yolo_flag.contains("trust"));

        // Test Gemini config
        let gemini = registry.get("gemini").unwrap();
        assert!(gemini.cmd.contains("gemini"));
    }

    #[test]
    fn test_fallback_config_defaults() {
        let config = FallbackConfig::default();
        assert!(config.developer.is_empty());
        assert!(config.reviewer.is_empty());
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
    }

    #[test]
    fn test_fallback_config_get_fallbacks() {
        let config = FallbackConfig {
            developer: vec!["claude".to_string(), "codex".to_string()],
            reviewer: vec!["codex".to_string(), "goose".to_string()],
            ..Default::default()
        };

        assert_eq!(
            config.get_fallbacks(AgentRole::Developer),
            &["claude", "codex"]
        );
        assert_eq!(
            config.get_fallbacks(AgentRole::Reviewer),
            &["codex", "goose"]
        );
    }

    #[test]
    fn test_fallback_config_has_fallbacks() {
        let mut config = FallbackConfig::default();
        assert!(!config.has_fallbacks(AgentRole::Developer));
        assert!(!config.has_fallbacks(AgentRole::Reviewer));

        config.developer = vec!["claude".to_string()];
        assert!(config.has_fallbacks(AgentRole::Developer));
        assert!(!config.has_fallbacks(AgentRole::Reviewer));
    }

    #[test]
    fn test_agent_error_kind_classify() {
        // Rate limiting
        assert_eq!(
            AgentErrorKind::classify(1, "rate limit exceeded"),
            AgentErrorKind::RateLimited
        );
        assert_eq!(
            AgentErrorKind::classify(1, "Error: 429 Too Many Requests"),
            AgentErrorKind::RateLimited
        );

        // Token exhaustion
        assert_eq!(
            AgentErrorKind::classify(1, "context length exceeded"),
            AgentErrorKind::TokenExhausted
        );
        assert_eq!(
            AgentErrorKind::classify(1, "maximum token limit"),
            AgentErrorKind::TokenExhausted
        );

        // API unavailable
        assert_eq!(
            AgentErrorKind::classify(1, "service unavailable"),
            AgentErrorKind::ApiUnavailable
        );
        assert_eq!(
            AgentErrorKind::classify(1, "connection refused"),
            AgentErrorKind::ApiUnavailable
        );

        // Auth failures
        assert_eq!(
            AgentErrorKind::classify(1, "unauthorized"),
            AgentErrorKind::AuthFailure
        );
        assert_eq!(
            AgentErrorKind::classify(1, "invalid api key"),
            AgentErrorKind::AuthFailure
        );

        // Command not found
        assert_eq!(
            AgentErrorKind::classify(127, ""),
            AgentErrorKind::CommandNotFound
        );
        assert_eq!(
            AgentErrorKind::classify(1, "command not found"),
            AgentErrorKind::CommandNotFound
        );
    }

    #[test]
    fn test_agent_error_kind_should_retry() {
        assert!(AgentErrorKind::RateLimited.should_retry());
        assert!(AgentErrorKind::ApiUnavailable.should_retry());
        assert!(AgentErrorKind::Transient.should_retry());

        assert!(!AgentErrorKind::TokenExhausted.should_retry());
        assert!(!AgentErrorKind::AuthFailure.should_retry());
        assert!(!AgentErrorKind::CommandNotFound.should_retry());
        assert!(!AgentErrorKind::Permanent.should_retry());
    }

    #[test]
    fn test_agent_error_kind_should_fallback() {
        assert!(AgentErrorKind::TokenExhausted.should_fallback());
        assert!(AgentErrorKind::AuthFailure.should_fallback());
        assert!(AgentErrorKind::CommandNotFound.should_fallback());

        assert!(!AgentErrorKind::RateLimited.should_fallback());
        assert!(!AgentErrorKind::ApiUnavailable.should_fallback());
        assert!(!AgentErrorKind::Transient.should_fallback());
        assert!(!AgentErrorKind::Permanent.should_fallback());
    }

    #[test]
    fn test_registry_available_fallbacks() {
        let mut registry = AgentRegistry::new().unwrap();
        registry.set_fallback(FallbackConfig {
            developer: vec![
                "claude".to_string(),
                "nonexistent".to_string(),
                "codex".to_string(),
            ],
            reviewer: vec![],
            max_retries: 3,
            retry_delay_ms: 1000,
        });

        let fallbacks = registry.available_fallbacks(AgentRole::Developer);
        assert!(fallbacks.contains(&"claude"));
        assert!(fallbacks.contains(&"codex"));
        assert!(!fallbacks.contains(&"nonexistent"));
    }

    #[test]
    fn test_fallback_config_from_toml() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("agents.toml");

        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[fallback]
developer = ["claude", "codex", "goose"]
reviewer = ["codex", "claude"]
max_retries = 5
retry_delay_ms = 2000

[agents.testbot]
cmd = "testbot exec"
"#
        )
        .unwrap();

        let registry = AgentRegistry::with_config_file(&config_path).unwrap();
        let fallback = registry.fallback_config();

        assert_eq!(fallback.developer, vec!["claude", "codex", "goose"]);
        assert_eq!(fallback.reviewer, vec!["codex", "claude"]);
        assert_eq!(fallback.max_retries, 5);
        assert_eq!(fallback.retry_delay_ms, 2000);
    }

    #[test]
    fn test_agent_role_display() {
        assert_eq!(format!("{}", AgentRole::Developer), "developer");
        assert_eq!(format!("{}", AgentRole::Reviewer), "reviewer");
    }

    #[test]
    fn test_agent_chain_alias() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("agents.toml");

        // Use the new [agent_chain] section name
        let mut file = std::fs::File::create(&config_path).unwrap();
        writeln!(
            file,
            r#"
[agent_chain]
developer = ["opencode", "claude", "codex"]
reviewer = ["claude", "codex"]
max_retries = 2
retry_delay_ms = 500
"#
        )
        .unwrap();

        let registry = AgentRegistry::with_config_file(&config_path).unwrap();
        let fallback = registry.fallback_config();

        // Should work with agent_chain alias
        assert_eq!(fallback.developer, vec!["opencode", "claude", "codex"]);
        assert_eq!(fallback.reviewer, vec!["claude", "codex"]);
        assert_eq!(fallback.max_retries, 2);
        assert_eq!(fallback.retry_delay_ms, 500);
    }

    #[test]
    fn test_ensure_config_exists_creates_file() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join(".agent/agents.toml");

        // File should not exist initially
        assert!(!config_path.exists());

        // ensure_config_exists should create it
        let result = AgentsConfigFile::ensure_config_exists(&config_path).unwrap();
        assert_eq!(result, ConfigInitResult::Created);

        // File should now exist
        assert!(config_path.exists());

        // Content should match the default template
        let content = fs::read_to_string(&config_path).unwrap();
        assert!(content.contains("Ralph Agents Configuration File"));
        assert!(content.contains("[agents.claude]"));
        assert!(content.contains("[agents.codex]"));
    }

    #[test]
    fn test_ensure_config_exists_already_exists() {
        let dir = tempfile::tempdir().unwrap();
        let agent_dir = dir.path().join(".agent");
        fs::create_dir_all(&agent_dir).unwrap();
        let config_path = agent_dir.join("agents.toml");

        // Create an existing file
        fs::write(&config_path, "# Custom config\n").unwrap();

        // ensure_config_exists should return AlreadyExists
        let result = AgentsConfigFile::ensure_config_exists(&config_path).unwrap();
        assert_eq!(result, ConfigInitResult::AlreadyExists);

        // Content should be unchanged
        let content = fs::read_to_string(&config_path).unwrap();
        assert_eq!(content, "# Custom config\n");
    }

    #[test]
    fn test_ensure_config_exists_creates_parent_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let config_path = dir.path().join("deep/nested/path/.agent/agents.toml");

        // Parent directories don't exist
        assert!(!config_path.parent().unwrap().exists());

        // ensure_config_exists should create parent directories
        let result = AgentsConfigFile::ensure_config_exists(&config_path).unwrap();
        assert_eq!(result, ConfigInitResult::Created);

        // Both file and parent directories should exist
        assert!(config_path.exists());
        assert!(config_path.parent().unwrap().exists());
    }

    #[test]
    fn test_default_agents_toml_is_valid() {
        // Verify the embedded default template can be parsed
        let config: AgentsConfigFile = toml::from_str(DEFAULT_AGENTS_TOML).unwrap();

        // Check that all expected agents are present
        assert!(config.agents.contains_key("claude"));
        assert!(config.agents.contains_key("codex"));
        assert!(config.agents.contains_key("opencode"));
        assert!(config.agents.contains_key("aider"));
        assert!(config.agents.contains_key("goose"));
        assert!(config.agents.contains_key("cline"));
        assert!(config.agents.contains_key("continue"));
        assert!(config.agents.contains_key("amazon-q"));
        assert!(config.agents.contains_key("gemini"));
        assert!(config.agents.contains_key("driver"));
        assert!(config.agents.contains_key("reviewer"));

        // Verify Claude config is correct
        let claude = &config.agents["claude"];
        assert_eq!(claude.cmd, "claude -p");
        assert_eq!(claude.json_parser, "claude");
    }

    #[test]
    fn test_registry_defaults_come_from_default_toml() {
        let config: AgentsConfigFile = toml::from_str(DEFAULT_AGENTS_TOML).unwrap();
        let registry = AgentRegistry::new().unwrap();

        let mut expected_names: Vec<String> = config.agents.keys().cloned().collect();
        expected_names.sort();

        let mut actual_names: Vec<String> = registry.agents.keys().cloned().collect();
        actual_names.sort();

        assert_eq!(expected_names, actual_names);

        for (name, cfg_toml) in config.agents {
            let expected: AgentConfig = cfg_toml.into();
            let actual = registry.get(&name).unwrap();
            assert_eq!(actual.cmd, expected.cmd);
            assert_eq!(actual.json_flag, expected.json_flag);
            assert_eq!(actual.yolo_flag, expected.yolo_flag);
            assert_eq!(actual.verbose_flag, expected.verbose_flag);
            assert_eq!(actual.can_commit, expected.can_commit);
            assert_eq!(actual.json_parser, expected.json_parser);
        }
    }

    #[test]
    fn test_with_merged_configs_no_files() {
        let dir = tempfile::tempdir().unwrap();
        let local_path = dir.path().join("nonexistent/agents.toml");

        let (registry, sources, warnings) =
            AgentRegistry::with_merged_configs(&local_path).unwrap();

        // Should have built-in defaults
        assert!(registry.is_known("claude"));
        assert!(registry.is_known("codex"));

        // No sources loaded (no files existed)
        assert!(sources.is_empty());
        assert!(warnings.is_empty());
    }

    #[test]
    fn test_with_merged_configs_local_only() {
        use std::io::Write;

        let dir = tempfile::tempdir().unwrap();
        let local_path = dir.path().join("agents.toml");

        // Create a local config that overrides claude
        let mut file = std::fs::File::create(&local_path).unwrap();
        writeln!(
            file,
            r#"
[agents.claude]
cmd = "claude-custom"
json_parser = "generic"

[agents.mybot]
cmd = "mybot run"
"#
        )
        .unwrap();

        let (registry, sources, warnings) =
            AgentRegistry::with_merged_configs(&local_path).unwrap();
        assert!(warnings.is_empty());

        // Should have both built-in and custom agents
        assert!(registry.is_known("codex")); // Built-in
        assert!(registry.is_known("mybot")); // Custom

        // Claude should be overridden
        let claude = registry.get("claude").unwrap();
        assert_eq!(claude.cmd, "claude-custom");
        assert_eq!(claude.json_parser, JsonParserType::Generic);

        // One source should be loaded
        assert_eq!(sources.len(), 1);
        assert_eq!(sources[0].path, local_path);
        assert_eq!(sources[0].agents_loaded, 2); // claude + mybot
    }

    #[test]
    fn test_global_config_dir_returns_some() {
        // Should return Some on most systems (may fail in very minimal environments)
        // This is more of a smoke test
        if let Some(path) = global_config_dir() {
            assert!(path.ends_with("ralph") || path.to_string_lossy().contains("ralph"));
        }
    }

    #[test]
    fn test_global_agents_config_path() {
        if let Some(path) = global_agents_config_path() {
            assert!(path.ends_with("agents.toml"));
            assert!(path.to_string_lossy().contains("ralph"));
        }
    }

    #[test]
    fn test_config_source_struct() {
        let source = ConfigSource {
            path: PathBuf::from("/test/agents.toml"),
            agents_loaded: 5,
        };
        assert_eq!(source.path, PathBuf::from("/test/agents.toml"));
        assert_eq!(source.agents_loaded, 5);
    }

    #[test]
    fn test_validate_agent_chains_empty() {
        let mut registry = AgentRegistry::new().unwrap();
        registry.set_fallback(FallbackConfig::default());
        assert!(registry.validate_agent_chains().is_err());
        let err = registry.validate_agent_chains().unwrap_err();
        assert!(err.contains("No agent chain configured"));
    }

    #[test]
    fn test_validate_agent_chains_developer_only() {
        let mut registry = AgentRegistry::new().unwrap();
        registry.set_fallback(FallbackConfig {
            developer: vec!["claude".to_string()],
            reviewer: vec![],
            ..Default::default()
        });
        assert!(registry.validate_agent_chains().is_err());
        let err = registry.validate_agent_chains().unwrap_err();
        assert!(err.contains("No reviewer agent chain"));
    }

    #[test]
    fn test_validate_agent_chains_reviewer_only() {
        let mut registry = AgentRegistry::new().unwrap();
        registry.set_fallback(FallbackConfig {
            developer: vec![],
            reviewer: vec!["codex".to_string()],
            ..Default::default()
        });
        assert!(registry.validate_agent_chains().is_err());
        let err = registry.validate_agent_chains().unwrap_err();
        assert!(err.contains("No developer agent chain"));
    }

    #[test]
    fn test_validate_agent_chains_complete() {
        let mut registry = AgentRegistry::new().unwrap();
        registry.set_fallback(FallbackConfig {
            developer: vec!["claude".to_string()],
            reviewer: vec!["codex".to_string()],
            ..Default::default()
        });
        assert!(registry.validate_agent_chains().is_ok());
    }
}
