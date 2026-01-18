//! Core Logger implementation.
//!
//! Provides structured, colorized logging output for Ralph's pipeline
//! with support for file logging and various log levels.
//!
//! # Logger Design and Behavior
//!
//! The `Logger` struct provides dual-output logging:
//! - **Console output**: Colorized, human-readable messages to stdout/stderr
//! - **File output**: Plain text (ANSI codes stripped) with timestamps
//!
//! ## How Logger Writes to Files
//!
//! Logger's file logging is **not** done through the `std::io::Write` trait.
//! Instead, file logging happens via the `Loggable` trait's `log()` method, which
//! is called by each log level method (`info()`, `success()`, `warn()`, `error()`).
//!
//! ### Important: `Write` Trait Behavior
//!
//! Logger implements `std::io::Write`, but the `write()` method **only writes to
//! stdout**, NOT to the log file. This is intentional design:
//!
//! ```ignore
//! let logger = Logger::new(Colors::new()).with_log_file("app.log");
//!
//! // This writes to BOTH console AND file:
//! logger.info("This message goes everywhere");
//!
//! // This writes ONLY to console (via Write trait):
//! writeln!(logger, "This only goes to console").unwrap();
//! ```
//!
//! If you need file output, always use the Logger's methods (`info()`, `success()`,
//! etc.) rather than the `Write` trait. The `Write` trait implementation exists
//! for compatibility with code that expects a writer, but it's a console-only path.
//!
//! ## Using the `Loggable` Trait
//!
//! The `Loggable` trait provides a unified interface for logging that works with
//! both `Logger` (production) and `TestLogger` (testing):
//!
//! ```ignore
//! use ralph_workflow::logger::Loggable;
//!
//! fn process_logs<L: Loggable>(logger: &L) {
//!     logger.info("Starting process");
//!     logger.success("Process completed");
//!     logger.warn("Potential issue");
//!     logger.error("Critical error");
//! }
//! ```
//!
//! ## Logger → File → Extraction Flow
//!
//! The review and planning phases extract structured output from agent logs:
//!
//! 1. **Agent writes JSON events**: Agents emit `{"type": "result", "result": "..."}` events
//! 2. **Events written to log files**: Via direct file writes or Logger's file logging
//! 3. **Extraction reads log files**: `extract_last_result()` parses JSON from log files
//! 4. **Result content captured**: The orchestrator uses extracted content for ISSUES.md/PLAN.md
//!
//! ### Last Line Handling
//!
//! A key concern is whether the last line without a trailing newline is extracted.
//! The extraction uses `BufReader::lines()`, which **does** return the last line
//! even without a trailing newline (this is documented Rust stdlib behavior).
//!
//! Reference: <https://doc.rust-lang.org/std/io/struct.BufReader.html#method.lines>
//!
//! ### Testing Logger Output
//!
//! For testing, use `TestLogger` from this module:
//!
//! ```ignore
//! use ralph_workflow::logger::output::TestLogger;
//! use ralph_workflow::logger::Loggable;
//!
//! let logger = TestLogger::new();
//! logger.info("Test message");
//!
//! assert!(logger.has_log("Test message"));
//! assert!(logger.has_log("[INFO]"));
//! ```
//!
//! `TestLogger` follows the same pattern as `TestPrinter` with line buffering
//! and implements the same traits (`Printable`, `std::io::Write`, `Loggable`).

use super::{
    Colors, ARROW, BOX_BL, BOX_BR, BOX_H, BOX_TL, BOX_TR, BOX_V, CHECK, CROSS, INFO, WARN,
};
use crate::checkpoint::timestamp;
use crate::common::truncate_text;
use crate::config::Verbosity;
use crate::json_parser::printer::Printable;
use std::fs::{self, OpenOptions};
use std::io::{IsTerminal, Write};
use std::path::Path;

#[cfg(test)]
use std::cell::RefCell;

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
                let _ = file.flush();
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
            c.bold(),
            BOX_TL,
            BOX_H.to_string().repeat(width),
            BOX_TR,
            c.reset()
        );
        println!(
            "{}{}{}{}{}{}{}{}{}{}",
            color,
            c.bold(),
            BOX_V,
            " ".repeat(padding),
            c.white(),
            title,
            color,
            " ".repeat(width - padding - title_len),
            BOX_V,
            c.reset()
        );
        println!(
            "{}{}{}{}{}{}",
            color,
            c.bold(),
            BOX_BL,
            BOX_H.to_string().repeat(width),
            BOX_BR,
            c.reset()
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

// ===== Loggable Implementation =====

impl Loggable for Logger {
    fn log(&self, msg: &str) {
        self.log_to_file(msg);
    }

    fn info(&self, msg: &str) {
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
        self.log(&format!("[{}] [INFO] {msg}", timestamp()));
    }

    fn success(&self, msg: &str) {
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
        self.log(&format!("[{}] [OK] {msg}", timestamp()));
    }

    fn warn(&self, msg: &str) {
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
        self.log(&format!("[{}] [WARN] {msg}", timestamp()));
    }

    fn error(&self, msg: &str) {
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
        self.log(&format!("[{}] [ERROR] {msg}", timestamp()));
    }

    fn header(&self, title: &str, color_fn: fn(Colors) -> &'static str) {
        // Call the existing Logger::header method
        Self::header(self, title, color_fn);
    }
}

// ===== Printable and Write Implementations =====

impl std::io::Write for Logger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        // Write directly to stdout
        std::io::stdout().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        std::io::stdout().flush()
    }
}

impl Printable for Logger {
    fn is_terminal(&self) -> bool {
        std::io::stdout().is_terminal()
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

/// Trait for logger output destinations.
///
/// This trait allows loggers to write to different destinations
/// (console, files, test collectors) without hardcoding the specific destination.
/// This makes loggers testable by allowing output capture.
///
/// This trait mirrors the `Printable` trait pattern used for printers,
/// providing a unified interface for both production and test loggers.
pub trait Loggable {
    /// Write a log message to the sink.
    ///
    /// This is the core logging method that all loggers must implement.
    /// For `Logger`, this writes to the configured log file (if any).
    /// For `TestLogger`, this captures the message in memory for testing.
    fn log(&self, msg: &str);

    /// Log an informational message.
    ///
    /// Default implementation formats the message with [INFO] prefix
    /// and delegates to the `log` method.
    fn info(&self, msg: &str) {
        self.log(&format!("[INFO] {msg}"));
    }

    /// Log a success message.
    ///
    /// Default implementation formats the message with [OK] prefix
    /// and delegates to the `log` method.
    fn success(&self, msg: &str) {
        self.log(&format!("[OK] {msg}"));
    }

    /// Log a warning message.
    ///
    /// Default implementation formats the message with [WARN] prefix
    /// and delegates to the `log` method.
    fn warn(&self, msg: &str) {
        self.log(&format!("[WARN] {msg}"));
    }

    /// Log an error message.
    ///
    /// Default implementation formats the message with [ERROR] prefix
    /// and delegates to the `log` method.
    fn error(&self, msg: &str) {
        self.log(&format!("[ERROR] {msg}"));
    }

    /// Print a section header with box drawing.
    ///
    /// Default implementation does nothing (test loggers may not need headers).
    /// Production loggers override this to display styled headers.
    fn header(&self, _title: &str, _color_fn: fn(Colors) -> &'static str) {
        // Default: no-op for test loggers
    }
}

/// Test logger that captures log output for assertion.
///
/// This logger stores all log messages in memory for testing purposes.
/// It provides methods to retrieve and inspect the captured log output.
/// Uses line buffering similar to `TestPrinter` to handle partial writes.
#[cfg(test)]
#[derive(Debug, Default)]
pub struct TestLogger {
    /// Captured complete log lines.
    logs: RefCell<Vec<String>>,
    /// Buffer for incomplete lines (content without trailing newline).
    buffer: RefCell<String>,
}

#[cfg(test)]
impl TestLogger {
    /// Create a new test logger.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all captured log messages including partial buffered content.
    pub fn get_logs(&self) -> Vec<String> {
        let mut result = self.logs.borrow().clone();
        let buffer = self.buffer.borrow();
        if !buffer.is_empty() {
            result.push(buffer.clone());
        }
        result
    }

    /// Clear all captured log messages and buffered content.
    pub fn clear(&self) {
        self.logs.borrow_mut().clear();
        self.buffer.borrow_mut().clear();
    }

    /// Check if a specific message exists in the logs.
    pub fn has_log(&self, msg: &str) -> bool {
        self.get_logs().iter().any(|l| l.contains(msg))
    }

    /// Get the number of times a specific pattern appears in logs.
    pub fn count_pattern(&self, pattern: &str) -> usize {
        self.get_logs()
            .iter()
            .filter(|l| l.contains(pattern))
            .count()
    }
}

#[cfg(test)]
impl Loggable for TestLogger {
    fn log(&self, msg: &str) {
        self.logs.borrow_mut().push(msg.to_string());
    }

    fn info(&self, msg: &str) {
        self.log(&format!("[INFO] {msg}"));
    }

    fn success(&self, msg: &str) {
        self.log(&format!("[OK] {msg}"));
    }

    fn warn(&self, msg: &str) {
        self.log(&format!("[WARN] {msg}"));
    }

    fn error(&self, msg: &str) {
        self.log(&format!("[ERROR] {msg}"));
    }
}

#[cfg(test)]
impl Printable for TestLogger {
    fn is_terminal(&self) -> bool {
        // Test logger is never a terminal
        false
    }
}

#[cfg(test)]
impl std::io::Write for TestLogger {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        let s = std::str::from_utf8(buf)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::InvalidData, e))?;
        let mut buffer = self.buffer.borrow_mut();
        buffer.push_str(s);

        // Process complete lines (similar to TestPrinter)
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer.drain(..=newline_pos).collect::<String>();
            self.logs.borrow_mut().push(line);
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> std::io::Result<()> {
        // Flush any remaining buffer content to logs
        let mut buffer = self.buffer.borrow_mut();
        if !buffer.is_empty() {
            self.logs.borrow_mut().push(buffer.clone());
            buffer.clear();
        }
        Ok(())
    }
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

    #[test]
    fn test_logger_captures_output() {
        let logger = TestLogger::new();
        logger.log("Test message");
        assert!(logger.has_log("Test message"));
    }

    #[test]
    fn test_logger_get_logs() {
        let logger = TestLogger::new();
        logger.log("Message 1");
        logger.log("Message 2");
        let logs = logger.get_logs();
        assert_eq!(logs.len(), 2);
        assert_eq!(logs[0], "Message 1");
        assert_eq!(logs[1], "Message 2");
    }

    #[test]
    fn test_logger_clear() {
        let logger = TestLogger::new();
        logger.log("Before clear");
        assert!(!logger.get_logs().is_empty());
        logger.clear();
        assert!(logger.get_logs().is_empty());
    }

    #[test]
    fn test_logger_count_pattern() {
        let logger = TestLogger::new();
        logger.log("test message 1");
        logger.log("test message 2");
        logger.log("other message");
        assert_eq!(logger.count_pattern("test"), 2);
    }
}

// ===== Output Formatting Functions =====

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
