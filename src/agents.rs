//! Agent Abstraction Module
//!
//! Provides a pluggable agent system for different
//! AI coding assistants (Claude, Codex, OpenCode, Goose, Cline, etc.)
//!
//! ## Configuration
//!
//! Agents can be configured via:
//! 1. Built-in defaults (claude, codex, opencode, aider, goose, cline, continue, amazon-q, gemini)
//! 2. TOML configuration file (`.agent/agents.toml`)
//! 3. Environment variables (`CLAUDE_CMD`, `CODEX_CMD`)
//! 4. Programmatic registration via `AgentRegistry::register()`
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
use std::path::Path;

/// JSON parser type for agent output
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub enum JsonParserType {
    /// Claude's stream-json format
    #[default]
    Claude,
    /// Codex's JSON format
    Codex,
    /// Generic line-based output (no parsing)
    Generic,
}

impl JsonParserType {
    /// Parse parser type from string
    pub fn parse(s: &str) -> Self {
        match s.to_lowercase().as_str() {
            "claude" => JsonParserType::Claude,
            "codex" => JsonParserType::Codex,
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
            JsonParserType::Generic => write!(f, "generic"),
        }
    }
}

/// Agent capabilities
#[derive(Debug, Clone)]
pub struct AgentConfig {
    /// Base command to run the agent
    pub cmd: String,
    /// Flag to enable JSON output
    pub json_flag: String,
    /// Flag for autonomous mode (no prompts)
    pub yolo_flag: String,
    /// Flag for verbose output
    pub verbose_flag: String,
    /// Whether the agent can run git commit
    pub can_commit: bool,
    /// Which JSON parser to use for this agent's output
    pub json_parser: JsonParserType,
}

/// TOML configuration for an agent (for deserialization)
#[derive(Debug, Clone, Deserialize)]
pub struct AgentConfigToml {
    /// Base command to run the agent
    pub cmd: String,
    /// Flag to enable JSON output (optional, defaults to empty)
    #[serde(default)]
    pub json_flag: String,
    /// Flag for autonomous mode (optional, defaults to empty)
    #[serde(default)]
    pub yolo_flag: String,
    /// Flag for verbose output (optional, defaults to empty)
    #[serde(default)]
    pub verbose_flag: String,
    /// Whether the agent can run git commit (optional, defaults to true)
    #[serde(default = "default_can_commit")]
    pub can_commit: bool,
    /// Which JSON parser to use: "claude", "codex", or "generic" (optional, defaults to "generic")
    #[serde(default)]
    pub json_parser: String,
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
pub struct AgentsConfigFile {
    /// Map of agent name to configuration
    #[serde(default)]
    pub agents: HashMap<String, AgentConfigToml>,
    /// Fallback configuration for agent switching
    #[serde(default)]
    pub fallback: FallbackConfig,
}

/// Error type for agent configuration loading
#[derive(Debug, thiserror::Error)]
pub enum AgentConfigError {
    #[error("Failed to read config file: {0}")]
    IoError(#[from] std::io::Error),
    #[error("Failed to parse TOML: {0}")]
    TomlError(#[from] toml::de::Error),
}

impl AgentsConfigFile {
    /// Load agents configuration from a TOML file
    ///
    /// Returns Ok(None) if the file doesn't exist.
    /// Returns Err if the file exists but can't be parsed.
    pub fn load_from_file<P: AsRef<Path>>(path: P) -> Result<Option<Self>, AgentConfigError> {
        let path = path.as_ref();
        if !path.exists() {
            return Ok(None);
        }

        let contents = fs::read_to_string(path)?;
        let config: AgentsConfigFile = toml::from_str(&contents)?;
        Ok(Some(config))
    }
}

impl AgentConfig {
    /// Build full command string with specified flags
    pub fn build_cmd(&self, json: bool, yolo: bool, verbose: bool) -> String {
        let mut parts = vec![self.cmd.clone()];

        if json && !self.json_flag.is_empty() {
            parts.push(self.json_flag.clone());
        }
        if yolo && !self.yolo_flag.is_empty() {
            parts.push(self.yolo_flag.clone());
        }
        if verbose && !self.verbose_flag.is_empty() {
            parts.push(self.verbose_flag.clone());
        }

        parts.join(" ")
    }
}

/// Fallback configuration for agent switching
#[derive(Debug, Clone, Deserialize)]
pub struct FallbackConfig {
    /// Ordered list of fallback agents for developer role
    #[serde(default)]
    pub developer: Vec<String>,
    /// Ordered list of fallback agents for reviewer role
    #[serde(default)]
    pub reviewer: Vec<String>,
    /// Maximum number of retries before giving up
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Delay between retries in milliseconds
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
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
    pub fn get_fallbacks(&self, role: AgentRole) -> &[String] {
        match role {
            AgentRole::Developer => &self.developer,
            AgentRole::Reviewer => &self.reviewer,
        }
    }

    /// Check if fallback is configured for a role
    pub fn has_fallbacks(&self, role: AgentRole) -> bool {
        !self.get_fallbacks(role).is_empty()
    }
}

/// Agent role (developer or reviewer)
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRole {
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
pub enum AgentErrorKind {
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
    pub fn should_retry(&self) -> bool {
        matches!(
            self,
            AgentErrorKind::RateLimited
                | AgentErrorKind::ApiUnavailable
                | AgentErrorKind::Transient
        )
    }

    /// Determine if this error should trigger a fallback to another agent
    pub fn should_fallback(&self) -> bool {
        matches!(
            self,
            AgentErrorKind::TokenExhausted
                | AgentErrorKind::AuthFailure
                | AgentErrorKind::CommandNotFound
        )
    }

    /// Classify an error from exit code and output
    pub fn classify(exit_code: i32, stderr: &str) -> Self {
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
pub struct AgentRegistry {
    agents: HashMap<String, AgentConfig>,
    fallback: FallbackConfig,
}

impl AgentRegistry {
    /// Create a new registry with default agents
    pub fn new() -> Self {
        let mut registry = Self {
            agents: HashMap::new(),
            fallback: FallbackConfig::default(),
        };

        // Register default agents
        registry.register(
            "claude",
            AgentConfig {
                cmd: "claude -p".to_string(),
                json_flag: "--output-format=stream-json".to_string(),
                yolo_flag: "--dangerously-skip-permissions".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Claude,
            },
        );

        // Role-friendly aliases (so users don't have to hardcode a specific tool name).
        // These intentionally default to the historical Claude/Codex pipeline, but can be
        // overridden in `.agent/agents.toml` by redefining `agents.driver` / `agents.reviewer`.
        registry.register(
            "driver",
            AgentConfig {
                cmd: "claude -p".to_string(),
                json_flag: "--output-format=stream-json".to_string(),
                yolo_flag: "--dangerously-skip-permissions".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Claude,
            },
        );

        registry.register(
            "codex",
            AgentConfig {
                cmd: "codex exec".to_string(),
                json_flag: "--json".to_string(),
                yolo_flag: "--yolo".to_string(),
                verbose_flag: String::new(),
                can_commit: true,
                json_parser: JsonParserType::Codex,
            },
        );

        registry.register(
            "reviewer",
            AgentConfig {
                cmd: "codex exec".to_string(),
                json_flag: "--json".to_string(),
                yolo_flag: "--yolo".to_string(),
                verbose_flag: String::new(),
                can_commit: true,
                json_parser: JsonParserType::Codex,
            },
        );

        registry.register(
            "opencode",
            AgentConfig {
                cmd: "opencode".to_string(),
                json_flag: "--json".to_string(),
                yolo_flag: "--auto".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Generic,
            },
        );

        registry.register(
            "aider",
            AgentConfig {
                cmd: "aider".to_string(),
                json_flag: String::new(),
                yolo_flag: "--yes".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Generic,
            },
        );

        // Goose - Block's open-source AI agent
        // https://github.com/block/goose
        registry.register(
            "goose",
            AgentConfig {
                cmd: "goose run".to_string(),
                json_flag: "--json".to_string(),
                yolo_flag: "--auto-approve".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Generic,
            },
        );

        // Cline - Autonomous coding agent
        // https://github.com/cline/cline
        registry.register(
            "cline",
            AgentConfig {
                cmd: "cline".to_string(),
                json_flag: "--json".to_string(),
                yolo_flag: "--auto".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Generic,
            },
        );

        // Continue.dev CLI (cn)
        // https://docs.continue.dev/guides/cli
        registry.register(
            "continue",
            AgentConfig {
                cmd: "cn".to_string(),
                json_flag: String::new(),
                yolo_flag: "--allow".to_string(),
                verbose_flag: String::new(),
                can_commit: true,
                json_parser: JsonParserType::Generic,
            },
        );

        // Amazon Q Developer CLI
        // https://docs.aws.amazon.com/amazonq/latest/qdeveloper-ug/command-line.html
        registry.register(
            "amazon-q",
            AgentConfig {
                cmd: "q chat".to_string(),
                json_flag: String::new(),
                yolo_flag: "--trust-all-tools".to_string(),
                verbose_flag: String::new(),
                can_commit: true,
                json_parser: JsonParserType::Generic,
            },
        );

        // Gemini CLI
        // https://github.com/google-gemini/gemini-cli
        registry.register(
            "gemini",
            AgentConfig {
                cmd: "gemini".to_string(),
                json_flag: "--json".to_string(),
                yolo_flag: "--auto".to_string(),
                verbose_flag: "--verbose".to_string(),
                can_commit: true,
                json_parser: JsonParserType::Generic,
            },
        );

        registry
    }

    /// Register a new agent
    pub fn register(&mut self, name: &str, config: AgentConfig) {
        self.agents.insert(name.to_string(), config);
    }

    /// Get agent configuration
    pub fn get(&self, name: &str) -> Option<&AgentConfig> {
        self.agents.get(name)
    }

    /// Check if agent exists
    pub fn is_known(&self, name: &str) -> bool {
        self.agents.contains_key(name)
    }

    /// List all registered agents
    pub fn list(&self) -> Vec<(&str, &AgentConfig)> {
        self.agents.iter().map(|(k, v)| (k.as_str(), v)).collect()
    }

    /// Get command for developer role
    pub fn developer_cmd(&self, agent_name: &str) -> Option<String> {
        self.get(agent_name).map(|c| c.build_cmd(true, true, true))
    }

    /// Get command for reviewer role
    pub fn reviewer_cmd(&self, agent_name: &str) -> Option<String> {
        self.get(agent_name).map(|c| c.build_cmd(true, true, false))
    }

    /// Get the JSON parser type for an agent
    pub fn parser_type(&self, agent_name: &str) -> JsonParserType {
        self.get(agent_name)
            .map(|c| c.json_parser)
            .unwrap_or(JsonParserType::Generic)
    }

    /// Load custom agents from a TOML configuration file
    ///
    /// Custom agents override built-in defaults if they have the same name.
    /// Returns the number of agents loaded, or an error if the file can't be parsed.
    pub fn load_from_file<P: AsRef<Path>>(&mut self, path: P) -> Result<usize, AgentConfigError> {
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
    pub fn with_config_file<P: AsRef<Path>>(path: P) -> Result<Self, AgentConfigError> {
        let mut registry = Self::new();
        registry.load_from_file(path)?;
        Ok(registry)
    }

    /// Get the fallback configuration
    pub fn fallback_config(&self) -> &FallbackConfig {
        &self.fallback
    }

    /// Set the fallback configuration
    pub fn set_fallback(&mut self, fallback: FallbackConfig) {
        self.fallback = fallback;
    }

    /// Get all fallback agents for a role that are registered in this registry
    pub fn available_fallbacks(&self, role: AgentRole) -> Vec<&str> {
        self.fallback
            .get_fallbacks(role)
            .iter()
            .filter(|name| self.is_known(name))
            .map(|s| s.as_str())
            .collect()
    }

    /// Check if an agent is available (command exists and is executable)
    pub fn is_agent_available(&self, name: &str) -> bool {
        if let Some(config) = self.get(name) {
            // Extract the base command (first word)
            let base_cmd = config.cmd.split_whitespace().next().unwrap_or(&config.cmd);
            // Check if the command exists in PATH (portable; avoids shelling out)
            which::which(base_cmd).is_ok()
        } else {
            false
        }
    }

    /// List all available (installed) agents
    pub fn list_available(&self) -> Vec<&str> {
        self.agents
            .keys()
            .filter(|name| self.is_agent_available(name))
            .map(|s| s.as_str())
            .collect()
    }
}

impl Default for AgentRegistry {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_registry_defaults() {
        let registry = AgentRegistry::new();

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
        let registry = AgentRegistry::new();

        let claude = registry.get("claude").unwrap();
        assert!(claude.cmd.contains("claude"));

        let codex = registry.get("codex").unwrap();
        assert!(codex.cmd.contains("codex"));
    }

    #[test]
    fn test_agent_build_cmd() {
        let registry = AgentRegistry::new();
        let claude = registry.get("claude").unwrap();

        let cmd = claude.build_cmd(true, true, false);
        assert!(cmd.contains("claude"));
        assert!(cmd.contains("json"));
        assert!(cmd.contains("skip-permissions"));
        assert!(!cmd.contains("verbose"));
    }

    #[test]
    fn test_agent_developer_cmd() {
        let registry = AgentRegistry::new();
        let cmd = registry.developer_cmd("claude").unwrap();
        assert!(cmd.contains("claude"));
        assert!(cmd.contains("json"));
    }

    #[test]
    fn test_agent_reviewer_cmd() {
        let registry = AgentRegistry::new();
        let cmd = registry.reviewer_cmd("codex").unwrap();
        assert!(cmd.contains("codex"));
        assert!(cmd.contains("json"));
    }

    #[test]
    fn test_register_custom_agent() {
        let mut registry = AgentRegistry::new();

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
        let registry = AgentRegistry::new();

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
        assert_eq!(JsonParserType::parse("generic"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("none"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("raw"), JsonParserType::Generic);
        assert_eq!(JsonParserType::parse("unknown"), JsonParserType::Generic);
    }

    #[test]
    fn test_json_parser_type_display() {
        assert_eq!(format!("{}", JsonParserType::Claude), "claude");
        assert_eq!(format!("{}", JsonParserType::Codex), "codex");
        assert_eq!(format!("{}", JsonParserType::Generic), "generic");
    }

    #[test]
    fn test_default_agent_parser_types() {
        let registry = AgentRegistry::new();

        assert_eq!(registry.parser_type("claude"), JsonParserType::Claude);
        assert_eq!(registry.parser_type("codex"), JsonParserType::Codex);
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
        let mut registry = AgentRegistry::new();
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

        let mut registry = AgentRegistry::new();
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
        let registry = AgentRegistry::new();

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
        let mut registry = AgentRegistry::new();
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
}
