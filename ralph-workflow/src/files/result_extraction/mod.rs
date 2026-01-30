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
//! The extraction uses prefix mode: treat `log_path` as a prefix and search for
//! files matching `{prefix}_*.log` in the parent directory.
//!
//! Example: For `.agent/logs/planning_1`, search for `.agent/logs/planning_1_*.log`.
//!
//! Note: Many functions in this module are currently unused in production
//! (XML extraction is used instead). Kept for potential future use and test compatibility.

mod file_extraction;
pub mod file_finder;
mod json_extraction;
mod plan_extraction;
mod scoring;
mod text_extraction;
mod types;
mod validation;

pub use file_extraction::extract_file_paths_from_issues;
pub use json_extraction::extract_last_result;
pub use types::ExtractionResult;
pub use validation::validate_issues_content;

use crate::workspace::Workspace;
use std::io;
use std::path::Path;

/// Extract and validate plan content from agent logs.
///
/// # Arguments
///
/// * `workspace` - Workspace for file operations
/// * `log_dir` - Path to the planning log directory
///
/// # Returns
///
/// An `ExtractionResult` containing:
/// - The raw content (if any result event was found)
/// - Validation status (whether it looks like a valid plan)
/// - Warning message (if validation failed)
#[cfg(any(test, feature = "test-utils"))]
pub fn extract_plan(workspace: &dyn Workspace, log_dir: &Path) -> io::Result<ExtractionResult> {
    let raw_content = extract_last_result(workspace, log_dir)?;

    raw_content.map_or_else(
        || Ok(ExtractionResult::empty()),
        |content| {
            let content_clean = content.trim().to_string();
            let (is_valid, warning) =
                crate::files::result_extraction::validation::validate_plan_content(&content_clean);
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
/// * `workspace` - Workspace for file operations
/// * `log_dir` - Path to the reviewer log directory
///
/// # Returns
///
/// An `ExtractionResult` containing:
/// - The raw content (if any result event was found)
/// - Validation status (whether it looks like valid issues)
/// - Warning message (if validation failed)
pub fn extract_issues(workspace: &dyn Workspace, log_dir: &Path) -> io::Result<ExtractionResult> {
    let raw_content = extract_last_result(workspace, log_dir)?;

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
    use crate::workspace::MemoryWorkspace;
    use std::path::PathBuf;

    // =========================================================================
    // JSON log builders - create properly escaped JSON log content
    // =========================================================================

    /// Create a JSON result event line. Content is properly escaped for JSON.
    fn result_event(content: &str) -> String {
        let escaped = content
            .replace('\\', "\\\\")
            .replace('"', "\\\"")
            .replace('\n', "\\n");
        format!(r#"{{"type": "result", "result": "{escaped}"}}"#)
    }

    /// Create a JSON system event line.
    fn system_event(message: &str) -> String {
        format!(r#"{{"type": "system", "message": "{message}"}}"#)
    }

    /// Create a JSON tool_use event line.
    fn tool_event(tool: &str) -> String {
        format!(r#"{{"type": "tool_use", "tool": "{tool}"}}"#)
    }

    /// Join multiple JSON lines into log file content.
    fn log_lines(lines: &[&str]) -> String {
        lines.join("\n")
    }

    // =========================================================================
    // Tests
    // =========================================================================

    #[test]
    fn test_extract_valid_plan() {
        let plan = "# Plan\n\n## Step 1\nImplement the feature\n\n## Step 2\nAdd tests";
        let log = log_lines(&[&system_event("starting"), &result_event(plan)]);

        // Use prefix-based naming: planning_1_agent_0.log in parent directory
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/planning_1_agent_0.log", &log);
        let log_dir = PathBuf::from("/test/repo/planning_1");

        let result = extract_plan(&workspace, &log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(result.is_valid);
        assert!(result.validation_warning.is_none());
    }

    #[test]
    fn test_extract_invalid_but_present_content() {
        let log = result_event("Hello world");
        // Use prefix-based naming: planning_1_agent_0.log in parent directory
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/planning_1_agent_0.log", &log);
        let log_dir = PathBuf::from("/test/repo/planning_1");

        let result = extract_plan(&workspace, &log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(!result.is_valid);
        assert!(result.validation_warning.is_some());
        assert_eq!(result.raw_content.unwrap(), "Hello world");
    }

    #[test]
    fn test_extract_no_result_events() {
        let log = log_lines(&[&system_event("starting"), &tool_event("read_file")]);
        // Use prefix-based naming: planning_1_agent_0.log in parent directory
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/planning_1_agent_0.log", &log);
        let log_dir = PathBuf::from("/test/repo/planning_1");

        let result = extract_plan(&workspace, &log_dir).unwrap();
        assert!(result.raw_content.is_none());
        assert!(!result.is_valid);
    }

    #[test]
    fn test_extract_malformed_json() {
        // Mix of invalid JSON and one valid result event
        let valid_result = result_event("# Valid Plan\n\nStep 1: Create feature");
        let log = format!("not json\n{{\"invalid json\n{valid_result}");
        // Use prefix-based naming: planning_1_agent_0.log in parent directory
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/planning_1_agent_0.log", &log);
        let log_dir = PathBuf::from("/test/repo/planning_1");

        let result = extract_plan(&workspace, &log_dir).unwrap();
        assert!(result.raw_content.is_some());
        // Should still find the valid JSON line
    }

    #[test]
    fn test_extract_empty_log_dir() {
        let workspace = MemoryWorkspace::new_test();
        let log_dir = PathBuf::from("/test/repo/nonexistent");

        let result = extract_plan(&workspace, &log_dir).unwrap();
        assert!(result.raw_content.is_none());
    }

    #[test]
    fn test_extract_valid_issues() {
        let issues = "# Issues\n\nCritical:\n- [ ] Fix security bug";
        let log = result_event(issues);
        // Use prefix-based naming: reviewer_1_agent_0.log in parent directory
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/reviewer_1_agent_0.log", &log);
        let log_dir = PathBuf::from("/test/repo/reviewer_1");

        let result = extract_issues(&workspace, &log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(result.is_valid);
    }

    #[test]
    fn test_extract_no_issues_found() {
        let log = result_event("No issues found. The code looks good.");
        // Use prefix-based naming: reviewer_1_agent_0.log in parent directory
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/reviewer_1_agent_0.log", &log);
        let log_dir = PathBuf::from("/test/repo/reviewer_1");

        let result = extract_issues(&workspace, &log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(result.is_valid); // "no issues" is valid
    }

    #[test]
    fn test_uses_best_result_event() {
        // Multiple result events - should use the best (most complete) one
        let log = log_lines(&[
            &result_event("First result"),
            &result_event("# Final Plan\n\nStep 1: Implement feature"),
        ]);
        // Use prefix-based naming: planning_1_agent_0.log in parent directory
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/planning_1_agent_0.log", &log);
        let log_dir = PathBuf::from("/test/repo/planning_1");

        let result = extract_plan(&workspace, &log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(result.raw_content.unwrap().contains("Final Plan"));
    }

    #[test]
    fn test_best_result_bug_regression() {
        // Simulate the exact bug scenario:
        // Multiple result events where first is complete/longest, last is partial/short
        let complete = "# Complete Plan\n\n## Implementation Steps\n\nStep 1: Create module with functionality.\nStep 2: Add comprehensive tests.\nStep 3: Write documentation.\nStep 4: Integrate and verify.";
        let partial = "# Partial Plan\n\nJust a short summary.";
        let short = "Last paragraph";

        let log = log_lines(&[
            &result_event(complete),
            &result_event(partial),
            &result_event(short),
        ]);
        // Use prefix-based naming: planning_1_agent_0.log in parent directory
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/planning_1_agent_0.log", &log);
        let log_dir = PathBuf::from("/test/repo/planning_1");

        let result = extract_plan(&workspace, &log_dir).unwrap();
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
    // PREFIX-BASED FILE MATCHING TESTS
    // =====================================================

    #[test]
    fn test_extract_from_prefix_pattern() {
        // Log file matching prefix pattern: planning_1_glm_0.log
        let plan = "# Plan\n\n## Summary\nTest plan with implementation steps";
        let log = result_event(plan);
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/planning_1_glm_0.log", &log);

        let prefix = PathBuf::from("/test/repo/planning_1");
        let result = extract_plan(&workspace, &prefix).unwrap();

        assert!(
            result.raw_content.is_some(),
            "Should find content from prefix-matched file"
        );
        assert!(result.raw_content.unwrap().contains("## Summary"));
    }

    #[test]
    fn test_extract_from_prefix_with_multiple_files() {
        // Multiple log files simulating fallback/retry scenario
        let log1 = result_event("First attempt - no plan");
        let log2 = result_event("# Plan\n\n## Summary\nSuccess with implementation steps!");

        let workspace = MemoryWorkspace::new_test()
            .with_file("/test/repo/planning_1_agent1_0.log", &log1)
            .with_file("/test/repo/planning_1_agent2_0.log", &log2);

        let prefix = PathBuf::from("/test/repo/planning_1");
        let result = extract_plan(&workspace, &prefix).unwrap();

        assert!(result.raw_content.is_some());
        let content = result.raw_content.unwrap();
        assert!(
            content.contains("Success") || content.contains("First attempt"),
            "Should extract content from one of the files"
        );
    }

    #[test]
    fn test_extract_prefix_no_matching_files() {
        let workspace = MemoryWorkspace::new_test()
            .with_file("/test/repo/other_file.log", "some content")
            .with_file("/test/repo/planning_2_glm_0.log", "wrong prefix");

        let prefix = PathBuf::from("/test/repo/planning_1");
        let result = extract_plan(&workspace, &prefix).unwrap();

        assert!(
            result.raw_content.is_none(),
            "Should not find content when no files match prefix"
        );
    }

    #[test]
    fn test_extract_directory_mode_no_longer_supported() {
        // Directory mode is no longer supported - only prefix mode works
        let plan = "# Plan\n\n## Summary\nTest from directory";
        let log = result_event(plan);
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/planning_1/output.log", &log);
        let log_dir = PathBuf::from("/test/repo/planning_1");

        let result = extract_plan(&workspace, &log_dir).unwrap();
        // Directory mode is removed - should not find content
        assert!(
            result.raw_content.is_none(),
            "Directory mode is no longer supported"
        );
    }

    #[test]
    fn test_extract_prefix_exact_file_fallback() {
        let plan = "# Plan\n\n## Summary\nDirect file";
        let log = result_event(plan);
        let workspace = MemoryWorkspace::new_test().with_file("/test/repo/planning_1.log", &log);
        let exact_file = PathBuf::from("/test/repo/planning_1.log");

        let result = extract_last_result(&workspace, &exact_file).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("Direct file"));
    }

    #[test]
    fn test_find_log_files_with_prefix() {
        let workspace = MemoryWorkspace::new_test()
            .with_file("/test/repo/planning_1_glm_0.log", "a")
            .with_file("/test/repo/planning_1_opus_1.log", "b")
            .with_file("/test/repo/planning_2_glm_0.log", "c")
            .with_file("/test/repo/other.txt", "d");

        let parent = PathBuf::from("/test/repo");
        let files = find_log_files_with_prefix(&workspace, &parent, "planning_1").unwrap();

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
    // ISSUES EXTRACTION WITH PREFIX TESTS
    // =====================================================

    #[test]
    fn test_extract_issues_from_prefix_pattern() {
        let issues = "# Issues\n\nCritical:\n- [ ] Fix the security vulnerability";
        let log = result_event(issues);
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/reviewer_review_1_glm_0.log", &log);

        let prefix = PathBuf::from("/test/repo/reviewer_review_1");
        let result = extract_issues(&workspace, &prefix).unwrap();

        assert!(result.raw_content.is_some());
        assert!(result.is_valid);
    }

    #[test]
    fn test_extract_issues_no_issues_from_prefix() {
        let log = result_event("No issues found. The code looks good.");
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/reviewer_1_opus_0.log", &log);

        let prefix = PathBuf::from("/test/repo/reviewer_1");
        let result = extract_issues(&workspace, &prefix).unwrap();

        assert!(result.raw_content.is_some());
        assert!(result.is_valid);
    }

    // =====================================================
    // SUBDIRECTORY FALLBACK TESTS (LEGACY MODE REMOVED)
    // =====================================================

    #[test]
    fn test_subdirectory_fallback_no_longer_supported() {
        // Subdirectory fallback is no longer supported - only flat prefix mode works
        let plan = "# Plan\n\n## Summary\nPlan from nested subdirectory";
        let log = result_event(plan);
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/planning_1_ccs/glm_0.log", &log);

        let prefix = PathBuf::from("/test/repo/planning_1");
        let result = extract_plan(&workspace, &prefix).unwrap();

        // Subdirectory fallback is removed - should not find content
        assert!(
            result.raw_content.is_none(),
            "Subdirectory fallback is no longer supported"
        );
    }

    // =====================================================
    // EDGE CASES AND ERROR HANDLING
    // =====================================================

    #[test]
    fn test_extract_empty_prefix() {
        let workspace = MemoryWorkspace::new_test();

        // Test with path that has no file name component
        let result = extract_last_result(&workspace, Path::new("/test/repo")).unwrap();
        assert!(result.is_none(), "Empty directory should return None");
    }

    #[test]
    fn test_extract_nonexistent_parent_directory() {
        let workspace = MemoryWorkspace::new_test();
        let result =
            extract_last_result(&workspace, Path::new("/nonexistent/path/planning_1")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_result_from_single_file() {
        let plan = "# Plan\n\n## Summary\nThe plan content";
        let log = log_lines(&[&system_event("start"), &result_event(plan)]);
        let workspace = MemoryWorkspace::new_test().with_file("/test/repo/test.log", &log);
        let file_path = PathBuf::from("/test/repo/test.log");

        let result = extract_result_from_file(&workspace, &file_path).unwrap();
        assert!(result.is_some());
        assert!(result.unwrap().contains("## Summary"));
    }

    #[test]
    fn test_extract_result_from_file_not_found() {
        let workspace = MemoryWorkspace::new_test();
        let result =
            extract_result_from_file(&workspace, Path::new("/nonexistent/file.log")).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_handles_empty_log_file() {
        let workspace = MemoryWorkspace::new_test().with_file("/test/repo/empty.log", "");
        let file_path = PathBuf::from("/test/repo/empty.log");

        let result = extract_result_from_file(&workspace, &file_path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_extract_handles_whitespace_only_file() {
        let workspace =
            MemoryWorkspace::new_test().with_file("/test/repo/whitespace.log", "   \n\n   \n");
        let file_path = PathBuf::from("/test/repo/whitespace.log");

        let result = extract_result_from_file(&workspace, &file_path).unwrap();
        assert!(result.is_none());
    }
}
