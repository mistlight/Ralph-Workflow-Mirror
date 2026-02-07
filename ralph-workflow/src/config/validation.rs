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
pub fn validate_config_file(path: &Path, content: &str) -> ValidationResult<()> {
    let mut errors = Vec::new();

    // Validate TOML syntax
    match toml::from_str::<toml::Value>(content) {
        Ok(_) => {
            // Syntax is valid
            // TODO: Add semantic validation (unknown keys, type checking)
        }
        Err(e) => {
            errors.push(ConfigValidationError::TomlSyntax {
                file: path.to_path_buf(),
                error: e,
            });
        }
    }

    if errors.is_empty() {
        Ok(())
    } else {
        Err(errors)
    }
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
}
