//! Plan-specific extraction logic.
//!
//! This module provides text-based fallback extraction for plan content,
//! used when JSON result events are not available.

use std::fs::File;
use std::io::{self, Read};
use std::path::Path;

use super::file_finder::{find_log_files_with_prefix, find_subdirs_with_prefix};
use super::scoring::score_text_plan;
use super::text_extraction::extract_plan_from_text;

/// Extract plan content from log files using text-based fallback.
///
/// This scans all log files matching the prefix and looks for markdown plan structure.
/// Also checks subdirectories matching the prefix pattern (for legacy logs where agent
/// names with "/" created nested directories).
pub fn extract_plan_from_logs_text(log_path: &Path) -> io::Result<Option<String>> {
    // Helper to extract from a list of files by finding the best plan across all files
    fn extract_from_files(files: &[std::path::PathBuf]) -> Option<String> {
        let mut best_plan: Option<String> = None;
        let mut best_score: u32 = 0;
        let mut candidates_count = 0;

        for log_file in files {
            let mut content = String::new();
            if let Ok(mut file) = File::open(log_file) {
                if file.read_to_string(&mut content).is_ok() {
                    if let Some(plan) = extract_plan_from_text(&content) {
                        let score = score_text_plan(&plan);
                        candidates_count += 1;
                        if score > best_score {
                            best_score = score;
                            best_plan = Some(plan);
                        }
                    }
                }
            }
        }

        // Log diagnostic info when multiple candidates were found
        if candidates_count > 1 {
            eprintln!(
                "[result_extraction] Found {} plan candidates across {} files, selected plan with score {} (length: {})",
                candidates_count,
                files.len(),
                best_score,
                best_plan.as_ref().map_or(0, std::string::String::len)
            );
        }

        best_plan
    }

    // Strategy 1: If log_path is a directory, scan all files in it
    if log_path.is_dir() {
        let log_files: Vec<_> = std::fs::read_dir(log_path)?
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
    let parent = log_path.parent().unwrap_or_else(|| Path::new("."));
    let prefix = log_path.file_name().and_then(|s| s.to_str()).unwrap_or("");

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
        let subdir_files: Vec<_> = std::fs::read_dir(&subdir)?
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

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    // =====================================================
    // TEXT-BASED FALLBACK EXTRACTION TESTS
    // =====================================================

    #[test]
    fn test_extract_plan_from_logs_text_fallback() {
        let temp = TempDir::new().unwrap();

        // Create log file with no JSON result event, but text plan content
        let text_log = "[agent] Starting...
[agent] Thinking about the plan...
## Summary
This is a text-based plan without proper JSON wrapping but with enough content.

## Implementation Steps
1. First step with detailed explanation
2. Second step with implementation details
";
        fs::write(temp.path().join("planning_1_glm_0.log"), text_log).unwrap();

        let prefix = temp.path().join("planning_1");
        let result = extract_plan_from_logs_text(&prefix).unwrap();

        assert!(result.is_some());
        assert!(result.unwrap().contains("## Summary"));
    }

    #[test]
    fn test_text_fallback_uses_best_plan() {
        let temp = TempDir::new().unwrap();

        // Simulate the exact bug scenario:
        // Multiple log files where earlier files have complete plans,
        // later files have only partial/truncated plans
        let log1_content = "Agent output...
## Summary

Fix an indeterministic bug where PLAN.md sometimes contains only the last few paragraphs instead of the complete plan.

## Implementation Steps

Step 1: Add scoring utility for text-based plan extraction in result_extraction.rs
Step 2: Update extract_plan_from_text to use scoring and return the best candidate
Step 3: Update extract_from_files helper to score all candidates across files
Step 4: Add comprehensive regression test to verify the fix
Step 5: Add diagnostic logging for debugging

## Critical Files

1. src/files/result_extraction.rs - Core extraction logic
2. src/phases/development.rs - Calls the extraction functions
";

        let log2_content = "More agent output...
## Summary

This is a truncated plan that only has a summary section.
";

        let log3_content = "Even more output...
## Summary

Just a short paragraph at the end.
";

        fs::write(temp.path().join("planning_1_glm_0.log"), log1_content).unwrap();

        // Sleep briefly to ensure different modification times
        std::thread::sleep(std::time::Duration::from_millis(10));

        fs::write(temp.path().join("planning_1_opus_0.log"), log2_content).unwrap();

        std::thread::sleep(std::time::Duration::from_millis(10));

        fs::write(temp.path().join("planning_1_sonnet_0.log"), log3_content).unwrap();

        let prefix = temp.path().join("planning_1");
        let result = extract_plan_from_logs_text(&prefix).unwrap();

        assert!(result.is_some(), "Should extract a plan");
        let content = result.unwrap();

        // The fix should select the highest-scoring (most complete) plan
        // NOT the first or last one found
        assert!(
            content.contains("Implementation Steps"),
            "Should select the complete plan with Implementation Steps, got: {content}"
        );
        assert!(
            content.contains("scoring utility"),
            "Should contain the detailed steps from the complete plan"
        );
        assert!(
            content.contains("Critical Files"),
            "Should contain all sections from the complete plan"
        );
        assert!(
            content.len() > 400,
            "Complete plan should be selected (length > 400), got length: {}",
            content.len()
        );
    }

    #[test]
    fn test_extract_plan_from_logs_text_permissive_fallback() {
        let temp = TempDir::new().unwrap();

        // Create log file with plaintext plan (no JSON result events, no markdown markers)
        let text_log = "The agent needs to implement a user authentication feature.
Step 1: Create a new auth module with login and registration functions.
Step 2: Add password hashing using bcrypt for security.
Step 3: Implement JWT token generation for session management.
Step 4: Add middleware to protect routes that require authentication.
Step 5: Write comprehensive tests for all auth functionality.";

        fs::write(temp.path().join("planning_1_glm_0.log"), text_log).unwrap();

        let prefix = temp.path().join("planning_1");
        let result = extract_plan_from_logs_text(&prefix).unwrap();

        assert!(
            result.is_some(),
            "Should extract plaintext plan via permissive fallback"
        );
        assert!(result.unwrap().contains("authentication"));
    }

    #[test]
    fn test_extract_from_subdirectory_fallback() {
        let temp = TempDir::new().unwrap();

        // Create nested structure like: planning_1_ccs/glm_0.log
        // This simulates what happens when agent name is "ccs/glm"
        let subdir = temp.path().join("planning_1_ccs");
        fs::create_dir(&subdir).unwrap();

        // Create log file with text plan content (no JSON result event)
        let text_log = "[agent] Starting...
## Summary
This is a text-based plan from nested subdirectory with enough content to pass validation.

## Implementation Steps
1. First step with detailed explanation
";
        fs::write(subdir.join("glm_0.log"), text_log).unwrap();

        // Extract using prefix (not the nested path)
        let prefix = temp.path().join("planning_1");
        let result = extract_plan_from_logs_text(&prefix).unwrap();

        assert!(
            result.is_some(),
            "Should find text content from subdirectory fallback"
        );
        assert!(result.unwrap().contains("nested subdirectory"));
    }

    #[test]
    fn test_extract_text_from_subdirectory_fallback() {
        let temp = TempDir::new().unwrap();

        // Create nested structure like: planning_1_ccs/glm_0.log
        let subdir = temp.path().join("planning_1_ccs");
        fs::create_dir(&subdir).unwrap();

        // Create log file with text plan content (no JSON result event)
        let text_log = "[agent] Starting...
## Summary
This is a text-based plan from nested subdirectory with enough content to pass validation.

## Implementation Steps
1. First step with detailed explanation
";
        fs::write(subdir.join("glm_0.log"), text_log).unwrap();

        let prefix = temp.path().join("planning_1");
        let result = extract_plan_from_logs_text(&prefix).unwrap();

        assert!(
            result.is_some(),
            "Should find text content from subdirectory fallback"
        );
        assert!(result.unwrap().contains("nested subdirectory"));
    }
}
