//! Commit Message Validation and Recovery Functions
//!
//! This module provides utilities for validating commit messages, salvaging
//! valid messages from mixed output, and generating fallback messages from
//! git diff metadata.

use regex::Regex;
use serde::Deserialize;

use super::cleaning::{find_conventional_commit_start, looks_like_analysis_text};

/// Result of commit message extraction with detail about the extraction method.
///
/// This enum allows callers to distinguish between different extraction outcomes
/// and take appropriate action (e.g., re-prompt when receiving a Fallback result).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitExtractionResult {
    /// Successfully extracted from structured agent output (JSON schema or pattern-based)
    Extracted(String),
    /// Recovered via salvage mechanism (found conventional commit within mixed output)
    Salvaged(String),
    /// Using deterministic fallback generated from diff metadata
    Fallback(String),
}

impl CommitExtractionResult {
    /// Convert into the inner message string.
    pub fn into_message(self) -> String {
        match self {
            Self::Extracted(msg) | Self::Salvaged(msg) | Self::Fallback(msg) => msg,
        }
    }

    /// Check if this was a fallback result (should trigger re-prompt).
    pub const fn is_fallback(&self) -> bool {
        matches!(self, Self::Fallback(_))
    }
}

/// Structured commit message schema for JSON parsing.
#[derive(Debug, Deserialize)]
struct StructuredCommitMessage {
    subject: String,
    body: Option<String>,
}

/// Try to extract commit message from JSON schema output.
///
/// This function attempts to parse the LLM output as a structured JSON object
/// following the schema `{"subject": "...", "body": "..."}`.
///
/// Supports multiple input formats:
/// - Direct JSON: `{"subject": "feat: ...", "body": "..."}`
/// - JSON in markdown code fence: ` ```json\n{...}\n``` `
/// - NDJSON streams where commit is in the `result` field
///
/// # Returns
///
/// * `Some(message)` if valid JSON with a valid conventional commit subject was found
/// * `None` if parsing fails or subject is invalid
pub fn try_extract_structured_commit(content: &str) -> Option<String> {
    let trimmed = content.trim();

    // If content looks like NDJSON stream, extract from result field first
    if looks_like_ndjson(trimmed) {
        for line in trimmed.lines() {
            let line = line.trim();
            if !line.starts_with('{') {
                continue;
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(line) {
                if json.get("type").and_then(|v| v.as_str()) == Some("result") {
                    if let Some(result_str) = json.get("result").and_then(|v| v.as_str()) {
                        // Try to extract commit from the result content
                        if let Some(msg) = try_extract_from_text(result_str) {
                            return Some(msg);
                        }
                    }
                }
            }
        }
    }

    // Try extraction from text content (direct JSON, code fence, or embedded)
    try_extract_from_text(trimmed)
}

/// Try to extract a structured commit message from text content.
///
/// This handles:
/// - Direct JSON parsing
/// - JSON inside markdown code fences
/// - JSON embedded within other text
fn try_extract_from_text(content: &str) -> Option<String> {
    let trimmed = content.trim();

    // Try extracting from markdown code fence first
    if let Some(json_content) = extract_json_from_code_fence(trimmed) {
        if let Ok(msg) = serde_json::from_str::<StructuredCommitMessage>(&json_content) {
            return format_structured_commit(&msg);
        }
    }

    // Try direct parse
    if let Ok(msg) = serde_json::from_str::<StructuredCommitMessage>(trimmed) {
        return format_structured_commit(&msg);
    }

    // Try to find JSON object within content (in case of minor preamble)
    if let Some(start) = trimmed.find('{') {
        if let Some(end) = trimmed.rfind('}') {
            if start < end {
                let json_str = &trimmed[start..=end];
                if let Ok(msg) = serde_json::from_str::<StructuredCommitMessage>(json_str) {
                    return format_structured_commit(&msg);
                }
            }
        }
    }

    None
}

/// Format a structured commit message into the final string format.
fn format_structured_commit(msg: &StructuredCommitMessage) -> Option<String> {
    let subject = msg.subject.trim();

    // Validate conventional commit format
    if !is_conventional_commit_subject(subject) {
        return None;
    }

    // Format the commit message
    match &msg.body {
        Some(body) if !body.trim().is_empty() => Some(format!("{}\n\n{}", subject, body.trim())),
        _ => Some(subject.to_string()),
    }
}

/// Check if a string is a valid conventional commit subject line.
fn is_conventional_commit_subject(subject: &str) -> bool {
    let valid_types = [
        "feat", "fix", "docs", "style", "refactor", "perf", "test", "build", "ci", "chore",
    ];

    // Find the colon
    let Some(colon_pos) = subject.find(':') else {
        return false;
    };

    let prefix = &subject[..colon_pos];

    // Extract type (before optional scope and !)
    let type_end = prefix
        .find('(')
        .unwrap_or_else(|| prefix.find('!').unwrap_or(prefix.len()));
    let commit_type = &prefix[..type_end];

    valid_types.contains(&commit_type)
}

/// Extract JSON content from markdown code fences.
///
/// Handles both regular code fences and nested fences (e.g., within NDJSON streams).
fn extract_json_from_code_fence(content: &str) -> Option<String> {
    // Look for ```json code fence
    let fence_start = content.find("```json")?;
    let after_fence = &content[fence_start + 7..]; // Skip past ```json

    // Find the end of the code fence
    let fence_end = after_fence.find("\n```")?;
    let json_content = after_fence[..fence_end].trim();

    if json_content.is_empty() {
        None
    } else {
        Some(json_content.to_string())
    }
}

/// Check if content looks like NDJSON stream.
fn looks_like_ndjson(content: &str) -> bool {
    content.lines().count() > 1 && content.contains("{\"type\":")
}

/// Validate extracted content for use as a commit message.
///
/// # Returns
///
/// `Ok(())` if valid, `Err(reason)` if invalid
pub fn validate_commit_message(content: &str) -> Result<(), String> {
    let content = content.trim();

    // Check for empty
    if content.is_empty() {
        return Err("Commit message is empty".to_string());
    }

    // Check minimum length
    if content.len() < 5 {
        return Err(format!(
            "Commit message too short ({} chars, minimum 5)",
            content.len()
        ));
    }

    // Check maximum length (Git convention: first line <72, total <1000)
    if content.len() > 2000 {
        return Err(format!(
            "Commit message too long ({} chars, maximum 2000)",
            content.len()
        ));
    }

    // Check for JSON artifacts that indicate extraction failure
    let json_indicators = [
        r#"{"type":"#,
        r#"{"result":"#,
        r#"{"content":"#,
        r#""session_id":"#,
        r#""timestamp":"#,
        "stream_event",
        "content_block",
    ];
    for indicator in json_indicators {
        if content.contains(indicator) {
            return Err(format!(
                "Commit message contains JSON artifacts: {}...",
                &indicator[..indicator.len().min(20)]
            ));
        }
    }

    // Check for error markers
    let error_markers = [
        "error:",
        "failed to",
        "unable to",
        "i cannot",
        "i'm unable",
        "as an ai",
        "i don't have access",
        "cannot generate",
    ];
    let content_lower = content.to_lowercase();
    for marker in error_markers {
        if content_lower.starts_with(marker) {
            return Err(format!("Commit message starts with error marker: {marker}"));
        }
    }

    // Check for AI thought process leakage at the start of the message
    // This validation catches cases where the filtering in remove_thought_process_patterns
    // failed to remove the AI analysis before the actual commit message
    let thought_process_prefixes = [
        "looking at this diff",
        "i can see",
        "the main changes are",
        "several distinct categories",
        "key categories",
        "based on the diff",
        "analyzing the changes",
        "this diff shows",
        "looking at the changes",
        "i've analyzed",
        "after reviewing",
        // Additional patterns to catch more variations
        "based on the git diff",
        "here are the changes",
        "here's what changed",
        "here is what changed",
        "the following changes",
        "changes include",
        "after reviewing the diff",
        "after reviewing the changes",
        "after analyzing",
        "i've analyzed the changes",
        "i've analyzed the diff",
        "key changes",
        "several changes",
        "distinct changes",
        "key changes include",
        "several changes include",
        "this diff shows the following",
    ];
    for prefix in &thought_process_prefixes {
        if content_lower.starts_with(prefix) {
            return Err(format!(
                "Commit message starts with AI thought process ({prefix}). This indicates a bug in the thought process filtering."
            ));
        }
    }

    // Check for numbered analysis at the start (1., 2., 3., etc.)
    if content.trim_start().starts_with("1. ")
        || content.trim_start().starts_with("1)\n")
        || content_lower.starts_with("- first")
        || content_lower.starts_with("* first")
    {
        return Err(
            "Commit message starts with numbered analysis. This indicates AI thought process leakage.".to_string()
        );
    }

    // Check for formatted thinking output patterns (e.g., "[Claude] Thinking:")
    // This catches formatted thinking output from CLI display that leaked into the log
    let formatted_thinking_patterns = [
        "[claude] thinking:",
        "[claude] Thinking:",
        "[agent] thinking:",
        "[agent] Thinking:",
        "[assistant] thinking:",
        "[assistant] Thinking:",
        "] thinking:",
        "] Thinking:",
    ];
    for pattern in &formatted_thinking_patterns {
        if content_lower.starts_with(pattern) || content.contains(pattern) {
            return Err(format!(
                "Commit message contains formatted thinking pattern ({pattern}). This indicates AI thinking output leaked into the commit message."
            ));
        }
    }

    // Check for placeholder content
    let placeholders = [
        "[commit message]",
        "<commit message>",
        "placeholder",
        "your commit message here",
        "[insert",
        "<insert",
    ];
    for placeholder in placeholders {
        if content_lower.contains(placeholder) {
            return Err(format!(
                "Commit message contains placeholder: {placeholder}"
            ));
        }
    }

    // Check for bad commit message patterns (vague, meaningless messages)
    // Use regex to catch ALL variants, not just hardcoded numbers

    // Pattern 1: "chore: N file(s) changed" for ANY number N
    // Handles: "file(s) changed", "files changed", "file changed" variations
    let file_count_pattern = Regex::new(r"^chore:\s*\d+\s+(?:file\(s\)|files?)\s+changed$")
        .expect("file count regex should be valid");
    if file_count_pattern.is_match(&content_lower) {
        return Err(format!(
            "Commit message matches bad pattern (file count pattern): '{content}'. Use semantic description instead."
        ));
    }

    // Pattern 2: Generic vague patterns
    let vague_patterns = [
        ("chore: apply changes", "vague 'apply changes' pattern"),
        ("chore: update code", "vague 'update code' pattern"),
    ];
    for (pattern, description) in vague_patterns {
        if content_lower == pattern {
            return Err(format!(
                "Commit message matches bad pattern ({description}): {pattern}"
            ));
        }
    }

    // Check for filename list patterns like "chore: update src/file.rs" or "chore: src/file.rs, src/other.rs"
    // These are bad because they just list filenames without semantic meaning
    let first_line = content.lines().next().unwrap_or(content);
    let first_line_lower = first_line.to_lowercase();

    // Check both "chore: update <path>" and "chore: <path>" patterns
    if first_line_lower.starts_with("chore: update ") || first_line_lower.starts_with("chore:") {
        let subject = first_line_lower
            .replacen("chore: update ", "", 1)
            .replacen("chore:", "", 1)
            .trim()
            .to_string();

        // Check if subject looks like a file path or list of file paths
        // File paths contain '/' or end with common extensions
        // We need to check for multiple patterns:
        // 1. Single file path: "src/file.rs"
        // 2. Multiple files with commas: "src/a.rs, src/b.rs"
        // 3. Multiple files with "and": "src/a.rs and src/b.rs"

        // Common code file extensions
        let code_extensions = [
            ".rs", ".js", ".ts", ".py", ".go", ".java", ".c", ".cpp", ".h", ".cs", ".php", ".rb",
            ".swift", ".kt",
        ];

        // Check if subject looks like a file path or list of file paths
        let looks_like_file_list = subject.contains('/')
            || subject.contains('\\') || // Windows paths
            code_extensions.iter().any(|ext| subject.ends_with(ext));

        // Additional check: if there are commas and file extensions, it's definitely a file list
        let has_comma_separated_files =
            subject.contains(", ") && code_extensions.iter().any(|ext| subject.contains(ext));

        // Check for "and" separated files
        let has_and_separated_files =
            subject.contains(" and ") && code_extensions.iter().any(|ext| subject.contains(ext));

        if looks_like_file_list || has_comma_separated_files || has_and_separated_files {
            return Err(format!(
                "Commit message appears to be a file list: '{}'. Use semantic description instead.",
                first_line.trim()
            ));
        }
    }

    Ok(())
}

// =========================================================================
// Commit Message Recovery Functions
// =========================================================================

/// Attempt to salvage a valid commit message from content that failed validation.
///
/// Uses aggressive pattern matching to find a conventional commit message
/// embedded within mixed AI output (thinking + actual message).
///
/// This is used when `validate_commit_message()` fails, to try extracting a valid
/// commit message from mixed output containing thinking patterns and actual content.
///
/// # Returns
///
/// `Some(message)` if a valid commit message was salvaged, `None` otherwise.
pub fn try_salvage_commit_message(content: &str) -> Option<String> {
    // Look for conventional commit pattern anywhere in the content
    let commit_pos = find_conventional_commit_start(content)?;

    // Extract from that position
    let from_commit = &content[commit_pos..];

    // Find where the commit message ends (next blank line or end of content)
    // But include the body paragraph if it follows immediately
    let lines: Vec<&str> = from_commit.lines().collect();

    if lines.is_empty() {
        return None;
    }

    // First line is the subject
    let subject = lines[0].trim();
    if subject.is_empty() {
        return None;
    }

    // Collect body lines (everything after subject until double newline or analysis)
    let mut body_lines: Vec<&str> = Vec::new();
    let mut found_blank = false;

    for line in lines.iter().skip(1) {
        let trimmed: &str = line.trim();

        if trimmed.is_empty() {
            if found_blank {
                // Double blank line - end of message
                break;
            }
            found_blank = true;
            body_lines.push("");
            continue;
        }

        // Check if this line looks like analysis starting up again
        if looks_like_analysis_text(trimmed)
            || trimmed.starts_with("1. ")
            || trimmed.starts_with("- ")
            || trimmed.starts_with("* ")
        {
            break;
        }

        body_lines.push(trimmed);
        found_blank = false;
    }

    // Build the salvaged message
    let mut salvaged = subject.to_string();
    if !body_lines.is_empty() {
        // Remove trailing empty lines from body
        while body_lines.last().is_some_and(|l| l.is_empty()) {
            body_lines.pop();
        }
        if !body_lines.is_empty() {
            salvaged.push('\n');
            salvaged.push_str(&body_lines.join("\n"));
        }
    }

    // Validate the salvaged message before returning
    match validate_commit_message(&salvaged) {
        Ok(()) => Some(salvaged),
        Err(_) => None,
    }
}

/// Generate a deterministic fallback commit message from diff metadata.
///
/// This is the last resort when:
/// 1. Agent output extraction failed
/// 2. Salvage attempt failed
///
/// # Arguments
///
/// * `diff` - The git diff content
///
/// # Returns
///
/// A valid commit message based on changed files. The message is designed to:
/// - Use "chore" type with an appropriate scope
/// - Describe the change semantically (not just file names)
/// - Pass validation (avoids bad patterns like file lists)
pub fn generate_fallback_commit_message(diff: &str) -> String {
    let files = extract_files_from_diff(diff);

    if files.is_empty() {
        // No files found in diff - minimal fallback
        return "chore: apply automated changes".to_string();
    }

    // Find common directory to use as scope
    let common_dir = find_common_directory(&files);

    // Derive a scope from the common directory
    let scope = common_dir
        .as_ref()
        .and_then(|dir| derive_scope_from_path(dir));

    // Determine if this is a single file or multiple files
    let file_count = files.len();

    // Build the commit message
    match (file_count, scope) {
        (1, Some(scope)) => {
            // Single file with a scope - use scope in message
            format!("chore({scope}): update module")
        }
        (1, None) => {
            // Single file without clear scope
            files
                .first()
                .and_then(|f| derive_scope_from_path(f))
                .map_or_else(
                    || "chore: update module".to_string(),
                    |component| format!("chore({component}): update module"),
                )
        }
        (n, Some(scope)) => {
            // Multiple files with common scope
            format!("chore({scope}): update {n} components")
        }
        (n, None) => {
            // Multiple files without common scope
            // Try to find any meaningful scope from the first file
            files
                .first()
                .and_then(|f| derive_scope_from_path(f))
                .map_or_else(
                    || format!("chore: update {n} components"),
                    |component| format!("chore({component}): update {n} components"),
                )
        }
    }
}

/// Extract changed file paths from a git diff.
///
/// Parses "diff --git a/<path> b/<path>" lines to get file paths.
fn extract_files_from_diff(diff: &str) -> Vec<String> {
    let mut files = Vec::new();

    for line in diff.lines() {
        // Match "diff --git a/<path> b/<path>" pattern
        if let Some(rest) = line.strip_prefix("diff --git a/") {
            // The path is up to " b/" (space before b/)
            if let Some(space_b_pos) = rest.find(" b/") {
                let path = &rest[..space_b_pos];
                if !path.is_empty() {
                    files.push(path.to_string());
                }
            }
        }
    }

    files
}

/// Find the common directory prefix for a set of file paths.
///
/// Returns the longest common directory path shared by all files.
fn find_common_directory(paths: &[String]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }

    if paths.len() == 1 {
        // For a single file, return its parent directory
        let path = &paths[0];
        if let Some(last_slash) = path.rfind('/') {
            return Some(path[..last_slash].to_string());
        }
        return None;
    }

    // Split all paths into components
    let split_paths: Vec<Vec<&str>> = paths.iter().map(|p| p.split('/').collect()).collect();

    // Find common prefix
    let mut common_components: Vec<&str> = Vec::new();

    // Use the first path as reference
    let first = &split_paths[0];
    for (i, component) in first.iter().enumerate() {
        // Check if all other paths have this component at this position
        let all_match = split_paths.iter().skip(1).all(|path| {
            // Don't compare the filename itself (last component)
            i < path.len().saturating_sub(1) && path.get(i) == Some(component)
        });

        if all_match && i < first.len().saturating_sub(1) {
            common_components.push(component);
        } else {
            break;
        }
    }

    if common_components.is_empty() {
        None
    } else {
        Some(common_components.join("/"))
    }
}

/// Derive a semantic scope name from a file path.
///
/// Extracts a meaningful component name from the path, preferring:
/// - Last directory name for nested paths (e.g., "files" from "src/files/extraction.rs")
/// - First directory for shallow paths (e.g., "src" from "src/lib.rs")
fn derive_scope_from_path(path: &str) -> Option<String> {
    let components: Vec<&str> = path.split('/').collect();

    if components.is_empty() {
        return None;
    }

    // Filter out common non-semantic directories
    let skip_dirs = ["src", "lib", "bin", "tests", "test", "benches", "examples"];

    // Try to find a meaningful component (prefer second-to-last directory)
    for component in components.iter().rev().skip(1) {
        let comp_lower = component.to_lowercase();
        if !skip_dirs.contains(&comp_lower.as_str()) && !component.is_empty() {
            return Some(component.to_string());
        }
    }

    // If all directories are skipped, try the first non-skip directory
    for component in &components {
        if !skip_dirs.contains(&component.to_lowercase().as_str())
            && !component.is_empty()
            && !component.contains('.')
        {
            return Some(component.to_string());
        }
    }

    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_validate_empty_message() {
        let result = validate_commit_message("");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("empty"));
    }

    #[test]
    fn test_validate_too_short() {
        let result = validate_commit_message("fix");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("too short"));
    }

    #[test]
    fn test_validate_valid_message() {
        let result = validate_commit_message("feat: add new feature");
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_json_artifacts() {
        let result = validate_commit_message("feat: add feature {\"type\":\"result\"}");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("JSON artifacts"));
    }

    #[test]
    fn test_validate_error_markers() {
        let result = validate_commit_message("error: unable to generate");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("error marker"));
    }

    #[test]
    fn test_validate_thought_process_leakage() {
        let result = validate_commit_message("Looking at this diff, I can see changes");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("AI thought process"));
    }

    #[test]
    fn test_validate_numbered_analysis() {
        let result = validate_commit_message("1. First change\n2. Second change");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("numbered analysis"));
    }

    #[test]
    fn test_validate_bad_file_count_pattern() {
        let result = validate_commit_message("chore: 5 files changed");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("file count pattern"));
    }

    #[test]
    fn test_validate_file_list_pattern() {
        let result = validate_commit_message("chore: update src/file.rs");
        assert!(result.is_err());
        assert!(result.unwrap_err().contains("file list"));
    }

    #[test]
    fn test_try_salvage_commit_message() {
        let content = "Looking at this diff...\n\nfeat: add feature";
        let salvaged = try_salvage_commit_message(content);
        assert!(salvaged.is_some());
        assert_eq!(salvaged.unwrap(), "feat: add feature");
    }

    #[test]
    fn test_try_salvage_with_body() {
        let content = "Analysis text\n\nfix(parser): resolve bug\n\nAdd proper error handling.";
        let salvaged = try_salvage_commit_message(content);
        assert!(salvaged.is_some());
        let msg = salvaged.unwrap();
        assert!(msg.starts_with("fix(parser):"));
        assert!(msg.contains("Add proper error handling"));
    }

    #[test]
    fn test_generate_fallback_empty_diff() {
        let fallback = generate_fallback_commit_message("");
        assert_eq!(fallback, "chore: apply automated changes");
    }

    #[test]
    fn test_generate_fallback_single_file() {
        let diff = r"diff --git a/src/files/extraction.rs b/src/files/extraction.rs";
        let fallback = generate_fallback_commit_message(diff);
        assert!(validate_commit_message(&fallback).is_ok());
        assert!(fallback.contains("files") || fallback.contains("update"));
    }

    #[test]
    fn test_generate_fallback_multiple_files_same_dir() {
        let diff = r"diff --git a/src/files/a.rs b/src/files/a.rs
diff --git a/src/files/b.rs b/src/files/b.rs";
        let fallback = generate_fallback_commit_message(diff);
        assert!(validate_commit_message(&fallback).is_ok());
        assert!(fallback.contains("files") || fallback.contains("components"));
    }

    #[test]
    fn test_generate_fallback_multiple_dirs() {
        let diff = r"diff --git a/src/a.rs b/src/a.rs
diff --git a/lib/b.rs b/lib/b.rs
diff --git a/tests/c.rs b/tests/c.rs";
        let fallback = generate_fallback_commit_message(diff);
        assert!(validate_commit_message(&fallback).is_ok());
        assert!(fallback.contains("3 components") || fallback.contains("chore"));
    }

    #[test]
    fn test_regression_thinking_leakage_recovery() {
        // The exact scenario from the bug report
        let log_content = r"[Claude] Thinking: Looking at this diff, I need to analyze...

feat(pipeline): add recovery mechanism

When commit validation fails, attempt to salvage valid message.";

        // Verify salvage recovers it
        let salvaged = try_salvage_commit_message(log_content);
        assert!(salvaged.is_some());
        let msg = salvaged.unwrap();
        assert!(validate_commit_message(&msg).is_ok());
        assert!(msg.starts_with("feat(pipeline):"));
    }

    #[test]
    fn test_extract_files_from_diff() {
        let diff = r"diff --git a/src/files/extraction.rs b/src/files/extraction.rs
--- a/src/files/extraction.rs
+++ b/src/files/extraction.rs
diff --git a/src/phases/commit.rs b/src/phases/commit.rs
--- a/src/phases/commit.rs
+++ b/src/phases/commit.rs";

        let files = extract_files_from_diff(diff);
        assert_eq!(files.len(), 2);
        assert_eq!(files[0], "src/files/extraction.rs");
        assert_eq!(files[1], "src/phases/commit.rs");
    }

    #[test]
    fn test_find_common_directory_same_dir() {
        let paths = vec![
            "src/files/a.rs".to_string(),
            "src/files/b.rs".to_string(),
            "src/files/c.rs".to_string(),
        ];
        let common = find_common_directory(&paths);
        assert_eq!(common, Some("src/files".to_string()));
    }

    #[test]
    fn test_find_common_directory_partial_overlap() {
        let paths = vec![
            "src/files/extraction.rs".to_string(),
            "src/phases/commit.rs".to_string(),
        ];
        let common = find_common_directory(&paths);
        assert_eq!(common, Some("src".to_string()));
    }

    #[test]
    fn test_find_common_directory_no_overlap() {
        let paths = vec!["src/a.rs".to_string(), "lib/b.rs".to_string()];
        let common = find_common_directory(&paths);
        assert!(common.is_none());
    }

    #[test]
    fn test_derive_scope_from_path() {
        // Should extract "files" from nested path
        assert_eq!(
            derive_scope_from_path("src/files/extraction.rs"),
            Some("files".to_string())
        );

        // Should extract "phases" from path
        assert_eq!(
            derive_scope_from_path("src/phases/commit.rs"),
            Some("phases".to_string())
        );

        // Should skip "src" as non-semantic
        assert_ne!(
            derive_scope_from_path("src/files/foo.rs"),
            Some("src".to_string())
        );
    }

    #[test]
    fn test_derive_scope_from_shallow_path() {
        // For shallow paths like "foo.rs" or "src/lib.rs", should return None or meaningful component
        let scope = derive_scope_from_path("lib.rs");
        // lib.rs has no meaningful directory scope
        assert!(scope.is_none());
    }
}
