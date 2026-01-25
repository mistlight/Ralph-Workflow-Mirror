//! JSON-based extraction from agent log files.
//!
//! This module provides utilities to extract "result" events from JSON log files.
//! It supports multiple log file resolution strategies and selects the best result
//! using a scoring function.

use crate::files::result_extraction::{
    file_finder::{find_log_files_with_prefix, find_subdirs_with_prefix},
    scoring::score_result,
};
use crate::workspace::Workspace;

use serde_json::Value as JsonValue;
use std::io::{self, BufRead, BufReader};
use std::path::Path;

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
///
/// # Important: Last Line Handling
///
/// This function uses `BufReader::lines()` which correctly returns the last line
/// of a file **even if it doesn't have a trailing newline**. According to Rust's
/// standard library documentation, `lines()` returns an iterator that includes
/// the last line regardless of whether it ends with `\n` or not.
///
/// This behavior is critical for our use case because agents (especially AI CLI tools)
/// may write JSON events without proper trailing newlines. We rely on this behavior
/// to ensure result events are always extracted.
///
/// Reference: <https://doc.rust-lang.org/std/io/struct.BufReader.html#method.lines>
///
/// # Note
///
/// This function is public for testing purposes. The main public API is [`extract_last_result`].
pub fn extract_result_from_file(
    workspace: &dyn Workspace,
    path: &Path,
) -> io::Result<Option<String>> {
    let content = match workspace.read_bytes(path) {
        Ok(c) => c,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };

    let reader = BufReader::new(content.as_slice());
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

/// Extract from a directory by scanning all files in it.
///
/// Selects the best result across all files using the scoring function to handle
/// retry scenarios where multiple log files may exist. Prefers structured plans
/// over simple longest strings.
pub fn extract_from_directory(
    workspace: &dyn Workspace,
    log_dir: &Path,
) -> io::Result<Option<String>> {
    let log_entries = match workspace.read_dir(log_dir) {
        Ok(entries) => entries,
        Err(e) if e.kind() == io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e),
    };

    let mut best_result: Option<String> = None;
    let mut best_score: u32 = 0;

    for entry in log_entries {
        let path = entry.path();
        if !entry.is_file() {
            continue;
        }

        if let Some(result) = extract_result_from_file(workspace, path)? {
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

/// Extract the best "result" event from agent JSON logs.
///
/// Supports four modes, checked in order:
/// 1. **Prefix mode**: Treat `log_path` as a prefix and search for files matching
///    `{prefix}_*.log` in the parent directory (primary mode for current code)
/// 2. **Subdirectory fallback**: If no files found, check for subdirectories matching
///    `{prefix}_*` (handles legacy logs where agent names with "/" created nested dirs)
/// 3. **Directory mode**: If `log_path` is a directory, scan all files in it (legacy)
/// 4. **Exact file fallback**: Check if the exact path exists as a file
///
/// The "best" result is determined by selecting the content with the highest score,
/// which handles cases where agents emit multiple partial result events during streaming
/// or retries.
///
/// # Arguments
///
/// * `workspace` - Workspace for file operations
/// * `log_path` - Path to the log directory OR log file prefix
///
/// # Returns
///
/// The raw content from the best result event, or None if no result found.
///
/// # Note
///
/// Prefix mode is checked FIRST to prevent old directories from shadowing new
/// prefix-based log files. For example, if `.agent/logs/rebase_conflict_resolution/`
/// exists as an empty directory (from old runs), we still want to find
/// `.agent/logs/rebase_conflict_resolution_ccs-glm_0.log` files.
pub fn extract_last_result(
    workspace: &dyn Workspace,
    log_path: &Path,
) -> io::Result<Option<String>> {
    let parent = log_path.parent().unwrap_or_else(|| Path::new("."));
    let prefix = log_path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    if prefix.is_empty() {
        return Ok(None);
    }

    // Strategy 1: Prefix mode (PRIMARY - checked first to avoid directory shadowing)
    // This prevents old directories from shadowing new prefix-based log files.
    // For example, if `.agent/logs/rebase_conflict_resolution/` exists as a directory,
    // we still want to find `.agent/logs/rebase_conflict_resolution_ccs-glm_0.log`.
    let log_files = find_log_files_with_prefix(workspace, parent, prefix)?;

    if !log_files.is_empty() {
        let mut best_result: Option<String> = None;
        let mut best_score: u32 = 0;
        for log_file in log_files {
            if let Some(result) = extract_result_from_file(workspace, &log_file)? {
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

    // Strategy 2: Check for subdirectories matching prefix pattern
    // This handles the legacy case where agent names with "/" created nested directories
    // (e.g., "planning_1_ccs/glm_0.log" instead of "planning_1_ccs-glm_0.log")
    let subdirs = find_subdirs_with_prefix(workspace, parent, prefix)?;
    for subdir in subdirs {
        if let Some(result) = extract_from_directory(workspace, &subdir)? {
            return Ok(Some(result));
        }
    }

    // Strategy 3: Directory mode (LEGACY - checked after prefix mode)
    // Only used if log_path is actually a directory and no prefix-based files were found.
    // This is the old behavior where logs were stored directly in the directory.
    if workspace.is_dir(log_path) {
        return extract_from_directory(workspace, log_path);
    }

    // Strategy 4: Exact file fallback
    // Check if the exact path exists as a file.
    if workspace.is_file(log_path) {
        return extract_result_from_file(workspace, log_path);
    }

    Ok(None)
}
