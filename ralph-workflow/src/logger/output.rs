//! Core Logger implementation.
//!
//! Provides structured, colorized logging output for Ralph's pipeline
//! with support for file logging and various log levels.

use super::{
    Colors, ARROW, BOX_BL, BOX_BR, BOX_H, BOX_TL, BOX_TR, BOX_V, CHECK, CROSS, INFO, WARN,
};
use crate::checkpoint::timestamp;
use crate::config::Verbosity;
use std::fs::{self, OpenOptions};
use std::io::Write;
use std::path::Path;

/// Logger for Ralph output.
///
/// Provides consistent, colorized output with optional file logging.
/// All messages include timestamps and appropriate icons.
pub struct Logger {
    colors: Colors,
    log_file: Option<String>,
}

impl Logger {
    /// Create a new Logger with the given colors configuration.
    pub const fn new(colors: Colors) -> Self {
        Self {
            colors,
            log_file: None,
        }
    }

    /// Configure the logger to also write to a file.
    ///
    /// Log messages written to the file will have ANSI codes stripped.
    pub fn with_log_file(mut self, path: &str) -> Self {
        self.log_file = Some(path.to_string());
        self
    }

    /// Write a message to the log file (if configured).
    fn log_to_file(&self, msg: &str) {
        if let Some(ref path) = self.log_file {
            // Strip ANSI codes for file logging
            let clean_msg = strip_ansi_codes(msg);
            if let Some(parent) = Path::new(path).parent() {
                let _ = fs::create_dir_all(parent);
            }
            if let Ok(mut file) = OpenOptions::new().create(true).append(true).open(path) {
                let _ = writeln!(file, "{clean_msg}");
            }
        }
    }

    /// Log an informational message.
    pub fn info(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}",
            c.dim(),
            timestamp(),
            c.reset(),
            c.blue(),
            INFO,
            c.reset(),
            msg
        );
        self.log_to_file(&format!("[{}] [INFO] {}", timestamp(), msg));
    }

    /// Log a success message.
    pub fn success(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}{}{}",
            c.dim(),
            timestamp(),
            c.reset(),
            c.green(),
            CHECK,
            c.reset(),
            c.green(),
            msg,
            c.reset()
        );
        self.log_to_file(&format!("[{}] [OK] {}", timestamp(), msg));
    }

    /// Log a warning message.
    pub fn warn(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}{}{}",
            c.dim(),
            timestamp(),
            c.reset(),
            c.yellow(),
            WARN,
            c.reset(),
            c.yellow(),
            msg,
            c.reset()
        );
        self.log_to_file(&format!("[{}] [WARN] {}", timestamp(), msg));
    }

    /// Log an error message.
    pub fn error(&self, msg: &str) {
        let c = &self.colors;
        eprintln!(
            "{}[{}]{} {}{}{} {}{}{}",
            c.dim(),
            timestamp(),
            c.reset(),
            c.red(),
            CROSS,
            c.reset(),
            c.red(),
            msg,
            c.reset()
        );
        self.log_to_file(&format!("[{}] [ERROR] {}", timestamp(), msg));
    }

    /// Log a step/action message.
    pub fn step(&self, msg: &str) {
        let c = &self.colors;
        println!(
            "{}[{}]{} {}{}{} {}",
            c.dim(),
            timestamp(),
            c.reset(),
            c.magenta(),
            ARROW,
            c.reset(),
            msg
        );
        self.log_to_file(&format!("[{}] [STEP] {}", timestamp(), msg));
    }

    /// Print a section header with box drawing.
    ///
    /// # Arguments
    ///
    /// * `title` - The header title text
    /// * `color_fn` - Function that returns the color to use
    pub fn header(&self, title: &str, color_fn: fn(Colors) -> &'static str) {
        let c = self.colors;
        let color = color_fn(c);
        let width = 60;
        let title_len = title.chars().count();
        let padding = (width - title_len - 2) / 2;

        println!();
        println!(
            "{}{}{}{}{}{}",
            color,
            self.colors.bold(),
            BOX_TL,
            BOX_H.to_string().repeat(width),
            BOX_TR,
            self.colors.reset()
        );
        println!(
            "{}{}{}{}{}{}{}{}{}{}",
            color,
            self.colors.bold(),
            BOX_V,
            " ".repeat(padding),
            self.colors.white(),
            title,
            color,
            " ".repeat(width - padding - title_len),
            BOX_V,
            self.colors.reset()
        );
        println!(
            "{}{}{}{}{}{}",
            color,
            self.colors.bold(),
            BOX_BL,
            BOX_H.to_string().repeat(width),
            BOX_BR,
            self.colors.reset()
        );
    }

    /// Print a sub-header (less prominent than header).
    pub fn subheader(&self, title: &str) {
        let c = &self.colors;
        println!();
        println!("{}{}{} {}{}", c.bold(), c.blue(), ARROW, title, c.reset());
        println!("{}{}──{}", c.dim(), "─".repeat(title.len()), c.reset());
    }
}

impl Default for Logger {
    fn default() -> Self {
        Self::new(Colors::new())
    }
}

/// Strip ANSI escape sequences from a string.
///
/// Used when writing to log files where ANSI codes are not supported.
pub fn strip_ansi_codes(s: &str) -> String {
    static ANSI_RE: std::sync::LazyLock<Result<regex::Regex, regex::Error>> =
        std::sync::LazyLock::new(|| regex::Regex::new(r"\x1b\[[0-9;]*m"));
    (*ANSI_RE)
        .as_ref()
        .map_or_else(|_| s.to_string(), |re| re.replace_all(s, "").to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_strip_ansi_codes() {
        let input = "\x1b[31mred\x1b[0m text";
        assert_eq!(strip_ansi_codes(input), "red text");
    }

    #[test]
    fn test_strip_ansi_codes_no_codes() {
        let input = "plain text";
        assert_eq!(strip_ansi_codes(input), "plain text");
    }

    #[test]
    fn test_strip_ansi_codes_multiple() {
        let input = "\x1b[1m\x1b[32mbold green\x1b[0m \x1b[34mblue\x1b[0m";
        assert_eq!(strip_ansi_codes(input), "bold green blue");
    }
}

// ===== Output Formatting Functions =====

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

/// Detect if command-line arguments request JSON output.
///
/// Scans the provided argv for common JSON output flags used by various CLIs:
/// - `--json` or `--json=...`
/// - `--output-format` with json value
/// - `--format json`
/// - `-F json`
/// - `-o stream-json` or similar
pub fn argv_requests_json(argv: &[String]) -> bool {
    // Skip argv[0] (the executable); scan flags/args only.
    let mut iter = argv.iter().skip(1).peekable();
    while let Some(arg) = iter.next() {
        if arg == "--json" || arg.starts_with("--json=") {
            return true;
        }

        if arg == "--output-format" {
            if let Some(next) = iter.peek() {
                let next = next.as_str();
                if next.contains("json") {
                    return true;
                }
            }
        }
        if let Some((flag, value)) = arg.split_once('=') {
            if flag == "--output-format" && value.contains("json") {
                return true;
            }
            if flag == "--format" && value == "json" {
                return true;
            }
        }

        if arg == "--format" {
            if let Some(next) = iter.peek() {
                if next.as_str() == "json" {
                    return true;
                }
            }
        }

        // Some CLIs use short flags like -F json or -o stream-json
        if arg == "-F" {
            if let Some(next) = iter.peek() {
                if next.as_str() == "json" {
                    return true;
                }
            }
        }
        if arg.starts_with("-F") && arg != "-F" && arg.trim_start_matches("-F") == "json" {
            return true;
        }

        if arg == "-o" {
            if let Some(next) = iter.peek() {
                let next = next.as_str();
                if next.contains("json") {
                    return true;
                }
            }
        }
        if arg.starts_with("-o") && arg != "-o" && arg.trim_start_matches("-o").contains("json") {
            return true;
        }
    }
    false
}

/// Format generic JSON output for display.
///
/// Parses the input as JSON and formats it according to verbosity level:
/// - `Full` or `Debug`: Pretty-print with indentation
/// - Other levels: Compact single-line format
///
/// Output is truncated according to verbosity limits.
pub fn format_generic_json_for_display(line: &str, verbosity: Verbosity) -> String {
    let Ok(value) = serde_json::from_str::<serde_json::Value>(line) else {
        return truncate_text(line, verbosity.truncate_limit("agent_msg"));
    };

    let formatted = match verbosity {
        Verbosity::Full | Verbosity::Debug => {
            serde_json::to_string_pretty(&value).unwrap_or_else(|_| line.to_string())
        }
        _ => serde_json::to_string(&value).unwrap_or_else(|_| line.to_string()),
    };
    truncate_text(&formatted, verbosity.truncate_limit("agent_msg"))
}

#[cfg(test)]
mod output_formatting_tests {
    use super::*;

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

    #[test]
    fn test_argv_requests_json_detects_common_flags() {
        assert!(argv_requests_json(&[
            "tool".to_string(),
            "--json".to_string()
        ]));
        assert!(argv_requests_json(&[
            "tool".to_string(),
            "--output-format=stream-json".to_string()
        ]));
        assert!(argv_requests_json(&[
            "tool".to_string(),
            "--output-format".to_string(),
            "stream-json".to_string()
        ]));
        assert!(argv_requests_json(&[
            "tool".to_string(),
            "--format".to_string(),
            "json".to_string()
        ]));
        assert!(argv_requests_json(&[
            "tool".to_string(),
            "-F".to_string(),
            "json".to_string()
        ]));
        assert!(argv_requests_json(&[
            "tool".to_string(),
            "-o".to_string(),
            "stream-json".to_string()
        ]));
    }

    #[test]
    fn test_format_generic_json_for_display_pretty_prints_when_full() {
        let line = r#"{"type":"message","content":{"text":"hello"}}"#;
        let formatted = format_generic_json_for_display(line, Verbosity::Full);
        assert!(formatted.contains('\n'));
        assert!(formatted.contains("\"type\""));
        assert!(formatted.contains("\"message\""));
    }
}
