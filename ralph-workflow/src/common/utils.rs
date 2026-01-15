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

use std::io;

use regex::Regex;

// Re-exports from checkpoint module
pub use crate::checkpoint::{
    checkpoint_exists, clear_checkpoint, load_checkpoint, save_checkpoint, PipelineCheckpoint,
    PipelinePhase,
};

// Re-export timestamp from checkpoint
pub use crate::checkpoint::timestamp;

// Re-exports from logger module
pub use crate::logger::{print_progress, Logger};
pub use crate::logger::strip_ansi_codes;

// Re-exports from files module
pub use crate::files::{
    clean_context_for_reviewer, cleanup_generated_files, create_prompt_backup,
    delete_commit_message_file, delete_issues_file_for_isolation, delete_plan_file, ensure_files,
    file_contains_marker, make_prompt_read_only, read_commit_message_file,
    reset_context_for_isolation, update_status, validate_prompt_md, write_commit_message_file,
    GENERATED_FILES, PromptValidationResult,
};

// Keep backward-compatibility re-exports "used" without suppressing lints.
const _: () = {
    let _ = strip_ansi_codes as fn(&str) -> String;
    let _ = timestamp as fn() -> String;
    let _ = GENERATED_FILES;
    let _ = std::mem::size_of::<PromptValidationResult>();
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
            format!("Failed to parse command string: {err}"),
        )
    })
}

static SECRET_LIKE_RE: std::sync::LazyLock<Option<Regex>> = std::sync::LazyLock::new(|| {
    Regex::new(
        r"(?ix)
        \b(
          sk-[a-z0-9]{20,} |
          ghp_[a-z0-9]{20,} |
          github_pat_[a-z0-9_]{20,} |
          xox[baprs]-[a-z0-9-]{10,} |
          AKIA[0-9A-Z]{16}
        )\b
        ",
    )
    .ok()
});

fn is_sensitive_key(key: &str) -> bool {
    let key = key.trim().trim_start_matches('-').trim_start_matches('-');
    let key = key
        .split_once('=')
        .or_else(|| key.split_once(':'))
        .map_or(key, |(k, _)| k)
        .trim()
        .to_ascii_lowercase()
        .replace('_', "-");

    matches!(
        key.as_str(),
        "token"
            | "access-token"
            | "api-key"
            | "apikey"
            | "auth"
            | "authorization"
            | "bearer"
            | "client-secret"
            | "password"
            | "pass"
            | "passwd"
            | "private-key"
            | "secret"
    )
}

fn redact_arg_value(key: &str, value: &str) -> String {
    if is_sensitive_key(key) {
        return "<redacted>".to_string();
    }
    SECRET_LIKE_RE.as_ref().map_or_else(
        || value.to_string(),
        |re| re.replace_all(value, "<redacted>").to_string(),
    )
}

fn shell_quote_for_log(arg: &str) -> String {
    if arg.is_empty() {
        return "''".to_string();
    }
    if !arg
        .chars()
        .any(|c| c.is_whitespace() || matches!(c, '"' | '\'' | '\\'))
    {
        return arg.to_string();
    }
    let escaped = arg.replace('\'', r#"'\"'\"'"#);
    format!("'{escaped}'")
}

/// Format argv for logs, redacting likely secrets.
pub fn format_argv_for_log(argv: &[String]) -> String {
    let mut out = Vec::with_capacity(argv.len());
    let mut redact_next_value = false;

    for arg in argv {
        if redact_next_value {
            out.push("<redacted>".to_string());
            redact_next_value = false;
            continue;
        }
        redact_next_value = false;

        if let Some((k, v)) = arg.split_once('=') {
            // Flag-style (--token=...) or env-style (GITHUB_TOKEN=...)
            let env_key = k.to_ascii_uppercase();
            let looks_like_secret_env = env_key.contains("TOKEN")
                || env_key.contains("SECRET")
                || env_key.contains("PASSWORD")
                || env_key.contains("PASS")
                || env_key.contains("KEY");
            if is_sensitive_key(k) || looks_like_secret_env {
                out.push(format!("{}=<redacted>", shell_quote_for_log(k)));
                continue;
            }
            let redacted = redact_arg_value(k, v);
            out.push(shell_quote_for_log(&format!("{k}={redacted}")));
            continue;
        }

        if is_sensitive_key(arg) {
            out.push(shell_quote_for_log(arg));
            redact_next_value = true;
            continue;
        }

        let redacted = SECRET_LIKE_RE.as_ref().map_or_else(
            || arg.clone(),
            |re| re.replace_all(arg, "<redacted>").to_string(),
        );
        out.push(shell_quote_for_log(&redacted));
    }

    out.join(" ")
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
        format!("{truncated}...")
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
