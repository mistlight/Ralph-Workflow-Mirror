// Part 1: Imports and core functions for building and parsing log file paths

use crate::workspace::Workspace;
use std::path::{Path, PathBuf};

/// Sanitize an agent name for use in file paths.
///
/// Replaces slashes with hyphens to avoid creating subdirectories.
///
/// # Examples
///
/// ```
/// use ralph_workflow::pipeline::logfile::sanitize_agent_name;
///
/// assert_eq!(sanitize_agent_name("ccs/glm"), "ccs-glm");
/// assert_eq!(sanitize_agent_name("opencode/anthropic/claude-sonnet-4"),
///            "opencode-anthropic-claude-sonnet-4");
/// assert_eq!(sanitize_agent_name("claude"), "claude");
/// ```
pub fn sanitize_agent_name(agent_name: &str) -> String {
    agent_name.replace('/', "-")
}

/// Build a log file path from components.
///
/// # Arguments
///
/// * `prefix` - The log file prefix (e.g., ".agent/logs/planning_1")
/// * `agent_name` - The agent registry name (will be sanitized)
/// * `model_index` - The model fallback index
///
/// # Examples
///
/// ```
/// use ralph_workflow::pipeline::logfile::build_logfile_path;
///
/// assert_eq!(
///     build_logfile_path(".agent/logs/planning_1", "ccs/glm", 0),
///     ".agent/logs/planning_1_ccs-glm_0.log"
/// );
/// ```
pub fn build_logfile_path(prefix: &str, agent_name: &str, model_index: usize) -> String {
    let safe_agent_name = sanitize_agent_name(agent_name);
    format!("{}_{safe_agent_name}_{model_index}.log", prefix)
}

/// Build a log file path with retry attempt index for enhanced observability.
///
/// This variant includes a retry attempt counter to distinguish between
/// multiple invocations of the same agent/model combination (e.g., during
/// XSD retry cycles or after timeout-triggered agent switches).
///
/// # Arguments
///
/// * `prefix` - The log file prefix (e.g., ".agent/logs/planning_1")
/// * `agent_name` - The agent registry name (will be sanitized)
/// * `model_index` - The model fallback index
/// * `attempt` - The retry attempt number (0 for first attempt)
///
/// # Pattern
///
/// `{prefix}_{agent}_{model_index}_a{attempt}.log`
///
/// # Examples
///
/// ```
/// use ralph_workflow::pipeline::logfile::build_logfile_path_with_attempt;
///
/// assert_eq!(
///     build_logfile_path_with_attempt(".agent/logs/planning_1", "claude", 0, 0),
///     ".agent/logs/planning_1_claude_0_a0.log"
/// );
/// assert_eq!(
///     build_logfile_path_with_attempt(".agent/logs/planning_1", "ccs/glm", 1, 2),
///     ".agent/logs/planning_1_ccs-glm_1_a2.log"
/// );
/// ```
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
