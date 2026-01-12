//! Environment variable parsing for configuration.
//!
//! This module handles loading configuration from environment variables.
//! Default values are applied when environment variables are not set.

use super::types::{Config, ReviewDepth, Verbosity};
use std::env;
use std::path::PathBuf;

/// Parse a boolean from an environment variable value.
///
/// Accepts common truthy and falsy values:
/// - Truthy: "1", "true", "yes", "y", "on"
/// - Falsy: "0", "false", "no", "n", "off"
///
/// # Arguments
///
/// * `value` - The string value to parse
///
/// # Returns
///
/// Returns `Some(true)` for truthy values, `Some(false)` for falsy values,
/// and `None` for empty or unrecognized values.
pub(crate) fn parse_env_bool(value: &str) -> Option<bool> {
    let normalized = value.trim().to_ascii_lowercase();
    match normalized.as_str() {
        "" => None,
        "1" | "true" | "yes" | "y" | "on" => Some(true),
        "0" | "false" | "no" | "n" | "off" => Some(false),
        _ => None,
    }
}

/// Load configuration from environment variables.
///
/// This function reads all RALPH_* environment variables and constructs
/// a [`Config`] with appropriate defaults for any unset variables.
///
/// # Environment Variables
///
/// ## Agent Selection
/// - `RALPH_DEVELOPER_AGENT` / `RALPH_DRIVER_AGENT`: Developer agent name
/// - `RALPH_REVIEWER_AGENT`: Reviewer agent name
/// - `RALPH_DEVELOPER_CMD`: Custom developer command
/// - `RALPH_REVIEWER_CMD`: Custom reviewer command
/// - `RALPH_DEVELOPER_MODEL`: Developer model override
/// - `RALPH_REVIEWER_MODEL`: Reviewer model override
/// - `RALPH_DEVELOPER_PROVIDER`: Developer provider override
/// - `RALPH_REVIEWER_PROVIDER`: Reviewer provider override
///
/// ## Iteration Control
/// - `RALPH_DEVELOPER_ITERS`: Number of developer iterations (default: 5)
/// - `RALPH_REVIEWER_REVIEWS`: Number of review passes (default: 2)
///
/// ## Check Commands
/// - `FAST_CHECK_CMD`: Fast check command (optional)
/// - `FULL_CHECK_CMD`: Full check command (optional)
///
/// ## Behavior
/// - `RALPH_INTERACTIVE`: Interactive mode (default: true)
/// - `RALPH_VERBOSITY`: Verbosity level 0-4 (default: 2/Verbose)
/// - `RALPH_AUTO_DETECT_STACK`: Auto-detect project stack (default: true)
/// - `RALPH_CHECKPOINT_ENABLED`: Enable checkpoints (default: true)
/// - `RALPH_STRICT_VALIDATION`: Strict PROMPT.md validation (default: false)
/// - `RALPH_REVIEW_DEPTH`: Review depth level (default: standard)
/// - `RALPH_ISOLATION_MODE`: Isolation mode (default: true)
///
/// ## Paths
/// - `RALPH_PROMPT_PATH`: Path to save last prompt
/// - `RALPH_AGENTS_CONFIG`: Path to agents.toml
///
/// ## Context Levels
/// - `RALPH_DEVELOPER_CONTEXT`: Developer context level (default: 1)
/// - `RALPH_REVIEWER_CONTEXT`: Reviewer context level (default: 0)
///
/// # Note
///
/// Agent selection defaults are NOT hardcoded here. The agent_chain
/// configuration in agents.toml is the single source of truth for
/// default agent selection. CLI/env vars can override the agent_chain.
pub(crate) fn from_env() -> Config {
    let developer_agent = env::var("RALPH_DEVELOPER_AGENT")
        .or_else(|_| env::var("RALPH_DRIVER_AGENT"))
        .ok();
    let reviewer_agent = env::var("RALPH_REVIEWER_AGENT").ok();

    let developer_cmd = env::var("RALPH_DEVELOPER_CMD").ok();
    let reviewer_cmd = env::var("RALPH_REVIEWER_CMD").ok();

    let developer_model = env::var("RALPH_DEVELOPER_MODEL").ok();
    let reviewer_model = env::var("RALPH_REVIEWER_MODEL").ok();

    let developer_provider = env::var("RALPH_DEVELOPER_PROVIDER").ok();
    let reviewer_provider = env::var("RALPH_REVIEWER_PROVIDER").ok();

    Config {
        developer_agent,
        reviewer_agent,
        developer_cmd,
        reviewer_cmd,
        developer_model,
        reviewer_model,
        developer_provider,
        reviewer_provider,
        developer_iters: env::var("RALPH_DEVELOPER_ITERS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(5),
        reviewer_reviews: env::var("RALPH_REVIEWER_REVIEWS")
            .ok()
            .and_then(|s| s.parse().ok())
            .unwrap_or(2),
        fast_check_cmd: env::var("FAST_CHECK_CMD").ok().filter(|s| !s.is_empty()),
        full_check_cmd: env::var("FULL_CHECK_CMD").ok().filter(|s| !s.is_empty()),
        interactive: env::var("RALPH_INTERACTIVE")
            .ok()
            .and_then(|s| parse_env_bool(&s))
            .unwrap_or(true),
        prompt_path: PathBuf::from(
            env::var("RALPH_PROMPT_PATH").unwrap_or_else(|_| ".agent/last_prompt.txt".to_string()),
        ),
        agents_config_path: PathBuf::from(
            env::var("RALPH_AGENTS_CONFIG").unwrap_or_else(|_| ".agent/agents.toml".to_string()),
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
        auto_detect_stack: env::var("RALPH_AUTO_DETECT_STACK")
            .ok()
            .and_then(|s| parse_env_bool(&s))
            .unwrap_or(true),
        checkpoint_enabled: env::var("RALPH_CHECKPOINT_ENABLED")
            .ok()
            .and_then(|s| parse_env_bool(&s))
            .unwrap_or(true),
        strict_validation: env::var("RALPH_STRICT_VALIDATION")
            .ok()
            .and_then(|s| parse_env_bool(&s))
            .unwrap_or(false),
        review_depth: env::var("RALPH_REVIEW_DEPTH")
            .ok()
            .and_then(|s| ReviewDepth::from_str(&s))
            .unwrap_or_default(),
        // Isolation mode is ON by default to prevent context contamination.
        // Set RALPH_ISOLATION_MODE=0 to disable (allows NOTES.md/ISSUES.md).
        isolation_mode: env::var("RALPH_ISOLATION_MODE")
            .ok()
            .and_then(|s| parse_env_bool(&s))
            .unwrap_or(true),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_parse_env_bool() {
        assert_eq!(parse_env_bool("1"), Some(true));
        assert_eq!(parse_env_bool("true"), Some(true));
        assert_eq!(parse_env_bool(" TRUE "), Some(true));
        assert_eq!(parse_env_bool("on"), Some(true));
        assert_eq!(parse_env_bool("yes"), Some(true));

        assert_eq!(parse_env_bool("0"), Some(false));
        assert_eq!(parse_env_bool("false"), Some(false));
        assert_eq!(parse_env_bool(" FALSE "), Some(false));
        assert_eq!(parse_env_bool("off"), Some(false));
        assert_eq!(parse_env_bool("no"), Some(false));

        assert_eq!(parse_env_bool(""), None);
        assert_eq!(parse_env_bool("maybe"), None);
    }

    #[test]
    fn test_config_bool_env_parsing() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Ensure default behavior when unset
        env::remove_var("RALPH_INTERACTIVE");
        let cfg = from_env();
        assert!(cfg.interactive);

        // Accept common truthy values
        env::set_var("RALPH_INTERACTIVE", "true");
        let cfg = from_env();
        assert!(cfg.interactive);

        // Accept common falsy values
        env::set_var("RALPH_INTERACTIVE", "0");
        let cfg = from_env();
        assert!(!cfg.interactive);

        // Clean up
        env::remove_var("RALPH_INTERACTIVE");
    }

    #[test]
    fn test_config_defaults() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Clear environment variables that might affect defaults
        env::remove_var("RALPH_DEVELOPER_AGENT");
        env::remove_var("RALPH_DRIVER_AGENT");
        env::remove_var("RALPH_REVIEWER_AGENT");
        env::remove_var("RALPH_DEVELOPER_ITERS");
        env::remove_var("RALPH_REVIEWER_REVIEWS");
        env::remove_var("RALPH_VERBOSITY");
        env::remove_var("RALPH_AUTO_DETECT_STACK");
        env::remove_var("RALPH_CHECKPOINT_ENABLED");
        env::remove_var("RALPH_STRICT_VALIDATION");
        env::remove_var("RALPH_REVIEW_DEPTH");

        let config = from_env();
        // Agent selection is NOT hardcoded - it comes from agent_chain in agents.toml
        // If no env var is set, these should be None
        assert!(config.developer_agent.is_none());
        assert!(config.reviewer_agent.is_none());
        assert_eq!(config.developer_iters, 5);
        assert_eq!(config.reviewer_reviews, 2);
        // Default verbosity is now Verbose
        assert_eq!(config.verbosity, Verbosity::Verbose);
        // New config options defaults
        assert!(config.auto_detect_stack);
        assert!(config.checkpoint_enabled);
        assert!(!config.strict_validation);
        // Isolation mode is ON by default to prevent context contamination
        assert!(config.isolation_mode);
    }

    #[test]
    fn test_new_config_options_from_env() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Test auto_detect_stack
        env::set_var("RALPH_AUTO_DETECT_STACK", "false");
        let cfg = from_env();
        assert!(!cfg.auto_detect_stack);
        env::remove_var("RALPH_AUTO_DETECT_STACK");

        // Test checkpoint_enabled
        env::set_var("RALPH_CHECKPOINT_ENABLED", "0");
        let cfg = from_env();
        assert!(!cfg.checkpoint_enabled);
        env::remove_var("RALPH_CHECKPOINT_ENABLED");

        // Test strict_validation
        env::set_var("RALPH_STRICT_VALIDATION", "true");
        let cfg = from_env();
        assert!(cfg.strict_validation);
        env::remove_var("RALPH_STRICT_VALIDATION");
    }

    #[test]
    fn test_review_depth_from_env() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Default is Standard
        env::remove_var("RALPH_REVIEW_DEPTH");
        let cfg = from_env();
        assert_eq!(cfg.review_depth, ReviewDepth::Standard);

        // Test comprehensive
        env::set_var("RALPH_REVIEW_DEPTH", "comprehensive");
        let cfg = from_env();
        assert_eq!(cfg.review_depth, ReviewDepth::Comprehensive);

        // Test security
        env::set_var("RALPH_REVIEW_DEPTH", "security");
        let cfg = from_env();
        assert_eq!(cfg.review_depth, ReviewDepth::Security);

        // Test incremental
        env::set_var("RALPH_REVIEW_DEPTH", "incremental");
        let cfg = from_env();
        assert_eq!(cfg.review_depth, ReviewDepth::Incremental);

        // Invalid falls back to default
        env::set_var("RALPH_REVIEW_DEPTH", "invalid_value");
        let cfg = from_env();
        assert_eq!(cfg.review_depth, ReviewDepth::Standard);

        env::remove_var("RALPH_REVIEW_DEPTH");
    }

    #[test]
    fn test_isolation_mode_env_parsing() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Default is true (isolation on)
        env::remove_var("RALPH_ISOLATION_MODE");
        let cfg = from_env();
        assert!(cfg.isolation_mode);

        // Can disable with common falsy values
        env::set_var("RALPH_ISOLATION_MODE", "0");
        let cfg = from_env();
        assert!(!cfg.isolation_mode);

        env::set_var("RALPH_ISOLATION_MODE", "false");
        let cfg = from_env();
        assert!(!cfg.isolation_mode);

        // Can explicitly enable
        env::set_var("RALPH_ISOLATION_MODE", "1");
        let cfg = from_env();
        assert!(cfg.isolation_mode);

        // Clean up
        env::remove_var("RALPH_ISOLATION_MODE");
    }
}
