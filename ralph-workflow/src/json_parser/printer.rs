//! Printer abstraction for testable output.
//!
//! This module provides a trait-based abstraction for output destinations,
//! allowing parsers to write to stdout, stderr, or test collectors without
//! changing their core logic.

use std::cell::RefCell;
use std::io::{self, IsTerminal, Stdout};
use std::rc::Rc;

#[cfg(any(test, feature = "test-utils"))]
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
#[cfg(any(test, feature = "test-utils"))]
pub struct StderrPrinter {
    stderr: Stderr,
    is_terminal: bool,
}

#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
impl Default for StderrPrinter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl std::io::Write for StderrPrinter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.stderr.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        self.stderr.flush()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Printable for StderrPrinter {
    fn is_terminal(&self) -> bool {
        self.is_terminal
    }
}

/// Test printer that captures output for assertion.
///
/// This printer stores all output in memory for testing purposes.
/// It provides methods to retrieve and inspect the captured output.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Default)]
pub struct TestPrinter {
    /// Captured output lines.
    output: RefCell<Vec<String>>,
    /// Buffer for incomplete lines.
    buffer: RefCell<String>,
}

#[cfg(any(test, feature = "test-utils"))]
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

    /// Get statistics about the output.
    ///
    /// Returns a tuple of (`line_count`, `char_count`).
    pub fn get_stats(&self) -> (usize, usize) {
        let lines = self.get_lines();
        let char_count: usize = lines.iter().map(String::len).sum();
        (lines.len(), char_count)
    }
}

#[cfg(any(test, feature = "test-utils"))]
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

#[cfg(any(test, feature = "test-utils"))]
impl Printable for TestPrinter {
    fn is_terminal(&self) -> bool {
        // Test printer is never a terminal
        false
    }
}

/// Record of a single `write()` call for streaming analysis.
///
/// Captures the content and timestamp of each write operation,
/// allowing tests to verify incremental streaming behavior.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone)]
pub struct WriteCall {
    /// The content written in this call.
    pub content: String,
    /// Timestamp when write occurred.
    pub timestamp: std::time::Instant,
}

/// Record of a flush() call with metadata.
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug, Clone)]
pub struct FlushCall {
    /// Index of the last write before this flush (None if no writes yet).
    pub last_write_index: Option<usize>,
    /// Timestamp when flush occurred.
    pub timestamp: std::time::Instant,
}

/// Test printer that captures EVERY `write()` call for streaming verification.
///
/// Unlike [`TestPrinter`] which processes complete lines, this tracks:
/// - Each individual `write()` call as a separate record
/// - Each `flush()` call for verifying real-time output behavior
/// - Content progression over time
/// - Timing between writes for streaming analysis
///
/// Use this to verify that streaming produces incremental output
/// (multiple small writes) rather than batched output (one large write).
///
/// # Example
///
/// ```ignore
/// use ralph_workflow::json_parser::printer::{StreamingTestPrinter, Printable};
/// use std::io::Write;
///
/// let mut printer = StreamingTestPrinter::new();
/// printer.write_all(b"Hello").unwrap();
/// printer.flush().unwrap();
/// printer.write_all(b" World").unwrap();
/// printer.flush().unwrap();
///
/// assert_eq!(printer.write_count(), 2);
/// assert_eq!(printer.flush_count(), 2);
/// assert!(printer.get_full_output().contains("Hello World"));
/// ```
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug)]
pub struct StreamingTestPrinter {
    /// Each individual write() call recorded.
    write_calls: RefCell<Vec<WriteCall>>,
    /// Each flush() call recorded.
    flush_calls: RefCell<Vec<FlushCall>>,
    /// Simulated terminal status for testing different terminal modes.
    simulated_is_terminal: bool,
}

#[cfg(any(test, feature = "test-utils"))]
impl StreamingTestPrinter {
    /// Create a new streaming test printer (simulates non-terminal).
    pub fn new() -> Self {
        Self {
            write_calls: RefCell::new(Vec::new()),
            flush_calls: RefCell::new(Vec::new()),
            simulated_is_terminal: false,
        }
    }

    /// Create a new streaming test printer with specified terminal simulation.
    ///
    /// # Arguments
    /// * `is_terminal` - Whether to simulate being connected to a terminal
    pub fn new_with_terminal(is_terminal: bool) -> Self {
        Self {
            write_calls: RefCell::new(Vec::new()),
            flush_calls: RefCell::new(Vec::new()),
            simulated_is_terminal: is_terminal,
        }
    }

    /// Get all write calls for inspection.
    pub fn get_write_calls(&self) -> Vec<WriteCall> {
        self.write_calls.borrow().clone()
    }

    /// Get the number of write() calls made.
    pub fn write_count(&self) -> usize {
        self.write_calls.borrow().len()
    }

    /// Get the full output (all writes concatenated).
    pub fn get_full_output(&self) -> String {
        self.write_calls
            .borrow()
            .iter()
            .map(|w| w.content.clone())
            .collect()
    }

    /// Get the content at a specific write index.
    pub fn get_content_at_write(&self, index: usize) -> Option<String> {
        self.write_calls
            .borrow()
            .get(index)
            .map(|w| w.content.clone())
    }

    /// Verify that multiple incremental writes occurred.
    ///
    /// # Arguments
    /// * `min_expected` - Minimum number of writes expected
    ///
    /// # Returns
    /// `Ok(())` if at least `min_expected` writes occurred, `Err` with details otherwise.
    pub fn verify_incremental_writes(&self, min_expected: usize) -> Result<(), String> {
        let count = self.write_count();
        if count >= min_expected {
            Ok(())
        } else {
            Err(format!(
                "Expected at least {} incremental writes, but only {} occurred. \
                 This suggests output is batched rather than streamed.",
                min_expected, count
            ))
        }
    }

    /// Check if the output contains a specific ANSI escape sequence.
    pub fn contains_escape_sequence(&self, seq: &str) -> bool {
        self.get_full_output().contains(seq)
    }

    /// Check if any ANSI escape sequences are present in the output.
    pub fn has_any_escape_sequences(&self) -> bool {
        self.get_full_output().contains('\x1b')
    }

    /// Strip ANSI escape sequences from a string.
    ///
    /// Uses a simple state machine approach to remove all ANSI codes.
    pub fn strip_ansi(s: &str) -> String {
        let mut result = String::with_capacity(s.len());
        let mut chars = s.chars().peekable();

        while let Some(c) = chars.next() {
            if c == '\x1b' {
                // Skip escape sequence: ESC [ ... letter
                if chars.peek() == Some(&'[') {
                    chars.next(); // consume '['
                                  // Skip until we hit a letter (the terminator)
                    while let Some(&next) = chars.peek() {
                        chars.next();
                        if next.is_ascii_alphabetic() {
                            break;
                        }
                    }
                }
            } else {
                result.push(c);
            }
        }
        result
    }

    /// Get the content progression across all writes (ANSI stripped).
    ///
    /// Returns a vector of accumulated content at each write point,
    /// useful for verifying that content grows incrementally.
    pub fn get_content_progression(&self) -> Vec<String> {
        let mut accumulated = String::new();
        let mut progression = Vec::new();

        for call in self.write_calls.borrow().iter() {
            accumulated.push_str(&call.content);
            // Strip ANSI and control characters for content comparison
            let clean = Self::strip_ansi(&accumulated)
                .replace('\r', "")
                .replace('\n', " ")
                .trim()
                .to_string();
            if !clean.is_empty() {
                progression.push(clean);
            }
        }
        progression
    }

    /// Clear all recorded write and flush calls.
    pub fn clear(&self) {
        self.write_calls.borrow_mut().clear();
        self.flush_calls.borrow_mut().clear();
    }

    /// Get all flush calls for inspection.
    pub fn get_flush_calls(&self) -> Vec<FlushCall> {
        self.flush_calls.borrow().clone()
    }

    /// Get the number of flush() calls made.
    pub fn flush_count(&self) -> usize {
        self.flush_calls.borrow().len()
    }

    /// Verify that flush was called after writes occurred.
    ///
    /// This is the critical test for real-time streaming behavior:
    /// if flush isn't called, output buffers and appears "all at once".
    ///
    /// # Returns
    /// `Ok(())` if at least one flush occurred after writes, `Err` with details otherwise.
    pub fn verify_flush_after_writes(&self) -> Result<(), String> {
        let writes = self.write_calls.borrow();
        let flushes = self.flush_calls.borrow();

        if writes.is_empty() {
            return Err("No writes occurred".to_string());
        }

        if flushes.is_empty() {
            return Err(format!(
                "No flush() calls occurred after {} write(s). \
                 This means output is buffered and will appear 'all at once' \
                 instead of streaming incrementally.",
                writes.len()
            ));
        }

        Ok(())
    }

    /// Verify that flush was called at least `min_expected` times.
    ///
    /// For true streaming, flush should be called after each delta event
    /// to push content to the user's terminal immediately.
    pub fn verify_flush_count(&self, min_expected: usize) -> Result<(), String> {
        let count = self.flush_count();
        if count >= min_expected {
            Ok(())
        } else {
            Err(format!(
                "Expected at least {} flush() calls, but only {} occurred. \
                 This suggests output is not being flushed frequently enough for streaming.",
                min_expected, count
            ))
        }
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for StreamingTestPrinter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl std::io::Write for StreamingTestPrinter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let content =
            std::str::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        self.write_calls.borrow_mut().push(WriteCall {
            content: content.to_string(),
            timestamp: std::time::Instant::now(),
        });

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        let last_write_index = if self.write_calls.borrow().is_empty() {
            None
        } else {
            Some(self.write_calls.borrow().len() - 1)
        };
        self.flush_calls.borrow_mut().push(FlushCall {
            last_write_index,
            timestamp: std::time::Instant::now(),
        });
        Ok(())
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Printable for StreamingTestPrinter {
    fn is_terminal(&self) -> bool {
        self.simulated_is_terminal
    }
}

/// A virtual terminal that simulates real terminal behavior for testing.
///
/// Unlike [`TestPrinter`] which just collects raw output, this accurately simulates
/// how a real terminal renders text, including:
///
/// - **Cursor positioning**: Tracks row and column
/// - **Carriage return (`\r`)**: Moves cursor to column 0 (doesn't erase)
/// - **Newline (`\n`)**: Moves cursor to next row, column 0
/// - **ANSI clear line (`\x1b[2K`)**: Erases entire current line
/// - **ANSI cursor up (`\x1b[1A`)**: Moves cursor up one row
/// - **ANSI cursor down (`\x1b[1B`)**: Moves cursor down one row
/// - **Text overwriting**: Writing after `\r` replaces previous content
///
/// This allows tests to verify what the user actually SEES, not just what was written.
///
/// # Example
///
/// ```ignore
/// use ralph_workflow::json_parser::printer::VirtualTerminal;
/// use std::io::Write;
///
/// let mut term = VirtualTerminal::new();
/// write!(term, "Hello").unwrap();
/// write!(term, "\rWorld").unwrap();  // Overwrites "Hello"
/// assert_eq!(term.get_visible_output(), "World");
/// ```
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug)]
pub struct VirtualTerminal {
    /// The terminal buffer - each element is a line (row)
    lines: RefCell<Vec<String>>,
    /// Current cursor row (0-indexed)
    cursor_row: RefCell<usize>,
    /// Current cursor column (0-indexed)
    cursor_col: RefCell<usize>,
    /// Whether to simulate terminal mode (affects is_terminal())
    simulated_is_terminal: bool,
    /// Raw write history for debugging
    write_history: RefCell<Vec<String>>,
}

#[cfg(any(test, feature = "test-utils"))]
impl VirtualTerminal {
    /// Create a new virtual terminal (simulates being a TTY by default).
    pub fn new() -> Self {
        Self {
            lines: RefCell::new(vec![String::new()]),
            cursor_row: RefCell::new(0),
            cursor_col: RefCell::new(0),
            simulated_is_terminal: true,
            write_history: RefCell::new(Vec::new()),
        }
    }

    /// Create a new virtual terminal with specified terminal simulation.
    pub fn new_with_terminal(is_terminal: bool) -> Self {
        Self {
            lines: RefCell::new(vec![String::new()]),
            cursor_row: RefCell::new(0),
            cursor_col: RefCell::new(0),
            simulated_is_terminal: is_terminal,
            write_history: RefCell::new(Vec::new()),
        }
    }

    /// Get the visible output as the user would see it.
    ///
    /// This returns the final rendered state of the terminal, with all
    /// ANSI sequences processed and overwrites applied.
    pub fn get_visible_output(&self) -> String {
        let lines = self.lines.borrow();
        // Join non-empty lines, trimming trailing whitespace from each
        lines
            .iter()
            .map(|line| line.trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get visible lines (non-empty lines only).
    pub fn get_visible_lines(&self) -> Vec<String> {
        self.lines
            .borrow()
            .iter()
            .map(|line| line.trim_end().to_string())
            .filter(|line| !line.is_empty())
            .collect()
    }

    /// Get the raw write history for debugging.
    pub fn get_write_history(&self) -> Vec<String> {
        self.write_history.borrow().clone()
    }

    /// Get current cursor position as (row, col).
    pub fn cursor_position(&self) -> (usize, usize) {
        (*self.cursor_row.borrow(), *self.cursor_col.borrow())
    }

    /// Clear the terminal.
    pub fn clear(&self) {
        self.lines.borrow_mut().clear();
        self.lines.borrow_mut().push(String::new());
        *self.cursor_row.borrow_mut() = 0;
        *self.cursor_col.borrow_mut() = 0;
        self.write_history.borrow_mut().clear();
    }

    /// Ensure the current row exists in the buffer.
    fn ensure_row_exists(&self) {
        let row = *self.cursor_row.borrow();
        let mut lines = self.lines.borrow_mut();
        while lines.len() <= row {
            lines.push(String::new());
        }
    }

    /// Write a character at the current cursor position.
    /// Write a string of regular characters at the current cursor position.
    /// This is more efficient than write_char for multiple characters.
    fn write_str(&self, s: &str) {
        if s.is_empty() {
            return;
        }

        self.ensure_row_exists();
        let row = *self.cursor_row.borrow();
        let col = *self.cursor_col.borrow();
        let mut lines = self.lines.borrow_mut();
        let line = &mut lines[row];

        // Extend the line with spaces if needed
        while line.chars().count() < col {
            line.push(' ');
        }

        // Build new line: prefix + new content + suffix
        let prefix: String = line.chars().take(col).collect();
        let suffix: String = line.chars().skip(col + s.chars().count()).collect();
        *line = format!("{}{}{}", prefix, s, suffix);

        // Move cursor right
        *self.cursor_col.borrow_mut() = col + s.chars().count();
    }

    /// Clear the current line.
    fn clear_line(&self) {
        self.ensure_row_exists();
        let row = *self.cursor_row.borrow();
        let mut lines = self.lines.borrow_mut();
        lines[row].clear();
        // Note: cursor position is NOT changed by clear line
    }

    /// Move cursor up n rows.
    fn cursor_up(&self, n: usize) {
        let mut row = self.cursor_row.borrow_mut();
        *row = row.saturating_sub(n);
    }

    /// Move cursor down n rows.
    fn cursor_down(&self, n: usize) {
        *self.cursor_row.borrow_mut() += n;
        self.ensure_row_exists();
    }

    /// Process a string, interpreting control characters and ANSI sequences.
    fn process_string(&self, s: &str) {
        let mut chars = s.chars().peekable();
        let mut text_buffer = String::new();

        // Flush accumulated text to the terminal
        let flush_text = |term: &Self, buf: &mut String| {
            if !buf.is_empty() {
                term.write_str(buf);
                buf.clear();
            }
        };

        while let Some(c) = chars.next() {
            match c {
                '\r' => {
                    flush_text(self, &mut text_buffer);
                    // Carriage return: move to column 0
                    *self.cursor_col.borrow_mut() = 0;
                }
                '\n' => {
                    flush_text(self, &mut text_buffer);
                    // Newline: move to next row, column 0
                    *self.cursor_row.borrow_mut() += 1;
                    *self.cursor_col.borrow_mut() = 0;
                    self.ensure_row_exists();
                }
                '\x1b' => {
                    flush_text(self, &mut text_buffer);
                    // ANSI escape sequence
                    if chars.peek() == Some(&'[') {
                        chars.next(); // consume '['

                        // Parse the numeric parameter (if any)
                        let mut param = String::new();
                        while let Some(&c) = chars.peek() {
                            if c.is_ascii_digit() {
                                param.push(c);
                                chars.next();
                            } else {
                                break;
                            }
                        }

                        // Get the command character
                        if let Some(cmd) = chars.next() {
                            let n: usize = param.parse().unwrap_or(1);
                            match cmd {
                                'A' => self.cursor_up(n),   // Cursor up
                                'B' => self.cursor_down(n), // Cursor down
                                'K' => {
                                    // Erase in line
                                    // \x1b[K or \x1b[0K - erase from cursor to end
                                    // \x1b[1K - erase from start to cursor
                                    // \x1b[2K - erase entire line
                                    let mode: usize = param.parse().unwrap_or(0);
                                    if mode == 2 {
                                        self.clear_line();
                                    }
                                    // For now, we only implement mode 2 (full line clear)
                                    // which is what the streaming code uses
                                }
                                'm' => {
                                    // SGR (Select Graphic Rendition) - colors/styles
                                    // We ignore these as they don't affect text content
                                }
                                _ => {
                                    // Unknown command, ignore
                                }
                            }
                        }
                    }
                }
                _ => {
                    // Regular character: buffer it for batch writing
                    text_buffer.push(c);
                }
            }
        }

        // Flush any remaining text
        flush_text(self, &mut text_buffer);
    }

    /// Check for duplicate visible lines (useful for detecting rendering bugs).
    pub fn has_duplicate_lines(&self) -> bool {
        let lines = self.get_visible_lines();
        for i in 1..lines.len() {
            if !lines[i].is_empty() && lines[i] == lines[i - 1] {
                return true;
            }
        }
        false
    }

    /// Count occurrences of a pattern in the visible output.
    pub fn count_visible_pattern(&self, pattern: &str) -> usize {
        self.get_visible_output().matches(pattern).count()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Default for VirtualTerminal {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl std::io::Write for VirtualTerminal {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        let s =
            std::str::from_utf8(buf).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))?;

        // Record raw write for debugging
        self.write_history.borrow_mut().push(s.to_string());

        // Process the string through the terminal emulator
        self.process_string(s);

        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        // Virtual terminal doesn't need flushing - content is immediately available
        Ok(())
    }
}

#[cfg(any(test, feature = "test-utils"))]
impl Printable for VirtualTerminal {
    fn is_terminal(&self) -> bool {
        self.simulated_is_terminal
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

    #[cfg(test)]
    #[test]
    fn test_printable_trait_is_terminal() {
        let printer = StdoutPrinter::new();
        // Test that the Printable trait's is_terminal method works
        let _should_use_colors = printer.is_terminal();
    }

    #[test]
    #[cfg(any(test, feature = "test-utils"))]
    fn test_stderr_printer() {
        let mut printer = StderrPrinter::new();
        // Just ensure it compiles and works
        let result = printer.write_all(b"test\n");
        assert!(result.is_ok());
        assert!(printer.flush().is_ok());
    }

    #[test]
    #[cfg(any(test, feature = "test-utils"))]
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
    #[cfg(any(test, feature = "test-utils"))]
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
    #[cfg(any(test, feature = "test-utils"))]
    fn test_printer_clear() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"Before\n").unwrap();
        printer.flush().unwrap();

        assert!(!printer.get_output().is_empty());

        printer.clear();
        assert!(printer.get_output().is_empty());
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_printer_has_line() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"Hello World\n").unwrap();
        printer.flush().unwrap();

        assert!(printer.has_line("Hello"));
        assert!(printer.has_line("World"));
        assert!(!printer.has_line("Goodbye"));
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_printer_count_pattern() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"test\nmore test\ntest again\n").unwrap();
        printer.flush().unwrap();

        assert_eq!(printer.count_pattern("test"), 3);
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_printer_detects_duplicates() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"Line 1\nLine 1\nLine 2\n").unwrap();
        printer.flush().unwrap();

        assert!(printer.has_duplicate_consecutive_lines());
    }

    #[cfg(any(test, feature = "test-utils"))]
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

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_printer_no_false_positives() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"Line 1\nLine 2\nLine 3\n").unwrap();
        printer.flush().unwrap();

        assert!(!printer.has_duplicate_consecutive_lines());
    }

    #[cfg(any(test, feature = "test-utils"))]
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

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_printer_get_stats() {
        let mut printer = TestPrinter::new();

        printer.write_all(b"Line 1\nLine 2\n").unwrap();
        printer.flush().unwrap();

        let (line_count, char_count) = printer.get_stats();
        assert_eq!(line_count, 2);
        assert!(char_count > 0);
    }

    #[test]
    fn test_shared_stdout() {
        let printer = shared_stdout();
        // Verify the function creates a valid SharedPrinter
        let _borrowed = printer.borrow();
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_streaming_printer_captures_individual_writes() {
        let mut printer = StreamingTestPrinter::new();

        printer.write_all(b"Hello").unwrap();
        printer.write_all(b" ").unwrap();
        printer.write_all(b"World").unwrap();

        assert_eq!(printer.write_count(), 3);
        assert_eq!(printer.get_full_output(), "Hello World");
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_streaming_printer_verify_incremental_writes() {
        let mut printer = StreamingTestPrinter::new();

        printer.write_all(b"A").unwrap();
        printer.write_all(b"B").unwrap();
        printer.write_all(b"C").unwrap();
        printer.write_all(b"D").unwrap();

        assert!(printer.verify_incremental_writes(4).is_ok());
        assert!(printer.verify_incremental_writes(5).is_err());
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_streaming_printer_detects_escape_sequences() {
        let mut printer = StreamingTestPrinter::new();

        printer.write_all(b"Normal text").unwrap();
        assert!(!printer.has_any_escape_sequences());

        printer.clear();
        printer.write_all(b"\x1b[2K\rUpdated").unwrap();
        assert!(printer.has_any_escape_sequences());
        assert!(printer.contains_escape_sequence("\x1b[2K"));
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_streaming_printer_strip_ansi() {
        let input = "\x1b[2K\r\x1b[1mBold\x1b[0m text\x1b[1A";
        let stripped = StreamingTestPrinter::strip_ansi(input);
        assert_eq!(stripped, "\rBold text");
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_streaming_printer_content_progression() {
        let mut printer = StreamingTestPrinter::new();

        printer.write_all(b"[agent] Hello\n").unwrap();
        printer
            .write_all(b"\x1b[2K\r[agent] Hello World\n")
            .unwrap();

        let progression = printer.get_content_progression();
        assert!(progression.len() >= 1);
        // Later entries should contain more content
        if progression.len() >= 2 {
            assert!(progression[1].len() >= progression[0].len());
        }
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_streaming_printer_terminal_simulation() {
        let printer_non_term = StreamingTestPrinter::new();
        assert!(!printer_non_term.is_terminal());

        let printer_term = StreamingTestPrinter::new_with_terminal(true);
        assert!(printer_term.is_terminal());
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_streaming_printer_get_content_at_write() {
        let mut printer = StreamingTestPrinter::new();

        printer.write_all(b"First").unwrap();
        printer.write_all(b"Second").unwrap();
        printer.write_all(b"Third").unwrap();

        assert_eq!(printer.get_content_at_write(0), Some("First".to_string()));
        assert_eq!(printer.get_content_at_write(1), Some("Second".to_string()));
        assert_eq!(printer.get_content_at_write(2), Some("Third".to_string()));
        assert_eq!(printer.get_content_at_write(3), None);
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_streaming_printer_clear() {
        let mut printer = StreamingTestPrinter::new();

        printer.write_all(b"Some content").unwrap();
        assert_eq!(printer.write_count(), 1);

        printer.clear();
        assert_eq!(printer.write_count(), 0);
        assert!(printer.get_full_output().is_empty());
    }

    // =========================================================================
    // VirtualTerminal Tests
    // =========================================================================

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_simple_text() {
        let mut term = VirtualTerminal::new();
        write!(term, "Hello World").unwrap();
        assert_eq!(term.get_visible_output(), "Hello World");
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_newlines() {
        let mut term = VirtualTerminal::new();
        write!(term, "Line 1\nLine 2\nLine 3").unwrap();
        assert_eq!(term.get_visible_output(), "Line 1\nLine 2\nLine 3");
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_carriage_return_overwrites() {
        let mut term = VirtualTerminal::new();
        // Write "Hello", then \r moves to start, then "World" overwrites
        write!(term, "Hello\rWorld").unwrap();
        assert_eq!(term.get_visible_output(), "World");
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_carriage_return_partial_overwrite() {
        let mut term = VirtualTerminal::new();
        // "Hello World" then \r moves to start, "Hi" overwrites first 2 chars
        write!(term, "Hello World\rHi").unwrap();
        // Result: "Hillo World" (only first 2 chars overwritten)
        assert_eq!(term.get_visible_output(), "Hillo World");
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_ansi_clear_line() {
        let mut term = VirtualTerminal::new();
        // Write text, clear line, write new text
        write!(term, "Old text\x1b[2K\rNew text").unwrap();
        assert_eq!(term.get_visible_output(), "New text");
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_cursor_up() {
        let mut term = VirtualTerminal::new();
        // Line 1, newline, Line 2, cursor up, overwrite Line 1
        write!(term, "Line 1\nLine 2\x1b[1A\rOverwritten").unwrap();
        let lines = term.get_visible_lines();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0], "Overwritten");
        assert_eq!(lines[1], "Line 2");
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_cursor_down() {
        let mut term = VirtualTerminal::new();
        // Write on row 0, move down, write on row 1
        write!(term, "Row 0\x1b[1B\rRow 1").unwrap();
        let output = term.get_visible_output();
        assert!(output.contains("Row 0"));
        assert!(output.contains("Row 1"));
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_streaming_simulation() {
        // Simulate the actual streaming pattern used by parsers:
        // 1. Write "[agent] Hello" + newline + cursor up
        // 2. Clear line + carriage return + write "[agent] Hello World" + newline + cursor up
        // 3. Cursor down at end
        let mut term = VirtualTerminal::new();

        // First delta
        write!(term, "[agent] Hello\n\x1b[1A").unwrap();
        assert_eq!(term.get_visible_lines(), vec!["[agent] Hello"]);

        // Second delta (updates in place)
        write!(term, "\x1b[2K\r[agent] Hello World\n\x1b[1A").unwrap();
        assert_eq!(term.get_visible_lines(), vec!["[agent] Hello World"]);

        // Completion (cursor down)
        write!(term, "\x1b[1B\n").unwrap();
        // Should still show the final content
        assert!(term.get_visible_output().contains("[agent] Hello World"));
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_no_duplicate_lines_in_streaming() {
        let mut term = VirtualTerminal::new();

        // Simulate streaming with in-place updates
        write!(term, "[agent] A\n\x1b[1A").unwrap();
        write!(term, "\x1b[2K\r[agent] AB\n\x1b[1A").unwrap();
        write!(term, "\x1b[2K\r[agent] ABC\n\x1b[1A").unwrap();
        write!(term, "\x1b[1B\n").unwrap();

        // Should NOT have duplicate lines
        assert!(
            !term.has_duplicate_lines(),
            "Virtual terminal should not show duplicate lines after streaming. Got: {:?}",
            term.get_visible_lines()
        );

        // Final content should be the complete message
        assert!(term.get_visible_output().contains("[agent] ABC"));
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_ignores_color_codes() {
        let mut term = VirtualTerminal::new();
        // Write with color codes (SGR sequences)
        write!(term, "\x1b[32mGreen\x1b[0m Normal").unwrap();
        assert_eq!(term.get_visible_output(), "Green Normal");
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_is_terminal() {
        let term_tty = VirtualTerminal::new();
        assert!(term_tty.is_terminal());

        let term_non_tty = VirtualTerminal::new_with_terminal(false);
        assert!(!term_non_tty.is_terminal());
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_cursor_position() {
        let mut term = VirtualTerminal::new();

        assert_eq!(term.cursor_position(), (0, 0));

        write!(term, "Hello").unwrap();
        assert_eq!(term.cursor_position(), (0, 5));

        write!(term, "\n").unwrap();
        assert_eq!(term.cursor_position(), (1, 0));

        write!(term, "World").unwrap();
        assert_eq!(term.cursor_position(), (1, 5));

        write!(term, "\r").unwrap();
        assert_eq!(term.cursor_position(), (1, 0));
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_count_pattern() {
        let mut term = VirtualTerminal::new();
        write!(term, "Hello World\nHello Again\nGoodbye").unwrap();
        assert_eq!(term.count_visible_pattern("Hello"), 2);
        assert_eq!(term.count_visible_pattern("Goodbye"), 1);
        assert_eq!(term.count_visible_pattern("NotFound"), 0);
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_clear() {
        let mut term = VirtualTerminal::new();
        write!(term, "Some content\nMore content").unwrap();
        assert!(!term.get_visible_output().is_empty());

        term.clear();
        assert!(term.get_visible_output().is_empty());
        assert_eq!(term.cursor_position(), (0, 0));
    }

    #[cfg(any(test, feature = "test-utils"))]
    #[test]
    fn test_virtual_terminal_write_history() {
        let mut term = VirtualTerminal::new();
        write!(term, "First").unwrap();
        write!(term, "Second").unwrap();
        write!(term, "Third").unwrap();

        let history = term.get_write_history();
        assert_eq!(history.len(), 3);
        assert_eq!(history[0], "First");
        assert_eq!(history[1], "Second");
        assert_eq!(history[2], "Third");
    }
}
