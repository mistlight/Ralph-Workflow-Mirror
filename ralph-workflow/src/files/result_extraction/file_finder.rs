//! File finding utilities for log file discovery.

use std::fs;
use std::io;
use std::path::{Path, PathBuf};

/// Find log files matching a prefix pattern in a directory.
///
/// Returns all files that start with `{prefix}_` and end with `.log`.
pub fn find_log_files_with_prefix(parent_dir: &Path, prefix: &str) -> io::Result<Vec<PathBuf>> {
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
pub fn find_subdirs_with_prefix(parent_dir: &Path, prefix: &str) -> io::Result<Vec<PathBuf>> {
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
