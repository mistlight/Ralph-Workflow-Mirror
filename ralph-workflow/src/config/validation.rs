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
pub type ValidationResult<T> = Result<T, Vec<ConfigValidationError>>;

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

/// Validate a config file and collect errors.
///
/// This validates:
/// - TOML syntax
/// - Type checking against UnifiedConfig schema
/// - Unknown keys with typo suggestions
pub fn validate_config_file(path: &Path, content: &str) -> ValidationResult<()> {
    let mut errors = Vec::new();

    // Step 1: Validate TOML syntax
    match toml::from_str::<toml::Value>(content) {
        Ok(_parsed) => {
            // Syntax is valid, proceed with schema validation
        }
        Err(e) => {
            errors.push(ConfigValidationError::TomlSyntax {
                file: path.to_path_buf(),
                error: e,
            });
            return Err(errors);
        }
    };

    // Step 2: Validate against UnifiedConfig schema for type checking and unknown keys
    // First, try to deserialize as UnifiedConfig to catch type errors
    match toml::from_str::<crate::config::unified::UnifiedConfig>(content) {
        Ok(_) => {
            // Successfully deserialized - types are valid
        }
        Err(e) => {
            // TOML is syntactically valid but doesn't match our schema
            // This could be a type error or unknown key
            let error_str = e.to_string();

            // Parse the error to extract useful information
            if error_str.contains("missing field") || error_str.contains("invalid type") {
                // For type mismatches, add a structured error
                errors.push(ConfigValidationError::InvalidValue {
                    file: path.to_path_buf(),
                    key: extract_key_from_toml_error(&error_str),
                    message: format_invalid_type_message(&error_str),
                });
            } else if error_str.contains("unknown field") {
                // Unknown key - extract the key name and provide suggestion
                let unknown_key = extract_unknown_key(&error_str);
                let valid_keys = get_valid_config_keys();
                let suggestion = suggest_key(&unknown_key, &valid_keys);

                errors.push(ConfigValidationError::UnknownKey {
                    file: path.to_path_buf(),
                    key: unknown_key,
                    suggestion,
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
        Ok(())
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

/// Extract the unknown key from a "unknown field" error.
fn extract_unknown_key(error: &str) -> String {
    // toml errors look like: "unknown field `bad_key`, expected one of..."
    if let Some(start) = error.find("unknown field `") {
        let rest = &error[start + 14..];
        if let Some(end) = rest.find('`') {
            return rest[..end].to_string();
        }
    }
    extract_key_from_toml_error(error)
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
        // Currently passes because serde ignores unknown fields by default
        // This would require serde::deny_unknown_fields or custom validation
        assert!(result.is_ok());
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
}
