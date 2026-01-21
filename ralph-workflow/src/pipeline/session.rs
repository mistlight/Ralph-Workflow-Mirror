//! Session management for agent continuation.
//!
//! This module provides utilities for extracting and managing session IDs
//! from agent output logs, enabling session continuation for XSD retries.
//!
//! # Session Continuation
//!
//! When XSD validation fails, we want to continue the same agent session
//! rather than starting a fresh one. This allows the AI to retain memory
//! of its previous reasoning and the requirements it analyzed.
//!
//! # Supported Agents
//!
//! - **OpenCode**: Uses `--session <id>` flag with `sessionID` from NDJSON output
//! - **Claude CLI**: Uses `--continue` or `--resume <id>` flag with `session_id` from JSON
//!
//! # Fallback Behavior
//!
//! **IMPORTANT**: Session continuation is agent-specific and does NOT affect the
//! fallback mechanism. When an agent fails (rate limit, crash, etc.), the system
//! falls back to a different agent with a **fresh session**.
//!
//! Session continuation only applies to **XSD retries within the same agent**:
//!
//! ```text
//! Agent A: First attempt → XSD error → Retry (continue session) → XSD error → ...
//!          ↓ (agent failure, e.g., rate limit)
//! Agent B: Fresh session → XSD error → Retry (continue session) → ...
//! ```
//!
//! The `SessionState` struct tracks both the session ID and the agent name,
//! ensuring that session continuation is only attempted with the same agent.

use std::fs;
use std::path::Path;

/// Tracks session state for agent continuation.
///
/// This struct ensures session continuation is only attempted with the same agent.
/// When the agent changes (due to fallback), the session must be reset.
/// Tracks session state for agent continuation.
///
/// This struct ensures session continuation is only attempted with the same agent.
/// When the agent changes (due to fallback), the session must be reset.
///
/// NOTE: This is infrastructure for potential future use. Currently, SessionInfo
/// is passed directly without using SessionState for tracking.
#[allow(dead_code)]
#[derive(Debug, Clone, Default)]
pub struct SessionState {
    /// The session ID from the agent's output
    session_id: Option<String>,
    /// The agent name that created this session
    agent_name: Option<String>,
}

#[allow(dead_code)]
impl SessionState {
    /// Create a new empty session state.
    pub fn new() -> Self {
        Self::default()
    }

    /// Update the session state after a successful agent run.
    ///
    /// # Arguments
    ///
    /// * `session_id` - The session ID extracted from the agent's output
    /// * `agent_name` - The name of the agent that ran
    pub fn update(&mut self, session_id: Option<String>, agent_name: &str) {
        self.session_id = session_id;
        self.agent_name = Some(agent_name.to_string());
    }

    /// Get the session ID if it's valid for the given agent.
    ///
    /// Returns `None` if:
    /// - No session ID has been set
    /// - The agent name doesn't match (session belongs to a different agent)
    ///
    /// This ensures we only continue sessions with the same agent.
    pub fn get_for_agent(&self, agent_name: &str) -> Option<&str> {
        match (&self.session_id, &self.agent_name) {
            (Some(id), Some(name)) if name == agent_name => Some(id.as_str()),
            _ => None,
        }
    }

    /// Check if we have a valid session for the given agent.
    pub fn has_session_for_agent(&self, agent_name: &str) -> bool {
        self.get_for_agent(agent_name).is_some()
    }

    /// Clear the session state.
    ///
    /// Call this when starting a completely new operation (not an XSD retry).
    pub fn clear(&mut self) {
        self.session_id = None;
        self.agent_name = None;
    }
}

/// Extract session ID from an OpenCode log file.
///
/// OpenCode outputs NDJSON with session IDs in the format:
/// ```json
/// {"type":"step_start","timestamp":1234567890,"sessionID":"ses_44f9562d4ffe",...}
/// ```
///
/// We look for the first `sessionID` field and return it.
///
/// # Arguments
///
/// * `log_path` - Path to the log file containing OpenCode NDJSON output
///
/// # Returns
///
/// * `Some(session_id)` if a valid session ID was found
/// * `None` if no session ID could be extracted
pub fn extract_opencode_session_id(log_path: &Path) -> Option<String> {
    let content = fs::read_to_string(log_path).ok()?;
    extract_opencode_session_id_from_content(&content)
}

/// Extract session ID from OpenCode NDJSON content string.
///
/// This is the internal implementation that works on content directly,
/// useful for testing and for cases where content is already in memory.
pub fn extract_opencode_session_id_from_content(content: &str) -> Option<String> {
    // Look for "sessionID":"ses_..." pattern
    // We use a simple regex-free approach for performance
    for line in content.lines() {
        if let Some(session_id) = extract_session_id_from_json_line(line, "sessionID") {
            // Validate it looks like an OpenCode session ID (starts with "ses_")
            if session_id.starts_with("ses_") {
                return Some(session_id);
            }
        }
    }
    None
}

/// Extract session ID from a Claude CLI log file.
///
/// Claude CLI outputs JSON with session IDs in the format:
/// ```json
/// {"type":"system","subtype":"init","session_id":"abc123"}
/// ```
///
/// # Arguments
///
/// * `log_path` - Path to the log file containing Claude CLI JSON output
///
/// # Returns
///
/// * `Some(session_id)` if a valid session ID was found
/// * `None` if no session ID could be extracted
pub fn extract_claude_session_id(log_path: &Path) -> Option<String> {
    let content = fs::read_to_string(log_path).ok()?;
    extract_claude_session_id_from_content(&content)
}

/// Extract session ID from Claude CLI JSON content string.
pub fn extract_claude_session_id_from_content(content: &str) -> Option<String> {
    // Look for "session_id":"..." pattern
    for line in content.lines() {
        if let Some(session_id) = extract_session_id_from_json_line(line, "session_id") {
            // Claude session IDs don't have a specific prefix requirement
            if !session_id.is_empty() {
                return Some(session_id);
            }
        }
    }
    None
}

/// Extract a string value for a given key from a JSON line.
///
/// This is a simple, fast extraction that doesn't require full JSON parsing.
/// It looks for `"key":"value"` patterns and extracts the value.
///
/// # Arguments
///
/// * `line` - A single line of JSON
/// * `key` - The key to search for
///
/// # Returns
///
/// * `Some(value)` if the key was found with a string value
/// * `None` if the key was not found or didn't have a string value
fn extract_session_id_from_json_line(line: &str, key: &str) -> Option<String> {
    // Build the search pattern: "key":"
    let pattern = format!("\"{}\":\"", key);

    // Find the pattern
    let start_idx = line.find(&pattern)?;
    let value_start = start_idx + pattern.len();

    // Find the closing quote
    let remaining = &line[value_start..];
    let end_idx = remaining.find('"')?;

    // Extract the value
    let value = &remaining[..end_idx];

    // Basic validation: non-empty and no control characters
    if !value.is_empty()
        && value
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-')
    {
        Some(value.to_string())
    } else {
        None
    }
}

/// Result of extracting session info from a log file.
#[derive(Debug, Clone)]
pub struct SessionInfo {
    /// The session ID extracted from the log file.
    pub session_id: String,
    /// The agent name extracted from the log file name.
    pub agent_name: String,
    /// The log file path (kept for debugging/future use).
    #[allow(dead_code)]
    pub log_file: std::path::PathBuf,
}

/// Find the most recent log file matching a prefix pattern and extract session info.
///
/// This function finds the log file, extracts the agent name from the filename,
/// and extracts the session ID from the content based on the JSON parser type.
///
/// # Arguments
///
/// * `log_prefix` - The log file prefix (e.g., `.agent/logs/planning_1`)
/// * `parser_type` - The JSON parser type to determine session ID format
///
/// # Returns
///
/// * `Some(SessionInfo)` if session info was found
/// * `None` if no session info could be extracted
pub fn extract_session_info_from_log_prefix(
    log_prefix: &Path,
    parser_type: crate::agents::JsonParserType,
) -> Option<SessionInfo> {
    use crate::agents::JsonParserType;

    // Find the most recent log file matching the prefix
    let log_file = find_most_recent_log_file(log_prefix)?;

    // Extract agent name from log file name
    // Log files are named: {prefix}_{agent}_{model_index}.log or {prefix}_{agent}.log
    let agent_name = extract_agent_name_from_log_file(&log_file, log_prefix)?;

    // Extract session ID based on parser type
    let session_id = match parser_type {
        JsonParserType::OpenCode => extract_opencode_session_id(&log_file),
        JsonParserType::Claude => extract_claude_session_id(&log_file),
        // Other parsers don't support session continuation
        JsonParserType::Codex | JsonParserType::Gemini | JsonParserType::Generic => None,
    }?;

    Some(SessionInfo {
        session_id,
        agent_name,
        log_file,
    })
}

/// Extract agent name from a log file path.
///
/// Log files are named: `{prefix}_{agent}_{model_index}.log` or `{prefix}_{agent}.log`
/// For example: `.agent/logs/planning_1_ccs-glm_0.log` -> `ccs-glm`
fn extract_agent_name_from_log_file(log_file: &Path, log_prefix: &Path) -> Option<String> {
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
/// Log files are named `{prefix}_{agent}_{model}.log`, e.g.:
/// `.agent/logs/planning_1_ccs-glm_0.log`
fn find_most_recent_log_file(log_prefix: &Path) -> Option<std::path::PathBuf> {
    let parent = log_prefix.parent().unwrap_or(Path::new("."));
    let prefix_str = log_prefix
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    let mut best_file: Option<(std::path::PathBuf, std::time::SystemTime)> = None;

    if let Ok(entries) = fs::read_dir(parent) {
        for entry in entries.flatten() {
            let path = entry.path();
            if let Some(filename) = path.file_name().and_then(|s| s.to_str()) {
                // Match files that start with our prefix and end with .log
                if filename.starts_with(prefix_str)
                    && filename.len() > prefix_str.len()
                    && filename.ends_with(".log")
                {
                    // Get modification time for this file
                    if let Ok(metadata) = fs::metadata(&path) {
                        if let Ok(modified) = metadata.modified() {
                            match &best_file {
                                None => best_file = Some((path.clone(), modified)),
                                Some((_, best_time)) if modified > *best_time => {
                                    best_file = Some((path.clone(), modified));
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }
        }
    }

    best_file.map(|(path, _)| path)
}

#[cfg(test)]
mod tests {
    use super::*;

    // ===== SessionState tests =====

    #[test]
    fn test_session_state_new_is_empty() {
        let state = SessionState::new();
        assert!(!state.has_session_for_agent("opencode"));
        assert!(state.get_for_agent("opencode").is_none());
    }

    #[test]
    fn test_session_state_update_and_get() {
        let mut state = SessionState::new();
        state.update(Some("ses_123".to_string()), "opencode");

        assert!(state.has_session_for_agent("opencode"));
        assert_eq!(state.get_for_agent("opencode"), Some("ses_123"));
    }

    #[test]
    fn test_session_state_different_agent_returns_none() {
        let mut state = SessionState::new();
        state.update(Some("ses_123".to_string()), "opencode");

        // Different agent should not get the session
        assert!(!state.has_session_for_agent("claude"));
        assert!(state.get_for_agent("claude").is_none());
    }

    #[test]
    fn test_session_state_clear() {
        let mut state = SessionState::new();
        state.update(Some("ses_123".to_string()), "opencode");
        state.clear();

        assert!(!state.has_session_for_agent("opencode"));
        assert!(state.get_for_agent("opencode").is_none());
    }

    #[test]
    fn test_session_state_update_replaces_previous() {
        let mut state = SessionState::new();
        state.update(Some("ses_123".to_string()), "opencode");
        state.update(Some("ses_456".to_string()), "claude");

        // Old session should be replaced
        assert!(!state.has_session_for_agent("opencode"));
        assert!(state.has_session_for_agent("claude"));
        assert_eq!(state.get_for_agent("claude"), Some("ses_456"));
    }

    #[test]
    fn test_session_state_none_session_id() {
        let mut state = SessionState::new();
        state.update(None, "opencode");

        // No session ID means no continuation possible
        assert!(!state.has_session_for_agent("opencode"));
    }

    // ===== Session ID extraction tests =====

    #[test]
    fn test_extract_opencode_session_id_from_content() {
        let content = r#"{"type":"step_start","timestamp":1768191337567,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06aa45c001"}}
{"type":"text","timestamp":1768191347231,"sessionID":"ses_44f9562d4ffe","part":{"text":"Hello"}}"#;

        let session_id = extract_opencode_session_id_from_content(content);
        assert_eq!(session_id, Some("ses_44f9562d4ffe".to_string()));
    }

    #[test]
    fn test_extract_opencode_session_id_no_match() {
        let content = r#"{"type":"text","part":{"text":"Hello"}}"#;
        let session_id = extract_opencode_session_id_from_content(content);
        assert_eq!(session_id, None);
    }

    #[test]
    fn test_extract_opencode_session_id_invalid_prefix() {
        // Session ID without "ses_" prefix should be rejected
        let content = r#"{"type":"step_start","sessionID":"invalid_session"}"#;
        let session_id = extract_opencode_session_id_from_content(content);
        assert_eq!(session_id, None);
    }

    #[test]
    fn test_extract_claude_session_id_from_content() {
        let content = r#"{"type":"system","subtype":"init","session_id":"abc123"}
{"type":"text","content":"Hello"}"#;

        let session_id = extract_claude_session_id_from_content(content);
        assert_eq!(session_id, Some("abc123".to_string()));
    }

    #[test]
    fn test_extract_claude_session_id_no_match() {
        let content = r#"{"type":"text","content":"Hello"}"#;
        let session_id = extract_claude_session_id_from_content(content);
        assert_eq!(session_id, None);
    }

    #[test]
    fn test_extract_session_id_from_json_line() {
        let line = r#"{"sessionID":"ses_abc123","other":"value"}"#;
        let result = extract_session_id_from_json_line(line, "sessionID");
        assert_eq!(result, Some("ses_abc123".to_string()));
    }

    #[test]
    fn test_extract_session_id_from_json_line_not_found() {
        let line = r#"{"other":"value"}"#;
        let result = extract_session_id_from_json_line(line, "sessionID");
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_session_id_from_json_line_with_special_chars() {
        // Underscores and hyphens are allowed
        let line = r#"{"sessionID":"ses_abc-123_def"}"#;
        let result = extract_session_id_from_json_line(line, "sessionID");
        assert_eq!(result, Some("ses_abc-123_def".to_string()));
    }

    #[test]
    fn test_extract_session_id_rejects_invalid_chars() {
        // Control characters or other special chars should be rejected
        let line = r#"{"sessionID":"ses_abc<script>"}"#;
        let result = extract_session_id_from_json_line(line, "sessionID");
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_session_id_empty_value() {
        let line = r#"{"sessionID":""}"#;
        let result = extract_session_id_from_json_line(line, "sessionID");
        // Empty string should be allowed by extraction but filtered by caller
        assert_eq!(result, None); // Empty fails the all() check
    }

    // ===== Agent name extraction tests =====

    #[test]
    fn test_extract_agent_name_with_model_index() {
        use std::path::PathBuf;
        let log_file = PathBuf::from(".agent/logs/planning_1_ccs-glm_0.log");
        let log_prefix = PathBuf::from(".agent/logs/planning_1");
        let result = extract_agent_name_from_log_file(&log_file, &log_prefix);
        assert_eq!(result, Some("ccs-glm".to_string()));
    }

    #[test]
    fn test_extract_agent_name_without_model_index() {
        use std::path::PathBuf;
        let log_file = PathBuf::from(".agent/logs/planning_1_claude.log");
        let log_prefix = PathBuf::from(".agent/logs/planning_1");
        let result = extract_agent_name_from_log_file(&log_file, &log_prefix);
        assert_eq!(result, Some("claude".to_string()));
    }

    #[test]
    fn test_extract_agent_name_with_dashes() {
        use std::path::PathBuf;
        let log_file = PathBuf::from(".agent/logs/planning_1_glm-direct_2.log");
        let log_prefix = PathBuf::from(".agent/logs/planning_1");
        let result = extract_agent_name_from_log_file(&log_file, &log_prefix);
        assert_eq!(result, Some("glm-direct".to_string()));
    }

    #[test]
    fn test_extract_agent_name_opencode_provider() {
        use std::path::PathBuf;
        // OpenCode agents with provider/model format
        let log_file =
            PathBuf::from(".agent/logs/planning_1_opencode-anthropic-claude-sonnet-4_0.log");
        let log_prefix = PathBuf::from(".agent/logs/planning_1");
        let result = extract_agent_name_from_log_file(&log_file, &log_prefix);
        assert_eq!(
            result,
            Some("opencode-anthropic-claude-sonnet-4".to_string())
        );
    }

    #[test]
    fn test_extract_agent_name_wrong_prefix() {
        use std::path::PathBuf;
        let log_file = PathBuf::from(".agent/logs/review_1_claude_0.log");
        let log_prefix = PathBuf::from(".agent/logs/planning_1");
        let result = extract_agent_name_from_log_file(&log_file, &log_prefix);
        assert_eq!(result, None);
    }
}
