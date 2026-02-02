//! Test logger for capturing log output in tests.
//!
//! Provides `TestLogger` which implements `Loggable` and captures all log
//! messages in memory for assertion in tests.

use super::loggable::Loggable;
use crate::json_parser::printer::Printable;
use std::cell::RefCell;

/// Test logger that captures log output for assertion.
///
/// This logger stores all log messages in memory for testing purposes.
/// It provides methods to retrieve and inspect the captured log output.
/// Uses line buffering similar to `TestPrinter` to handle partial writes.
///
/// # Availability
///
/// `TestLogger` is available in test builds (`#[cfg(any(test, feature = "test-utils"))]`) and when the
/// `test-utils` feature is enabled (for integration tests). In production
/// binary builds with `--all-features`, the `test-utils` feature enables
/// this code but it's not used by the binary, which is expected behavior.
#[derive(Debug, Default)]
pub struct TestLogger {
    /// Captured complete log lines.
    logs: RefCell<Vec<String>>,
    /// Buffer for incomplete lines (content without trailing newline).
    buffer: RefCell<String>,
}

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

impl Printable for TestLogger {
    fn is_terminal(&self) -> bool {
        // Test logger is never a terminal
        false
    }
}

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
