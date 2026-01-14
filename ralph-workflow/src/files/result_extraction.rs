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

/// Safely convert usize to u32, capping at `u32::MAX` to avoid truncation.
fn saturate_u32(value: usize) -> u32 {
    value.try_into().unwrap_or(u32::MAX)
}

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
    const fn valid(content: String) -> Self {
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
    const fn empty() -> Self {
        Self {
            raw_content: None,
            is_valid: false,
            validation_warning: None,
        }
    }
}

/// Calculate a score for a result to determine its quality.
///
/// Higher scores indicate better results. Scoring considers:
/// - Presence of plan structure markers (## Summary, ## Implementation Steps, etc.)
/// - Markdown headers (#)
/// - Content length (longer is generally better)
/// - Plan-like keywords
fn score_result(content: &str) -> u32 {
    let mut score: u32 = 0;
    let content_lower = content.to_lowercase();

    // Strong structure markers (very high weight)
    let structure_markers = [
        "## Summary",
        "## Implementation Steps",
        "## Implementation",
        "### Implementation",
        "# Summary",
        "# Implementation Plan",
        "# Plan",
    ];
    for marker in &structure_markers {
        if content.contains(marker) {
            score += 1000;
        }
    }

    // Secondary headers (medium weight)
    let secondary_headers = ["###", "####", "## Risks", "## Verification", "## Testing"];
    for header in &secondary_headers {
        if content.contains(header) {
            score += 100;
        }
    }

    // Any markdown headers (low weight)
    for line in content.lines() {
        if line.trim().starts_with('#') {
            score += 10;
        }
    }

    // Plan keywords (very low weight as tiebreaker)
    let keywords = [
        "step",
        "implement",
        "create",
        "add",
        "build",
        "task",
        "phase",
        "first",
        "second",
        "then",
        "finally",
        "next",
    ];
    for keyword in &keywords {
        if content_lower.contains(keyword) {
            score += 1;
        }
    }

    // Length bonus (slight preference for longer content with same structure)
    // Cap the bonus to avoid length overriding structure
    let length_bonus = saturate_u32(content.len()).min(500);
    score += length_bonus;

    score
}

/// Extract the best "result" event from a single log file.
///
/// Scans the file for JSON lines and returns the best `{"type": "result", "result": "..."}`
/// event's content. The "best" result is determined by a scoring function that considers:
/// 1. Plan structure markers (## Summary, ## Implementation Steps, etc.)
/// 2. Markdown headers
/// 3. Content length (as a tiebreaker)
///
/// This handles cases where agents emit multiple partial result events during streaming
/// or retries, preferring results with proper plan structure over simple length.
fn extract_result_from_file(path: &Path) -> io::Result<Option<String>> {
    let file = match File::open(path) {
        Ok(f) => f,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };

    let reader = BufReader::new(file);
    let mut best_result: Option<String> = None;
    let mut best_score: u32 = 0;

    for line in reader.lines() {
        let Ok(line) = line else { continue };

        // Skip non-JSON lines
        if !line.trim().starts_with('{') {
            continue;
        }

        // Parse JSON and look for "result" events
        if let Ok(value) = serde_json::from_str::<JsonValue>(&line) {
            if let Some(typ) = value.get("type").and_then(|v| v.as_str()) {
                if typ == "result" {
                    if let Some(result) = value.get("result").and_then(|v| v.as_str()) {
                        let result_string = result.to_string();
                        let result_score = score_result(&result_string);

                        // Select the result with the highest score
                        // This prefers structured plans over simple longest strings
                        if result_score > best_score {
                            best_score = result_score;
                            best_result = Some(result_string);
                        }
                    }
                }
            }
        }
    }

    Ok(best_result)
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
    let prefix_pattern = format!("{prefix}_");

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        let Some(file_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
        };

        // Match files like "planning_1_glm_0.log" when prefix is "planning_1"
        if file_name.starts_with(&prefix_pattern)
            && file_name.to_ascii_lowercase().ends_with(".log")
        {
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
/// nested directories (e.g., "`planning_1_ccs/glm_0.log`" instead of flat files).
fn find_subdirs_with_prefix(parent_dir: &Path, prefix: &str) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(parent_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(e) => return Err(e),
    };

    let mut subdirs = Vec::new();
    let prefix_pattern = format!("{prefix}_");

    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }

        let Some(dir_name) = path.file_name().and_then(|s| s.to_str()) else {
            continue;
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

/// Extract the best "result" event from agent JSON logs.
///
/// Supports three modes:
/// 1. **Directory mode**: If `log_path` is a directory, scan all files in it
/// 2. **Prefix mode**: If `log_path` is not a directory, treat it as a prefix and
///    search for files matching `{prefix}_*.log` in the parent directory
/// 3. **Subdirectory fallback**: If no files found, check for subdirectories matching
///    `{prefix}_*` (handles legacy logs where agent names with "/" created nested dirs)
///
/// The "best" result is determined by selecting the longest content, which handles
/// cases where agents emit multiple partial result events during streaming or retries.
///
/// # Arguments
///
/// * `log_path` - Path to the log directory OR log file prefix
///
/// # Returns
///
/// The raw content from the best result event, or None if no result found.
pub fn extract_last_result(log_path: &Path) -> io::Result<Option<String>> {
    // Strategy 1: If log_path is a directory, scan all files in it (legacy mode)
    if log_path.is_dir() {
        return extract_from_directory(log_path);
    }

    // Strategy 2: Treat log_path as a prefix and search parent directory
    let parent = log_path.parent().unwrap_or_else(|| Path::new("."));
    let prefix = log_path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    if prefix.is_empty() {
        return Ok(None);
    }

    let log_files = find_log_files_with_prefix(parent, prefix)?;

    if !log_files.is_empty() {
        let mut best_result: Option<String> = None;
        let mut best_score: u32 = 0;
        for log_file in log_files {
            if let Some(result) = extract_result_from_file(&log_file)? {
                let result_score = score_result(&result);
                // Select the result with the highest score across all files
                if result_score > best_score {
                    best_score = result_score;
                    best_result = Some(result);
                }
            }
        }
        if best_result.is_some() {
            return Ok(best_result);
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
///
/// Selects the best result across all files using the scoring function to handle
/// retry scenarios where multiple log files may exist. Prefers structured plans
/// over simple longest strings.
fn extract_from_directory(log_dir: &Path) -> io::Result<Option<String>> {
    let log_entries = match fs::read_dir(log_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };

    let mut best_result: Option<String> = None;
    let mut best_score: u32 = 0;

    for entry in log_entries.flatten() {
        let path = entry.path();
        if !path.is_file() {
            continue;
        }

        if let Some(result) = extract_result_from_file(&path)? {
            let result_score = score_result(&result);
            // Select the result with the highest score across all files
            if result_score > best_score {
                best_score = result_score;
                best_result = Some(result);
            }
        }
    }

    Ok(best_result)
}

/// Calculate a score for text content to determine plan completeness.
///
/// This is similar to `score_result()` but works on raw text content rather than
/// JSON result events. Higher scores indicate more complete plans.
fn score_text_plan(content: &str) -> u32 {
    let mut score: u32 = 0;
    let content_lower = content.to_lowercase();

    // Strong structure markers (very high weight)
    let structure_markers = [
        "## Summary",
        "## Implementation Steps",
        "## Implementation",
        "### Implementation",
        "# Summary",
        "# Implementation Plan",
        "# Plan",
    ];
    for marker in &structure_markers {
        if content.contains(marker) {
            score += 1000;
        }
    }

    // Secondary headers (medium weight)
    let secondary_headers = ["###", "####", "## Risks", "## Verification", "## Testing"];
    for header in &secondary_headers {
        if content.contains(header) {
            score += 100;
        }
    }

    // Any markdown headers (low weight)
    for line in content.lines() {
        if line.trim().starts_with('#') {
            score += 10;
        }
    }

    // Plan keywords (very low weight as tiebreaker)
    let keywords = [
        "step",
        "implement",
        "create",
        "add",
        "build",
        "task",
        "phase",
        "first",
        "second",
        "then",
        "finally",
        "next",
    ];
    for keyword in &keywords {
        if content_lower.contains(keyword) {
            score += 1;
        }
    }

    // Length bonus (slight preference for longer content with same structure)
    // Cap the bonus to avoid length overriding structure
    let length_bonus = saturate_u32(content.len()).min(500);
    score += length_bonus;

    score
}

/// Extract plan content from text by looking for markdown structure.
///
/// This is a fallback method for cases where JSON result events are not available.
/// It looks for common plan markers like `## Summary` and `## Implementation Steps`.
/// If multiple plan candidates are found, it returns the highest-scoring one.
/// If no markers are found, it falls back to extracting substantial text content
/// that contains plan-like keywords.
pub fn extract_plan_from_text(content: &str) -> Option<String> {
    // Look for plan start markers - these indicate where a plan begins
    let start_markers = [
        "## Summary",
        "# Plan",
        "# Implementation Plan",
        "## Implementation Steps",
    ];

    // Find all potential plan candidates
    // Each candidate starts at a marker and continues to the end of content
    let mut candidates: Vec<(usize, &str)> = Vec::new();

    for marker in start_markers {
        if let Some(start) = content.find(marker) {
            // Extract from the marker to the end of the content
            let plan_content = &content[start..];
            let trimmed = plan_content.trim();

            if trimmed.len() > 50 {
                candidates.push((start, trimmed));
            }
        }
    }

    if !candidates.is_empty() {
        // Score each candidate and return the best one
        let mut best_candidate: Option<&str> = None;
        let mut best_score: u32 = 0;

        for (_start, candidate) in &candidates {
            let score = score_text_plan(candidate);
            if score > best_score {
                best_score = score;
                best_candidate = Some(candidate);
            }
        }

        if candidates.len() > 1 {
            eprintln!(
                "[result_extraction] Found {} plan candidates in text, selected one with score {}",
                candidates.len(),
                best_score
            );
        }

        return best_candidate.map(std::string::ToString::to_string);
    }

    // Permissive fallback: if no markdown markers found, look for substantial
    // content that contains plan-like keywords. This handles plaintext mode where
    // the agent outputs plan content without structured markdown.
    extract_plan_from_text_permissive(content)
}

/// Permissive extraction that finds substantial plan-like content without
/// requiring specific markdown markers.
///
/// This is a final fallback for plaintext mode logs where the agent may have
/// output a valid plan but without the expected markdown structure.
#[expect(clippy::items_after_statements)]
fn extract_plan_from_text_permissive(content: &str) -> Option<String> {
    let content = content.trim();

    // Filter out obvious non-plan content
    // - JSON lines
    // - Debug/tool output patterns
    let filtered: String = content
        .lines()
        .filter(|line| {
            let trimmed = line.trim();
            // Skip JSON lines
            if trimmed.starts_with('{') || trimmed.starts_with('[') {
                return false;
            }
            // Skip debug/tool markers
            if trimmed.starts_with("[debug]")
                || trimmed.starts_with("[tool]")
                || trimmed.starts_with("[error]")
                || trimmed.starts_with("[warn]")
            {
                return false;
            }
            // Skip empty lines
            if trimmed.is_empty() {
                return false;
            }
            true
        })
        .collect::<Vec<_>>()
        .join("\n");

    // Minimum content length (increased from 50 to 200 for permissive mode)
    const MIN_PERMISSIVE_LENGTH: usize = 200;

    if filtered.len() < MIN_PERMISSIVE_LENGTH {
        return None;
    }

    // Check for plan-like keywords (case-insensitive)
    let plan_keywords = [
        "step",
        "implement",
        "create",
        "add",
        "build",
        "develop",
        "write",
        "function",
        "feature",
        "component",
        "module",
        "task",
        "phase",
        "first",
        "second",
        "third",
        "next",
        "then",
        "finally",
        "approach",
        "strategy",
        "design",
        "architecture",
    ];

    let filtered_lower = filtered.to_lowercase();
    let has_plan_keyword = plan_keywords
        .iter()
        .any(|keyword| filtered_lower.contains(keyword));

    if has_plan_keyword {
        return Some(filtered);
    }

    None
}

/// Extract plan content from log files using text-based fallback.
///
/// This scans all log files matching the prefix and looks for markdown plan structure.
/// Also checks subdirectories matching the prefix pattern (for legacy logs where agent
/// names with "/" created nested directories).
pub fn extract_plan_from_logs_text(log_path: &Path) -> io::Result<Option<String>> {
    // Helper to extract from a list of files by finding the best plan across all files
    fn extract_from_files(files: &[PathBuf]) -> Option<String> {
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

    let has_header = content_clean
        .lines()
        .any(|line| line.trim().starts_with('#'));
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
/// - Contains checkboxes (- \[ ] or - \[x])
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
#[expect(clippy::option_if_let_else)]
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
#[expect(clippy::option_if_let_else)]
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
    fn test_uses_last_result_event() {
        let temp = TempDir::new().unwrap();
        let log_dir = temp.path().join("planning_1");

        // Multiple result events - should use the best (longest/most complete) one
        let json_log = r##"{"type": "result", "result": "First result"}
{"type": "result", "result": "# Final Plan\n\nStep 1: Implement feature"}"##;
        create_log_file(&log_dir, "output.log", json_log);

        let result = extract_plan(&log_dir).unwrap();
        assert!(result.raw_content.is_some());
        assert!(result.raw_content.unwrap().contains("Final Plan"));
    }

    #[test]
    fn test_incomplete_plan_bug_regression() {
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

        // The fix should select the longest/most complete result
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
        // Substantial content without plan-like keywords (avoiding: step, implement, create, add, build, develop, write, then, etc.)
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
}
