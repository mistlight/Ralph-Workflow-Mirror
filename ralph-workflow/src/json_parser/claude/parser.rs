// Claude parser implementation.
//
// Contains the ClaudeParser struct and its core methods.

/// Claude event parser
///
/// Note: This parser is designed for single-threaded use only.
/// The internal state uses `Rc<RefCell<>>` for convenience, not for thread safety.
/// Do not share this parser across threads.
pub struct ClaudeParser {
    colors: Colors,
    pub(crate) verbosity: Verbosity,
    /// Relative path to log file (if logging enabled)
    log_path: Option<std::path::PathBuf>,
    display_name: String,
    /// Unified streaming session tracker
    /// Provides single source of truth for streaming state across all content types
    streaming_session: Rc<RefCell<StreamingSession>>,
    /// Terminal mode for output formatting
    /// Detected at parse time and cached for performance
    terminal_mode: RefCell<TerminalMode>,
    /// Whether to show streaming quality metrics
    show_streaming_metrics: bool,
    /// Output printer for capturing or displaying output
    printer: SharedPrinter,

    /// Tracks whether a thinking delta line is currently being streamed.
    ///
    /// - In `TerminalMode::Full`, thinking deltas use the multi-line in-place update pattern
    ///   and must be finalized (cursor down + newline) before emitting other newline-based output.
    /// - In `TerminalMode::Basic|None`, we suppress per-delta thinking output and flush a single
    ///   final thinking line at the next output boundary (or at `message_stop`).
    thinking_active_index: RefCell<Option<u64>>,
}

impl ClaudeParser {
    /// Create a new `ClaudeParser` with the given colors and verbosity.
    ///
    /// # Arguments
    ///
    /// * `colors` - Colors for terminal output
    /// * `verbosity` - Verbosity level for output
    ///
    /// # Returns
    ///
    /// A new `ClaudeParser` instance
    ///
    /// # Example
    ///
    /// ```ignore
    /// use ralph_workflow::json_parser::ClaudeParser;
    /// use ralph_workflow::logger::Colors;
    /// use ralph_workflow::config::Verbosity;
    ///
    /// let parser = ClaudeParser::new(Colors::new(), Verbosity::Normal);
    /// ```
    pub fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self::with_printer(colors, verbosity, super::printer::shared_stdout())
    }

    /// Create a new `ClaudeParser` with a custom printer.
    ///
    /// # Arguments
    ///
    /// * `colors` - Colors for terminal output
    /// * `verbosity` - Verbosity level for output
    /// * `printer` - Shared printer for output
    ///
    /// # Returns
    ///
    /// A new `ClaudeParser` instance
    pub fn with_printer(colors: Colors, verbosity: Verbosity, printer: SharedPrinter) -> Self {
        let verbose_warnings = matches!(verbosity, Verbosity::Debug);
        let streaming_session = StreamingSession::new().with_verbose_warnings(verbose_warnings);

        // Use the printer's is_terminal method to validate it's connected correctly
        // This is a sanity check that also satisfies the compiler that the method is used
        let _printer_is_terminal = printer.borrow().is_terminal();

        Self {
            colors,
            verbosity,
            log_path: None,
            display_name: "Claude".to_string(),
            streaming_session: Rc::new(RefCell::new(streaming_session)),
            terminal_mode: RefCell::new(TerminalMode::detect()),
            show_streaming_metrics: false,
            printer,
            thinking_active_index: RefCell::new(None),
        }
    }

    pub(crate) const fn with_show_streaming_metrics(mut self, show: bool) -> Self {
        self.show_streaming_metrics = show;
        self
    }

    /// Set the display name for this parser.
    ///
    /// # Arguments
    ///
    /// * `display_name` - The name to display in output
    ///
    /// # Returns
    ///
    /// Self for builder pattern chaining
    pub fn with_display_name(mut self, display_name: &str) -> Self {
        self.display_name = display_name.to_string();
        self
    }

    pub(crate) fn with_log_file(mut self, path: &str) -> Self {
        self.log_path = Some(std::path::PathBuf::from(path));
        self
    }

    /// Set the terminal mode for this parser.
    ///
    /// # Arguments
    ///
    /// * `mode` - The terminal mode to use
    ///
    /// # Returns
    ///
    /// Self for builder pattern chaining
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_terminal_mode(self, mode: TerminalMode) -> Self {
        *self.terminal_mode.borrow_mut() = mode;
        self
    }

    /// Get a shared reference to the printer.
    ///
    /// This allows tests, monitoring, and other code to access the printer after parsing
    /// to verify output content, check for duplicates, or capture output for analysis.
    ///
    /// # Returns
    ///
    /// A clone of the shared printer reference (`Rc<RefCell<dyn Printable>>`)
    ///
    /// # Example
    ///
    /// ```ignore
    /// use ralph_workflow::json_parser::{ClaudeParser, printer::TestPrinter};
    /// use std::rc::Rc;
    /// use std::cell::RefCell;
    ///
    /// let printer = Rc::new(RefCell::new(TestPrinter::new()));
    /// let parser = ClaudeParser::with_printer(colors, verbosity, Rc::clone(&printer));
    ///
    /// // Parse events...
    ///
    /// // Now access the printer to verify output
    /// let printer_ref = parser.printer().borrow();
    /// assert!(!printer_ref.has_duplicate_consecutive_lines());
    /// ```
    /// Get a clone of the printer used by this parser.
    ///
    /// This is primarily useful for testing and monitoring.
    /// Only available with the `test-utils` feature.
    #[cfg(feature = "test-utils")]
    pub fn printer(&self) -> SharedPrinter {
        Rc::clone(&self.printer)
    }

    /// Get streaming quality metrics from the current session.
    ///
    /// This provides insight into the deduplication and streaming quality of the
    /// parsing session, including:
    /// - Number of snapshot repairs (when the agent sent accumulated content as a delta)
    /// - Number of large deltas (potential protocol violations)
    /// - Total deltas processed
    ///
    /// Useful for testing, monitoring, and debugging streaming behavior.
    /// Only available with the `test-utils` feature.
    ///
    /// # Returns
    ///
    /// A copy of the streaming quality metrics from the internal `StreamingSession`.
    ///
    /// # Example
    ///
    /// ```ignore
    /// use ralph_workflow::json_parser::{ClaudeParser, printer::TestPrinter};
    /// use std::rc::Rc;
    /// use std::cell::RefCell;
    ///
    /// let printer = Rc::new(RefCell::new(TestPrinter::new()));
    /// let parser = ClaudeParser::with_printer(colors, verbosity, Rc::clone(&printer));
    ///
    /// // Parse events...
    ///
    /// // Verify deduplication logic triggered
    /// let metrics = parser.streaming_metrics();
    /// assert!(metrics.snapshot_repairs_count > 0, "Snapshot repairs should occur");
    /// ```
    #[cfg(feature = "test-utils")]
    pub fn streaming_metrics(&self) -> StreamingQualityMetrics {
        self.streaming_session
            .borrow()
            .get_streaming_quality_metrics()
    }

    /// Parse and display a single Claude JSON event
    ///
    /// Returns `Some(formatted_output)` for valid events, or None for:
    /// - Malformed JSON (logged at debug level)
    /// - Unknown event types
    /// - Empty or whitespace-only output
    pub fn parse_event(&self, line: &str) -> Option<String> {
        let event: ClaudeEvent = if let Ok(e) = serde_json::from_str(line) {
            e
        } else {
            // Non-JSON line - could be raw text output from agent
            // Pass through as-is if it looks like real output (not empty)
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('{') {
                return Some(format!("{trimmed}\n"));
            }
            return None;
        };
        let c = &self.colors;
        let prefix = &self.display_name;

        let output = match event {
            ClaudeEvent::System {
                subtype,
                session_id,
                cwd,
            } => self.format_system_event(subtype.as_ref(), session_id, cwd),
            ClaudeEvent::Assistant { message } => self.format_assistant_event(message),
            ClaudeEvent::User { message } => self.format_user_event(message),
            ClaudeEvent::Result {
                subtype,
                duration_ms,
                total_cost_usd,
                num_turns,
                result,
                error,
            } => self.format_result_event(
                subtype,
                duration_ms,
                total_cost_usd,
                num_turns,
                result,
                error,
            ),
            ClaudeEvent::StreamEvent { event } => {
                // Handle streaming events for delta/partial updates
                self.parse_stream_event(event)
            }
            ClaudeEvent::Unknown => {
                // Use the generic unknown event formatter for consistent handling
                // In verbose mode, this will show the event type and key fields
                // In normal mode, this returns empty string
                format_unknown_json_event(line, prefix, *c, self.verbosity.is_verbose())
            }
        };

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    /// Parse a streaming event for delta/partial updates
    ///
    /// Handles the nested events within `stream_event`:
    /// - MessageStart/Stop: Manage session state
    /// - `ContentBlockStart`: Initialize new content blocks
    /// - ContentBlockDelta/TextDelta: Accumulate and display incrementally
    /// - `ContentBlockStop`: Finalize content blocks
    /// - `MessageDelta`: Process message metadata without output
    /// - Error: Display appropriately
    ///
    /// Returns String for display content, empty String for control events.
    fn parse_stream_event(&self, event: StreamInnerEvent) -> String {
        let mut session = self.streaming_session.borrow_mut();

        match event {
            StreamInnerEvent::MessageStart {
                message,
                message_id,
            } => {
                // Reset any pending thinking line from a previous message.
                *self.thinking_active_index.borrow_mut() = None;

                // Extract message_id from either the top-level field or nested message.id
                // The Claude API typically puts the ID in message.id, not at the top level
                let effective_message_id =
                    message_id.or_else(|| message.as_ref().and_then(|m| m.id.clone()));
                // Set message ID for tracking and clear session state on new message
                session.set_current_message_id(effective_message_id);
                session.on_message_start();
                String::new()
            }
            StreamInnerEvent::ContentBlockStart {
                index: Some(index),
                content_block: Some(block),
            } => {
                // Initialize a new content block at this index
                session.on_content_block_start(index);
                match &block {
                    ContentBlock::Text { text: Some(t) } if !t.is_empty() => {
                        // Initial text in ContentBlockStart - treat as first delta
                        session.on_text_delta(index, t);
                    }
                    ContentBlock::ToolUse { name, input } => {
                        // Track tool name for GLM/CCS deduplication.
                        // IMPORTANT: Track the tool name when provided, even when input is None.
                        // GLM may send ContentBlockStart with name but no input, then send input via delta.
                        // We only store when we have a name to avoid overwriting a previous tool name with None.
                        if let Some(n) = name {
                            session.set_tool_name(index, Some(n.clone()));
                        }

                        // Initialize tool input accumulator only if input is present
                        if let Some(i) = input {
                            let input_str = if let serde_json::Value::String(s) = &i {
                                s.clone()
                            } else {
                                format_tool_input(i)
                            };
                            session.on_tool_input_delta(index, &input_str);
                        }
                    }
                    _ => {}
                }
                String::new()
            }
            StreamInnerEvent::ContentBlockStart {
                index: Some(index),
                content_block: None,
            } => {
                // Content block started but no initial content provided
                session.on_content_block_start(index);
                String::new()
            }
            StreamInnerEvent::ContentBlockStart { .. } => {
                // Content block without index - ignore
                String::new()
            }
            StreamInnerEvent::ContentBlockDelta {
                index: Some(index),
                delta: Some(delta),
            } => self.handle_content_block_delta(&mut session, index, delta),
            StreamInnerEvent::TextDelta { text: Some(text) } => {
                self.handle_text_delta(&mut session, &text)
            }
            StreamInnerEvent::ContentBlockStop { .. } => {
                // Content block completion event - no output needed
                // This event marks the end of a content block but doesn't produce
                // any displayable content. It's a control event for state management.
                String::new()
            }
            StreamInnerEvent::MessageDelta { .. } => {
                // Message delta event with usage/metadata - no output needed
                // This event contains final message metadata (stop_reason, usage stats)
                // but is used for tracking/monitoring purposes only, not display.
                String::new()
            }
            StreamInnerEvent::ContentBlockDelta { .. }
            | StreamInnerEvent::Ping
            | StreamInnerEvent::TextDelta { text: None }
            | StreamInnerEvent::Error { error: None } => String::new(),
            StreamInnerEvent::MessageStop => self.handle_message_stop(&mut session),
            StreamInnerEvent::Error {
                error: Some(err), ..
            } => self.handle_error_event(err),
            StreamInnerEvent::Unknown => self.handle_unknown_event(),
        }
    }
}
