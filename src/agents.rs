//! Agent Abstraction Module
//!
//! Provides a pluggable agent system for different
//! AI coding assistants (Claude, Codex, OpenCode, etc.)

use std::collections::HashMap;

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

/// Known agent type
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AgentType {
    Claude,
    Codex,
    OpenCode,
    Aider,
    Custom,
}

impl std::fmt::Display for AgentType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AgentType::Claude => write!(f, "claude"),
            AgentType::Codex => write!(f, "codex"),
            AgentType::OpenCode => write!(f, "opencode"),
            AgentType::Aider => write!(f, "aider"),
            AgentType::Custom => write!(f, "custom"),
        }
    }
}

impl AgentType {
    /// Parse agent type from string
    pub fn parse(s: &str) -> Option<AgentType> {
        match s.to_lowercase().as_str() {
            "claude" => Some(AgentType::Claude),
            "codex" => Some(AgentType::Codex),
            "opencode" => Some(AgentType::OpenCode),
            "aider" => Some(AgentType::Aider),
            _ => None,
        }
    }

    /// Detect agent type from command string
    pub fn from_cmd(cmd: &str) -> AgentType {
        if cmd.contains("claude") {
            AgentType::Claude
        } else if cmd.contains("codex") {
            AgentType::Codex
        } else if cmd.contains("opencode") {
            AgentType::OpenCode
        } else if cmd.contains("aider") {
            AgentType::Aider
        } else {
            AgentType::Custom
        }
    }
}

/// Agent registry
pub struct AgentRegistry {
    agents: HashMap<String, AgentConfig>,
}

impl AgentRegistry {
    /// Create a new registry with default agents
    pub fn new() -> Self {
        let mut registry = Self {
            agents: HashMap::new(),
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
        self.get(agent_name)
            .map(|c| c.build_cmd(true, true, true))
    }

    /// Get command for reviewer role
    pub fn reviewer_cmd(&self, agent_name: &str) -> Option<String> {
        self.get(agent_name)
            .map(|c| c.build_cmd(true, true, false))
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

        assert!(registry.is_known("claude"));
        assert!(registry.is_known("codex"));
        assert!(registry.is_known("opencode"));
        assert!(registry.is_known("aider"));
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
            },
        );

        assert!(registry.is_known("testbot"));
        let config = registry.get("testbot").unwrap();
        assert_eq!(config.cmd, "testbot run");
    }

    #[test]
    fn test_agent_type_parse() {
        assert_eq!(AgentType::parse("claude"), Some(AgentType::Claude));
        assert_eq!(AgentType::parse("CODEX"), Some(AgentType::Codex));
        assert_eq!(AgentType::parse("unknown"), None);
    }

    #[test]
    fn test_agent_type_from_cmd() {
        assert_eq!(AgentType::from_cmd("claude -p --json"), AgentType::Claude);
        assert_eq!(AgentType::from_cmd("codex exec --json"), AgentType::Codex);
        assert_eq!(AgentType::from_cmd("some-other-tool"), AgentType::Custom);
    }

    #[test]
    fn test_can_commit() {
        let registry = AgentRegistry::new();

        let claude = registry.get("claude").unwrap();
        assert!(claude.can_commit);

        let codex = registry.get("codex").unwrap();
        assert!(codex.can_commit);
    }
}
