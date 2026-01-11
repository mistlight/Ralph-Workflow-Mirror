//! Ralph: PROMPT-driven multi-agent orchestrator for git repos
//!
//! This crate provides the core functionality for orchestrating AI agents
//! (Claude, Codex, etc.) in a development workflow.
//!
//! ## Custom Agent Configuration
//!
//! Custom agents can be defined in `.agent/agents.toml`:
//!
//! ```toml
//! [agents.myagent]
//! cmd = "my-ai-tool run"
//! json_flag = "--json-stream"
//! yolo_flag = "--auto-fix"
//! verbose_flag = "--verbose"
//! can_commit = true
//! json_parser = "claude"  # Options: "claude", "codex", "generic"
//! ```
//!
//! Then set the agent via environment variable or CLI:
//! ```bash
//! RALPH_DEVELOPER_AGENT=myagent ralph
//! # or
//! ralph --developer-agent myagent
//! ```

pub mod agents;
pub mod colors;
pub mod config;
pub mod git_helpers;
pub mod json_parser;
pub mod prompts;
pub mod timer;
pub mod utils;

// Re-export core types for convenience
pub use agents::{
    AgentConfig, AgentConfigError, AgentConfigToml, AgentRegistry, AgentType, AgentsConfigFile,
    JsonParserType,
};
pub use config::Config;
