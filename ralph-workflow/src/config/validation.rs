//! Configuration validation and error reporting.
//!
//! This module provides validation for configuration files with:
//! - TOML syntax validation
//! - Type checking (expected vs actual types)
//! - Unknown key detection with typo suggestions (Levenshtein distance)
//! - Multi-file error aggregation
//! - User-friendly error messages

use std::path::{Path, PathBuf};
use thiserror::Error;

/// Configuration validation error.
#[derive(Debug, Error)]
pub enum ConfigValidationError {
    #[error("TOML syntax error in {file}: {error}")]
    TomlSyntax {
        file: PathBuf,
        error: toml::de::Error,
    },

    #[error("Invalid value in {file} at '{key}': {message}")]
    InvalidValue {
        file: PathBuf,
        key: String,
        message: String,
    },

    #[error("Unknown key in {file}: '{key}'")]
    UnknownKey {
        file: PathBuf,
        key: String,
        suggestion: Option<String>,
    },
}

/// Result of config validation.
/// On success: Ok(warnings) where warnings is a Vec<String> of deprecation warnings
/// On failure: Err(errors) where errors is a Vec<ConfigValidationError>
pub type ValidationResult = Result<Vec<String>, Vec<ConfigValidationError>>;

/// Type alias for a list of (key_name, location) pairs.
/// Used for tracking unknown and deprecated keys found during validation.
type KeyLocationList = Vec<(String, String)>;

/// Calculate Levenshtein distance between two strings.
fn levenshtein_distance(a: &str, b: &str) -> usize {
    let a_len = a.len();
    let b_len = b.len();

    if a_len == 0 {
        return b_len;
    }
    if b_len == 0 {
        return a_len;
    }

    let mut prev_row: Vec<usize> = (0..=b_len).collect();
    let mut curr_row = vec![0; b_len + 1];

    for (i, a_char) in a.chars().enumerate() {
        curr_row[0] = i + 1;

        for (j, b_char) in b.chars().enumerate() {
            let cost = if a_char == b_char { 0 } else { 1 };
            curr_row[j + 1] = std::cmp::min(
                std::cmp::min(
                    curr_row[j] + 1,     // insertion
                    prev_row[j + 1] + 1, // deletion
                ),
                prev_row[j] + cost, // substitution
            );
        }

        std::mem::swap(&mut prev_row, &mut curr_row);
    }

    prev_row[b_len]
}

/// Find the closest valid key name for typo detection.
pub fn suggest_key(unknown_key: &str, valid_keys: &[&str]) -> Option<String> {
    let threshold = 3; // Maximum edit distance for suggestions

    valid_keys
        .iter()
        .map(|&key| (key, levenshtein_distance(unknown_key, key)))
        .filter(|(_, distance)| *distance <= threshold)
        .min_by_key(|(_, distance)| *distance)
        .map(|(key, _)| key.to_string())
}

/// Validate a config file and collect errors and warnings.
///
/// This validates:
/// - TOML syntax
/// - Type checking against UnifiedConfig schema
/// - Unknown keys with typo suggestions
/// - Deprecated keys (returns as warnings, not errors)
///
/// Returns Ok((warnings)) on success with optional deprecation warnings,
/// or Err(errors) on validation failure.
pub fn validate_config_file(
    path: &Path,
    content: &str,
) -> Result<Vec<String>, Vec<ConfigValidationError>> {
    let mut errors = Vec::new();
    let mut warnings = Vec::new();

    // Step 1: Validate TOML syntax and parse to generic Value for unknown key detection
    let parsed_value: toml::Value = match toml::from_str(content) {
        Ok(value) => value,
        Err(e) => {
            errors.push(ConfigValidationError::TomlSyntax {
                file: path.to_path_buf(),
                error: e,
            });
            return Err(errors);
        }
    };

    // Step 2: Detect unknown and deprecated keys by walking the TOML structure
    // This is necessary because #[serde(default)] causes serde to silently ignore unknown fields
    let (unknown_keys, deprecated_keys) = detect_unknown_and_deprecated_keys(&parsed_value);

    // Unknown keys are errors
    for (key, location) in unknown_keys {
        let valid_keys = get_valid_config_keys();
        let suggestion = suggest_key(&key, &valid_keys);
        errors.push(ConfigValidationError::UnknownKey {
            file: path.to_path_buf(),
            key: format!("{}{}", location, key),
            suggestion,
        });
    }

    // Deprecated keys are warnings
    for (key, location) in deprecated_keys {
        let full_key = format!("{}{}", location, key);
        warnings.push(format!(
            "Deprecated key '{}' in {} - this key is no longer used and can be safely removed",
            full_key,
            path.display()
        ));
    }

    // Step 3: Validate against UnifiedConfig schema for type checking
    // Unknown keys won't cause deserialization to fail due to #[serde(default)],
    // but we've already detected them in Step 2
    match toml::from_str::<crate::config::unified::UnifiedConfig>(content) {
        Ok(_) => {
            // Successfully deserialized - types are valid
        }
        Err(e) => {
            // TOML is syntactically valid but doesn't match our schema
            // This could be a type error or missing required field
            let error_str = e.to_string();

            // Parse the error to extract useful information
            if error_str.contains("missing field") || error_str.contains("invalid type") {
                // For type mismatches, add a structured error
                errors.push(ConfigValidationError::InvalidValue {
                    file: path.to_path_buf(),
                    key: extract_key_from_toml_error(&error_str),
                    message: format_invalid_type_message(&error_str),
                });
            } else {
                // Other deserialization errors
                errors.push(ConfigValidationError::InvalidValue {
                    file: path.to_path_buf(),
                    key: "config".to_string(),
                    message: error_str,
                });
            }
        }
    }

    if errors.is_empty() {
        Ok(warnings)
    } else {
        Err(errors)
    }
}

/// Extract the key name from a TOML deserialization error.
fn extract_key_from_toml_error(error: &str) -> String {
    // toml errors look like: "missing field `developer_iters` at line 5"
    // or "invalid type: string \"five\", expected u32 for field `developer_iters`"
    if let Some(start) = error.find('`') {
        if let Some(end) = error[start + 1..].find('`') {
            return error[start + 1..start + 1 + end].to_string();
        }
    }
    "unknown".to_string()
}

/// Format an invalid type error message.
fn format_invalid_type_message(error: &str) -> String {
    // Parse the toml error to extract expected vs actual types
    // Format: "invalid type: string \"five\", expected u32 for field `developer_iters`"
    if error.contains("invalid type") {
        if let Some(start) = error.find("invalid type: ") {
            let rest = &error[start + 13..];
            if let Some(comma) = rest.find(',') {
                let actual = &rest[..comma];
                if let Some(expected_start) = rest.find("expected ") {
                    let expected_part = &rest[expected_start + 9..];
                    if let Some(end) = expected_part.find(' ') {
                        return format!("Expected {}, got {}", &expected_part[..end], actual);
                    }
                }
                return format!("Invalid value: {}", actual);
            }
        }
    }
    error.to_string()
}

/// Detect unknown keys and deprecated keys in a parsed TOML value.
///
/// Returns a tuple of:
/// - KeyLocationList for unknown keys
/// - KeyLocationList for deprecated keys
///
/// The location helps identify which section the key is in (e.g., "general.", "agents.claude.").
fn detect_unknown_and_deprecated_keys(value: &toml::Value) -> (KeyLocationList, KeyLocationList) {
    let mut unknown = Vec::new();
    let mut deprecated = Vec::new();

    // Get the top-level table
    if let Some(table) = value.as_table() {
        for (key, value) in table {
            match key.as_str() {
                // Valid top-level sections
                "general" | "ccs" | "agents" | "ccs_aliases" | "agent_chain" => {
                    // Recursively check subsections
                    let (section_unknown, section_deprecated) =
                        check_section(key.as_str(), value, &format!("{}.", key));
                    unknown.extend(section_unknown);
                    deprecated.extend(section_deprecated);
                }
                // Unknown top-level section
                _ => {
                    unknown.push((key.clone(), String::new()));
                }
            }
        }
    }

    (unknown, deprecated)
}

/// Check a section for unknown and deprecated keys.
///
/// Returns a tuple of:
/// - KeyLocationList for unknown keys
/// - KeyLocationList for deprecated keys
///
/// The location includes the section prefix.
fn check_section(
    section: &str,
    value: &toml::Value,
    prefix: &str,
) -> (KeyLocationList, KeyLocationList) {
    let mut unknown = Vec::new();
    let mut deprecated = Vec::new();

    match section {
        "general" => {
            if let Some(table) = value.as_table() {
                for key in table.keys() {
                    let key_str = key.as_str();
                    if DEPRECATED_GENERAL_KEYS.contains(&key_str) {
                        deprecated.push((key.clone(), prefix.to_string()));
                    } else if !VALID_GENERAL_KEYS.contains(&key_str) {
                        unknown.push((key.clone(), prefix.to_string()));
                    }
                }
            }
        }
        "ccs" => {
            if let Some(table) = value.as_table() {
                for key in table.keys() {
                    if !VALID_CCS_KEYS.contains(&key.as_str()) {
                        unknown.push((key.clone(), prefix.to_string()));
                    }
                }
            }
        }
        "agents" => {
            // agents is a map of agent names to configs
            // We don't validate agent names (they're user-defined)
            // But we can validate the keys within each agent config
            if let Some(table) = value.as_table() {
                for (agent_name, agent_value) in table {
                    if let Some(agent_table) = agent_value.as_table() {
                        for key in agent_table.keys() {
                            if !VALID_AGENT_CONFIG_KEYS.contains(&key.as_str()) {
                                unknown.push((key.clone(), format!("{}{}.", prefix, agent_name)));
                            }
                        }
                    }
                }
            }
        }
        "ccs_aliases" => {
            // ccs_aliases is a map of alias names to configs
            // We don't validate alias names (they're user-defined)
            if let Some(table) = value.as_table() {
                for (alias_name, alias_value) in table {
                    if let Some(alias_table) = alias_value.as_table() {
                        for key in alias_table.keys() {
                            if !VALID_CCS_ALIAS_CONFIG_KEYS.contains(&key.as_str()) {
                                unknown.push((key.clone(), format!("{}{}.", prefix, alias_name)));
                            }
                        }
                    }
                }
            }
        }
        "agent_chain" => {
            // agent_chain has developer and reviewer keys
            if let Some(table) = value.as_table() {
                for key in table.keys() {
                    if !VALID_AGENT_CHAIN_KEYS.contains(&key.as_str()) {
                        unknown.push((key.clone(), prefix.to_string()));
                    }
                }
            }
        }
        _ => {
            // Unknown section - should have been caught at top level
        }
    }

    (unknown, deprecated)
}

/// Deprecated keys for the [general] section.
/// These keys are accepted for backward compatibility but their use should trigger warnings.
const DEPRECATED_GENERAL_KEYS: &[&str] = &[
    "auto_rebase",           // Never implemented, removed in favor of manual git control
    "max_recovery_attempts", // Never implemented, superseded by retry mechanisms
];

/// Valid keys for the [general] section.
const VALID_GENERAL_KEYS: &[&str] = &[
    "verbosity",
    "interactive",
    "auto_detect_stack",
    "strict_validation",
    "checkpoint_enabled",
    "force_universal_prompt",
    "isolation_mode",
    "developer_iters",
    "reviewer_reviews",
    "developer_context",
    "reviewer_context",
    "review_depth",
    "prompt_path",
    "templates_dir",
    "git_user_name",
    "git_user_email",
    "max_dev_continuations",
    "max_xsd_retries",
    "max_same_agent_retries",
    "behavior",
    "workflow",
    "execution",
    // Note: Deprecated keys (auto_rebase, max_recovery_attempts) are included here
    // to avoid breaking existing configs. They trigger warnings, not errors.
    "auto_rebase",
    "max_recovery_attempts",
];

/// Valid keys for the [ccs] section.
const VALID_CCS_KEYS: &[&str] = &[
    "output_flag",
    "yolo_flag",
    "verbose_flag",
    "print_flag",
    "streaming_flag",
    "json_parser",
    "session_flag",
    "can_commit",
];

/// Valid keys for agent configurations (within [agents.<name>]).
const VALID_AGENT_CONFIG_KEYS: &[&str] = &[
    "cmd",
    "output_flag",
    "yolo_flag",
    "verbose_flag",
    "print_flag",
    "streaming_flag",
    "session_flag",
    "can_commit",
    "json_parser",
    "model_flag",
    "display_name",
];

/// Valid keys for CCS alias configurations (within [ccs_aliases.<name>]).
const VALID_CCS_ALIAS_CONFIG_KEYS: &[&str] = &[
    "cmd",
    "output_flag",
    "yolo_flag",
    "verbose_flag",
    "print_flag",
    "streaming_flag",
    "json_parser",
    "session_flag",
    "can_commit",
    "model_flag",
];

/// Valid keys for the [agent_chain] section.
///
/// This must match all fields in FallbackConfig from agents/fallback.rs.
const VALID_AGENT_CHAIN_KEYS: &[&str] = &[
    "developer",
    "reviewer",
    "commit",
    "analysis",
    "provider_fallback",
    "max_retries",
    "retry_delay_ms",
    "backoff_multiplier",
    "max_backoff_ms",
    "max_cycles",
];

/// Get all valid configuration keys for typo detection.
fn get_valid_config_keys() -> Vec<&'static str> {
    vec![
        // Top-level sections
        "general",
        "ccs",
        "agents",
        "ccs_aliases",
        "agent_chain",
        // General config keys
        "verbosity",
        "interactive",
        "auto_detect_stack",
        "strict_validation",
        "checkpoint_enabled",
        "force_universal_prompt",
        "isolation_mode",
        "developer_iters",
        "reviewer_reviews",
        "developer_context",
        "reviewer_context",
        "review_depth",
        "prompt_path",
        "templates_dir",
        "git_user_name",
        "git_user_email",
        "max_dev_continuations",
        "max_xsd_retries",
        "max_same_agent_retries",
        // Behavior flags (nested)
        "behavior",
        // Workflow flags (nested)
        "workflow",
        // Execution flags (nested)
        "execution",
        // CCS config keys
        "output_flag",
        "yolo_flag",
        "verbose_flag",
        "print_flag",
        "streaming_flag",
        "json_parser",
        "session_flag",
        "can_commit",
        // Agent config keys
        "cmd",
        "model_flag",
        "display_name",
        // CCS alias config keys
        "ccs_aliases",
    ]
}

/// Format validation errors for user display.
pub fn format_validation_errors(errors: &[ConfigValidationError]) -> String {
    let mut output = String::new();

    for error in errors {
        output.push_str(&format!("  {}\n", error));

        if let ConfigValidationError::UnknownKey {
            suggestion: Some(s),
            ..
        } = error
        {
            output.push_str(&format!("    Did you mean '{}'?\n", s));
        }
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levenshtein_distance() {
        assert_eq!(levenshtein_distance("", ""), 0);
        assert_eq!(levenshtein_distance("abc", "abc"), 0);
        assert_eq!(levenshtein_distance("abc", "abd"), 1);
        assert_eq!(levenshtein_distance("developer_iters", "develper_iters"), 1);
    }

    #[test]
    fn test_suggest_key() {
        let valid_keys = &["developer_iters", "reviewer_reviews", "verbosity"];

        assert_eq!(
            suggest_key("develper_iters", valid_keys),
            Some("developer_iters".to_string())
        );

        assert_eq!(
            suggest_key("verbozity", valid_keys),
            Some("verbosity".to_string())
        );

        // No suggestion for completely different key
        assert_eq!(suggest_key("completely_different", valid_keys), None);
    }

    #[test]
    fn test_validate_config_file_valid_toml() {
        let content = r#"
[general]
verbosity = 2
developer_iters = 5
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_config_file_invalid_toml() {
        let content = r#"
[general
verbosity = 2
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(result.is_err());

        if let Err(errors) = result {
            assert_eq!(errors.len(), 1);
            match &errors[0] {
                ConfigValidationError::TomlSyntax { file, .. } => {
                    assert_eq!(file, Path::new("test.toml"));
                }
                _ => panic!("Expected TomlSyntax error"),
            }
        }
    }

    #[test]
    fn test_format_validation_errors_with_suggestion() {
        let errors = vec![ConfigValidationError::UnknownKey {
            file: PathBuf::from("test.toml"),
            key: "develper_iters".to_string(),
            suggestion: Some("developer_iters".to_string()),
        }];

        let formatted = format_validation_errors(&errors);
        assert!(formatted.contains("develper_iters"));
        assert!(formatted.contains("Did you mean 'developer_iters'?"));
    }

    #[test]
    fn test_format_validation_errors_without_suggestion() {
        let errors = vec![ConfigValidationError::UnknownKey {
            file: PathBuf::from("test.toml"),
            key: "completely_unknown".to_string(),
            suggestion: None,
        }];

        let formatted = format_validation_errors(&errors);
        assert!(formatted.contains("completely_unknown"));
        assert!(!formatted.contains("Did you mean"));
    }

    #[test]
    fn test_format_validation_errors_multiple() {
        // Create a real TOML parse error
        let toml_error = toml::from_str::<toml::Value>("[invalid\nkey = value").unwrap_err();

        let errors = vec![
            ConfigValidationError::TomlSyntax {
                file: PathBuf::from("global.toml"),
                error: toml_error,
            },
            ConfigValidationError::UnknownKey {
                file: PathBuf::from("local.toml"),
                key: "bad_key".to_string(),
                suggestion: Some("good_key".to_string()),
            },
        ];

        let formatted = format_validation_errors(&errors);
        assert!(formatted.contains("global.toml"));
        assert!(formatted.contains("local.toml"));
        assert!(formatted.contains("Did you mean 'good_key'?"));
    }

    #[test]
    fn test_validate_config_file_unknown_key() {
        let content = r#"
[general]
develper_iters = 5
verbosity = 2
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        // Unknown keys are now detected via custom validation
        assert!(result.is_err());

        if let Err(errors) = result {
            assert_eq!(errors.len(), 1);
            match &errors[0] {
                ConfigValidationError::UnknownKey {
                    key, suggestion, ..
                } => {
                    assert!(key.contains("develper_iters"));
                    assert_eq!(suggestion.as_ref().unwrap(), "developer_iters");
                }
                _ => panic!("Expected UnknownKey error"),
            }
        }
    }

    #[test]
    fn test_validate_config_file_invalid_type() {
        // This test verifies that type errors during deserialization are caught.
        // When a string is provided where an integer is expected, validation should fail.
        let content = r#"
[general]
developer_iters = "five"
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(result.is_err(), "Should fail with string instead of int");
    }

    #[test]
    fn test_validate_config_file_valid_with_all_sections() {
        let content = r#"
[general]
verbosity = 2
developer_iters = 5
reviewer_reviews = 2

[ccs]
output_flag = "--output=json"

[agents.claude]
cmd = "claude"

[ccs_aliases]
work = "ccs work"
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(result.is_ok(), "Valid config with all sections should pass");
    }

    #[test]
    fn test_validate_config_file_empty_file() {
        let content = "";
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(result.is_ok(), "Empty file should use default values");
    }

    #[test]
    fn test_validate_agent_chain_with_all_valid_keys() {
        // Verify all FallbackConfig fields are accepted in agent_chain section
        let content = r#"
[general]
developer_iters = 5

[agent_chain]
developer = ["claude", "codex"]
reviewer = ["claude"]
commit = ["claude"]
analysis = ["claude"]
max_retries = 5
retry_delay_ms = 2000
backoff_multiplier = 2.5
max_backoff_ms = 120000
max_cycles = 5

[agent_chain.provider_fallback]
opencode = ["-m opencode/glm-4.7-free", "-m opencode/claude-sonnet-4"]
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(result.is_ok(), "All FallbackConfig fields should be valid");
    }

    #[test]
    fn test_validate_agent_chain_commit_key() {
        // The commit key was missing from VALID_AGENT_CHAIN_KEYS
        let content = r#"
[agent_chain]
developer = ["claude"]
commit = ["claude"]
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(result.is_ok(), "commit key should be valid in agent_chain");
    }

    #[test]
    fn test_validate_agent_chain_analysis_key() {
        // The analysis key was missing from VALID_AGENT_CHAIN_KEYS
        let content = r#"
[agent_chain]
developer = ["claude"]
analysis = ["claude"]
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(
            result.is_ok(),
            "analysis key should be valid in agent_chain"
        );
    }

    #[test]
    fn test_validate_agent_chain_retry_keys() {
        // These retry/backoff keys were missing from VALID_AGENT_CHAIN_KEYS
        let content = r#"
[agent_chain]
developer = ["claude"]
max_retries = 3
retry_delay_ms = 5000
backoff_multiplier = 1.5
max_backoff_ms = 30000
max_cycles = 2
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(
            result.is_ok(),
            "retry/backoff keys should be valid in agent_chain"
        );
    }

    #[test]
    fn test_validate_agent_chain_provider_fallback_key() {
        // The provider_fallback nested table was missing from VALID_AGENT_CHAIN_KEYS
        let content = r#"
[agent_chain]
developer = ["opencode"]

[agent_chain.provider_fallback]
opencode = ["-m opencode/glm-4.7-free", "-m opencode/claude-sonnet-4"]
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(
            result.is_ok(),
            "provider_fallback nested table should be valid in agent_chain"
        );
    }

    #[test]
    fn test_validate_config_file_deprecated_key_warning() {
        let content = r#"
[general]
verbosity = 2
auto_rebase = true
max_recovery_attempts = 3
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(result.is_ok(), "Deprecated keys should not cause errors");

        if let Ok(warnings) = result {
            assert_eq!(warnings.len(), 2, "Should have 2 deprecation warnings");
            assert!(
                warnings.iter().any(|w| w.contains("auto_rebase")),
                "Should warn about auto_rebase"
            );
            assert!(
                warnings.iter().any(|w| w.contains("max_recovery_attempts")),
                "Should warn about max_recovery_attempts"
            );
        }
    }

    #[test]
    fn test_validate_config_file_no_warnings_without_deprecated() {
        let content = r#"
[general]
verbosity = 2
developer_iters = 5
"#;
        let result = validate_config_file(Path::new("test.toml"), content);
        assert!(result.is_ok(), "Valid config should pass");

        if let Ok(warnings) = result {
            assert_eq!(warnings.len(), 0, "Should have no warnings");
        }
    }
}
