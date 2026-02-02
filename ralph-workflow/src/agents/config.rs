//! Agent configuration types and TOML parsing.
//!
//! This module provides types for loading and managing agent configurations
//! from TOML files, including support for global and per-project configs.

#[path = "config/file.rs"]
mod file;
#[path = "config/types.rs"]
mod types;

pub use file::{AgentConfigError, AgentsConfigFile, ConfigInitResult};
pub use types::{AgentConfig, AgentConfigBuilder, ConfigSource, DEFAULT_AGENTS_TOML};
