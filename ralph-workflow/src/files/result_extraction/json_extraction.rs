//! JSON-based extraction from agent log files.
//!
//! This module provides utilities to extract "result" events from JSON log files.
//! It supports multiple log file resolution strategies and selects the best result
//! using a scoring function.

use crate::files::result_extraction::{
    file_finder::{find_log_files_with_prefix, find_subdirs_with_prefix},
    scoring::score_result,
};

use serde_json::Value as JsonValue;
use std::fs::File;
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
/// # Note
///
/// This function is public for testing purposes. The main public API is [`extract_last_result`].
pub fn extract_result_from_file(path: &Path) -> io::Result<Option<String>> {
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

/// Extract from a directory by scanning all files in it.
///
/// Selects the best result across all files using the scoring function to handle
/// retry scenarios where multiple log files may exist. Prefers structured plans
/// over simple longest strings.
pub fn extract_from_directory(log_dir: &Path) -> io::Result<Option<String>> {
    let log_entries = match std::fs::read_dir(log_dir) {
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

/// Extract the best "result" event from agent JSON logs.
///
/// Supports three modes:
/// 1. **Directory mode**: If `log_path` is a directory, scan all files in it
/// 2. **Prefix mode**: If `log_path` is not a directory, treat it as a prefix and
///    search for files matching `{prefix}_*.log` in the parent directory
/// 3. **Subdirectory fallback**: If no files found, check for subdirectories matching
///    `{prefix}_*` (handles legacy logs where agent names with "/" created nested dirs)
///
/// The "best" result is determined by selecting the content with the highest score,
/// which handles cases where agents emit multiple partial result events during streaming
/// or retries.
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
