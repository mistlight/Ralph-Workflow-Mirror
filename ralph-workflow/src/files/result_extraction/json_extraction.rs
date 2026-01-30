//! JSON-based extraction from agent log files.
//!
//! This module provides utilities to extract "result" events from JSON log files.
//! It supports multiple log file resolution strategies and selects the best result
//! using a scoring function.

use crate::files::result_extraction::{
    file_finder::find_log_files_with_prefix, scoring::score_result,
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
/// Extract the best "result" event from agent JSON logs.
///
/// Supports two modes, checked in order:
/// 1. **Prefix mode**: Treat `log_path` as a prefix and search for files matching
///    `{prefix}_*.log` in the parent directory (primary mode)
/// 2. **Exact file fallback**: Check if the exact path exists as a file
///
/// Legacy modes (subdirectory fallback, directory mode) have been removed.
///
/// The "best" result is determined by selecting the content with the highest score,
/// which handles cases where agents emit multiple partial result events during streaming
/// or retries.
///
/// # Arguments
///
/// * `workspace` - Workspace for file operations
/// * `log_path` - Path to the log file prefix
///
/// # Returns
///
/// The raw content from the best result event, or None if no result found.
pub fn extract_last_result(
    workspace: &dyn Workspace,
    log_path: &Path,
) -> io::Result<Option<String>> {
    let parent = log_path.parent().unwrap_or_else(|| Path::new("."));
    let prefix = log_path.file_name().and_then(|s| s.to_str()).unwrap_or("");

    if prefix.is_empty() {
        return Ok(None);
    }

    // Strategy 1: Prefix mode (PRIMARY)
    // Search for files matching `{prefix}_*.log` in the parent directory.
    // This is the current naming convention for agent log files.
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

    // Legacy strategies removed:
    // - Subdirectory fallback (agent names with "/" creating nested directories)
    // - Directory mode (logs stored directly in directory)

    // Strategy 2: Exact file fallback
    // Check if the exact path exists as a file.
    if workspace.is_file(log_path) {
        return extract_result_from_file(workspace, log_path);
    }

    Ok(None)
}
