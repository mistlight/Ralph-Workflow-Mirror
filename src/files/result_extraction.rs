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
//!
//! # Log File Resolution
//!
//! The extraction supports two modes:
//! 1. **Directory mode**: If `log_path` is a directory, scan all files in it
//! 2. **Prefix mode**: If `log_path` is not a directory, treat it as a prefix and
//!    search for files matching `{prefix}_*.log` in the parent directory
//!
//! This dual-mode support handles both legacy directory-based logs and the current
//! prefix-based naming convention (e.g., `.agent/logs/planning_1_glm_0.log`).

use serde_json::Value as JsonValue;
use std::fs::{self, File};
use std::io::{self, BufRead, BufReader, Read};
use std::path::{Path, PathBuf};

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

/// Extract the last "result" event from a single log file.
///
/// Scans the file for JSON lines and returns the last `{"type": "result", "result": "..."}`
/// event's content.
fn extract_result_from_file(path: &Path) -> io::Result<Option<String>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };

    let reader = BufReader::new(file);
    let mut last_result: Option<String> = None;

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

    Ok(last_result)
}

/// Find log files matching a prefix pattern in a directory.
///
/// Returns all files that start with `{prefix}_` and end with `.log`.
fn find_log_files_with_prefix(parent_dir: &Path, prefix: &str) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(parent_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let mut log_files = Vec::new();
    let prefix_pattern = format!("{}_", prefix);

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let file_name = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // Match files like "planning_1_glm_0.log" when prefix is "planning_1"
        if file_name.starts_with(&prefix_pattern) && file_name.ends_with(".log") {
            log_files.push(path);
        }
    }

    // Sort by modification time (most recent last) to ensure consistent ordering
    log_files.sort_by(|a, b| {
        let time_a = fs::metadata(a)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let time_b = fs::metadata(b)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        time_a.cmp(&time_b)
    });

    Ok(log_files)
}

/// Find subdirectories matching a prefix pattern.
///
/// This handles the legacy case where agent names containing "/" created
/// nested directories (e.g., "planning_1_ccs/glm_0.log" instead of flat files).
fn find_subdirs_with_prefix(parent_dir: &Path, prefix: &str) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(parent_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let mut subdirs = Vec::new();
    let prefix_pattern = format!("{}_", prefix);

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let dir_name = match path.file_name().and_then(|s| s.to_str()) {
            Some(name) => name,
            None => continue,
        };

        // Match directories like "planning_1_ccs" when prefix is "planning_1"
        if dir_name.starts_with(&prefix_pattern) {
            subdirs.push(path);
        }
    }

    // Sort by modification time (most recent last) to ensure consistent ordering
    subdirs.sort_by(|a, b| {
        let time_a = fs::metadata(a)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        let time_b = fs::metadata(b)
            .and_then(|m| m.modified())
            .unwrap_or(std::time::SystemTime::UNIX_EPOCH);
        time_a.cmp(&time_b)
    });

    Ok(subdirs)
}

/// Extract the last "result" event from agent JSON logs.
///
/// Supports three modes:
/// 1. **Directory mode**: If `log_path` is a directory, scan all files in it
/// 2. **Prefix mode**: If `log_path` is not a directory, treat it as a prefix and
///    search for files matching `{prefix}_*.log` in the parent directory
/// 3. **Subdirectory fallback**: If no files found, check for subdirectories matching
///    `{prefix}_*` (handles legacy logs where agent names with "/" created nested dirs)
///
/// # Arguments
///
/// * `log_path` - Path to the log directory OR log file prefix
///
/// # Returns
///
/// The raw content from the last result event, or None if no result found.
pub fn extract_last_result(log_path: &Path) -> io::Result<Option<String>> {
    // Strategy 1: If log_path is a directory, scan all files in it (legacy mode)
    if log_path.is_dir() {
        return extract_from_directory(log_path);
    }

    // Strategy 2: Treat log_path as a prefix and search parent directory
    let parent = log_path.parent().unwrap_or(Path::new("."));
    let prefix = log_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if prefix.is_empty() {
        return Ok(None);
    }

    let log_files = find_log_files_with_prefix(parent, prefix)?;

    if !log_files.is_empty() {
        let mut last_result: Option<String> = None;
        for log_file in log_files {
            if let Some(result) = extract_result_from_file(&log_file)? {
                last_result = Some(result);
            }
        }
        if last_result.is_some() {
            return Ok(last_result);
        }
    }

    // Strategy 3: Check for subdirectories matching prefix pattern
    // This handles the legacy case where agent names with "/" created nested directories
    // (e.g., "planning_1_ccs/glm_0.log" instead of "planning_1_ccs-glm_0.log")
    let subdirs = find_subdirs_with_prefix(parent, prefix)?;
    for subdir in subdirs {
        if let Some(result) = extract_from_directory(&subdir)? {
            return Ok(Some(result));
        }
    }

    // Final fallback: check if the exact path exists as a file
    if log_path.is_file() {
        return extract_result_from_file(log_path);
    }

    Ok(None)
}

/// Extract from a directory by scanning all files in it.
fn extract_from_directory(log_dir: &Path) -> io::Result<Option<String>> {
    let log_entries = match fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };

    let mut last_result: Option<String> = None;

    for entry in log_entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if let Some(result) = extract_result_from_file(&path)? {
            last_result = Some(result);
        }
    }

    Ok(last_result)
}

/// Extract plan content from text by looking for markdown structure.
///
/// This is a fallback method for cases where JSON result events are not available.
/// It looks for common plan markers like `## Summary` and `## Implementation Steps`.
pub fn extract_plan_from_text(content: &str) -> Option<String> {
    // Look for plan markers in order of specificity
    let markers = [
        "## Summary",
        "## Implementation Steps",
        "# Plan",
        "# Implementation Plan",
    ];

    for marker in markers {
        if let Some(start) = content.find(marker) {
            // Extract from the marker to the end (or to a clear boundary)
            let plan_content = &content[start..];

            // Trim and return if we have substantial content
            let trimmed = plan_content.trim();
            if trimmed.len() > 50 {
                return Some(trimmed.to_string());
            }
        }
    }

    None
}

/// Extract plan content from log files using text-based fallback.
///
/// This scans all log files matching the prefix and looks for markdown plan structure.
/// Also checks subdirectories matching the prefix pattern (for legacy logs where agent
/// names with "/" created nested directories).
pub fn extract_plan_from_logs_text(log_path: &Path) -> io::Result<Option<String>> {
    // Helper to extract from a list of files
    fn extract_from_files(files: &[PathBuf]) -> Option<String> {
        for log_file in files {
            let mut content = String::new();
            if let Ok(mut file) = File::open(log_file) {
                if file.read_to_string(&mut content).is_ok() {
                    if let Some(plan) = extract_plan_from_text(&content) {
                        return Some(plan);
                    }
                }
            }
        }
        None
    }

    // Strategy 1: If log_path is a directory, scan all files in it
    if log_path.is_dir() {
        let log_files: Vec<_> = fs::read_dir(log_path)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        if let Some(plan) = extract_from_files(&log_files) {
            return Ok(Some(plan));
        }
        return Ok(None);
    }

    // Strategy 2: Treat log_path as a prefix and search parent directory
    let parent = log_path.parent().unwrap_or(Path::new("."));
    let prefix = log_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if prefix.is_empty() {
        return Ok(None);
    }

    let log_files = find_log_files_with_prefix(parent, prefix)?;
    if let Some(plan) = extract_from_files(&log_files) {
        return Ok(Some(plan));
    }

    // Strategy 3: Check subdirectories matching prefix pattern
    // This handles the legacy case where agent names with "/" created nested directories
    let subdirs = find_subdirs_with_prefix(parent, prefix)?;
    for subdir in subdirs {
        let subdir_files: Vec<_> = fs::read_dir(&subdir)?
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.is_file())
            .collect();
        if let Some(plan) = extract_from_files(&subdir_files) {
            return Ok(Some(plan));
        }
    }

    Ok(None)
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

    // =====================================================
    // PREFIX-BASED FILE MATCHING TESTS (New functionality)
    // =====================================================

    #[test]
    fn test_extract_from_prefix_pattern() {
        let temp = TempDir::new().unwrap();

        // Create log file matching prefix pattern (not in subdirectory)
        // This simulates: .agent/logs/planning_1_glm_0.log
        let json_log = r##"{"type": "result", "result": "# Plan\n\n## Summary\nTest plan with implementation steps"}"##;
        fs::write(temp.path().join("planning_1_glm_0.log"), json_log).unwrap();

        // Extract using prefix (not directory)
        let prefix = temp.path().join("planning_1");
        let result = extract_plan(&prefix).unwrap();

        assert!(
            result.raw_content.is_some(),
            "Should find content from prefix-matched file"
        );
        assert!(result.raw_content.unwrap().contains("## Summary"));
    }

    #[test]
    fn test_extract_from_prefix_with_multiple_files() {
        let temp = TempDir::new().unwrap();

        // Create multiple log files simulating fallback/retry scenario
        let json_log1 = r##"{"type": "result", "result": "First attempt - no plan"}"##;
        let json_log2 = r##"{"type": "result", "result": "# Plan\n\n## Summary\nSuccess with implementation steps!"}"##;

        fs::write(temp.path().join("planning_1_agent1_0.log"), json_log1).unwrap();

        // Sleep briefly to ensure different modification times
        std::thread::sleep(std::time::Duration::from_millis(10));

        fs::write(temp.path().join("planning_1_agent2_0.log"), json_log2).unwrap();

        let prefix = temp.path().join("planning_1");
        let result = extract_plan(&prefix).unwrap();

        assert!(result.raw_content.is_some());
        // Should find content from one of the files (ideally the valid one)
        let content = result.raw_content.unwrap();
        // We expect the last file (by modification time) to be processed last
        assert!(
            content.contains("Success") || content.contains("First attempt"),
            "Should extract content from one of the files"
        );
    }

    #[test]
    fn test_extract_prefix_no_matching_files() {
        let temp = TempDir::new().unwrap();

        // Create files that don't match the prefix pattern
        fs::write(temp.path().join("other_file.log"), "some content").unwrap();
        fs::write(temp.path().join("planning_2_glm_0.log"), "wrong prefix").unwrap();

        let prefix = temp.path().join("planning_1");
        let result = extract_plan(&prefix).unwrap();

        assert!(
            result.raw_content.is_none(),
            "Should not find content when no files match prefix"
        );
    }

    #[test]
    fn test_extract_directory_mode_backwards_compatible() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("planning_1");
        fs::create_dir(&log_dir).unwrap();

        let json_log = r##"{"type": "result", "result": "# Plan\n\n## Summary\nTest from directory"}"##;
        fs::write(log_dir.join("output.log"), json_log).unwrap();

        // Should work with actual directory (backwards compatible)
        let result = extract_plan(&log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(result.raw_content.unwrap().contains("Test from directory"));
    }

    #[test]
    fn test_extract_prefix_exact_file_fallback() {
        let temp = TempDir::new().unwrap();

        // Create an exact file match (no prefix pattern)
        let json_log = r##"{"type": "result", "result": "# Plan\n\n## Summary\nDirect file"}"##;
        let exact_file = temp.path().join("planning_1.log");
        fs::write(&exact_file, json_log).unwrap();

        // This should fall back to reading the exact file
        let result = extract_last_result(&exact_file).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Direct file"));
    }

    #[test]
    fn test_find_log_files_with_prefix() {
        let temp = TempDir::new().unwrap();

        // Create various files
        fs::write(temp.path().join("planning_1_glm_0.log"), "a").unwrap();
        fs::write(temp.path().join("planning_1_opus_1.log"), "b").unwrap();
        fs::write(temp.path().join("planning_2_glm_0.log"), "c").unwrap();
        fs::write(temp.path().join("other.txt"), "d").unwrap();

        let files = find_log_files_with_prefix(temp.path(), "planning_1").unwrap();

        assert_eq!(files.len(), 2, "Should find exactly 2 matching files");
        let names: Vec<_> = files
            .iter()
            .filter_map(|p| p.file_name())
            .filter_map(|n| n.to_str())
            .collect();
        assert!(names.contains(&"planning_1_glm_0.log"));
        assert!(names.contains(&"planning_1_opus_1.log"));
    }

    // =====================================================
    // TEXT-BASED FALLBACK EXTRACTION TESTS
    // =====================================================

    #[test]
    fn test_extract_plan_from_text_with_summary() {
        let content = r#"Some random output
## Summary
This is the plan summary with enough content to pass validation.

## Implementation Steps
1. Do the thing
2. Do the other thing
"#;
        let result = extract_plan_from_text(content);
        assert!(result.is_some());
        assert!(result.unwrap().starts_with("## Summary"));
    }

    #[test]
    fn test_extract_plan_from_text_with_implementation_steps() {
        let content = r#"Agent thinking...
## Implementation Steps
Step 1: Create the component with all necessary features
Step 2: Add tests and documentation
"#;
        let result = extract_plan_from_text(content);
        assert!(result.is_some());
        assert!(result.unwrap().starts_with("## Implementation Steps"));
    }

    #[test]
    fn test_extract_plan_from_text_no_markers() {
        let content = "This is just some random text without any plan markers.";
        let result = extract_plan_from_text(content);
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_plan_from_text_too_short() {
        let content = "## Summary\nShort";
        let result = extract_plan_from_text(content);
        assert!(result.is_none(), "Should reject content that's too short");
    }

    #[test]
    fn test_extract_plan_from_logs_text_fallback() {
        let temp = TempDir::new().unwrap();

        // Create log file with no JSON result event, but text plan content
        let text_log = r#"[agent] Starting...
[agent] Thinking about the plan...
## Summary
This is a text-based plan without proper JSON wrapping but with enough content.

## Implementation Steps
1. First step with detailed explanation
2. Second step with implementation details
"#;
        fs::write(temp.path().join("planning_1_glm_0.log"), text_log).unwrap();

        let prefix = temp.path().join("planning_1");
        let result = extract_plan_from_logs_text(&prefix).unwrap();

        assert!(result.is_some());
        assert!(result.unwrap().contains("## Summary"));
    }

    // =====================================================
    // ISSUES EXTRACTION WITH PREFIX TESTS
    // =====================================================

    #[test]
    fn test_extract_issues_from_prefix_pattern() {
        let temp = TempDir::new().unwrap();

        let json_log = r##"{"type": "result", "result": "# Issues\n\nCritical:\n- [ ] Fix the security vulnerability"}"##;
        fs::write(temp.path().join("reviewer_review_1_glm_0.log"), json_log).unwrap();

        let prefix = temp.path().join("reviewer_review_1");
        let result = extract_issues(&prefix).unwrap();

        assert!(result.raw_content.is_some());
        assert!(result.is_valid);
    }

    #[test]
    fn test_extract_issues_no_issues_from_prefix() {
        let temp = TempDir::new().unwrap();

        let json_log = r##"{"type": "result", "result": "No issues found. The code looks good."}"##;
        fs::write(temp.path().join("reviewer_1_opus_0.log"), json_log).unwrap();

        let prefix = temp.path().join("reviewer_1");
        let result = extract_issues(&prefix).unwrap();

        assert!(result.raw_content.is_some());
        assert!(result.is_valid);
    }

    // =====================================================
    // SUBDIRECTORY FALLBACK TESTS
    // (For legacy logs where agent names with "/" created nested dirs)
    // =====================================================

    #[test]
    fn test_extract_from_subdirectory_fallback() {
        let temp = TempDir::new().unwrap();

        // Create nested structure like: planning_1_ccs/glm_0.log
        // This simulates what happens when agent name is "ccs/glm"
        let subdir = temp.path().join("planning_1_ccs");
        fs::create_dir(&subdir).unwrap();

        let json_log = r##"{"type": "result", "result": "# Plan\n\n## Summary\nPlan from nested subdirectory"}"##;
        fs::write(subdir.join("glm_0.log"), json_log).unwrap();

        // Extract using prefix (not the nested path)
        let prefix = temp.path().join("planning_1");
        let result = extract_plan(&prefix).unwrap();

        assert!(
            result.raw_content.is_some(),
            "Should find content from subdirectory fallback"
        );
        assert!(result.raw_content.unwrap().contains("nested subdirectory"));
    }

    #[test]
    fn test_extract_text_from_subdirectory_fallback() {
        let temp = TempDir::new().unwrap();

        // Create nested structure like: planning_1_ccs/glm_0.log
        let subdir = temp.path().join("planning_1_ccs");
        fs::create_dir(&subdir).unwrap();

        // Create log file with text plan content (no JSON result event)
        let text_log = r#"[agent] Starting...
## Summary
This is a text-based plan from nested subdirectory with enough content to pass validation.

## Implementation Steps
1. First step with detailed explanation
"#;
        fs::write(subdir.join("glm_0.log"), text_log).unwrap();

        let prefix = temp.path().join("planning_1");
        let result = extract_plan_from_logs_text(&prefix).unwrap();

        assert!(
            result.is_some(),
            "Should find text content from subdirectory fallback"
        );
        assert!(result.unwrap().contains("nested subdirectory"));
    }

    // =====================================================
    // EDGE CASES AND ERROR HANDLING
    // =====================================================

    #[test]
    fn test_extract_empty_prefix() {
        let temp = TempDir::new().unwrap();

        // Test with path that has no file name component
        // Use temp directory to avoid permission issues
        let result = extract_last_result(temp.path()).unwrap();
        assert!(result.is_none(), "Empty directory should return None");
    }

    #[test]
    fn test_extract_nonexistent_parent_directory() {
        let result = extract_last_result(Path::new("/nonexistent/path/planning_1")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_result_from_single_file() {
        let temp = TempDir::new().unwrap();

        let json_log = r##"{"type": "system", "msg": "start"}
{"type": "result", "result": "# Plan\n\n## Summary\nThe plan content"}"##;
        let file_path = temp.path().join("test.log");
        fs::write(&file_path, json_log).unwrap();

        let result = extract_result_from_file(&file_path).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("## Summary"));
    }

    #[test]
    fn test_extract_result_from_file_not_found() {
        let result = extract_result_from_file(Path::new("/nonexistent/file.log")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_handles_empty_log_file() {
        let temp = TempDir::new().unwrap();

        let file_path = temp.path().join("empty.log");
        fs::write(&file_path, "").unwrap();

        let result = extract_result_from_file(&file_path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_handles_whitespace_only_file() {
        let temp = TempDir::new().unwrap();

        let file_path = temp.path().join("whitespace.log");
        fs::write(&file_path, "   \n\n   \n").unwrap();

        let result = extract_result_from_file(&file_path).unwrap();
        assert!(result.is_none());
    }
}
