//! Attempt index discovery for log files.
//!
//! Provides functions to scan directories and determine the next available
//! attempt index for log files, preventing filename collisions.

use crate::workspace::Workspace;
use std::path::Path;

use super::naming::sanitize_agent_name;

/// Determine the next attempt index for a given `(prefix, agent, model_index)` logfile family.
///
/// This scans the parent directory for existing log files matching:
///
/// `{prefix_filename}_{agent}_{model_index}_a{attempt}.log`
///
/// and returns `max(attempt)+1`, or `0` if no matching files exist.
///
/// This avoids collisions when attempt numbers are computed from multiple counters
/// (retry cycles, continuation attempts, XSD retry count) that may exceed assumed bounds.
pub fn next_logfile_attempt_index(
    log_prefix: &Path,
    agent_name: &str,
    model_index: usize,
    workspace: &dyn Workspace,
) -> u32 {
    let parent = log_prefix.parent().unwrap_or_else(|| Path::new("."));
    let prefix_filename = match log_prefix.file_name().and_then(|s| s.to_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return 0,
    };

    let safe_agent = sanitize_agent_name(&agent_name.to_lowercase());
    let start = format!("{prefix_filename}_{safe_agent}_{model_index}_a");

    let mut max_attempt: Option<u32> = None;
    if let Ok(entries) = workspace.read_dir(parent) {
        for entry in entries {
            if !entry.is_file() {
                continue;
            }
            let Some(filename) = entry.file_name().and_then(|s| s.to_str()) else {
                continue;
            };
            let has_log_ext = entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("log"));
            if !filename.starts_with(&start) || !has_log_ext {
                continue;
            }

            let attempt_digits = &filename[start.len()..filename.len().saturating_sub(4)];
            if attempt_digits.is_empty() || !attempt_digits.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            if let Ok(n) = attempt_digits.parse::<u32>() {
                max_attempt = Some(max_attempt.map_or(n, |prev| prev.max(n)));
            }
        }
    }

    max_attempt.map_or(0, |n| n.saturating_add(1))
}

/// Determine the next attempt index for simplified per-run agent logs.
///
/// This scans the agents/ subdirectory for existing log files matching:
///
/// - `{base_filename}.log` (the base file, first attempt)
/// - `{base_filename}_a{attempt}.log` (retry attempts)
///
/// and returns the next available attempt index. If the base file exists,
/// it returns 1 or greater; otherwise it returns 0.
///
/// This supports the per-run log directory structure where agent identity
/// is recorded in log file headers rather than filenames.
pub fn next_simplified_logfile_attempt_index(
    base_log_path: &Path,
    workspace: &dyn Workspace,
) -> u32 {
    let parent = base_log_path.parent().unwrap_or_else(|| Path::new("."));
    let base_filename = match base_log_path.file_stem().and_then(|s| s.to_str()) {
        Some(s) if !s.is_empty() => s,
        _ => return 0,
    };

    let start = format!("{base_filename}_a");
    let base_log_name = format!("{base_filename}.log");

    let mut max_attempt: Option<u32> = None;
    let mut base_file_exists = false;

    if let Ok(entries) = workspace.read_dir(parent) {
        for entry in entries {
            if !entry.is_file() {
                continue;
            }
            let Some(filename) = entry.file_name().and_then(|s| s.to_str()) else {
                continue;
            };

            // Check if this is the base file (without attempt suffix)
            if filename == base_log_name {
                base_file_exists = true;
                continue;
            }

            // Check if this is a file with attempt suffix
            let has_log_ext = entry
                .path()
                .extension()
                .is_some_and(|ext| ext.eq_ignore_ascii_case("log"));
            if !filename.starts_with(&start) || !has_log_ext {
                continue;
            }

            let attempt_digits = &filename[start.len()..filename.len().saturating_sub(4)];
            if attempt_digits.is_empty() || !attempt_digits.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            if let Ok(n) = attempt_digits.parse::<u32>() {
                max_attempt = Some(max_attempt.map_or(n, |prev| prev.max(n)));
            }
        }
    }

    // If base file exists but no _aN files exist, return 1 (first retry)
    // If _aN files exist, return max(attempt) + 1
    // If neither exist, return 0 (first attempt)
    max_attempt.map_or_else(|| u32::from(base_file_exists), |max| max.saturating_add(1))
}
