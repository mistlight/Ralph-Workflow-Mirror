//! Unified log file path management.
//!
//! This module provides a single source of truth for log file path operations:
//! - Creating log file paths from components
//! - Parsing agent names from log file paths
//! - Finding log files matching a prefix
//!
//! # Log File Naming Convention
//!
//! Log files follow the pattern: `{prefix}_{agent}_{model_index}.log`
//!
//! Where:
//! - `prefix` is the log prefix (e.g., "planning_1", "developer_2")
//! - `agent` is the sanitized agent name (slashes replaced with hyphens)
//! - `model_index` is the model fallback index (0 for primary)
//!
//! Examples:
//! - `.agent/logs/planning_1_ccs-glm_0.log`
//! - `.agent/logs/developer_2_opencode-anthropic-claude-sonnet-4_1.log`
//! - `.agent/logs/reviewer_1_claude_0.log`
//!
//! # Agent Name Sanitization
//!
//! Agent registry names may contain slashes (e.g., "ccs/glm", "opencode/anthropic/model").
//! These are sanitized to hyphens for file system safety:
//! - `ccs/glm` → `ccs-glm`
//! - `opencode/anthropic/claude-sonnet-4` → `opencode-anthropic-claude-sonnet-4`

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
}
