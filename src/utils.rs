//! General utilities module.
//!
//! This module re-exports utilities from specialized modules for backward
//! compatibility and convenience. New code should import directly from
//! the appropriate module:
//!
//! - [`crate::checkpoint`] - Pipeline checkpoint system
//! - [`crate::logger`] - Logging utilities
//! - [`crate::files`] - File management utilities
//!
//! # Utility Functions
//!
//! This module also provides small utility functions that don't fit
//! in a specific module:
//!
//! - [`split_command`] - Parse shell command strings
//! - [`truncate_text`] - Truncate text with ellipsis

// Allow unused imports for re-exports (backward compatibility layer)
#![allow(unused_imports)]

use std::io;

// Re-exports from checkpoint module
pub use crate::checkpoint::{
    checkpoint_exists, clear_checkpoint, load_checkpoint, save_checkpoint, PipelineCheckpoint,
    PipelinePhase,
};

// Re-exports from logger module
pub use crate::logger::{print_progress, strip_ansi_codes, timestamp, Logger};

// Re-exports from files module
pub use crate::files::{
    clean_context_for_reviewer, cleanup_generated_files, delete_commit_message_file,
    delete_issues_file_for_isolation, delete_plan_file, ensure_files, file_contains_marker,
    read_commit_message_file, reset_context_for_isolation, update_status, validate_prompt_md,
    PromptValidationResult, GENERATED_FILES,
};

/// Split a shell-like command string into argv parts.
///
/// Supports quotes and backslash escapes (e.g. `cmd --flag "a b"`).
///
/// # Example
///
/// ```ignore
/// let argv = split_command("echo 'hello world'")?;
/// assert_eq!(argv, vec!["echo", "hello world"]);
/// ```
///
/// # Errors
///
/// Returns an error if the command string has unmatched quotes.
pub fn split_command(cmd: &str) -> io::Result<Vec<String>> {
    let cmd = cmd.trim();
    if cmd.is_empty() {
        return Ok(vec![]);
    }

    shell_words::split(cmd).map_err(|err| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Failed to parse command string '{}': {}", cmd, err),
        )
    })
}

/// Truncate text to a limit with ellipsis.
///
/// Uses character count rather than byte length to avoid panics on UTF-8 text.
/// Truncates at character boundaries and appends "..." when truncation occurs.
///
/// # Example
///
/// ```ignore
/// assert_eq!(truncate_text("hello world", 8), "hello...");
/// assert_eq!(truncate_text("short", 10), "short");
/// ```
pub fn truncate_text(text: &str, limit: usize) -> String {
    // Handle edge case where limit is too small for even "..."
    if limit <= 3 {
        return text.chars().take(limit).collect();
    }

    let char_count = text.chars().count();
    if char_count <= limit {
        text.to_string()
    } else {
        // Leave room for "..."
        let truncate_at = limit.saturating_sub(3);
        let truncated: String = text.chars().take(truncate_at).collect();
        format!("{}...", truncated)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_split_command_simple() {
        let result = split_command("echo hello").unwrap();
        assert_eq!(result, vec!["echo", "hello"]);
    }

    #[test]
    fn test_split_command_with_quotes() {
        let result = split_command("echo 'hello world'").unwrap();
        assert_eq!(result, vec!["echo", "hello world"]);
    }

    #[test]
    fn test_split_command_empty() {
        let result = split_command("").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_split_command_whitespace() {
        let result = split_command("   ").unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn test_truncate_text_no_truncation() {
        assert_eq!(truncate_text("hello", 10), "hello");
        assert_eq!(truncate_text("hello", 5), "hello");
    }

    #[test]
    fn test_truncate_text_with_ellipsis() {
        // "hello world" is 11 chars, limit 8 means 5 chars + "..."
        assert_eq!(truncate_text("hello world", 8), "hello...");
    }

    #[test]
    fn test_truncate_text_unicode() {
        // Should not panic on UTF-8 multibyte characters
        let text = "日本語テスト"; // 6 Japanese characters
        assert_eq!(truncate_text(text, 10), "日本語テスト");
        assert_eq!(truncate_text(text, 6), "日本語テスト");
        assert_eq!(truncate_text(text, 5), "日本...");
    }

    #[test]
    fn test_truncate_text_emoji() {
        // Emojis can be multi-byte but should be handled correctly
        let text = "Hello 👋 World";
        assert_eq!(truncate_text(text, 20), "Hello 👋 World");
        assert_eq!(truncate_text(text, 10), "Hello 👋...");
    }

    #[test]
    fn test_truncate_text_edge_cases() {
        assert_eq!(truncate_text("abc", 3), "abc");
        assert_eq!(truncate_text("abcd", 3), "abc"); // limit too small for ellipsis
        assert_eq!(truncate_text("ab", 1), "a");
        assert_eq!(truncate_text("", 5), "");
    }
}
