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
    let unified = config_path.map_or_else(UnifiedConfig::load_default, |path| {
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
    });

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

/// Create a Config from `UnifiedConfig`.
fn config_from_unified(unified: &UnifiedConfig, warnings: &mut Vec<String>) -> Config {
    let general = &unified.general;

    let review_depth = ReviewDepth::from_str(&general.review_depth).unwrap_or_else(|| {
        warnings.push(format!(
            "Invalid review_depth '{}' in config; falling back to 'standard'.",
            general.review_depth
        ));
        ReviewDepth::default()
    });

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
            .map_or_else(|| PathBuf::from(".agent/last_prompt.txt"), PathBuf::from),
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

/// Environment variable parsing constants.
const MAX_ITERS: u32 = 50;
const MAX_REVIEWS: u32 = 10;
const MAX_CONTEXT: u8 = 2;

/// Parse a u32 environment variable with max value clamping.
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
                "Env var {name}={n} is too large; clamping to {max}."
            ));
            Some(max)
        }
        Err(_) => {
            warnings.push(format!(
                "Env var {name}='{trimmed}' is not a valid number; ignoring."
            ));
            None
        }
    }
}

/// Parse a u8 environment variable with max value clamping.
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
                "Env var {name}={n} is out of range; clamping to {max}."
            ));
            Some(max)
        }
        Err(_) => {
            warnings.push(format!(
                "Env var {name}='{trimmed}' is not a valid number; ignoring."
            ));
            None
        }
    }
}

/// Parse a non-empty string environment variable with warnings.
fn parse_string_env(name: &str, warnings: &mut Vec<String>) -> Option<String> {
    let val = env::var(name).ok()?;
    let trimmed = val.trim();
    if trimmed.is_empty() {
        warnings.push(format!("Env var {name} is empty; ignoring."));
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse an optional string environment variable (no warning if empty).
fn parse_optional_string_env(name: &str) -> Option<String> {
    let val = env::var(name).ok()?;
    let trimmed = val.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Parse a string environment variable into a `PathBuf`.
fn parse_path_env(name: &str) -> Option<PathBuf> {
    env::var(name).ok().map(PathBuf::from)
}

/// Apply agent selection environment variables.
fn apply_agent_vars(config: &mut Config, warnings: &mut Vec<String>) {
    if let Some(agent) = parse_string_env("RALPH_DEVELOPER_AGENT", warnings) {
        config.developer_agent = Some(agent);
    } else if let Some(agent) = parse_string_env("RALPH_DRIVER_AGENT", warnings) {
        config.developer_agent = Some(agent);
    }
    if let Some(agent) = parse_string_env("RALPH_REVIEWER_AGENT", warnings) {
        config.reviewer_agent = Some(agent);
    }
}

/// Apply string-based override environment variables.
fn apply_string_overrides(config: &mut Config, warnings: &mut Vec<String>) {
    if let Some(cmd) = parse_string_env("RALPH_DEVELOPER_CMD", warnings) {
        config.developer_cmd = Some(cmd);
    }
    if let Some(cmd) = parse_string_env("RALPH_REVIEWER_CMD", warnings) {
        config.reviewer_cmd = Some(cmd);
    }
}

/// Apply model/provider environment variables.
fn apply_model_provider_vars(config: &mut Config) {
    if let Some(model) = parse_optional_string_env("RALPH_DEVELOPER_MODEL") {
        config.developer_model = Some(model);
    }
    if let Some(model) = parse_optional_string_env("RALPH_REVIEWER_MODEL") {
        config.reviewer_model = Some(model);
    }
    if let Some(provider) = parse_optional_string_env("RALPH_DEVELOPER_PROVIDER") {
        config.developer_provider = Some(provider);
    }
    if let Some(provider) = parse_optional_string_env("RALPH_REVIEWER_PROVIDER") {
        config.reviewer_provider = Some(provider);
    }
    if let Some(parser) = parse_optional_string_env("RALPH_REVIEWER_JSON_PARSER") {
        config.reviewer_json_parser = Some(parser);
    }
}

/// Apply boolean flag environment variables.
fn apply_bool_vars(config: &mut Config) {
    let apply_bool = |name: &str, target: &mut bool| {
        if let Ok(val) = env::var(name) {
            if let Some(b) = parse_env_bool(&val) {
                *target = b;
            }
        }
    };
    apply_bool(
        "RALPH_REVIEWER_UNIVERSAL_PROMPT",
        &mut config.force_universal_prompt,
    );
    apply_bool("RALPH_INTERACTIVE", &mut config.interactive);
    apply_bool("RALPH_AUTO_DETECT_STACK", &mut config.auto_detect_stack);
    apply_bool("RALPH_CHECKPOINT_ENABLED", &mut config.checkpoint_enabled);
    apply_bool("RALPH_STRICT_VALIDATION", &mut config.strict_validation);
    apply_bool("RALPH_ISOLATION_MODE", &mut config.isolation_mode);
}

/// Apply numeric environment variables.
fn apply_numeric_vars(config: &mut Config, warnings: &mut Vec<String>) {
    if let Some(n) = parse_u32_env("RALPH_DEVELOPER_ITERS", warnings, MAX_ITERS) {
        config.developer_iters = n;
    }
    if let Some(n) = parse_u32_env("RALPH_REVIEWER_REVIEWS", warnings, MAX_REVIEWS) {
        config.reviewer_reviews = n;
    }
    if let Some(n) = parse_u8_env("RALPH_DEVELOPER_CONTEXT", warnings, MAX_CONTEXT) {
        config.developer_context = n;
    }
    if let Some(n) = parse_u8_env("RALPH_REVIEWER_CONTEXT", warnings, MAX_CONTEXT) {
        config.reviewer_context = n;
    }
}

/// Apply verbosity environment variable.
fn apply_verbosity(config: &mut Config, warnings: &mut Vec<String>) {
    if let Ok(val) = env::var("RALPH_VERBOSITY") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            if let Ok(n) = trimmed.parse::<u8>() {
                if n > 4 {
                    warnings.push(format!(
                        "Env var RALPH_VERBOSITY={n} is out of range; clamping to 4 (debug)."
                    ));
                }
                config.verbosity = Verbosity::from(n.min(4));
            } else {
                warnings.push(format!(
                    "Env var RALPH_VERBOSITY='{trimmed}' is not a valid number; ignoring."
                ));
            }
        }
    }
}

/// Apply review depth environment variable.
fn apply_review_depth(config: &mut Config, warnings: &mut Vec<String>) {
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
}

/// Apply check command environment variables.
fn apply_check_vars(config: &mut Config) {
    if let Some(cmd) = parse_optional_string_env("FAST_CHECK_CMD") {
        config.fast_check_cmd = Some(cmd);
    }
    if let Some(cmd) = parse_optional_string_env("FULL_CHECK_CMD") {
        config.full_check_cmd = Some(cmd);
    }
}

/// Apply path-based environment variables.
fn apply_path_vars(config: &mut Config) {
    if let Some(path) = parse_path_env("RALPH_PROMPT_PATH") {
        config.prompt_path = path;
    }
}

/// Apply git user identity environment variables.
fn apply_git_vars(config: &mut Config) {
    if let Some(name) = parse_optional_string_env("RALPH_GIT_USER_NAME") {
        config.git_user_name = Some(name);
    }
    if let Some(email) = parse_optional_string_env("RALPH_GIT_USER_EMAIL") {
        config.git_user_email = Some(email);
    }
}

/// Apply environment variable overrides to config.
fn apply_env_overrides(mut config: Config, warnings: &mut Vec<String>) -> Config {
    apply_agent_vars(&mut config, warnings);
    apply_string_overrides(&mut config, warnings);
    apply_model_provider_vars(&mut config);
    apply_bool_vars(&mut config);
    apply_numeric_vars(&mut config, warnings);
    apply_check_vars(&mut config);
    apply_verbosity(&mut config, warnings);
    apply_review_depth(&mut config, warnings);
    apply_path_vars(&mut config);
    apply_git_vars(&mut config);
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
