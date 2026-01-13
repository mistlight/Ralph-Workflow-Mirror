//! Unified Configuration Loader
//!
//! This module handles loading configuration from the unified config file
//! at `~/.config/ralph-workflow.toml`, with environment variable overrides.
//!
//! # Configuration Priority
//!
//! 1. **Primary source**: `~/.config/ralph-workflow.toml`
//! 2. **Override layer**: Environment variables (RALPH_*)
//! 3. **CLI arguments**: Final override (handled at CLI layer)
//!
//! # Migration Support
//!
//! For backwards compatibility, the loader also checks legacy config locations
//! (`~/.config/ralph/agents.toml` and `.agent/agents.toml`) and emits
//! deprecation warnings when they are used.

use super::parser::parse_env_bool;
use super::types::{Config, ReviewDepth, Verbosity};
use super::unified::{unified_config_path, UnifiedConfig};
use std::env;
use std::path::PathBuf;

/// Load configuration with the unified approach.
///
/// This function loads configuration from the unified config file
/// (`~/.config/ralph-workflow.toml`) and applies environment variable overrides.
///
/// # Returns
///
/// Returns a tuple of `(Config, Vec<String>)` where the second element
/// contains any deprecation warnings to be displayed to the user.
pub fn load_config() -> (Config, Option<UnifiedConfig>, Vec<String>) {
    load_config_from_path(None)
}

/// Load configuration from a specific path or the default location.
///
/// If `config_path` is provided, loads from that file.
/// Otherwise, loads from the default unified config location.
///
/// # Arguments
///
/// * `config_path` - Optional path to a config file. If None, uses the default location.
///
/// # Returns
///
/// Returns a tuple of `(Config, Vec<String>)` where the second element
/// contains any deprecation warnings to be displayed to the user.
pub fn load_config_from_path(
    config_path: Option<&std::path::Path>,
) -> (Config, Option<UnifiedConfig>, Vec<String>) {
    let mut warnings = Vec::new();

    // Try to load unified config from specified path or default
    let unified = if let Some(path) = config_path {
        if path.exists() {
            match UnifiedConfig::load_from_path(path) {
                Ok(cfg) => Some(cfg),
                Err(e) => {
                    warnings.push(format!(
                        "Failed to load config from {}: {}",
                        path.display(),
                        e
                    ));
                    None
                }
            }
        } else {
            warnings.push(format!("Config file not found: {}", path.display()));
            None
        }
    } else {
        UnifiedConfig::load_default()
    };

    // Start with defaults, then apply unified config if found
    let config = if let Some(ref unified_cfg) = unified {
        config_from_unified(unified_cfg, &mut warnings)
    } else {
        // No unified config - check for legacy configs
        check_legacy_configs(&mut warnings);
        default_config()
    };

    // Apply environment variable overrides
    let config = apply_env_overrides(config, &mut warnings);

    (config, unified, warnings)
}

/// Create a Config from UnifiedConfig.
fn config_from_unified(unified: &UnifiedConfig, warnings: &mut Vec<String>) -> Config {
    let general = &unified.general;

    let review_depth = match ReviewDepth::from_str(&general.review_depth) {
        Some(d) => d,
        None => {
            warnings.push(format!(
                "Invalid review_depth '{}' in config; falling back to 'standard'.",
                general.review_depth
            ));
            ReviewDepth::default()
        }
    };

    Config {
        developer_agent: None, // Set from agent_chain or CLI
        reviewer_agent: None,  // Set from agent_chain or CLI
        developer_cmd: None,
        reviewer_cmd: None,
        developer_model: None,
        reviewer_model: None,
        developer_provider: None,
        reviewer_provider: None,
        reviewer_json_parser: None, // Set from env var or CLI
        force_universal_prompt: general.force_universal_prompt,
        developer_iters: general.developer_iters,
        reviewer_reviews: general.reviewer_reviews,
        fast_check_cmd: None,
        full_check_cmd: None,
        interactive: general.interactive,
        prompt_path: general
            .prompt_path
            .as_ref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from(".agent/last_prompt.txt")),
        developer_context: general.developer_context,
        reviewer_context: general.reviewer_context,
        verbosity: Verbosity::from(general.verbosity),
        commit_msg: "chore: apply PROMPT loop + review/fix/review".to_string(),
        auto_detect_stack: general.auto_detect_stack,
        checkpoint_enabled: general.checkpoint_enabled,
        strict_validation: general.strict_validation,
        review_depth,
        isolation_mode: general.isolation_mode,
        git_user_name: general.git_user_name.clone(),
        git_user_email: general.git_user_email.clone(),
    }
}

/// Default configuration when no config file is found.
fn default_config() -> Config {
    Config {
        developer_agent: None,
        reviewer_agent: None,
        developer_cmd: None,
        reviewer_cmd: None,
        developer_model: None,
        reviewer_model: None,
        developer_provider: None,
        reviewer_provider: None,
        reviewer_json_parser: None,
        force_universal_prompt: false,
        developer_iters: 5,
        reviewer_reviews: 2,
        fast_check_cmd: None,
        full_check_cmd: None,
        interactive: true,
        prompt_path: PathBuf::from(".agent/last_prompt.txt"),
        developer_context: 1,
        reviewer_context: 0,
        verbosity: Verbosity::Verbose,
        commit_msg: "chore: apply PROMPT loop + review/fix/review".to_string(),
        auto_detect_stack: true,
        checkpoint_enabled: true,
        strict_validation: false,
        review_depth: ReviewDepth::default(),
        isolation_mode: true,
        git_user_name: None,
        git_user_email: None,
    }
}

/// Apply environment variable overrides to config.
fn apply_env_overrides(mut config: Config, warnings: &mut Vec<String>) -> Config {
    const MAX_ITERS: u32 = 50;
    const MAX_REVIEWS: u32 = 10;
    const MAX_CONTEXT: u8 = 2;

    fn parse_u32_env(name: &str, warnings: &mut Vec<String>, max: u32) -> Option<u32> {
        let raw = std::env::var(name).ok()?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        match trimmed.parse::<u32>() {
            Ok(n) if n <= max => Some(n),
            Ok(n) => {
                warnings.push(format!(
                    "Env var {}={} is too large; clamping to {}.",
                    name, n, max
                ));
                Some(max)
            }
            Err(_) => {
                warnings.push(format!(
                    "Env var {}='{}' is not a valid number; ignoring.",
                    name, trimmed
                ));
                None
            }
        }
    }

    fn parse_u8_env(name: &str, warnings: &mut Vec<String>, max: u8) -> Option<u8> {
        let raw = std::env::var(name).ok()?;
        let trimmed = raw.trim();
        if trimmed.is_empty() {
            return None;
        }
        match trimmed.parse::<u8>() {
            Ok(n) if n <= max => Some(n),
            Ok(n) => {
                warnings.push(format!(
                    "Env var {}={} is out of range; clamping to {}.",
                    name, n, max
                ));
                Some(max)
            }
            Err(_) => {
                warnings.push(format!(
                    "Env var {}='{}' is not a valid number; ignoring.",
                    name, trimmed
                ));
                None
            }
        }
    }

    // Agent selection
    if let Ok(val) = env::var("RALPH_DEVELOPER_AGENT") {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            warnings.push("Env var RALPH_DEVELOPER_AGENT is empty; ignoring.".to_string());
        } else {
            config.developer_agent = Some(trimmed.to_string());
        }
    } else if let Ok(val) = env::var("RALPH_DRIVER_AGENT") {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            warnings.push("Env var RALPH_DRIVER_AGENT is empty; ignoring.".to_string());
        } else {
            config.developer_agent = Some(trimmed.to_string());
        }
    }
    if let Ok(val) = env::var("RALPH_REVIEWER_AGENT") {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            warnings.push("Env var RALPH_REVIEWER_AGENT is empty; ignoring.".to_string());
        } else {
            config.reviewer_agent = Some(trimmed.to_string());
        }
    }

    // Command overrides
    if let Ok(val) = env::var("RALPH_DEVELOPER_CMD") {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            warnings.push("Env var RALPH_DEVELOPER_CMD is empty; ignoring.".to_string());
        } else {
            config.developer_cmd = Some(trimmed.to_string());
        }
    }
    if let Ok(val) = env::var("RALPH_REVIEWER_CMD") {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            warnings.push("Env var RALPH_REVIEWER_CMD is empty; ignoring.".to_string());
        } else {
            config.reviewer_cmd = Some(trimmed.to_string());
        }
    }

    // Model overrides
    if let Ok(val) = env::var("RALPH_DEVELOPER_MODEL") {
        config.developer_model = Some(val);
    }
    if let Ok(val) = env::var("RALPH_REVIEWER_MODEL") {
        config.reviewer_model = Some(val);
    }

    // Provider overrides
    if let Ok(val) = env::var("RALPH_DEVELOPER_PROVIDER") {
        config.developer_provider = Some(val);
    }
    if let Ok(val) = env::var("RALPH_REVIEWER_PROVIDER") {
        config.reviewer_provider = Some(val);
    }

    // JSON parser override for reviewer (useful for testing different parsers)
    if let Ok(val) = env::var("RALPH_REVIEWER_JSON_PARSER") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            config.reviewer_json_parser = Some(trimmed.to_string());
        }
    }

    // Force universal review prompt (useful for problematic agents)
    if let Ok(val) = env::var("RALPH_REVIEWER_UNIVERSAL_PROMPT") {
        if let Some(b) = parse_env_bool(&val) {
            config.force_universal_prompt = b;
        }
    }

    // Iteration counts
    if let Some(n) = parse_u32_env("RALPH_DEVELOPER_ITERS", warnings, MAX_ITERS) {
        config.developer_iters = n;
    }
    if let Some(n) = parse_u32_env("RALPH_REVIEWER_REVIEWS", warnings, MAX_REVIEWS) {
        config.reviewer_reviews = n;
    }

    // Check commands
    if let Ok(val) = env::var("FAST_CHECK_CMD") {
        if !val.is_empty() {
            config.fast_check_cmd = Some(val);
        }
    }
    if let Ok(val) = env::var("FULL_CHECK_CMD") {
        if !val.is_empty() {
            config.full_check_cmd = Some(val);
        }
    }

    // Boolean flags
    if let Ok(val) = env::var("RALPH_INTERACTIVE") {
        if let Some(b) = parse_env_bool(&val) {
            config.interactive = b;
        }
    }
    if let Ok(val) = env::var("RALPH_AUTO_DETECT_STACK") {
        if let Some(b) = parse_env_bool(&val) {
            config.auto_detect_stack = b;
        }
    }
    if let Ok(val) = env::var("RALPH_CHECKPOINT_ENABLED") {
        if let Some(b) = parse_env_bool(&val) {
            config.checkpoint_enabled = b;
        }
    }
    if let Ok(val) = env::var("RALPH_STRICT_VALIDATION") {
        if let Some(b) = parse_env_bool(&val) {
            config.strict_validation = b;
        }
    }
    if let Ok(val) = env::var("RALPH_ISOLATION_MODE") {
        if let Some(b) = parse_env_bool(&val) {
            config.isolation_mode = b;
        }
    }

    // Verbosity
    if let Ok(val) = env::var("RALPH_VERBOSITY") {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            // ignore
        } else if let Ok(n) = trimmed.parse::<u8>() {
            if n > 4 {
                warnings.push(format!(
                    "Env var RALPH_VERBOSITY={} is out of range; clamping to 4 (debug).",
                    n
                ));
            }
            config.verbosity = Verbosity::from(n.min(4));
        } else {
            warnings.push(format!(
                "Env var RALPH_VERBOSITY='{}' is not a valid number; ignoring.",
                trimmed
            ));
        }
    }

    // Review depth
    if let Ok(val) = env::var("RALPH_REVIEW_DEPTH") {
        if let Some(depth) = ReviewDepth::from_str(&val) {
            config.review_depth = depth;
        } else if !val.trim().is_empty() {
            warnings.push(format!(
                "Env var RALPH_REVIEW_DEPTH='{}' is invalid; ignoring.",
                val.trim()
            ));
        }
    }

    // Paths
    if let Ok(val) = env::var("RALPH_PROMPT_PATH") {
        config.prompt_path = PathBuf::from(val);
    }

    // Context levels
    if let Some(n) = parse_u8_env("RALPH_DEVELOPER_CONTEXT", warnings, MAX_CONTEXT) {
        config.developer_context = n;
    }
    if let Some(n) = parse_u8_env("RALPH_REVIEWER_CONTEXT", warnings, MAX_CONTEXT) {
        config.reviewer_context = n;
    }

    // Git user identity
    if let Ok(val) = env::var("RALPH_GIT_USER_NAME") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            config.git_user_name = Some(trimmed.to_string());
        }
    }
    if let Ok(val) = env::var("RALPH_GIT_USER_EMAIL") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            config.git_user_email = Some(trimmed.to_string());
        }
    }

    config
}

/// Check for legacy config files and add deprecation warnings.
fn check_legacy_configs(warnings: &mut Vec<String>) {
    // Check for old global config
    if let Some(config_dir) = dirs::config_dir() {
        let old_global = config_dir.join("ralph").join("agents.toml");
        if old_global.exists() {
            warnings.push(format!(
                "DEPRECATION: Found legacy config at {}. \
                 Please migrate to ~/.config/ralph-workflow.toml",
                old_global.display()
            ));
        }
    }

    // Check for project-level config
    let project_config = PathBuf::from(".agent/agents.toml");
    if project_config.exists() && unified_config_path().is_some() && !unified_config_exists() {
        warnings.push(
            "DEPRECATION: Found legacy per-repo config at .agent/agents.toml. \
             Please migrate to ~/.config/ralph-workflow.toml."
                .to_string(),
        );
    }
}

/// Check if the unified config file exists.
pub fn unified_config_exists() -> bool {
    unified_config_path().is_some_and(|p| p.exists())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Mutex;

    static ENV_MUTEX: Mutex<()> = Mutex::new(());

    #[test]
    fn test_default_config() {
        let config = default_config();
        assert!(config.developer_agent.is_none());
        assert!(config.reviewer_agent.is_none());
        assert_eq!(config.developer_iters, 5);
        assert_eq!(config.reviewer_reviews, 2);
        assert!(config.interactive);
        assert!(config.isolation_mode);
        assert_eq!(config.verbosity, Verbosity::Verbose);
    }

    #[test]
    fn test_apply_env_overrides() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Set some env vars
        env::set_var("RALPH_DEVELOPER_ITERS", "10");
        env::set_var("RALPH_ISOLATION_MODE", "false");

        let mut warnings = Vec::new();
        let config = apply_env_overrides(default_config(), &mut warnings);
        assert_eq!(config.developer_iters, 10);
        assert!(!config.isolation_mode);
        assert!(warnings.is_empty());

        // Clean up
        env::remove_var("RALPH_DEVELOPER_ITERS");
        env::remove_var("RALPH_ISOLATION_MODE");
    }

    #[test]
    fn test_unified_config_exists_respects_xdg_config_home() {
        let _guard = ENV_MUTEX.lock().unwrap();

        let dir = tempfile::tempdir().unwrap();
        env::set_var("XDG_CONFIG_HOME", dir.path());

        let path = unified_config_path().unwrap();
        if path.exists() {
            std::fs::remove_file(&path).unwrap();
        }
        assert!(!unified_config_exists());

        std::fs::write(&path, "").unwrap();
        assert!(unified_config_exists());

        env::remove_var("XDG_CONFIG_HOME");
    }

    #[test]
    fn test_load_config_returns_defaults_without_file() {
        let _guard = ENV_MUTEX.lock().unwrap();

        // Clear env vars that might affect the test
        env::remove_var("RALPH_DEVELOPER_AGENT");
        env::remove_var("RALPH_REVIEWER_AGENT");
        env::remove_var("RALPH_DEVELOPER_ITERS");
        env::remove_var("RALPH_VERBOSITY");

        let (config, _unified, _warnings) = load_config();
        assert_eq!(config.developer_iters, 5);
        assert_eq!(config.verbosity, Verbosity::Verbose);
    }
}
