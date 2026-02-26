//! Unified Configuration Loader
//!
//! This module handles loading configuration from the unified config file
//! at `~/.config/ralph-workflow.toml`, with environment variable overrides.
//!
//! # Configuration Priority
//!
//! 1. **Global config**: `~/.config/ralph-workflow.toml`
//! 2. **Local config**: `.agent/ralph-workflow.toml` (overrides global)
//! 3. **Override layer**: Environment variables (RALPH_*)
//! 4. **CLI arguments**: Final override (handled at CLI layer)
//!
//! # Legacy Configs
//!
//! Legacy config discovery is intentionally not supported. Only the unified
//! config path is consulted, and missing config files fall back to defaults.
//!
//! # Fail-Fast Validation
//!
//! Ralph validates ALL config files before starting the pipeline. Invalid TOML,
//! type mismatches, or unknown keys will cause Ralph to refuse to start with
//! a clear error message. This is not optional - config validation runs on
//! every startup before any other CLI operation.
use super::parser::parse_env_bool;
use super::path_resolver::ConfigEnvironment;
use super::types::{Config, ReviewDepth, Verbosity};
use super::unified::UnifiedConfig;
use super::validation::{validate_config_file, ConfigValidationError};
use std::env;
use std::fmt::Write;
use std::path::PathBuf;

/// Error type for config loading with validation.
#[derive(Debug, thiserror::Error)]
pub enum ConfigLoadWithValidationError {
    #[error("Configuration validation failed")]
    ValidationErrors(Vec<ConfigValidationError>),
    #[error("Failed to read config file: {0}")]
    Io(#[from] std::io::Error),
}

impl ConfigLoadWithValidationError {
    /// Format all validation errors for user display.
    #[must_use]
    pub fn format_errors(&self) -> String {
        match self {
            Self::ValidationErrors(errors) => {
                let mut output =
                    String::from("Error: Configuration invalid - cannot start Ralph\n\n");

                // Group errors by file for clearer presentation
                let mut global_errors: Vec<&ConfigValidationError> = Vec::new();
                let mut local_errors: Vec<&ConfigValidationError> = Vec::new();
                let mut other_errors: Vec<&ConfigValidationError> = Vec::new();

                for error in errors {
                    let path_str = error.file().to_string_lossy();
                    if path_str.contains(".config") {
                        global_errors.push(error);
                    } else if path_str.contains(".agent") {
                        local_errors.push(error);
                    } else {
                        other_errors.push(error);
                    }
                }

                if !global_errors.is_empty() {
                    output.push_str("~/.config/ralph-workflow.toml:\n");
                    for error in global_errors {
                        writeln!(output, "  {}", format_single_error(error)).unwrap();
                    }
                    output.push('\n');
                }

                if !local_errors.is_empty() {
                    output.push_str(".agent/ralph-workflow.toml:\n");
                    for error in local_errors {
                        writeln!(output, "  {}", format_single_error(error)).unwrap();
                    }
                    output.push('\n');
                }

                if !other_errors.is_empty() {
                    use std::fmt::Write;
                    for error in other_errors {
                        write!(
                            output,
                            "{}:\n  {}\n",
                            error.file().display(),
                            format_single_error(error)
                        )
                        .unwrap();
                    }
                    output.push('\n');
                }

                output.push_str(
                    "Fix these errors and try again, or run `ralph --check-config` for details.",
                );
                output
            }
            Self::Io(e) => e.to_string(),
        }
    }
}

/// Format a single validation error for display.
fn format_single_error(error: &ConfigValidationError) -> String {
    match error {
        ConfigValidationError::TomlSyntax { error, .. } => {
            format!("TOML syntax error: {error}")
        }
        ConfigValidationError::UnknownKey {
            key, suggestion, ..
        } => suggestion.as_ref().map_or_else(
            || format!("Unknown key '{key}'"),
            |s| format!("Unknown key '{key}'. Did you mean '{s}'?"),
        ),
        ConfigValidationError::InvalidValue { key, message, .. } => {
            format!("Invalid value for '{key}': {message}")
        }
    }
}

impl ConfigValidationError {
    /// Get the file path from the error.
    #[must_use]
    pub fn file(&self) -> &std::path::Path {
        match self {
            Self::TomlSyntax { file, .. }
            | Self::InvalidValue { file, .. }
            | Self::UnknownKey { file, .. } => file,
        }
    }
}

/// Load configuration with the unified approach.
///
/// This function loads configuration from the unified config file
/// (`~/.config/ralph-workflow.toml`) and applies environment variable overrides.
///
/// # Returns
///
/// Returns a tuple of `(Config, Vec<String>)` where the second element
/// contains any deprecation warnings to be displayed to the user.
///
/// # Errors
///
/// Returns error if the operation fails.
pub fn load_config(
) -> Result<(Config, Option<UnifiedConfig>, Vec<String>), ConfigLoadWithValidationError> {
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
/// Returns a tuple of `(Config, Option<UnifiedConfig>, Vec<String>)` where the last element
/// contains any deprecation warnings to be displayed to the user.
///
/// # Panics
///
/// This function does not panic. Validation errors are returned to the caller.
///
/// # Errors
///
/// Returns error if the operation fails.
pub fn load_config_from_path(
    config_path: Option<&std::path::Path>,
) -> Result<(Config, Option<UnifiedConfig>, Vec<String>), ConfigLoadWithValidationError> {
    load_config_from_path_with_env(config_path, &super::path_resolver::RealConfigEnvironment)
}

/// Load configuration from a specific path or the default location using a [`ConfigEnvironment`].
///
/// This is the testable version of [`load_config_from_path`]. It uses the provided
/// environment for all filesystem operations.
///
/// # Arguments
///
/// * `config_path` - Optional path to a config file. If None, uses the environment's default.
/// * `env` - The configuration environment to use for filesystem operations.
///
/// # Returns
///
/// Returns a tuple of `(Config, Option<UnifiedConfig>, Vec<String>)` where the last element
/// contains any deprecation warnings to be displayed to the user.
///
/// # Errors
///
/// Returns `Err(ConfigLoadWithValidationError)` if any config file has validation errors
/// (invalid TOML, type mismatches, unknown keys). Per requirements, Ralph refuses to start
/// if ANY config file has errors.
pub fn load_config_from_path_with_env(
    config_path: Option<&std::path::Path>,
    env: &dyn ConfigEnvironment,
) -> Result<(Config, Option<UnifiedConfig>, Vec<String>), ConfigLoadWithValidationError> {
    let mut warnings = Vec::new();
    let mut validation_errors = Vec::new();

    // Step 1: Load and validate global config
    let global_unified = if let Some(path) = config_path {
        // Use provided path
        if env.file_exists(path) {
            let content = env.read_file(path)?;
            // Validate the config file
            match validate_config_file(path, &content) {
                Ok(config_warnings) => {
                    warnings.extend(config_warnings);
                }
                Err(errors) => {
                    validation_errors.extend(errors);
                }
            }
            match UnifiedConfig::load_from_content(&content) {
                Ok(cfg) => Some(cfg),
                Err(e) => {
                    validation_errors.push(ConfigValidationError::InvalidValue {
                        file: path.to_path_buf(),
                        key: "config".to_string(),
                        message: format!("Failed to parse config: {e}"),
                    });
                    None
                }
            }
        } else {
            warnings.push(format!("Global config file not found: {}", path.display()));
            None
        }
    } else {
        // Use default path
        if let Some(global_path) = env.unified_config_path() {
            if env.file_exists(&global_path) {
                let content = env.read_file(&global_path)?;
                // Validate the config file
                match validate_config_file(&global_path, &content) {
                    Ok(config_warnings) => {
                        warnings.extend(config_warnings);
                    }
                    Err(errors) => {
                        validation_errors.extend(errors);
                    }
                }
                match UnifiedConfig::load_from_content(&content) {
                    Ok(cfg) => Some(cfg),
                    Err(e) => {
                        validation_errors.push(ConfigValidationError::InvalidValue {
                            file: global_path,
                            key: "config".to_string(),
                            message: format!("Failed to parse config: {e}"),
                        });
                        None
                    }
                }
            } else {
                // File doesn't exist - not an error, use defaults
                None
            }
        } else {
            None
        }
    };

    // Step 2: Load and validate local config
    let (local_unified, local_content) = if let Some(local_path) = env.local_config_path() {
        if env.file_exists(&local_path) {
            let content = env.read_file(&local_path)?;
            // Validate the config file
            match validate_config_file(&local_path, &content) {
                Ok(config_warnings) => {
                    warnings.extend(config_warnings);
                }
                Err(errors) => {
                    validation_errors.extend(errors);
                }
            }
            match UnifiedConfig::load_from_content(&content) {
                Ok(cfg) => (Some(cfg), Some(content)),
                Err(e) => {
                    validation_errors.push(ConfigValidationError::InvalidValue {
                        file: local_path,
                        key: "config".to_string(),
                        message: format!("Failed to parse config: {e}"),
                    });
                    (None, None)
                }
            }
        } else {
            (None, None)
        }
    } else {
        (None, None)
    };

    // Fail-fast: if there are any validation errors, return them immediately
    if !validation_errors.is_empty() {
        return Err(ConfigLoadWithValidationError::ValidationErrors(
            validation_errors,
        ));
    }

    // Step 3: Merge configs (local overrides global)
    let merged_unified = match (global_unified, local_unified, local_content) {
        (Some(global), Some(local), Some(content)) => {
            // Both exist: merge with local overriding global
            // Pass raw TOML content for presence tracking
            Some(global.merge_with_content(&content, &local))
        }
        (Some(_global), Some(_local), None) => {
            // SAFETY: This case is impossible in production. If local_unified is Some,
            // then local_content must also be Some (they're set together at line 281).
            // If we reach here, there's a bug in the config loading logic.
            unreachable!(
                "BUG: local_unified is Some but local_content is None. \
                 This indicates a logic error in config loading - they should always be set together."
            )
        }
        (Some(global), None, _) => {
            // Only global exists
            Some(global)
        }
        (None, Some(local), _) => {
            // Only local exists (unusual but valid)
            Some(local)
        }
        (None, None, _) => {
            // Neither exists: use defaults
            None
        }
    };

    // Step 4: Convert to Config
    let config = merged_unified
        .as_ref()
        .map_or_else(default_config, |unified_cfg| {
            config_from_unified(unified_cfg, &mut warnings)
        });

    // Step 5: Apply environment variable overrides
    let config = apply_env_overrides(config, &mut warnings);

    // Step 6: Validate cloud configuration (fail-fast)
    if let Err(e) = config.cloud.validate() {
        return Err(ConfigLoadWithValidationError::ValidationErrors(vec![
            ConfigValidationError::InvalidValue {
                file: PathBuf::from("<environment>"),
                key: "cloud".to_string(),
                message: e,
            },
        ]));
    }

    Ok((config, merged_unified, warnings))
}

/// Create a Config from `UnifiedConfig`.
fn config_from_unified(unified: &UnifiedConfig, warnings: &mut Vec<String>) -> Config {
    use super::types::{BehavioralFlags, FeatureFlags};

    let general = &unified.general;
    // max_dev_continuations of 0 is valid and means "no continuations" (total attempts = 1).
    // Any non-negative value is accepted; max_dev_continuations comes from a u32 so can't be negative.
    // When omitted from config file, serde applies default_max_dev_continuations() -> 2.
    let max_dev_continuations = general.max_dev_continuations;
    // max_xsd_retries of 0 is valid and means "disable XSD retries" (immediate agent fallback).
    // Any non-negative value is accepted; max_xsd_retries comes from a u32 so can't be negative.
    // When omitted from config file, serde applies default_max_xsd_retries() -> 10.
    let max_xsd_retries = general.max_xsd_retries;
    // max_same_agent_retries of 0 is valid and means "disable same-agent retries"
    // (immediate fallback to next agent on timeout/internal error).
    // When omitted from config file, serde applies default_max_same_agent_retries() -> 2.
    let max_same_agent_retries = general.max_same_agent_retries;

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
        commit_cmd: None,
        developer_model: None,
        reviewer_model: None,
        developer_provider: None,
        reviewer_provider: None,
        reviewer_json_parser: None, // Set from env var or CLI
        features: FeatureFlags {
            checkpoint_enabled: general.workflow.checkpoint_enabled,
            force_universal_prompt: general.execution.force_universal_prompt,
        },
        developer_iters: general.developer_iters,
        reviewer_reviews: general.reviewer_reviews,
        fast_check_cmd: None,
        full_check_cmd: None,
        behavior: BehavioralFlags {
            interactive: general.behavior.interactive,
            auto_detect_stack: general.behavior.auto_detect_stack,
            strict_validation: general.behavior.strict_validation,
        },
        prompt_path: general
            .prompt_path
            .as_ref()
            .map_or_else(|| PathBuf::from(".agent/last_prompt.txt"), PathBuf::from),
        user_templates_dir: general.templates_dir.as_ref().map(PathBuf::from),
        developer_context: general.developer_context,
        reviewer_context: general.reviewer_context,
        verbosity: Verbosity::from(general.verbosity),
        review_depth,
        isolation_mode: general.execution.isolation_mode,
        git_user_name: general.git_user_name.clone(),
        git_user_email: general.git_user_email.clone(),
        show_streaming_metrics: false, // Default to false; can be enabled via CLI flag or config file
        review_format_retries: 5,      // Default to 5 retries for format correction
        // CRITICAL: Always wrap in Some(). The serde default ensures these fields are never
        // missing from UnifiedConfig, so Config always has a value. The Option<u32> type in
        // Config is for backward compatibility with direct Config construction (e.g., tests).
        max_dev_continuations: Some(max_dev_continuations),
        max_xsd_retries: Some(max_xsd_retries),
        max_same_agent_retries: Some(max_same_agent_retries),
        execution_history_limit: general.execution_history_limit,
        cloud: super::types::CloudConfig::from_env(),
    }
}

/// Default configuration when no config file is found.
pub(super) fn default_config() -> Config {
    use super::types::{BehavioralFlags, FeatureFlags};

    Config {
        developer_agent: None,
        reviewer_agent: None,
        developer_cmd: None,
        reviewer_cmd: None,
        commit_cmd: None,
        developer_model: None,
        reviewer_model: None,
        developer_provider: None,
        reviewer_provider: None,
        reviewer_json_parser: None,
        features: FeatureFlags {
            checkpoint_enabled: true,
            force_universal_prompt: false,
        },
        developer_iters: 5,
        reviewer_reviews: 2,
        fast_check_cmd: None,
        full_check_cmd: None,
        behavior: BehavioralFlags {
            interactive: true,
            auto_detect_stack: true,
            strict_validation: false,
        },
        prompt_path: PathBuf::from(".agent/last_prompt.txt"),
        user_templates_dir: None,
        developer_context: 1,
        reviewer_context: 0,
        verbosity: Verbosity::Verbose,
        review_depth: ReviewDepth::default(),
        isolation_mode: true,
        git_user_name: None,
        git_user_email: None,
        show_streaming_metrics: false,
        review_format_retries: 5,
        // Semantics: max_dev_continuations counts continuations beyond the initial attempt.
        // Default to 2 continuations (3 total attempts).
        max_dev_continuations: Some(2),
        max_xsd_retries: Some(10), // Default to 10 retries before agent fallback
        max_same_agent_retries: Some(2), // Default to 2 failures (initial + 1 retry) before agent fallback
        execution_history_limit: 1000,   // Default to 1000 entries (ring buffer)
        cloud: super::types::CloudConfig::from_env(),
    }
}

/// Apply environment variable overrides to config.
fn apply_env_overrides(mut config: Config, warnings: &mut Vec<String>) -> Config {
    const MAX_ITERS: u32 = 50;
    const MAX_REVIEWS: u32 = 10;
    const MAX_CONTEXT: u8 = 2;
    const MAX_FORMAT_RETRIES: u32 = 20;

    // Apply all environment variable overrides by category
    apply_agent_selection_env(&mut config, warnings);
    apply_command_env(&mut config, warnings);
    apply_model_provider_env(&mut config);
    apply_iteration_counts_env(&mut config, warnings, MAX_ITERS, MAX_REVIEWS);
    apply_review_config_env(&mut config, warnings, MAX_FORMAT_RETRIES);
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
    if let Ok(val) = env::var("RALPH_DEVELOPER_AGENT") {
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
        ("RALPH_COMMIT_CMD", &mut config.commit_cmd),
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
            config.features.force_universal_prompt = b;
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

/// Apply review-specific configuration environment variables.
fn apply_review_config_env(config: &mut Config, warnings: &mut Vec<String>, max_retries: u32) {
    if let Some(n) = parse_env_u32("RALPH_REVIEW_FORMAT_RETRIES", warnings, max_retries) {
        config.review_format_retries = n;
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
            "RALPH_INTERACTIVE" => config.behavior.interactive = value,
            "RALPH_AUTO_DETECT_STACK" => config.behavior.auto_detect_stack = value,
            "RALPH_CHECKPOINT_ENABLED" => config.features.checkpoint_enabled = value,
            "RALPH_STRICT_VALIDATION" => config.behavior.strict_validation = value,
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
    if let Ok(val) = env::var("RALPH_TEMPLATES_DIR") {
        let trimmed = val.trim();
        if !trimmed.is_empty() {
            config.user_templates_dir = Some(PathBuf::from(trimmed));
        }
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

mod env_parsing;
use env_parsing::{parse_env_u32, parse_env_u8};

mod unified_config_exists;

pub use unified_config_exists::{unified_config_exists, unified_config_exists_with_env};

#[cfg(test)]
mod tests;
