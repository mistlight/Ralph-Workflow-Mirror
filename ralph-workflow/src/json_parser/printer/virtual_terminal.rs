// Virtual terminal implementation.
//
// Contains VirtualTerminal for simulating real terminal behavior in tests.

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
/// - **Line wrapping**: Writing past terminal width wraps to next row
/// - **Scrolling**: Writing past terminal height scrolls content up
///
/// This allows tests to verify what the user actually SEES, not just what was written.
///
/// # Terminal Geometry
///
/// The virtual terminal supports fixed geometry (width and height) to simulate
/// real terminal constraints:
///
/// - **Wrapping**: Writing past column `cols` automatically wraps to next row
/// - **Scrolling**: When cursor advances past `rows`, the screen scrolls up
///   (top row is discarded, new blank row added at bottom)
/// - **No reflow**: Resizing is not supported; geometry is fixed at creation
///
/// # Example
///
/// ```ignore
/// use ralph_workflow::json_parser::printer::VirtualTerminal;
/// use std::io::Write;
///
/// // Create a terminal with 80 columns and 24 rows
/// let mut term = VirtualTerminal::new_with_geometry(80, 24);
/// write!(term, "A".repeat(100)).unwrap();  // Wraps to multiple rows
/// assert!(term.count_visible_lines() > 1);
///
/// // Traditional infinite-width terminal (backward compatible)
/// let mut term = VirtualTerminal::new();
/// write!(term, "Hello").unwrap();
/// write!(term, "\rWorld").unwrap();  // Overwrites "Hello"
/// assert_eq!(term.get_visible_output(), "World");
/// ```
#[cfg(any(test, feature = "test-utils"))]
#[derive(Debug)]
pub struct VirtualTerminal {
    /// The terminal screen buffer - each element is a line (row)
    /// In geometry mode, this is a fixed-size circular buffer (scrolling)
    /// In unbounded mode, this grows indefinitely
    screen: RefCell<Vec<String>>,
    /// Current cursor row (0-indexed, relative to screen buffer)
    cursor_row: RefCell<usize>,
    /// Current cursor column (0-indexed)
    cursor_col: RefCell<usize>,
    /// Whether to simulate terminal mode (affects is_terminal())
    simulated_is_terminal: bool,
    /// Raw write history for debugging
    write_history: RefCell<Vec<String>>,
    /// Terminal width in columns (None = unbounded, for backward compatibility)
    cols: Option<usize>,
    /// Terminal height in rows (None = unbounded, for backward compatibility)
    rows: Option<usize>,
}

#[cfg(any(test, feature = "test-utils"))]
impl VirtualTerminal {
    /// Create a new virtual terminal (simulates being a TTY by default).
    /// This creates an unbounded terminal (no wrapping, no scrolling) for backward compatibility.
    pub fn new() -> Self {
        Self {
            screen: RefCell::new(vec![String::new()]),
            cursor_row: RefCell::new(0),
            cursor_col: RefCell::new(0),
            simulated_is_terminal: true,
            write_history: RefCell::new(Vec::new()),
            cols: None,
            rows: None,
        }
    }

    /// Create a new virtual terminal with specified terminal simulation.
    /// This creates an unbounded terminal (no wrapping, no scrolling) for backward compatibility.
    pub fn new_with_terminal(is_terminal: bool) -> Self {
        Self {
            screen: RefCell::new(vec![String::new()]),
            cursor_row: RefCell::new(0),
            cursor_col: RefCell::new(0),
            simulated_is_terminal: is_terminal,
            write_history: RefCell::new(Vec::new()),
            cols: None,
            rows: None,
        }
    }

    /// Create a new virtual terminal with fixed geometry (width and height).
    ///
    /// This simulates a real terminal with specific dimensions:
    /// - Writing past `cols` wraps to the next row
    /// - Advancing past `rows` scrolls the screen (top row is discarded)
    ///
    /// # Arguments
    /// * `cols` - Terminal width in columns
    /// * `rows` - Terminal height in rows
    ///
    /// # Example
    /// ```ignore
    /// let term = VirtualTerminal::new_with_geometry(80, 24);
    /// // Writing 100 characters will wrap to multiple rows
    /// ```
    pub fn new_with_geometry(cols: usize, rows: usize) -> Self {
        Self {
            screen: RefCell::new(vec![String::new()]),
            cursor_row: RefCell::new(0),
            cursor_col: RefCell::new(0),
            simulated_is_terminal: true,
            write_history: RefCell::new(Vec::new()),
            cols: Some(cols),
            rows: Some(rows),
        }
    }

    /// Get the visible output as the user would see it.
    ///
    /// This returns the final rendered state of the terminal, with all
    /// ANSI sequences processed and overwrites applied.
    pub fn get_visible_output(&self) -> String {
        let screen = self.screen.borrow();
        // Join non-empty lines, trimming trailing whitespace from each
        screen
            .iter()
            .map(|line| line.trim_end().to_string())
            .collect::<Vec<_>>()
            .join("\n")
    }

    /// Get visible lines (non-empty lines only).
    pub fn get_visible_lines(&self) -> Vec<String> {
        self.screen
            .borrow()
            .iter()
            .map(|line| line.trim_end().to_string())
            .filter(|line| !line.is_empty())
            .collect()
    }

    /// Count the number of visible lines (non-empty lines).
    ///
    /// This is useful for detecting multi-line waterfall bugs where
    /// streaming output creates multiple visible lines instead of
    /// updating a single line in place.
    pub fn count_visible_lines(&self) -> usize {
        self.get_visible_lines().len()
    }

    /// Get the full screen content including empty lines.
    ///
    /// This is useful for debugging tests to see the exact screen state.
    pub fn get_screen_content(&self) -> Vec<String> {
        self.screen.borrow().clone()
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
        self.screen.borrow_mut().clear();
        self.screen.borrow_mut().push(String::new());
        *self.cursor_row.borrow_mut() = 0;
        *self.cursor_col.borrow_mut() = 0;
        self.write_history.borrow_mut().clear();
    }

    /// Ensure the current row exists in the buffer.
    ///
    /// In geometry mode, this may trigger scrolling if cursor is past the bottom.
    /// In unbounded mode, this grows the buffer as needed.
    fn ensure_row_exists(&self) {
        let row = *self.cursor_row.borrow();
        let mut screen = self.screen.borrow_mut();

        // In geometry mode, check if we need to scroll
        if let Some(max_rows) = self.rows {
            if row >= max_rows {
                // Scroll: remove top row, add blank row at bottom
                screen.remove(0);
                screen.push(String::new());
                // Cursor stays at bottom row (last row index)
                *self.cursor_row.borrow_mut() = max_rows - 1;
                return;
            }
        }

        // Ensure row exists (grow buffer if needed)
        while screen.len() <= row {
            screen.push(String::new());
        }
    }

    /// Write a string of regular characters at the current cursor position.
    ///
    /// In geometry mode, this handles automatic wrapping when writing past terminal width.
    /// In unbounded mode, the line grows indefinitely.
    fn write_str(&self, s: &str) {
        if s.is_empty() {
            return;
        }

        // Handle wrapping character by character in geometry mode
        if self.cols.is_some() {
            for ch in s.chars() {
                self.write_char_with_wrap(ch);
            }
        } else {
            // Unbounded mode: write entire string without wrapping
            self.write_str_unbounded(s);
        }
    }

    /// Write a single character with automatic wrapping (geometry mode).
    fn write_char_with_wrap(&self, ch: char) {
        self.ensure_row_exists();
        let mut row = *self.cursor_row.borrow();
        let mut col = *self.cursor_col.borrow();

        // Check if we need to wrap to next line
        if let Some(max_cols) = self.cols {
            if col >= max_cols {
                // Wrap to next row, column 0
                *self.cursor_row.borrow_mut() = row + 1;
                *self.cursor_col.borrow_mut() = 0;
                self.ensure_row_exists(); // May trigger scrolling
                row = *self.cursor_row.borrow();
                col = 0;
            }
        }

        let mut screen = self.screen.borrow_mut();
        let line = &mut screen[row];

        // Extend the line with spaces if needed
        while line.chars().count() < col {
            line.push(' ');
        }

        // Build new line: prefix + new char + suffix
        let prefix: String = line.chars().take(col).collect();
        let suffix: String = line.chars().skip(col + 1).collect();
        *line = format!("{}{}{}", prefix, ch, suffix);

        // Move cursor right
        *self.cursor_col.borrow_mut() = col + 1;
    }

    /// Write a string without wrapping (unbounded mode, backward compatible).
    fn write_str_unbounded(&self, s: &str) {
        self.ensure_row_exists();
        let row = *self.cursor_row.borrow();
        let col = *self.cursor_col.borrow();
        let mut screen = self.screen.borrow_mut();
        let line = &mut screen[row];

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
    ///
    /// This clears ONLY the current row where the cursor is positioned.
    /// If content has wrapped to multiple rows, this only clears the cursor's row.
    /// This behavior matches real terminal `\x1b[2K` (erase line).
    fn clear_line(&self) {
        self.ensure_row_exists();
        let row = *self.cursor_row.borrow();
        let mut screen = self.screen.borrow_mut();
        screen[row].clear();
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
