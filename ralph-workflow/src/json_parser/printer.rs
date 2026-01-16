//! Printer abstraction for testable output.
//!
//! This module provides a trait-based abstraction for output destinations,
//! allowing parsers to write to stdout, stderr, or test collectors without
//! changing their core logic.

use std::cell::RefCell;
use std::io::{self, IsTerminal, Stdout};
use std::rc::Rc;

#[cfg(feature = "test-utils")]
use std::io::Stderr;

/// Trait for output destinations in parsers.
///
/// This trait allows parsers to write to different output destinations
/// (stdout, stderr, or test collectors) without hardcoding the specific
/// destination. This makes parsers testable by allowing output capture.
pub trait Printable: std::io::Write {
    /// Check if this printer is connected to a terminal.
    ///
    /// This is used to determine whether to use terminal-specific features
    /// like colors and carriage return-based updates.
    fn is_terminal(&self) -> bool;
}

/// Printer that writes to stdout.
#[derive(Debug)]
pub struct StdoutPrinter {
    stdout: Stdout,
    is_terminal: bool,
}

impl StdoutPrinter {
    /// Create a new stdout printer.
    pub fn new() -> Self {
        let is_terminal = std::io::stdout().is_terminal();
        Self {
            stdout: std::io::stdout(),
            is_terminal,
        }
    }
}

impl Default for StdoutPrinter {
    fn default() -> Self {
        Self::new()
    }
}

impl std::io::Write for StdoutPrinter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stdout.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stdout.flush()
    }
}

impl Printable for StdoutPrinter {
    fn is_terminal(&self) -> bool {
        self.is_terminal
    }
}

/// Printer that writes to stderr.
#[derive(Debug)]
#[cfg(feature = "test-utils")]
pub struct StderrPrinter {
    stderr: Stderr,
    is_terminal: bool,
}

#[cfg(feature = "test-utils")]
impl StderrPrinter {
    /// Create a new stderr printer.
    pub fn new() -> Self {
        let is_terminal = std::io::stderr().is_terminal();
        Self {
            stderr: std::io::stderr(),
            is_terminal,
        }
    }
}

#[cfg(feature = "test-utils")]
impl Default for StderrPrinter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(feature = "test-utils")]
impl std::io::Write for StderrPrinter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stderr.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stderr.flush()
    }
}

#[cfg(feature = "test-utils")]
impl Printable for StderrPrinter {
    fn is_terminal(&self) -> bool {
        self.is_terminal
    }
}

/// Test printer that captures output for assertion.
///
/// This printer stores all output in memory for testing purposes.
/// It provides methods to retrieve and inspect the captured output.
#[derive(Debug, Default)]
#[cfg(feature = "test-utils")]
pub struct TestPrinter {
    /// Captured output lines.
    output: RefCell<Vec<String>>,
    /// Buffer for incomplete lines.
    buffer: RefCell<String>,
}

#[cfg(feature = "test-utils")]
impl TestPrinter {
    /// Create a new test printer.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get all captured output as a single string.
    pub fn get_output(&self) -> String {
        let mut result = self.buffer.borrow().clone();
        for line in self.output.borrow().iter() {
            result.push_str(line);
        }
        result
    }

    /// Get captured output lines.
    pub fn get_lines(&self) -> Vec<String> {
        let mut result: Vec<String> = self.output.borrow().clone();
        let buffer = self.buffer.borrow();
        if !buffer.is_empty() {
            result.push(buffer.clone());
        }
        result
    }

    /// Clear all captured output.
    pub fn clear(&self) {
        self.output.borrow_mut().clear();
        self.buffer.borrow_mut().clear();
    }

    /// Check if a specific line exists in the output.
    pub fn has_line(&self, line: &str) -> bool {
        self.get_lines().iter().any(|l| l.contains(line))
    }

    /// Get the number of times a specific pattern appears in output.
    pub fn count_pattern(&self, pattern: &str) -> usize {
        self.get_lines()
            .iter()
            .filter(|l| l.contains(pattern))
            .count()
    }

    /// Check if there are duplicate consecutive lines in output.
    pub fn has_duplicate_consecutive_lines(&self) -> bool {
        let lines = self.get_lines();
        for i in 1..lines.len() {
            if lines[i] == lines[i - 1] && !lines[i].is_empty() {
                return true;
            }
        }
        false
    }

    /// Find and return all duplicate consecutive lines.
    pub fn find_duplicate_consecutive_lines(&self) -> Vec<(usize, String)> {
        let mut duplicates = Vec::new();
        let lines = self.get_lines();
        for i in 1..lines.len() {
            if lines[i] == lines[i - 1] && !lines[i].is_empty() {
                duplicates.push((i - 1, lines[i - 1].clone()));
            }
        }
        duplicates
    }

    /// Count occurrences of a prefix pattern in output lines.
    ///
    /// Useful for detecting duplicate message prefixes, tool blocks, etc.
    pub fn count_prefix_occurrences(&self, prefix: &str) -> usize {
        self.get_lines()
            .iter()
            .filter(|l| l.trim().starts_with(prefix))
            .count()
    }

    /// Find all lines that match a pattern.
    ///
    /// Returns the line numbers and content for all matching lines.
    pub fn find_lines_with_pattern(&self, pattern: &str) -> Vec<(usize, String)> {
        self.get_lines()
            .iter()
            .enumerate()
            .filter(|(_, l)| l.contains(pattern))
            .map(|(i, l)| (i, l.clone()))
            .collect()
    }

    /// Get statistics about the output.
    ///
    /// Returns a tuple of (line_count, char_count).
    pub fn get_stats(&self) -> (usize, usize) {
        let lines = self.get_lines();
        let char_count: usize = lines.iter().map(String::len).sum();
        (lines.len(), char_count)
    }
}

#[cfg(feature = "test-utils")]
impl std::io::Write for TestPrinter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s =
            std::str::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;
        let mut buffer = self.buffer.borrow_mut();
        buffer.push_str(s);

        // Process complete lines
        while let Some(newline_pos) = buffer.find('\n') {
            let line = buffer.drain(..=newline_pos).collect::<String>();
            self.output.borrow_mut().push(line);
        }

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // Flush any remaining buffer content
        let mut buffer = self.buffer.borrow_mut();
        if !buffer.is_empty() {
            self.output.borrow_mut().push(buffer.clone());
            buffer.clear();
        }
        Ok(())
    }
}

#[cfg(feature = "test-utils")]
impl Printable for TestPrinter {
    fn is_terminal(&self) -> bool {
        // Test printer is never a terminal
        false
    }
}

/// Shared printer reference for use in parsers.
///
/// This type alias represents a shared, mutable reference to a printer
/// that can be used across parser methods.
pub type SharedPrinter = Rc<RefCell<dyn Printable>>;

/// Create a shared stdout printer.
pub fn shared_stdout() -> SharedPrinter {
    Rc::new(RefCell::new(StdoutPrinter::new()))
}

/// Create a shared stderr printer.
#[cfg(feature = "test-utils")]
pub fn shared_stderr() -> SharedPrinter {
    Rc::new(RefCell::new(StderrPrinter::new()))
}

/// Create a shared test printer.
#[cfg(feature = "test-utils")]
pub fn shared_test() -> SharedPrinter {
    Rc::new(RefCell::new(TestPrinter::new()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_stdout_printer() {
        let mut printer = StdoutPrinter::new();
        // Just ensure it compiles and works
        let result = printer.write_all(b"test\n");
        assert!(result.is_ok());
        assert!(printer.flush().is_ok());

        // Verify is_terminal() method is accessible
        let _is_term = printer.is_terminal();
    }

    #[test]
    fn test_printable_trait_is_terminal() {
        let printer = StdoutPrinter::new();
        // Test that the Printable trait's is_terminal method works
        let _should_use_colors = printer.is_terminal();
    }

    #[test]
    fn test_stderr_printer() {
        let mut printer = StderrPrinter::new();
        // Just ensure it compiles and works
        let result = printer.write_all(b"test\n");
        assert!(result.is_ok());
        assert!(printer.flush().is_ok());
    }

    #[test]
    fn test_printer_captures_output() {
        let mut printer = TestPrinter::new();

        printer
            .write_all(b"Hello World\n")
            .expect("Failed to write");
        printer.flush().expect("Failed to flush");

        let output = printer.get_output();
        assert!(output.contains("Hello World"));
    }

    #[test]
    fn test_printer_get_lines() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"Line 1\nLine 2\n").unwrap();
        printer.flush().unwrap();

        let lines = printer.get_lines();
        assert_eq!(lines.len(), 2);
        assert!(lines[0].contains("Line 1"));
        assert!(lines[1].contains("Line 2"));
    }

    #[test]
    fn test_printer_clear() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"Before\n").unwrap();
        printer.flush().unwrap();

        assert!(!printer.get_output().is_empty());

        printer.clear();
        assert!(printer.get_output().is_empty());
    }

    #[test]
    fn test_printer_has_line() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"Hello World\n").unwrap();
        printer.flush().unwrap();

        assert!(printer.has_line("Hello"));
        assert!(printer.has_line("World"));
        assert!(!printer.has_line("Goodbye"));
    }

    #[test]
    fn test_printer_count_pattern() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"test\nmore test\ntest again\n").unwrap();
        printer.flush().unwrap();

        assert_eq!(printer.count_pattern("test"), 3);
    }

    #[test]
    fn test_printer_detects_duplicates() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"Line 1\nLine 1\nLine 2\n").unwrap();
        printer.flush().unwrap();

        assert!(printer.has_duplicate_consecutive_lines());
    }

    #[test]
    fn test_printer_finds_duplicates() {
        let mut printer = TestPrinter::new();

        printer
            .write_all(b"Line 1\nLine 1\nLine 2\nLine 3\nLine 3\n")
            .unwrap();
        printer.flush().unwrap();

        let duplicates = printer.find_duplicate_consecutive_lines();
        assert_eq!(duplicates.len(), 2);
        assert_eq!(duplicates[0].0, 0); // First duplicate at line 0-1
        assert_eq!(duplicates[0].1, "Line 1\n");
        assert_eq!(duplicates[1].0, 3); // Second duplicate at line 3-4
        assert_eq!(duplicates[1].1, "Line 3\n");
    }

    #[test]
    fn test_printer_no_false_positives() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"Line 1\nLine 2\nLine 3\n").unwrap();
        printer.flush().unwrap();

        assert!(!printer.has_duplicate_consecutive_lines());
    }

    #[test]
    fn test_printer_buffer_handling() {
        let mut printer = TestPrinter::new();

        // Write without newline - buffer should hold it
        printer.write_all(b"Partial").unwrap();

        // Without flush, content is in buffer but accessible via get_output/get_lines
        // The TestPrinter stores partial content in buffer which is included in get_output
        assert!(printer.get_output().contains("Partial"));

        // Add newline to complete the line
        printer.write_all(b" content\n").unwrap();
        printer.flush().unwrap();

        // Now should have the complete content
        assert!(printer.has_line("Partial content"));

        // Verify the complete output
        let output = printer.get_output();
        assert!(output.contains("Partial content\n"));
    }
}
