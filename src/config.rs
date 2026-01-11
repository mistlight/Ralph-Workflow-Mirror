//! Configuration Module
//!
//! Handles environment variables and configuration for Ralph.

use std::env;
use std::path::PathBuf;

/// Verbosity levels for output
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Verbosity {
    /// Quiet - minimal output, aggressive truncation
    Quiet = 0,
    /// Normal - balanced output with moderate truncation
    Normal = 1,
    /// Verbose - expanded output limits (default)
    Verbose = 2,
    /// Full - no truncation, show all content
    Full = 3,
    /// Debug - maximum verbosity, includes raw JSON and detailed info
    Debug = 4,
}

impl From<u8> for Verbosity {
    fn from(v: u8) -> Self {
        match v {
            0 => Verbosity::Quiet,
            1 => Verbosity::Normal,
            2 => Verbosity::Verbose,
            3 => Verbosity::Full,
            _ => Verbosity::Debug,
        }
    }
}

impl Verbosity {
    /// Get truncation limit for content type
    ///
    /// Content types:
    /// - "text": Assistant text output
    /// - "tool_result": Tool execution results
    /// - "tool_input": Tool input parameters
    /// - "user": User messages
    /// - "result": Final result summaries
    /// - "command": Command execution strings
    /// - "agent_msg": Agent messages/thinking
    pub fn truncate_limit(&self, content_type: &str) -> usize {
        match self {
            Verbosity::Quiet => match content_type {
                "text" => 80,
                "tool_result" => 60,
                "tool_input" => 40,
                "user" => 40,
                "result" => 300,
                "command" => 60,
                "agent_msg" => 80,
                _ => 60,
            },
            // Normal mode: increased limits for better usability
            // Previously too aggressive truncation made debugging difficult
            Verbosity::Normal => match content_type {
                "text" => 400,
                "tool_result" => 300,
                "tool_input" => 200,
                "user" => 200,
                "result" => 1500,
                "command" => 200,
                "agent_msg" => 400,
                _ => 300,
            },
            // Verbose is the default - show generous amounts of context
            // Users need to see what's happening to understand agent behavior
            Verbosity::Verbose => match content_type {
                "text" => 800,
                "tool_result" => 600,
                "tool_input" => 500,
                "user" => 400,
                "result" => 3000,
                "command" => 400,
                "agent_msg" => 800,
                _ => 600,
            },
            // Full shows everything (essentially unlimited)
            Verbosity::Full | Verbosity::Debug => 999999,
        }
    }

    /// Returns true if this verbosity level should show debug information
    pub fn is_debug(&self) -> bool {
        matches!(self, Verbosity::Debug)
    }

    /// Returns true if this verbosity level is at least Verbose
    pub fn is_verbose(&self) -> bool {
        matches!(
            self,
            Verbosity::Verbose | Verbosity::Full | Verbosity::Debug
        )
    }

    /// Returns true if tool inputs should be shown (Normal and above)
    ///
    /// Tool inputs provide crucial context for understanding what the agent is doing.
    /// They are now shown at Normal level and above for better usability.
    pub fn show_tool_input(&self) -> bool {
        !matches!(self, Verbosity::Quiet)
    }
}

/// Ralph configuration
#[derive(Debug, Clone)]
pub struct Config {
    /// Developer (driver) agent (default: claude)
    pub developer_agent: String,
    /// Reviewer agent (default: codex)
    pub reviewer_agent: String,
    /// Developer command override (alias: CLAUDE_CMD)
    pub developer_cmd: Option<String>,
    /// Reviewer command override (alias: CODEX_CMD)
    pub reviewer_cmd: Option<String>,
    /// Number of developer iterations (alias: CLAUDE_ITERS)
    pub developer_iters: u32,
    /// Number of reviewer re-review passes after fix (alias: CODEX_REVIEWS)
    pub reviewer_reviews: u32,
    /// Fast check command (optional)
    pub fast_check_cmd: Option<String>,
    /// Full check command (optional)
    pub full_check_cmd: Option<String>,
    /// Interactive mode (keep agent in foreground)
    pub interactive: bool,
    /// Path to save last prompt
    pub prompt_path: PathBuf,
    /// Path to agents configuration file (default: .agent/agents.toml)
    pub agents_config_path: PathBuf,
    /// Developer context level (0=minimal, 1=normal)
    pub developer_context: u8,
    /// Reviewer context level (0=minimal/fresh eyes, 1=normal)
    pub reviewer_context: u8,
    /// Verbosity level
    pub verbosity: Verbosity,
    /// Commit message
    pub commit_msg: String,
    /// Enable automatic agent fallback on errors
    pub use_fallback: bool,
}

impl Config {
    /// Load configuration from environment variables
    pub fn from_env() -> Self {
        let developer_agent = env::var("RALPH_DEVELOPER_AGENT")
            .or_else(|_| env::var("RALPH_DRIVER_AGENT"))
            .unwrap_or_else(|_| "claude".to_string());
        let reviewer_agent =
            env::var("RALPH_REVIEWER_AGENT").unwrap_or_else(|_| "codex".to_string());

        let developer_cmd = env::var("RALPH_DEVELOPER_CMD")
            .ok()
            .or_else(|| env::var("CLAUDE_CMD").ok());
        let reviewer_cmd = env::var("RALPH_REVIEWER_CMD")
            .ok()
            .or_else(|| env::var("CODEX_CMD").ok());

        Self {
            developer_agent,
            reviewer_agent,
            developer_cmd,
            reviewer_cmd,
            developer_iters: env::var("RALPH_DEVELOPER_ITERS")
                .ok()
                .or_else(|| env::var("CLAUDE_ITERS").ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(5),
            reviewer_reviews: env::var("RALPH_REVIEWER_REVIEWS")
                .ok()
                .or_else(|| env::var("CODEX_REVIEWS").ok())
                .and_then(|s| s.parse().ok())
                .unwrap_or(2),
            fast_check_cmd: env::var("FAST_CHECK_CMD").ok().filter(|s| !s.is_empty()),
            full_check_cmd: env::var("FULL_CHECK_CMD").ok().filter(|s| !s.is_empty()),
            interactive: env::var("RALPH_INTERACTIVE")
                .map(|s| s == "1")
                .unwrap_or(true),
            prompt_path: PathBuf::from(
                env::var("RALPH_PROMPT_PATH")
                    .unwrap_or_else(|_| ".agent/last_prompt.txt".to_string()),
            ),
            agents_config_path: PathBuf::from(
                env::var("RALPH_AGENTS_CONFIG")
                    .unwrap_or_else(|_| ".agent/agents.toml".to_string()),
            ),
            developer_context: env::var("RALPH_DEVELOPER_CONTEXT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(1),
            reviewer_context: env::var("RALPH_REVIEWER_CONTEXT")
                .ok()
                .and_then(|s| s.parse().ok())
                .unwrap_or(0),
            verbosity: env::var("RALPH_VERBOSITY")
                .ok()
                .and_then(|s| s.parse::<u8>().ok())
                .map(Verbosity::from)
                .unwrap_or(Verbosity::Verbose),
            commit_msg: "chore: apply PROMPT loop + review/fix/review".to_string(),
            use_fallback: env::var("RALPH_USE_FALLBACK")
                .map(|s| s == "1" || s.to_lowercase() == "true")
                .unwrap_or(false),
        }
    }

    /// Set the commit message
    pub fn with_commit_msg(mut self, msg: String) -> Self {
        self.commit_msg = msg;
        self
    }
}

impl Default for Config {
    fn default() -> Self {
        Self::from_env()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_verbosity_from_u8() {
        assert_eq!(Verbosity::from(0), Verbosity::Quiet);
        assert_eq!(Verbosity::from(1), Verbosity::Normal);
        assert_eq!(Verbosity::from(2), Verbosity::Verbose);
        assert_eq!(Verbosity::from(3), Verbosity::Full);
        assert_eq!(Verbosity::from(4), Verbosity::Debug);
        assert_eq!(Verbosity::from(100), Verbosity::Debug);
    }

    #[test]
    fn test_truncate_limits() {
        // Quiet has reduced limits
        assert_eq!(Verbosity::Quiet.truncate_limit("text"), 80);
        assert_eq!(Verbosity::Quiet.truncate_limit("tool_input"), 40);

        // Normal has increased limits for better usability
        assert_eq!(Verbosity::Normal.truncate_limit("text"), 400);
        assert_eq!(Verbosity::Normal.truncate_limit("tool_input"), 200);

        // Verbose (default) has generous limits for understanding agent behavior
        assert_eq!(Verbosity::Verbose.truncate_limit("text"), 800);
        assert_eq!(Verbosity::Verbose.truncate_limit("tool_input"), 500);

        // Full and Debug have unlimited
        assert_eq!(Verbosity::Full.truncate_limit("text"), 999999);
        assert_eq!(Verbosity::Debug.truncate_limit("text"), 999999);
    }

    #[test]
    fn test_verbosity_helpers() {
        assert!(!Verbosity::Quiet.is_debug());
        assert!(!Verbosity::Normal.is_debug());
        assert!(!Verbosity::Verbose.is_debug());
        assert!(!Verbosity::Full.is_debug());
        assert!(Verbosity::Debug.is_debug());

        assert!(!Verbosity::Quiet.is_verbose());
        assert!(!Verbosity::Normal.is_verbose());
        assert!(Verbosity::Verbose.is_verbose());
        assert!(Verbosity::Full.is_verbose());
        assert!(Verbosity::Debug.is_verbose());

        // show_tool_input: true for Normal and above, false for Quiet
        assert!(!Verbosity::Quiet.show_tool_input());
        assert!(Verbosity::Normal.show_tool_input());
        assert!(Verbosity::Verbose.show_tool_input());
        assert!(Verbosity::Full.show_tool_input());
        assert!(Verbosity::Debug.show_tool_input());
    }

    #[test]
    fn test_config_defaults() {
        let config = Config::from_env();
        assert_eq!(config.developer_agent, "claude");
        assert_eq!(config.reviewer_agent, "codex");
        assert_eq!(config.developer_iters, 5);
        assert_eq!(config.reviewer_reviews, 2);
        // Default verbosity is now Verbose
        assert_eq!(config.verbosity, Verbosity::Verbose);
    }

    #[test]
    fn test_use_fallback_default() {
        // Default should be false when env var is not set
        let config = Config::from_env();
        assert!(!config.use_fallback);
    }
}
