//! Agent name extraction from log file paths.
//!
//! Provides utilities to parse log file names and extract the agent identifier.

use std::path::Path;

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
