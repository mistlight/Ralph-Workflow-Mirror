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

/// Apply environment variable overrides to config.
fn apply_env_overrides(mut config: Config, warnings: &mut Vec<String>) -> Config {
    const MAX_ITERS: u32 = 50;
    const MAX_REVIEWS: u32 = 10;
    const MAX_CONTEXT: u8 = 2;

    // Apply all environment variable overrides by category
    apply_agent_selection_env(&mut config, warnings);
    apply_command_env(&mut config, warnings);
    apply_model_provider_env(&mut config);
    apply_iteration_counts_env(&mut config, warnings, MAX_ITERS, MAX_REVIEWS);
    apply_boolean_flags_env(&mut config);
    apply_verbosity_env(&mut config, warnings);
    apply_review_depth_env(&mut config, warnings);
    apply_paths_env(&mut config);
    apply_context_levels_env(&mut config, warnings, MAX_CONTEXT);
    apply_git_identity_env(&mut config);

    config
}

/// Apply agent selection environment variables.
fn apply_agent_selection_env(config: &mut Config, warnings: &mut Vec<String>) {
    let developer_agent = env::var("RALPH_DEVELOPER_AGENT")
        .or_else(|_| env::var("RALPH_DRIVER_AGENT"))
        .ok();
    if let Some(val) = developer_agent {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            warnings.push("Env var RALPH_DEVELOPER_AGENT is empty; ignoring.".to_string());
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
}

/// Apply command override environment variables.
fn apply_command_env(config: &mut Config, warnings: &mut Vec<String>) {
    for (env_var, field) in [
        ("RALPH_DEVELOPER_CMD", &mut config.developer_cmd),
        ("RALPH_REVIEWER_CMD", &mut config.reviewer_cmd),
    ] {
        if let Ok(val) = env::var(env_var) {
            let trimmed = val.trim();
            if trimmed.is_empty() {
                warnings.push(format!("Env var {env_var} is empty; ignoring."));
            } else {
                *field = Some(trimmed.to_string());
            }
        }
    }

    for (env_var, field) in [
        ("FAST_CHECK_CMD", &mut config.fast_check_cmd),
        ("FULL_CHECK_CMD", &mut config.full_check_cmd),
    ] {
        if let Ok(val) = env::var(env_var) {
            if !val.is_empty() {
                *field = Some(val);
            }
        }
    }
}

/// Apply model and provider environment variables.
fn apply_model_provider_env(config: &mut Config) {
    for (env_var, field) in [
        ("RALPH_DEVELOPER_MODEL", &mut config.developer_model),
        ("RALPH_REVIEWER_MODEL", &mut config.reviewer_model),
        ("RALPH_DEVELOPER_PROVIDER", &mut config.developer_provider),
        ("RALPH_REVIEWER_PROVIDER", &mut config.reviewer_provider),
    ] {
        if let Ok(val) = env::var(env_var) {
            *field = Some(val);
        }
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
}

/// Apply iteration count environment variables.
fn apply_iteration_counts_env(
    config: &mut Config,
    warnings: &mut Vec<String>,
    max_iters: u32,
    max_reviews: u32,
) {
    if let Some(n) = parse_env_u32("RALPH_DEVELOPER_ITERS", warnings, max_iters) {
        config.developer_iters = n;
    }
    if let Some(n) = parse_env_u32("RALPH_REVIEWER_REVIEWS", warnings, max_reviews) {
        config.reviewer_reviews = n;
    }
}

/// Apply boolean flag environment variables.
fn apply_boolean_flags_env(config: &mut Config) {
    // Read all boolean env vars first
    let vars: std::collections::HashMap<&str, bool> = [
        "RALPH_INTERACTIVE",
        "RALPH_AUTO_DETECT_STACK",
        "RALPH_CHECKPOINT_ENABLED",
        "RALPH_STRICT_VALIDATION",
        "RALPH_ISOLATION_MODE",
    ]
    .iter()
    .filter_map(|&name| env::var(name).ok().map(|v| (name, v)))
    .filter_map(|(name, val)| parse_env_bool(&val).map(|b| (name, b)))
    .collect();

    // Apply each boolean flag
    for (name, value) in vars {
        match name {
            "RALPH_INTERACTIVE" => config.interactive = value,
            "RALPH_AUTO_DETECT_STACK" => config.auto_detect_stack = value,
            "RALPH_CHECKPOINT_ENABLED" => config.checkpoint_enabled = value,
            "RALPH_STRICT_VALIDATION" => config.strict_validation = value,
            "RALPH_ISOLATION_MODE" => config.isolation_mode = value,
            _ => {}
        }
    }
}

/// Apply verbosity environment variable.
fn apply_verbosity_env(config: &mut Config, warnings: &mut Vec<String>) {
    if let Ok(val) = env::var("RALPH_VERBOSITY") {
        let trimmed = val.trim();
        if trimmed.is_empty() {
            return;
        }
        match trimmed.parse::<u8>() {
            Ok(n) => {
                if n > 4 {
                    warnings.push(format!(
                        "Env var RALPH_VERBOSITY={n} is out of range; clamping to 4 (debug)."
                    ));
                }
                config.verbosity = Verbosity::from(n.min(4));
            }
            Err(_) => {
                warnings.push(format!(
                    "Env var RALPH_VERBOSITY='{trimmed}' is not a valid number; ignoring."
                ));
            }
        }
    }
}

/// Apply review depth environment variable.
fn apply_review_depth_env(config: &mut Config, warnings: &mut Vec<String>) {
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

/// Apply path environment variables.
fn apply_paths_env(config: &mut Config) {
    if let Ok(val) = env::var("RALPH_PROMPT_PATH") {
        config.prompt_path = PathBuf::from(val);
    }
}

/// Apply context level environment variables.
fn apply_context_levels_env(config: &mut Config, warnings: &mut Vec<String>, max_context: u8) {
    if let Some(n) = parse_env_u8("RALPH_DEVELOPER_CONTEXT", warnings, max_context) {
        config.developer_context = n;
    }
    if let Some(n) = parse_env_u8("RALPH_REVIEWER_CONTEXT", warnings, max_context) {
        config.reviewer_context = n;
    }
}

/// Apply git user identity environment variables.
fn apply_git_identity_env(config: &mut Config) {
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
}

/// Parse a u32 environment variable with validation.
fn parse_env_u32(name: &str, warnings: &mut Vec<String>, max: u32) -> Option<u32> {
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

/// Parse a u8 environment variable with validation.
fn parse_env_u8(name: &str, warnings: &mut Vec<String>, max: u8) -> Option<u8> {
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
