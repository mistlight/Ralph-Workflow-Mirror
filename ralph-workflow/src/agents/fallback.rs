//! Fallback chain configuration for agent fault tolerance.
//!
//! This module defines the `FallbackConfig` structure that controls how Ralph
//! handles agent failures. It supports:
//! - Agent-level fallback (try different agents)
//! - Provider-level fallback (try different models within same agent)
//! - Exponential backoff with cycling

use serde::Deserialize;
use std::collections::HashMap;

/// Agent role (developer, reviewer, or commit).
///
/// Each role can have its own chain of fallback agents.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AgentRole {
    /// Developer agent: implements features based on PROMPT.md.
    Developer,
    /// Reviewer agent: reviews code and fixes issues.
    Reviewer,
    /// Commit agent: generates commit messages from diffs.
    Commit,
}

impl std::fmt::Display for AgentRole {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Developer => write!(f, "developer"),
            Self::Reviewer => write!(f, "reviewer"),
            Self::Commit => write!(f, "commit"),
        }
    }
}

/// Agent chain configuration for preferred agents and fallback switching.
///
/// The agent chain defines both:
/// 1. The **preferred agent** (first in the list) for each role
/// 2. The **fallback agents** (remaining in the list) to try if the preferred fails
///
/// This provides a unified way to configure which agents to use and in what order.
/// Ralph automatically switches to the next agent in the chain when encountering
/// errors like rate limits or auth failures.
///
/// ## Provider-Level Fallback
///
/// In addition to agent-level fallback, you can configure provider-level fallback
/// within a single agent using the `provider_fallback` field. This is useful for
/// agents like opencode that support multiple providers/models via the `-m` flag.
///
/// Example:
/// ```toml
/// [agent_chain]
/// provider_fallback.opencode = ["-m opencode/glm-4.7-free", "-m opencode/claude-sonnet-4"]
/// ```
///
/// ## Exponential Backoff and Cycling
///
/// When all fallbacks are exhausted, Ralph uses exponential backoff and cycles
/// back to the first agent in the chain:
/// - Base delay starts at `retry_delay_ms` (default: 1000ms)
/// - Each cycle multiplies by `backoff_multiplier` (default: 2.0)
/// - Capped at `max_backoff_ms` (default: 60000ms = 1 minute)
/// - Maximum cycles controlled by `max_cycles` (default: 3)
#[derive(Debug, Clone, Deserialize)]
pub struct FallbackConfig {
    /// Ordered list of agents for developer role (first = preferred, rest = fallbacks).
    #[serde(default)]
    pub developer: Vec<String>,
    /// Ordered list of agents for reviewer role (first = preferred, rest = fallbacks).
    #[serde(default)]
    pub reviewer: Vec<String>,
    /// Ordered list of agents for commit role (first = preferred, rest = fallbacks).
    #[serde(default)]
    pub commit: Vec<String>,
    /// Provider-level fallback: maps agent name to list of model flags to try.
    /// Example: `opencode = ["-m opencode/glm-4.7-free", "-m opencode/claude-sonnet-4"]`
    #[serde(default)]
    pub provider_fallback: HashMap<String, Vec<String>>,
    /// Maximum number of retries per agent before moving to next.
    #[serde(default = "default_max_retries")]
    pub max_retries: u32,
    /// Base delay between retries in milliseconds.
    #[serde(default = "default_retry_delay_ms")]
    pub retry_delay_ms: u64,
    /// Multiplier for exponential backoff (default: 2).
    #[serde(default = "default_backoff_multiplier")]
    pub backoff_multiplier: u64,
    /// Maximum backoff delay in milliseconds (default: 60000 = 1 minute).
    #[serde(default = "default_max_backoff_ms")]
    pub max_backoff_ms: u64,
    /// Maximum number of cycles through all agents before giving up (default: 3).
    #[serde(default = "default_max_cycles")]
    pub max_cycles: u32,
}

const fn default_max_retries() -> u32 {
    3
}

const fn default_retry_delay_ms() -> u64 {
    1000
}

const fn default_backoff_multiplier() -> u64 {
    2
}

const fn default_max_backoff_ms() -> u64 {
    60000 // 1 minute
}

const fn default_max_cycles() -> u32 {
    3
}

impl Default for FallbackConfig {
    fn default() -> Self {
        Self {
            developer: Vec::new(),
            reviewer: Vec::new(),
            commit: Vec::new(),
            provider_fallback: HashMap::new(),
            max_retries: default_max_retries(),
            retry_delay_ms: default_retry_delay_ms(),
            backoff_multiplier: default_backoff_multiplier(),
            max_backoff_ms: default_max_backoff_ms(),
            max_cycles: default_max_cycles(),
        }
    }
}

impl FallbackConfig {
    /// Calculate exponential backoff delay for a given cycle.
    ///
    /// Uses the formula: min(base * multiplier^cycle, `max_backoff`)
    ///
    /// Uses saturating integer arithmetic for safety.
    pub fn calculate_backoff(&self, cycle: u32) -> u64 {
        // Calculate base * multiplier^cycle using saturating arithmetic
        let mut delay = self.retry_delay_ms;
        for _ in 0..cycle {
            delay = delay.saturating_mul(self.backoff_multiplier);
            // Early exit to avoid unnecessary computation
            if delay >= self.max_backoff_ms {
                return self.max_backoff_ms;
            }
        }

        delay.min(self.max_backoff_ms)
    }

    /// Get fallback agents for a role.
    pub fn get_fallbacks(&self, role: AgentRole) -> &[String] {
        match role {
            AgentRole::Developer => &self.developer,
            AgentRole::Reviewer => &self.reviewer,
            AgentRole::Commit => &self.commit,
        }
    }

    /// Check if fallback is configured for a role.
    pub fn has_fallbacks(&self, role: AgentRole) -> bool {
        !self.get_fallbacks(role).is_empty()
    }

    /// Get provider-level fallback model flags for an agent.
    ///
    /// Returns the list of model flags to try for the given agent name.
    /// Empty slice if no provider fallback is configured for this agent.
    pub fn get_provider_fallbacks(&self, agent_name: &str) -> &[String] {
        self.provider_fallback
            .get(agent_name)
            .map_or(&[], std::vec::Vec::as_slice)
    }

    /// Check if provider-level fallback is configured for an agent.
    pub fn has_provider_fallbacks(&self, agent_name: &str) -> bool {
        self.provider_fallback
            .get(agent_name)
            .is_some_and(|v| !v.is_empty())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_role_display() {
        assert_eq!(format!("{}", AgentRole::Developer), "developer");
        assert_eq!(format!("{}", AgentRole::Reviewer), "reviewer");
        assert_eq!(format!("{}", AgentRole::Commit), "commit");
    }

    #[test]
    fn test_fallback_config_defaults() {
        let config = FallbackConfig::default();
        assert!(config.developer.is_empty());
        assert!(config.reviewer.is_empty());
        assert!(config.commit.is_empty());
        assert_eq!(config.max_retries, 3);
        assert_eq!(config.retry_delay_ms, 1000);
        assert_eq!(config.backoff_multiplier, 2);
        assert_eq!(config.max_backoff_ms, 60000);
        assert_eq!(config.max_cycles, 3);
    }

    #[test]
    fn test_fallback_config_calculate_backoff() {
        let config = FallbackConfig {
            retry_delay_ms: 1000,
            backoff_multiplier: 2,
            max_backoff_ms: 60000,
            ..Default::default()
        };

        assert_eq!(config.calculate_backoff(0), 1000);
        assert_eq!(config.calculate_backoff(1), 2000);
        assert_eq!(config.calculate_backoff(2), 4000);
        assert_eq!(config.calculate_backoff(3), 8000);

        // Should cap at max
        assert_eq!(config.calculate_backoff(10), 60000);
    }

    #[test]
    fn test_fallback_config_get_fallbacks() {
        let config = FallbackConfig {
            developer: vec!["claude".to_string(), "codex".to_string()],
            reviewer: vec!["codex".to_string()],
            ..Default::default()
        };

        assert_eq!(
            config.get_fallbacks(AgentRole::Developer),
            &["claude", "codex"]
        );
        assert_eq!(config.get_fallbacks(AgentRole::Reviewer), &["codex"]);
    }

    #[test]
    fn test_fallback_config_has_fallbacks() {
        let config = FallbackConfig {
            developer: vec!["claude".to_string()],
            reviewer: vec![],
            ..Default::default()
        };

        assert!(config.has_fallbacks(AgentRole::Developer));
        assert!(!config.has_fallbacks(AgentRole::Reviewer));
    }

    #[test]
    fn test_fallback_config_defaults_provider_fallback() {
        let config = FallbackConfig::default();
        assert!(config.get_provider_fallbacks("opencode").is_empty());
        assert!(!config.has_provider_fallbacks("opencode"));
    }

    #[test]
    fn test_provider_fallback_config() {
        let mut provider_fallback = HashMap::new();
        provider_fallback.insert(
            "opencode".to_string(),
            vec![
                "-m opencode/glm-4.7-free".to_string(),
                "-m opencode/claude-sonnet-4".to_string(),
            ],
        );

        let config = FallbackConfig {
            provider_fallback,
            ..Default::default()
        };

        let fallbacks = config.get_provider_fallbacks("opencode");
        assert_eq!(fallbacks.len(), 2);
        assert_eq!(fallbacks[0], "-m opencode/glm-4.7-free");
        assert_eq!(fallbacks[1], "-m opencode/claude-sonnet-4");

        assert!(config.has_provider_fallbacks("opencode"));
        assert!(!config.has_provider_fallbacks("claude"));
    }

    #[test]
    fn test_fallback_config_from_toml() {
        let toml_str = r#"
            developer = ["claude", "codex"]
            reviewer = ["codex", "claude"]
            max_retries = 5
            retry_delay_ms = 2000

            [provider_fallback]
            opencode = ["-m opencode/glm-4.7-free", "-m zai/glm-4.7"]
        "#;

        let config: FallbackConfig = toml::from_str(toml_str).unwrap();
        assert_eq!(config.developer, vec!["claude", "codex"]);
        assert_eq!(config.reviewer, vec!["codex", "claude"]);
        assert_eq!(config.max_retries, 5);
        assert_eq!(config.retry_delay_ms, 2000);
        assert_eq!(config.get_provider_fallbacks("opencode").len(), 2);
    }
}
