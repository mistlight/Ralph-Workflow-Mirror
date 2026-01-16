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

mod file_extraction;
mod file_finder;
mod json_extraction;
mod plan_extraction;
mod scoring;
mod text_extraction;
mod types;
mod validation;

pub use file_extraction::extract_file_paths_from_issues;
pub use json_extraction::extract_last_result;
pub use plan_extraction::extract_plan_from_logs_text;
pub use types::ExtractionResult;
pub use validation::{validate_issues_content, validate_plan_content};

use std::io;
use std::path::Path;

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

    raw_content.map_or_else(
        || Ok(ExtractionResult::empty()),
        |content| {
            let content_clean = content.trim().to_string();
            let (is_valid, warning) = validate_plan_content(&content_clean);
            Ok(if is_valid {
                ExtractionResult::valid(content_clean)
            } else {
                ExtractionResult::invalid(content_clean, &warning.unwrap_or_default())
            })
        },
    )
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

    raw_content.map_or_else(
        || Ok(ExtractionResult::empty()),
        |content| {
            let content_clean = content.trim().to_string();
            let (is_valid, warning) = validate_issues_content(&content_clean);
            Ok(if is_valid {
                ExtractionResult::valid(content_clean)
            } else {
                ExtractionResult::invalid(content_clean, &warning.unwrap_or_default())
            })
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::files::result_extraction::file_finder::find_log_files_with_prefix;
    use crate::files::result_extraction::json_extraction::extract_result_from_file;
    use crate::files::result_extraction::text_extraction::extract_plan_from_text;
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

        let json_log = concat!(
            r##"{"type": "system", "message": "starting"}"##,
            "\n",
            r##"{"type": "result", "result": "# Plan\n\n## Step 1\nImplement the feature\n\n## Step 2\nAdd tests"}"##
        );
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

        let json_log =
            r##"{"type": "result", "result": "# Issues\n\nCritical:\n- [ ] Fix security bug"}"##;
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
    fn test_uses_best_result_event() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("planning_1");

        // Multiple result events - should use the best (most complete) one
        let json_log = r##"{"type": "result", "result": "First result"}
{"type": "result", "result": "# Final Plan\n\nStep 1: Implement feature"}"##;
        create_log_file(&log_dir, "output.log", json_log);

        let result = extract_plan(&log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(result.raw_content.unwrap().contains("Final Plan"));
    }

    #[test]
    fn test_best_result_bug_regression() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("planning_1");

        // Simulate the exact bug scenario:
        // Multiple result events where first is complete/longest, last is partial/short
        // Use format! with separate strings to avoid escaping issues
        let result1 = r##"{"type": "result", "result": "# Complete Plan\n\n## Implementation Steps\n\nStep 1: Create module with functionality.\nStep 2: Add comprehensive tests.\nStep 3: Write documentation.\nStep 4: Integrate and verify."}"##;
        let result2 =
            r##"{"type": "result", "result": "# Partial Plan\n\nJust a short summary."}"##;
        let result3 = r#"{"type": "result", "result": "Last paragraph"}"#;
        let json_log = format!("{result1}\n{result2}\n{result3}");
        create_log_file(&log_dir, "output.log", &json_log);

        let result = extract_plan(&log_dir).unwrap();
        assert!(result.raw_content.is_some());
        let content = result.raw_content.unwrap();

        // The fix should select the best (most complete) result
        // NOT the last one (which would be "Last paragraph")
        assert!(
            content.contains("Implementation Steps"),
            "Should select the complete plan, not the last partial result: {content}"
        );
        assert!(
            content.contains("Create module"),
            "Should contain the full plan content"
        );
        assert!(
            content.len() > 100,
            "Complete plan should be selected (length > 100), got length: {}",
            content.len()
        );
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
        let json_log1 = r#"{"type": "result", "result": "First attempt - no plan"}"#;
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

        let json_log =
            r##"{"type": "result", "result": "# Plan\n\n## Summary\nTest from directory"}"##;
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
        let content = r"Some random output
## Summary
This is the plan summary with enough content to pass validation.

## Implementation Steps
1. Do the thing
2. Do the other thing
";
        let result = extract_plan_from_text(content);
        assert!(result.is_some());
        assert!(result.unwrap().starts_with("## Summary"));
    }

    #[test]
    fn test_extract_plan_from_text_with_implementation_steps() {
        let content = r"Agent thinking...
## Implementation Steps
Step 1: Create the component with all necessary features
Step 2: Add tests and documentation
";
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

        let json_log = r#"{"type": "result", "result": "No issues found. The code looks good."}"#;
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

    // =====================================================
    // PERMISSIVE EXTRACTION TESTS (Plaintext mode fallback)
    // =====================================================

    #[test]
    fn test_extract_plan_from_text_permissive_no_markers() {
        // Plaintext content without markdown markers but with plan keywords
        let content = "I need to implement a new feature for the user authentication system.
First, I will create a new module that handles the login logic.
Then, I will add functions for password validation and session management.
Finally, I will write tests to ensure everything works correctly.

The approach involves using secure hashing for passwords and JWT tokens for sessions.";

        let result = extract_plan_from_text(content);
        assert!(
            result.is_some(),
            "Should extract substantial content with plan keywords even without markdown markers"
        );
        let extracted = result.unwrap();
        assert!(extracted.contains("implement"));
        assert!(extracted.contains("create"));
        assert!(extracted.len() > 200);
    }

    #[test]
    fn test_extract_plan_from_text_permissive_filters_json() {
        // Content with JSON lines mixed in - should filter them out
        let content = r#"{"type": "tool", "tool": "read_file"}
I need to build a new authentication module that handles user login.
{"type": "system", "message": "processing"}
This will handle registration for the web application.
The development approach should be secure and follow best practices.
We'll add password hashing, session management, and proper error handling.
The module will integrate with the existing database layer."#;

        let result = extract_plan_from_text(content);
        assert!(
            result.is_some(),
            "Should extract content while filtering out JSON lines"
        );
        let extracted = result.unwrap();
        assert!(!extracted.contains("{\"type\":"));
        assert!(extracted.contains("authentication"));
    }

    #[test]
    fn test_extract_plan_from_text_permissive_too_short() {
        // Content with plan keywords but too short
        let content = "I will build a feature.";
        let result = extract_plan_from_text(content);
        assert!(
            result.is_none(),
            "Should reject content that's too short even with plan keywords"
        );
    }

    #[test]
    fn test_extract_plan_from_text_permissive_no_plan_keywords() {
        // Substantial content without plan-like keywords
        let content = "The quick brown fox jumps over the lazy dog repeatedly.
This text was composed to be long enough to pass the length requirement.
It avoids using technical terminology that might trigger extraction.
Instead we just talk about random things like animals and weather.
Our purpose is to test that the extraction correctly filters out non-plan content.
This should definitely be long enough but still rejected due to lack of keywords.
We're discussing foxes, dogs, weather, and other non-technical subjects today.
The weather is nice so all of the animals are playing in a large field outside.
A sunny day with blue skies makes for perfect conditions to observe nature.";

        let result = extract_plan_from_text(content);
        assert!(
            result.is_none(),
            "Should reject content without plan-like keywords"
        );
    }

    #[test]
    fn test_extract_plan_from_text_permissive_filters_debug_output() {
        // Content with debug/tool markers
        let content = "[debug] Starting the process
I need to develop the new module by writing code for authentication.
[tool] Reading file: src/main.rs
Then I must add functions for handling user sessions and password hashing.
[warn] Deprecated API usage detected in legacy code
Finally, I must verify everything works correctly through comprehensive testing.";

        let result = extract_plan_from_text(content);
        assert!(
            result.is_some(),
            "Should extract content while filtering out debug/tool markers"
        );
        let extracted = result.unwrap();
        assert!(!extracted.contains("[debug]"));
        assert!(!extracted.contains("[tool]"));
        assert!(!extracted.contains("[warn]"));
        assert!(extracted.contains("develop"));
        assert!(extracted.contains("authentication"));
    }

    #[test]
    fn test_extract_plan_from_text_markers_take_precedence() {
        // When markdown markers exist, they should take precedence over permissive extraction
        let content = "Some initial text without structure.
## Summary
This is the structured plan that should be extracted.
The permissive fallback should not be used when markers are present.
## Implementation Steps
1. Step one
2. Step two";

        let result = extract_plan_from_text(content);
        assert!(result.is_some());
        let extracted = result.unwrap();
        assert!(
            extracted.starts_with("## Summary"),
            "Should use marker-based extraction when markers are present"
        );
    }
}
