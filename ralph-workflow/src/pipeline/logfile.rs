//! Unified log file path management.
//!
//! This module provides utilities for building log file paths. There are two
//! naming conventions supported:
//!
//! 1. **Legacy naming** (pre-per-run-logging):
//!    - `{prefix}_{agent}_{model_index}.log` (and with attempt suffix)
//!    - Used for special-purpose logs like commit generation and conflict resolution
//!    - These logs are written outside the per-run log directory structure
//!
//! 2. **Simplified naming** (per-run-logging):
//!    - `{phase}_{index}.log` (with optional `_aN` attempt suffix)
//!    - Used for main pipeline phase agent logs
//!    - These logs are written under `.agent/logs-<run_id>/agents/`
//!
//! The legacy naming functions (`build_logfile_path`, `build_logfile_path_with_attempt`)
//! are kept for special cases where agent identity in the filename is useful (e.g., commit
//! message generation logs, conflict resolution logs) and for backward compatibility with
//! tooling that may parse legacy log filenames.

use crate::workspace::Workspace;
use std::path::{Path, PathBuf};

/// Sanitize an agent name for use in file paths.
///
/// Replaces slashes with hyphens to avoid creating subdirectories.
pub fn sanitize_agent_name(agent_name: &str) -> String {
    agent_name.replace('/', "-")
}

/// Build a legacy-style log file path from components.
///
/// This generates a log filename with the pattern:
/// `{prefix}_{agent}_{model_index}.log`
///
/// This is the **legacy naming convention** used before per-run logging was introduced.
/// It is retained for special-purpose logs (e.g., commit generation, conflict resolution)
/// where embedding agent identity in the filename is useful for tooling.
///
/// For new per-run agent logs, use [`RunLogContext::agent_log`](crate::logging::RunLogContext::agent_log)
/// instead, which uses the simplified `{phase}_{index}[_aN].log` format.
///
/// # Arguments
///
/// * `prefix` - Log prefix path (e.g., ".agent/logs/commit_generation/commit_generation")
/// * `agent_name` - Agent identifier (will be sanitized to replace `/` with `-`)
/// * `model_index` - Model index for multi-model agents
///
/// # Returns
///
/// A log file path string with the legacy naming format.
pub fn build_logfile_path(prefix: &str, agent_name: &str, model_index: usize) -> String {
    let safe_agent_name = sanitize_agent_name(agent_name);
    format!("{}_{safe_agent_name}_{model_index}.log", prefix)
}

/// Build a legacy-style log file path with retry attempt index.
///
/// This generates a log filename with the pattern:
/// `{prefix}_{agent}_{model_index}_a{attempt}.log`
///
/// This is the **legacy naming convention** used before per-run logging was introduced.
/// The attempt suffix distinguishes between multiple invocations (e.g., during XSD retry
/// cycles or after timeout-triggered agent switches).
///
/// It is retained for special-purpose logs (e.g., commit generation, conflict resolution)
/// where embedding agent identity in the filename is useful for tooling.
///
/// For new per-run agent logs, use [`RunLogContext::agent_log`](crate::logging::RunLogContext::agent_log)
/// instead, which uses the simplified `{phase}_{index}[_aN].log` format.
///
/// # Arguments
///
/// * `prefix` - Log prefix path (e.g., ".agent/logs/commit_generation/commit_generation")
/// * `agent_name` - Agent identifier (will be sanitized to replace `/` with `-`)
/// * `model_index` - Model index for multi-model agents
/// * `attempt` - Retry attempt counter (0 for first retry, 1 for second retry, etc.)
///
/// # Returns
///
/// A log file path string with the legacy naming format including attempt suffix.
pub fn build_logfile_path_with_attempt(
    prefix: &str,
    agent_name: &str,
    model_index: usize,
    attempt: u32,
) -> String {
    let safe_agent_name = sanitize_agent_name(agent_name);
    format!("{}_{safe_agent_name}_{model_index}_a{attempt}.log", prefix)
}

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
    let parent = log_prefix.parent().unwrap_or(Path::new("."));
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
            if !filename.starts_with(&start) || !filename.ends_with(".log") {
                continue;
            }

            let attempt_digits = &filename[start.len()..filename.len().saturating_sub(4)];
            if attempt_digits.is_empty() || !attempt_digits.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            if let Ok(n) = attempt_digits.parse::<u32>() {
                max_attempt = Some(match max_attempt {
                    Some(prev) => prev.max(n),
                    None => n,
                });
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
    let parent = base_log_path.parent().unwrap_or(Path::new("."));
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
            if !filename.starts_with(&start) || !filename.ends_with(".log") {
                continue;
            }

            let attempt_digits = &filename[start.len()..filename.len().saturating_sub(4)];
            if attempt_digits.is_empty() || !attempt_digits.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            if let Ok(n) = attempt_digits.parse::<u32>() {
                max_attempt = Some(match max_attempt {
                    Some(prev) => prev.max(n),
                    None => n,
                });
            }
        }
    }

    // If base file exists but no _aN files exist, return 1 (first retry)
    // If _aN files exist, return max(attempt) + 1
    // If neither exist, return 0 (first attempt)
    if let Some(max) = max_attempt {
        max.saturating_add(1)
    } else if base_file_exists {
        1
    } else {
        0
    }
}

/// Extract the agent name from a log file path.
///
/// Parses a log file name like `planning_1_ccs-glm_0.log` to extract
/// the agent name (`ccs-glm`). The returned name is the sanitized form
/// (hyphens instead of slashes).
///
/// # Arguments
///
/// * `log_file` - The full path to the log file
/// * `log_prefix` - The prefix path used to generate the log file
///
/// # Returns
///
/// The sanitized agent name (e.g., "ccs-glm"), or `None` if parsing fails.
///
/// # Examples
///
/// ```
/// use std::path::Path;
/// use ralph_workflow::pipeline::logfile::extract_agent_name_from_logfile;
///
/// let log_file = Path::new(".agent/logs/planning_1_ccs-glm_0.log");
/// let prefix = Path::new(".agent/logs/planning_1");
/// assert_eq!(extract_agent_name_from_logfile(log_file, prefix), Some("ccs-glm".to_string()));
/// ```
pub fn extract_agent_name_from_logfile(log_file: &Path, log_prefix: &Path) -> Option<String> {
    let filename = log_file.file_name()?.to_str()?;
    let prefix_filename = log_prefix.file_name()?.to_str()?;

    // Remove the prefix and the leading underscore
    if !filename.starts_with(prefix_filename) {
        return None;
    }
    let after_prefix = &filename[prefix_filename.len()..];
    let after_prefix = after_prefix.strip_prefix('_')?;

    // Remove the .log extension
    let without_ext = after_prefix.strip_suffix(".log")?;

    // Strip optional retry attempt suffix ("_a{attempt}") if present.
    //
    // Important: only strip when the filename also contains a model index.
    // If a logfile ever uses the agent-only form (no model index) and the agent
    // name itself ends with "_a<digits>", we must not truncate the agent name.
    let without_ext = if let Some(attempt_pos) = without_ext.rfind("_a") {
        let attempt_digits = &without_ext[attempt_pos + 2..];
        if !attempt_digits.is_empty() && attempt_digits.chars().all(|c| c.is_ascii_digit()) {
            let before_attempt = &without_ext[..attempt_pos];

            // Confirm the segment before "_a{attempt}" ends with "_{model_index}".
            if let Some(model_pos) = before_attempt.rfind('_') {
                let model_digits = &before_attempt[model_pos + 1..];
                if !model_digits.is_empty() && model_digits.chars().all(|c| c.is_ascii_digit()) {
                    before_attempt
                } else {
                    without_ext
                }
            } else {
                without_ext
            }
        } else {
            without_ext
        }
    } else {
        without_ext
    };

    // The format is either "agent" or "agent_modelindex"
    // Find the last underscore followed by a number
    if let Some(last_underscore) = without_ext.rfind('_') {
        let after_underscore = &without_ext[last_underscore + 1..];
        // Check if what follows is a number (model index)
        if after_underscore.chars().all(|c| c.is_ascii_digit()) {
            // Return everything before the last underscore
            return Some(without_ext[..last_underscore].to_string());
        }
    }

    // No model index suffix, the whole thing is the agent name
    Some(without_ext.to_string())
}

/// Find the most recent log file matching a prefix pattern.
///
/// Searches the parent directory for log files that match the prefix pattern
/// and returns the most recently modified one.
///
/// # Arguments
///
/// * `log_prefix` - The prefix path (e.g., ".agent/logs/planning_1")
/// * `workspace` - The workspace to search in
///
/// # Returns
///
/// The path to the most recent matching log file, or `None` if no match found.
///
/// # Examples
///
/// ```ignore
/// use std::path::Path;
/// use ralph_workflow::pipeline::logfile::find_most_recent_logfile;
/// use ralph_workflow::workspace::WorkspaceFs;
///
/// let workspace = WorkspaceFs::new("/repo".into());
/// let prefix = Path::new(".agent/logs/planning_1");
/// if let Some(log_file) = find_most_recent_logfile(prefix, &workspace) {
///     println!("Most recent log: {:?}", log_file);
/// }
/// ```
pub fn find_most_recent_logfile(log_prefix: &Path, workspace: &dyn Workspace) -> Option<PathBuf> {
    let parent = log_prefix.parent().unwrap_or(Path::new("."));
    let prefix_str = log_prefix.file_name().and_then(|s| s.to_str())?;

    let mut best_file: Option<(PathBuf, std::time::SystemTime)> = None;

    if let Ok(entries) = workspace.read_dir(parent) {
        for entry in entries {
            if entry.is_file() {
                if let Some(filename) = entry.file_name().and_then(|s| s.to_str()) {
                    // Match files that start with our prefix, have more content, and end with .log
                    if filename.starts_with(prefix_str)
                        && filename.len() > prefix_str.len()
                        && filename.ends_with(".log")
                    {
                        // Get modification time for this file
                        if let Some(modified) = entry.modified() {
                            match &best_file {
                                None => best_file = Some((entry.path().to_path_buf(), modified)),
                                Some((_, best_time)) if modified > *best_time => {
                                    best_file = Some((entry.path().to_path_buf(), modified));
                                }
                                _ => {}
                            }
                        } else {
                            // No modification time available, use this if we have no best yet
                            if best_file.is_none() {
                                best_file = Some((
                                    entry.path().to_path_buf(),
                                    std::time::SystemTime::UNIX_EPOCH,
                                ));
                            }
                        }
                    }
                }
            }
        }
    }

    best_file.map(|(path, _)| path)
}

/// Read the content of the most recent log file matching a prefix.
///
/// This is a convenience function that combines `find_most_recent_logfile`
/// with reading the file content.
///
/// # Arguments
///
/// * `log_prefix` - The prefix path (e.g., ".agent/logs/planning_1")
/// * `workspace` - The workspace to read from
///
/// # Returns
///
/// The content of the most recent matching log file, or an empty string if not found.
pub fn read_most_recent_logfile(log_prefix: &Path, workspace: &dyn Workspace) -> String {
    find_most_recent_logfile(log_prefix, workspace)
        .and_then(|path| workspace.read(&path).ok())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::MemoryWorkspace;
    use std::time::{Duration, SystemTime};

    #[test]
    fn test_sanitize_agent_name() {
        assert_eq!(sanitize_agent_name("claude"), "claude");
        assert_eq!(sanitize_agent_name("ccs/glm"), "ccs-glm");
        assert_eq!(
            sanitize_agent_name("opencode/anthropic/claude-sonnet-4"),
            "opencode-anthropic-claude-sonnet-4"
        );
    }

    #[test]
    fn test_build_logfile_path() {
        assert_eq!(
            build_logfile_path(".agent/logs/planning_1", "claude", 0),
            ".agent/logs/planning_1_claude_0.log"
        );
        assert_eq!(
            build_logfile_path(".agent/logs/planning_1", "ccs/glm", 0),
            ".agent/logs/planning_1_ccs-glm_0.log"
        );
        assert_eq!(
            build_logfile_path(".agent/logs/dev_2", "opencode/anthropic/claude-sonnet-4", 1),
            ".agent/logs/dev_2_opencode-anthropic-claude-sonnet-4_1.log"
        );
    }

    #[test]
    fn test_build_logfile_path_with_attempt() {
        assert_eq!(
            build_logfile_path_with_attempt(".agent/logs/planning_1", "claude", 0, 0),
            ".agent/logs/planning_1_claude_0_a0.log"
        );
        assert_eq!(
            build_logfile_path_with_attempt(".agent/logs/planning_1", "ccs/glm", 1, 2),
            ".agent/logs/planning_1_ccs-glm_1_a2.log"
        );
        assert_eq!(
            build_logfile_path_with_attempt(
                ".agent/logs/dev_2",
                "opencode/anthropic/claude-sonnet-4",
                0,
                5
            ),
            ".agent/logs/dev_2_opencode-anthropic-claude-sonnet-4_0_a5.log"
        );
    }

    #[test]
    fn test_next_logfile_attempt_index_returns_zero_when_no_matches() {
        let workspace = MemoryWorkspace::new_test();
        let prefix = Path::new(".agent/logs/planning_1");
        assert_eq!(
            next_logfile_attempt_index(prefix, "claude", 0, &workspace),
            0
        );
    }

    #[test]
    fn test_next_logfile_attempt_index_increments_from_existing_attempts() {
        let workspace = MemoryWorkspace::new_test()
            .with_file(".agent/logs/planning_1_claude_0_a0.log", "")
            .with_file(".agent/logs/planning_1_claude_0_a2.log", "")
            .with_file(".agent/logs/planning_1_claude_0_a10.log", "")
            // Different agent/model should be ignored
            .with_file(".agent/logs/planning_1_other_0_a99.log", "")
            .with_file(".agent/logs/planning_1_claude_1_a7.log", "");

        let prefix = Path::new(".agent/logs/planning_1");
        assert_eq!(
            next_logfile_attempt_index(prefix, "claude", 0, &workspace),
            11
        );
    }

    #[test]
    fn test_extract_agent_name_with_model_index() {
        let log_file = Path::new(".agent/logs/planning_1_ccs-glm_0.log");
        let prefix = Path::new(".agent/logs/planning_1");
        assert_eq!(
            extract_agent_name_from_logfile(log_file, prefix),
            Some("ccs-glm".to_string())
        );
    }

    #[test]
    fn test_extract_agent_name_opencode_style() {
        let log_file = Path::new(".agent/logs/dev_1_opencode-anthropic-claude-sonnet-4_0.log");
        let prefix = Path::new(".agent/logs/dev_1");
        assert_eq!(
            extract_agent_name_from_logfile(log_file, prefix),
            Some("opencode-anthropic-claude-sonnet-4".to_string())
        );
    }

    #[test]
    fn test_extract_agent_name_with_attempt_suffix() {
        let log_file = Path::new(".agent/logs/planning_1_ccs-glm_0_a2.log");
        let prefix = Path::new(".agent/logs/planning_1");
        assert_eq!(
            extract_agent_name_from_logfile(log_file, prefix),
            Some("ccs-glm".to_string())
        );
    }

    #[test]
    fn test_extract_agent_name_opencode_style_with_attempt_suffix() {
        let log_file = Path::new(".agent/logs/dev_1_opencode-anthropic-claude-sonnet-4_0_a5.log");
        let prefix = Path::new(".agent/logs/dev_1");
        assert_eq!(
            extract_agent_name_from_logfile(log_file, prefix),
            Some("opencode-anthropic-claude-sonnet-4".to_string())
        );
    }

    #[test]
    fn test_extract_agent_name_does_not_strip_attempt_suffix_when_no_model_index() {
        // If a logfile uses the agent-only form (no model index) and the agent name
        // itself ends with "_a<digits>", we must NOT strip that suffix.
        let log_file = Path::new(".agent/logs/planning_1_agent_a123.log");
        let prefix = Path::new(".agent/logs/planning_1");
        assert_eq!(
            extract_agent_name_from_logfile(log_file, prefix),
            Some("agent_a123".to_string())
        );
    }

    #[test]
    fn test_extract_agent_name_wrong_prefix() {
        let log_file = Path::new(".agent/logs/review_1_claude_0.log");
        let prefix = Path::new(".agent/logs/planning_1");
        assert_eq!(extract_agent_name_from_logfile(log_file, prefix), None);
    }

    #[test]
    fn test_find_most_recent_logfile() {
        // Create workspace with two log files with different modification times
        let time1 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let time2 = SystemTime::UNIX_EPOCH + Duration::from_secs(2000);

        let workspace = MemoryWorkspace::new_test()
            .with_file_at_time(".agent/logs/test_1_agent-a_0.log", "old", time1)
            .with_file_at_time(".agent/logs/test_1_agent-b_0.log", "new", time2);

        let prefix = Path::new(".agent/logs/test_1");
        let result = find_most_recent_logfile(prefix, &workspace);
        assert_eq!(
            result,
            Some(PathBuf::from(".agent/logs/test_1_agent-b_0.log"))
        );
    }

    #[test]
    fn test_find_most_recent_logfile_no_match() {
        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/logs/other_1_claude_0.log", "content");

        let prefix = Path::new(".agent/logs/test_1");
        let result = find_most_recent_logfile(prefix, &workspace);
        assert_eq!(result, None);
    }

    #[test]
    fn test_read_most_recent_logfile() {
        let time1 = SystemTime::UNIX_EPOCH + Duration::from_secs(1000);
        let time2 = SystemTime::UNIX_EPOCH + Duration::from_secs(2000);

        let workspace = MemoryWorkspace::new_test()
            .with_file_at_time(".agent/logs/test_1_agent-a_0.log", "old content", time1)
            .with_file_at_time(".agent/logs/test_1_agent-b_0.log", "new content", time2);

        let prefix = Path::new(".agent/logs/test_1");
        let result = read_most_recent_logfile(prefix, &workspace);
        assert_eq!(result, "new content");
    }

    #[test]
    fn test_read_most_recent_logfile_empty_when_not_found() {
        let workspace = MemoryWorkspace::new_test();

        let prefix = Path::new(".agent/logs/nonexistent");
        let result = read_most_recent_logfile(prefix, &workspace);
        assert_eq!(result, "");
    }

    #[test]
    fn test_next_simplified_logfile_attempt_index_returns_zero_when_no_matches() {
        let workspace = MemoryWorkspace::new_test();
        // Create the run directory structure
        workspace
            .create_dir_all(Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents"))
            .unwrap();

        let base_path = Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents/planning_1.log");
        assert_eq!(
            next_simplified_logfile_attempt_index(base_path, &workspace),
            0
        );
    }

    #[test]
    fn test_next_simplified_logfile_attempt_index_increments_from_existing_attempts() {
        let workspace = MemoryWorkspace::new_test();
        // Create the run directory structure
        workspace
            .create_dir_all(Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents"))
            .unwrap();

        // Pre-populate some log files with attempt suffixes
        let base = ".agent/logs-2026-02-06_14-03-27.123Z/agents";
        workspace
            .write(
                &PathBuf::from(format!("{}/planning_1_a0.log", base)),
                "first",
            )
            .unwrap();
        workspace
            .write(
                &PathBuf::from(format!("{}/planning_1_a2.log", base)),
                "third",
            )
            .unwrap();
        workspace
            .write(
                &PathBuf::from(format!("{}/planning_1_a10.log", base)),
                "11th",
            )
            .unwrap();
        // Different phase should be ignored
        workspace
            .write(
                &PathBuf::from(format!("{}/developer_1_a5.log", base)),
                "other",
            )
            .unwrap();

        let base_path = Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents/planning_1.log");
        assert_eq!(
            next_simplified_logfile_attempt_index(base_path, &workspace),
            11
        );
    }

    #[test]
    fn test_next_simplified_logfile_attempt_index_returns_one_when_base_file_exists() {
        let workspace = MemoryWorkspace::new_test();
        // Create the run directory structure
        workspace
            .create_dir_all(Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents"))
            .unwrap();

        // Create only the base file (without attempt suffix)
        let base = ".agent/logs-2026-02-06_14-03-27.123Z/agents";
        workspace
            .write(&PathBuf::from(format!("{}/planning_1.log", base)), "base")
            .unwrap();

        let base_path = Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents/planning_1.log");
        // Should return 1 (first retry) since base file exists
        assert_eq!(
            next_simplified_logfile_attempt_index(base_path, &workspace),
            1
        );
    }

    #[test]
    fn test_next_simplified_logfile_attempt_index_returns_next_after_base_and_attempts() {
        let workspace = MemoryWorkspace::new_test();
        // Create the run directory structure
        workspace
            .create_dir_all(Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents"))
            .unwrap();

        // Create the base file (without attempt suffix) and some attempt files
        let base = ".agent/logs-2026-02-06_14-03-27.123Z/agents";
        workspace
            .write(&PathBuf::from(format!("{}/planning_1.log", base)), "base")
            .unwrap();
        workspace
            .write(
                &PathBuf::from(format!("{}/planning_1_a1.log", base)),
                "first retry",
            )
            .unwrap();
        workspace
            .write(
                &PathBuf::from(format!("{}/planning_1_a2.log", base)),
                "second retry",
            )
            .unwrap();

        let base_path = Path::new(".agent/logs-2026-02-06_14-03-27.123Z/agents/planning_1.log");
        // Should return 3 (max existing attempt + 1)
        assert_eq!(
            next_simplified_logfile_attempt_index(base_path, &workspace),
            3
        );
    }
}
