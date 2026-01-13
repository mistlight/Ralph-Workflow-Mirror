//! Result extraction from agent JSON logs.
//!
//! This module provides utilities to extract structured output from agent JSON logs.
//! The orchestrator uses this to capture plan and issues content from agent output,
//! ensuring that file writing is always controlled by the orchestrator.
//!
//! # Design Principles
//!
//! 1. **Orchestrator writes all files**: Agent file writes are ignored/overwritten
//! 2. **Always return content**: Even if validation fails, return raw content
//! 3. **Validation is advisory**: Validation results are warnings, not blockers

use serde_json::Value as JsonValue;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader};
use std::path::Path;

/// Result of extracting content from an agent's JSON log.
#[derive(Debug, Clone)]
pub struct ExtractionResult {
    /// The raw content extracted from the log (if any)
    pub raw_content: Option<String>,
    /// Whether the content passed validation
    pub is_valid: bool,
    /// Validation warning message (if validation failed but content exists)
    pub validation_warning: Option<String>,
}

impl ExtractionResult {
    /// Create a result with valid content
    fn valid(content: String) -> Self {
        Self {
            raw_content: Some(content),
            is_valid: true,
            validation_warning: None,
        }
    }

    /// Create a result with invalid content
    fn invalid(content: String, warning: &str) -> Self {
        Self {
            raw_content: Some(content),
            is_valid: false,
            validation_warning: Some(warning.to_string()),
        }
    }

    /// Create an empty result (no content found)
    fn empty() -> Self {
        Self {
            raw_content: None,
            is_valid: false,
            validation_warning: None,
        }
    }
}

/// Extract the last "result" event from agent JSON logs.
///
/// Scans all files in the log directory for JSON lines and returns the
/// last `{"type": "result", "result": "..."}` event's content.
///
/// # Arguments
///
/// * `log_dir` - Path to the log directory (e.g., `.agent/logs/planning_1`)
///
/// # Returns
///
/// The raw content from the last result event, or None if no result found.
pub fn extract_last_result(log_dir: &Path) -> io::Result<Option<String>> {
    let log_entries = match fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };

    let mut last_result: Option<String> = None;

    for entry in log_entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => continue,
        };

        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file = match File::open(&path) {
            Ok(f) => f,
            Err(_) => continue,
        };

        let reader = BufReader::new(file);

        for line in reader.lines() {
            let line = match line {
                Ok(l) => l,
                Err(_) => continue,
            };

            // Skip non-JSON lines
            if !line.trim().starts_with('{') {
                continue;
            }

            // Parse JSON and look for "result" events
            if let Ok(value) = serde_json::from_str::<JsonValue>(&line) {
                if let Some(typ) = value.get("type").and_then(|v| v.as_str()) {
                    if typ == "result" {
                        if let Some(result) = value.get("result").and_then(|v| v.as_str()) {
                            last_result = Some(result.to_string());
                        }
                    }
                }
            }
        }
    }

    Ok(last_result)
}

/// Validate plan content.
///
/// Checks if the content looks like a valid plan:
/// - Contains markdown headers (lines starting with #)
/// - Has reasonable length (> 50 chars)
/// - Contains plan-like structure indicators
fn validate_plan_content(content: &str) -> (bool, Option<String>) {
    let content_clean = content.trim();

    let has_header = content_clean.lines().any(|line| line.trim().starts_with('#'));
    let has_min_length = content_clean.len() > 50;
    let has_structure = content_clean.contains("step")
        || content_clean.contains("task")
        || content_clean.contains("phase")
        || content_clean.contains("implement")
        || content_clean.contains("create")
        || content_clean.contains("add")
        || content_clean.contains("Step")
        || content_clean.contains("Task")
        || content_clean.contains("Phase");

    if has_header && has_min_length && has_structure {
        (true, None)
    } else {
        let mut warnings = Vec::new();
        if !has_header {
            warnings.push("no markdown headers");
        }
        if !has_min_length {
            warnings.push("content too short");
        }
        if !has_structure {
            warnings.push("no plan structure keywords");
        }
        (false, Some(warnings.join(", ")))
    }
}

/// Validate issues content.
///
/// Checks if the content looks like valid issues:
/// - Contains checkboxes (- [ ] or - [x])
/// - Contains severity markers (Critical:, High:, etc.)
/// - Or contains "no issues" declaration
fn validate_issues_content(content: &str) -> (bool, Option<String>) {
    let content_clean = content.trim();

    let has_checkbox = content_clean.contains("- [")
        || content_clean.contains("- [x]")
        || content_clean.contains("- [ ]");
    let has_severity = content_clean.contains("Critical:")
        || content_clean.contains("High:")
        || content_clean.contains("Medium:")
        || content_clean.contains("Low:");
    let has_no_issues = content_clean.to_lowercase().contains("no issues");
    let has_min_length = content_clean.len() > 10;

    if (has_checkbox || has_severity || has_no_issues) && has_min_length {
        (true, None)
    } else {
        let mut warnings = Vec::new();
        if !has_checkbox && !has_severity && !has_no_issues {
            warnings.push("no issue markers found");
        }
        if !has_min_length {
            warnings.push("content too short");
        }
        (false, Some(warnings.join(", ")))
    }
}

/// Extract and validate plan content from agent logs.
///
/// # Arguments
///
/// * `log_dir` - Path to the planning log directory
///
/// # Returns
///
/// An `ExtractionResult` containing:
/// - The raw content (if any result event was found)
/// - Validation status (whether it looks like a valid plan)
/// - Warning message (if validation failed)
pub fn extract_plan(log_dir: &Path) -> io::Result<ExtractionResult> {
    let raw_content = extract_last_result(log_dir)?;

    match raw_content {
        Some(content) => {
            let content_clean = content.trim().to_string();
            let (is_valid, warning) = validate_plan_content(&content_clean);
            if is_valid {
                Ok(ExtractionResult::valid(content_clean))
            } else {
                Ok(ExtractionResult::invalid(
                    content_clean,
                    &warning.unwrap_or_default(),
                ))
            }
        }
        None => Ok(ExtractionResult::empty()),
    }
}

/// Extract and validate issues content from agent logs.
///
/// # Arguments
///
/// * `log_dir` - Path to the reviewer log directory
///
/// # Returns
///
/// An `ExtractionResult` containing:
/// - The raw content (if any result event was found)
/// - Validation status (whether it looks like valid issues)
/// - Warning message (if validation failed)
pub fn extract_issues(log_dir: &Path) -> io::Result<ExtractionResult> {
    let raw_content = extract_last_result(log_dir)?;

    match raw_content {
        Some(content) => {
            let content_clean = content.trim().to_string();
            let (is_valid, warning) = validate_issues_content(&content_clean);
            if is_valid {
                Ok(ExtractionResult::valid(content_clean))
            } else {
                Ok(ExtractionResult::invalid(
                    content_clean,
                    &warning.unwrap_or_default(),
                ))
            }
        }
        None => Ok(ExtractionResult::empty()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn create_log_file(dir: &Path, filename: &str, content: &str) {
        fs::create_dir_all(dir).unwrap();
        fs::write(dir.join(filename), content).unwrap();
    }

    #[test]
    fn test_extract_valid_plan() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("planning_1");

        let json_log = r##"{"type": "system", "message": "starting"}
{"type": "result", "result": "# Plan\n\n## Step 1\nImplement the feature\n\n## Step 2\nAdd tests"}"##;
        create_log_file(&log_dir, "output.log", json_log);

        let result = extract_plan(&log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(result.is_valid);
        assert!(result.validation_warning.is_none());
    }

    #[test]
    fn test_extract_invalid_but_present_content() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("planning_1");

        // Content exists but doesn't look like a plan
        let json_log = r#"{"type": "result", "result": "Hello world"}"#;
        create_log_file(&log_dir, "output.log", json_log);

        let result = extract_plan(&log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(!result.is_valid);
        assert!(result.validation_warning.is_some());
        assert_eq!(result.raw_content.unwrap(), "Hello world");
    }

    #[test]
    fn test_extract_no_result_events() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("planning_1");

        let json_log = r#"{"type": "system", "message": "starting"}
{"type": "tool_use", "tool": "read_file"}"#;
        create_log_file(&log_dir, "output.log", json_log);

        let result = extract_plan(&log_dir).unwrap();
        assert!(result.raw_content.is_none());
        assert!(!result.is_valid);
    }

    #[test]
    fn test_extract_malformed_json() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("planning_1");

        let json_log = r##"not json
{"invalid json
{"type": "result", "result": "# Valid Plan\n\nStep 1: Create feature"}"##;
        create_log_file(&log_dir, "output.log", json_log);

        let result = extract_plan(&log_dir).unwrap();
        assert!(result.raw_content.is_some());
        // Should still find the valid JSON line
    }

    #[test]
    fn test_extract_empty_log_dir() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("nonexistent");

        let result = extract_plan(&log_dir).unwrap();
        assert!(result.raw_content.is_none());
    }

    #[test]
    fn test_extract_valid_issues() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("reviewer_1");

        let json_log = r##"{"type": "result", "result": "# Issues\n\nCritical:\n- [ ] Fix security bug"}"##;
        create_log_file(&log_dir, "output.log", json_log);

        let result = extract_issues(&log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(result.is_valid);
    }

    #[test]
    fn test_extract_no_issues_found() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("reviewer_1");

        let json_log = r#"{"type": "result", "result": "No issues found. The code looks good."}"#;
        create_log_file(&log_dir, "output.log", json_log);

        let result = extract_issues(&log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(result.is_valid); // "no issues" is valid
    }

    #[test]
    fn test_uses_last_result_event() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("planning_1");

        // Multiple result events - should use the last one
        let json_log = r##"{"type": "result", "result": "First result"}
{"type": "result", "result": "# Final Plan\n\nStep 1: Implement feature"}"##;
        create_log_file(&log_dir, "output.log", json_log);

        let result = extract_plan(&log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(result.raw_content.unwrap().contains("Final Plan"));
    }
}
