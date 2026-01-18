//! Gemini CLI JSON parser.
//!
//! Parses NDJSON output from Gemini CLI and formats it for display.
//!
//! # Streaming Output Behavior
//!
//! This parser implements real-time streaming output for text deltas. When content
//! arrives in multiple chunks (via `message` events with `delta: true`), the parser:
//!
//! 1. **Accumulates** text deltas from each chunk into a buffer
//! 2. **Displays** the accumulated text after each chunk
//! 3. **Uses carriage return (`\r`) and line clearing (`\x1b[2K`)** to rewrite the entire line,
//!    creating an updating effect that shows the content building up in real-time
//! 4. **Shows prefix on every delta**, rewriting the entire line each time (industry standard)
//!
//! Example output sequence for streaming "Hello World" in two chunks:
//! ```text
//! [Gemini] Hello\r         (first delta with prefix, no newline)
//! \x1b[2K\r[Gemini] Hello World\r  (second delta clears line, rewrites with accumulated)
//! [Gemini] Hello World\n   (final non-delta message shows complete result)
//! ```
//!
//! # Single-Line Pattern
//!
//! The renderer uses a single-line pattern with carriage return for in-place updates.
//! This is the industry standard for streaming CLIs (used by Rich, Ink, Bubble Tea).
//!
//! Each delta rewrites the entire line with prefix, ensuring that:
//! - The user always sees the prefix
//! - Content updates in-place without visual artifacts
//! - Terminal state is clean and predictable

use crate::common::truncate_text;
use crate::config::Verbosity;
use crate::logger::{Colors, CHECK, CROSS};
use std::cell::RefCell;
use std::fmt::Write as _;
use std::io::{self, BufRead, Write};
use std::rc::Rc;

use super::delta_display;
use super::delta_display::{DeltaRenderer, TextDeltaRenderer};
use super::health::HealthMonitor;
#[cfg(feature = "test-utils")]
use super::health::StreamingQualityMetrics;
use super::printer::SharedPrinter;
use super::streaming_state::StreamingSession;
use super::terminal::TerminalMode;
use super::types::{format_tool_input, format_unknown_json_event, ContentType, GeminiEvent};

/// Gemini event parser
pub struct GeminiParser {
    colors: Colors,
    verbosity: Verbosity,
    log_file: Option<String>,
    display_name: String,
    /// Unified streaming session for state tracking
    streaming_session: Rc<RefCell<StreamingSession>>,
    /// Terminal mode for output formatting
    terminal_mode: RefCell<TerminalMode>,
    /// Whether to show streaming quality metrics
    show_streaming_metrics: bool,
    /// Output printer for capturing or displaying output
    printer: SharedPrinter,
}

impl GeminiParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self::with_printer(colors, verbosity, super::printer::shared_stdout())
    }

    /// Create a new `GeminiParser` with a custom printer.
    ///
    /// # Arguments
    ///
    /// * `colors` - Colors for terminal output
    /// * `verbosity` - Verbosity level for output
    /// * `printer` - Shared printer for output
    ///
    /// # Returns
    ///
    /// A new `GeminiParser` instance
    pub(crate) fn with_printer(
        colors: Colors,
        verbosity: Verbosity,
        printer: SharedPrinter,
    ) -> Self {
        let verbose_warnings = matches!(verbosity, Verbosity::Debug);
        let streaming_session = StreamingSession::new().with_verbose_warnings(verbose_warnings);

        // Use the printer's is_terminal method to validate it's connected correctly
        let _printer_is_terminal = printer.borrow().is_terminal();

        Self {
            colors,
            verbosity,
            log_file: None,
            display_name: "Gemini".to_string(),
            streaming_session: Rc::new(RefCell::new(streaming_session)),
            terminal_mode: RefCell::new(TerminalMode::detect()),
            show_streaming_metrics: false,
            printer,
        }
    }

    pub(crate) const fn with_show_streaming_metrics(mut self, show: bool) -> Self {
        self.show_streaming_metrics = show;
        self
    }

    pub(crate) fn with_display_name(mut self, display_name: &str) -> Self {
        self.display_name = display_name.to_string();
        self
    }

    pub(crate) fn with_log_file(mut self, path: &str) -> Self {
        self.log_file = Some(path.to_string());
        self
    }

    #[cfg(test)]
    pub fn with_terminal_mode(self, mode: TerminalMode) -> Self {
        *self.terminal_mode.borrow_mut() = mode;
        self
    }

    /// Create a new parser with a test printer.
    ///
    /// This is the primary entry point for integration tests that need
    /// to capture parser output for verification.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_printer_for_test(
        colors: Colors,
        verbosity: Verbosity,
        printer: SharedPrinter,
    ) -> Self {
        Self::with_printer(colors, verbosity, printer)
    }

    /// Set the log file path for testing.
    ///
    /// This allows tests to verify log file content after parsing.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_log_file_for_test(mut self, path: &str) -> Self {
        self.log_file = Some(path.to_string());
        self
    }

    /// Parse a stream for testing purposes.
    ///
    /// This exposes the internal `parse_stream` method for integration tests.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn parse_stream_for_test<R: std::io::BufRead>(&self, reader: R) -> std::io::Result<()> {
        self.parse_stream(reader)
    }

    /// Get a shared reference to the printer.
    ///
    /// This allows tests, monitoring, and other code to access the printer after parsing
    /// to verify output content, check for duplicates, or capture output for analysis.
    /// Only available with the `test-utils` feature.
    ///
    /// # Returns
    ///
    /// A clone of the shared printer reference (`Rc<RefCell<dyn Printable>>`)
    #[cfg(feature = "test-utils")]
    pub fn printer(&self) -> SharedPrinter {
        Rc::clone(&self.printer)
    }

    /// Get streaming quality metrics from the current session.
    ///
    /// This provides insight into the deduplication and streaming quality of the
    /// parsing session. Only available with the `test-utils` feature.
    ///
    /// # Returns
    ///
    /// A copy of the streaming quality metrics from the internal `StreamingSession`.
    #[cfg(feature = "test-utils")]
    pub fn streaming_metrics(&self) -> StreamingQualityMetrics {
        self.streaming_session
            .borrow()
            .get_streaming_quality_metrics()
    }

    /// Parse and display a single Gemini JSON event
    ///
    /// Returns `Some(formatted_output)` for valid events, or None for:
    /// - Malformed JSON (non-JSON text passed through if meaningful)
    /// - Unknown event types
    /// - Empty or whitespace-only output
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
        let event: GeminiEvent = if let Ok(e) = serde_json::from_str(line) {
            e
        } else {
            // Non-JSON line - pass through as-is if meaningful
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('{') {
                return Some(format!("{trimmed}\n"));
            }
            return None;
        };
        let c = &self.colors;
        let prefix = &self.display_name;

        let output = match event {
            GeminiEvent::Init {
                session_id, model, ..
            } => self.format_init_event(session_id, model),
            GeminiEvent::Message {
                role,
                content,
                delta,
            } => self.format_message_event(role, content, delta),
            GeminiEvent::ToolUse {
                tool_name,
                parameters,
                ..
            } => self.format_tool_use_event(tool_name, parameters.as_ref()),
            GeminiEvent::ToolResult { status, output, .. } => {
                self.format_tool_result_event(status, output.as_ref())
            }
            GeminiEvent::Error { message, code, .. } => self.format_error_event(message, code),
            GeminiEvent::Result { status, stats, .. } => self.format_result_event(status, stats),
            GeminiEvent::Unknown => {
                // Use the generic unknown event formatter for consistent handling
                format_unknown_json_event(line, prefix, *c, self.verbosity.is_verbose())
            }
        };

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    /// Format an Init event
    fn format_init_event(&self, session_id: Option<String>, model: Option<String>) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        // Reset streaming state on new session
        self.streaming_session.borrow_mut().on_message_start();
        let sid = session_id.unwrap_or_else(|| "unknown".to_string());
        // Set the current message ID for duplicate detection
        self.streaming_session
            .borrow_mut()
            .set_current_message_id(Some(sid.clone()));
        let model_str = model.unwrap_or_else(|| "unknown".to_string());
        format!(
            "{}[{}]{} {}Session started{} {}({:.8}..., {}){}\n",
            c.dim(),
            prefix,
            c.reset(),
            c.cyan(),
            c.reset(),
            c.dim(),
            sid,
            model_str,
            c.reset()
        )
    }

    /// Format a Message event
    fn format_message_event(
        &self,
        role: Option<String>,
        content: Option<String>,
        delta: Option<bool>,
    ) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        let role_str = role.unwrap_or_else(|| "unknown".to_string());
        let is_delta = delta.unwrap_or(false);

        if let Some(text) = content {
            if is_delta && role_str == "assistant" {
                // Accumulate delta content using StreamingSession
                let (show_prefix, accumulated_text, is_duplicate) = {
                    let mut session = self.streaming_session.borrow_mut();
                    let show_prefix = session.on_text_delta_key("main", &text);
                    // Get accumulated text for streaming display
                    let accumulated_text = session
                        .get_accumulated(ContentType::Text, "main")
                        .unwrap_or("")
                        .to_string();

                    // Sanitize the accumulated text to check if it's empty
                    let sanitized_text = delta_display::sanitize_for_display(&accumulated_text);

                    // Check if this sanitized content has already been rendered
                    // This prevents duplicates when accumulated content differs only by whitespace
                    let is_duplicate = session.is_content_hash_rendered(
                        ContentType::Text,
                        "main",
                        &sanitized_text,
                    );

                    // Mark this sanitized content as rendered for future duplicate detection
                    // We use the sanitized text (not the rendered output) to avoid false positives
                    // when the same accumulated text is rendered with different terminal modes
                    if !is_duplicate {
                        session.mark_rendered(ContentType::Text, "main");
                        session.mark_content_hash_rendered(
                            ContentType::Text,
                            "main",
                            &sanitized_text,
                        );
                    }

                    (show_prefix, accumulated_text, is_duplicate)
                };

                // Skip rendering if this content was already rendered
                if is_duplicate {
                    return String::new();
                }

                // Use TextDeltaRenderer for consistent rendering across all parsers
                let terminal_mode = *self.terminal_mode.borrow();
                if show_prefix {
                    // First delta: use renderer with prefix
                    return TextDeltaRenderer::render_first_delta(
                        &accumulated_text,
                        prefix,
                        *c,
                        terminal_mode,
                    );
                }
                // Subsequent deltas: use renderer for in-place update
                return TextDeltaRenderer::render_subsequent_delta(
                    &accumulated_text,
                    prefix,
                    *c,
                    terminal_mode,
                );
            } else if !is_delta && role_str == "assistant" {
                // Non-delta message - check for duplicate using message ID or fallback to streaming content check
                let session = self.streaming_session.borrow();
                let is_duplicate = session.get_current_message_id().map_or_else(
                    || session.has_any_streamed_content(),
                    |message_id| session.is_duplicate_final_message(message_id),
                );
                let was_streaming = session.has_any_streamed_content();
                let metrics = session.get_streaming_quality_metrics();
                drop(session);

                // Finalize the message (this marks it as displayed)
                let _was_in_block = self.streaming_session.borrow_mut().on_message_stop();

                // If this is a duplicate or content was streamed, use TextDeltaRenderer for completion
                if is_duplicate || was_streaming {
                    let terminal_mode = *self.terminal_mode.borrow();
                    let completion = TextDeltaRenderer::render_completion(terminal_mode);
                    let show_metrics = (self.verbosity.is_debug() || self.show_streaming_metrics)
                        && metrics.total_deltas > 0;
                    if show_metrics {
                        return format!("{}\n{}", completion, metrics.format(*c));
                    }
                    return completion;
                }

                // Otherwise, show the full content (non-streaming path)
                let limit = self.verbosity.truncate_limit("text");
                let preview = truncate_text(&text, limit);

                return format!(
                    "{}[{}]{} {}{}{}\n",
                    c.dim(),
                    prefix,
                    c.reset(),
                    c.white(),
                    preview,
                    c.reset()
                );
            }
            // User or other role messages
            let limit = self.verbosity.truncate_limit("text");
            let preview = truncate_text(&text, limit);
            return format!(
                "{}[{}]{} {}{}:{} {}{}{}\n",
                c.dim(),
                prefix,
                c.reset(),
                c.blue(),
                role_str,
                c.reset(),
                c.dim(),
                preview,
                c.reset()
            );
        }
        String::new()
    }

    /// Format a `ToolUse` event
    fn format_tool_use_event(
        &self,
        tool_name: Option<String>,
        parameters: Option<&serde_json::Value>,
    ) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        let tool_name = tool_name.unwrap_or_else(|| "unknown".to_string());
        let mut out = format!(
            "{}[{}]{} {}Tool{}: {}{}{}\n",
            c.dim(),
            prefix,
            c.reset(),
            c.magenta(),
            c.reset(),
            c.bold(),
            tool_name,
            c.reset()
        );
        if self.verbosity.show_tool_input() {
            if let Some(params) = parameters {
                let params_str = format_tool_input(params);
                let limit = self.verbosity.truncate_limit("tool_input");
                let preview = truncate_text(&params_str, limit);
                if !preview.is_empty() {
                    let _ = writeln!(
                        out,
                        "{}[{}]{} {}  └─ {}{}",
                        c.dim(),
                        prefix,
                        c.reset(),
                        c.dim(),
                        preview,
                        c.reset()
                    );
                }
            }
        }
        out
    }

    /// Format a `ToolResult` event
    fn format_tool_result_event(&self, status: Option<String>, output: Option<&String>) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        let status_str = status.unwrap_or_else(|| "unknown".to_string());
        let is_success = status_str == "success";
        let icon = if is_success { CHECK } else { CROSS };
        let color = if is_success { c.green() } else { c.red() };

        let mut out = format!(
            "{}[{}]{} {}{} Tool result{}\n",
            c.dim(),
            prefix,
            c.reset(),
            color,
            icon,
            c.reset()
        );

        if self.verbosity.is_verbose() {
            if let Some(output_text) = output {
                let limit = self.verbosity.truncate_limit("tool_result");
                let preview = truncate_text(output_text, limit);
                let _ = writeln!(
                    out,
                    "{}[{}]{} {}  └─ {}{}",
                    c.dim(),
                    prefix,
                    c.reset(),
                    c.dim(),
                    preview,
                    c.reset()
                );
            }
        }
        out
    }

    /// Format an `Error` event
    fn format_error_event(&self, message: Option<String>, code: Option<String>) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        let msg = message.unwrap_or_else(|| "unknown error".to_string());
        let code_str = code.map_or_else(String::new, |c| format!(" ({c})"));
        format!(
            "{}[{}]{} {}{} Error{}:{} {}\n",
            c.dim(),
            prefix,
            c.reset(),
            c.red(),
            CROSS,
            code_str,
            c.reset(),
            msg
        )
    }

    /// Format a `Result` event
    fn format_result_event(
        &self,
        status: Option<String>,
        event_stats: Option<crate::json_parser::types::GeminiStats>,
    ) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        let status_result = status.unwrap_or_else(|| "unknown".to_string());
        let is_success = status_result == "success";
        let icon = if is_success { CHECK } else { CROSS };
        let color = if is_success { c.green() } else { c.red() };

        let stats_display = event_stats.map_or_else(String::new, |s| {
            let duration_s = s.duration_ms.unwrap_or(0) / 1000;
            let duration_m = duration_s / 60;
            let duration_s_rem = duration_s % 60;
            let input = s.input_tokens.unwrap_or(0);
            let output = s.output_tokens.unwrap_or(0);
            let tools = s.tool_calls.unwrap_or(0);
            format!("({duration_m}m {duration_s_rem}s, in:{input} out:{output}, {tools} tools)")
        });

        format!(
            "{}[{}]{} {}{} {}{} {}{}{}\n",
            c.dim(),
            prefix,
            c.reset(),
            color,
            icon,
            status_result,
            c.reset(),
            c.dim(),
            stats_display,
            c.reset()
        )
    }

    /// Check if a Gemini event is a control event (state management with no user output)
    ///
    /// Control events are valid JSON that represent state transitions rather than
    /// user-facing content. They should be tracked separately from "ignored" events
    /// to avoid false health warnings.
    const fn is_control_event(event: &GeminiEvent) -> bool {
        match event {
            // Init and Result events are control events
            GeminiEvent::Init { .. } | GeminiEvent::Result { .. } => true,
            _ => false,
        }
    }

    /// Parse a stream of Gemini NDJSON events
    pub(crate) fn parse_stream<R: BufRead>(&self, mut reader: R) -> io::Result<()> {
        use super::incremental_parser::IncrementalNdjsonParser;

        let c = &self.colors;
        let monitor = HealthMonitor::new("Gemini");
        let mut log_writer = self.log_file.as_ref().and_then(|log_path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .ok()
                .map(std::io::BufWriter::new)
        });

        // Use incremental parser for true real-time streaming
        // This processes JSON as soon as it's complete, not waiting for newlines
        let mut incremental_parser = IncrementalNdjsonParser::new();
        let mut byte_buffer = Vec::new();

        loop {
            // Read available bytes
            byte_buffer.clear();
            let chunk = reader.fill_buf()?;
            if chunk.is_empty() {
                break;
            }

            // Process all bytes immediately
            byte_buffer.extend_from_slice(chunk);
            let consumed = chunk.len();
            reader.consume(consumed);

            // Feed bytes to incremental parser
            let json_events = incremental_parser.feed(&byte_buffer);

            // Process each complete JSON event immediately
            for line in json_events {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                // In debug mode, also show the raw JSON
                if self.verbosity.is_debug() {
                    let mut printer = self.printer.borrow_mut();
                    writeln!(
                        printer,
                        "{}[DEBUG]{} {}{}{}",
                        c.dim(),
                        c.reset(),
                        c.dim(),
                        &line,
                        c.reset()
                    )?;
                    printer.flush()?;
                }

                // Parse the event once - parse_event handles malformed JSON by returning None
                match self.parse_event(&line) {
                    Some(output) => {
                        monitor.record_parsed();
                        // Write output to printer
                        let mut printer = self.printer.borrow_mut();
                        write!(printer, "{output}")?;
                        printer.flush()?;
                    }
                    None => {
                        // Check if this was a control event (state management with no user output)
                        if trimmed.starts_with('{') {
                            if let Ok(event) = serde_json::from_str::<GeminiEvent>(&line) {
                                if Self::is_control_event(&event) {
                                    monitor.record_control_event();
                                } else {
                                    // Valid JSON but not a control event - track as unknown
                                    monitor.record_unknown_event();
                                }
                            } else {
                                // Failed to deserialize - track as parse error
                                monitor.record_parse_error();
                            }
                        } else {
                            monitor.record_ignored();
                        }
                    }
                }

                // Log raw JSON to file if configured
                if let Some(ref mut file) = log_writer {
                    writeln!(file, "{line}")?;
                }
            }
        }

        if let Some(ref mut file) = log_writer {
            file.flush()?;
            // Ensure data is written to disk before continuing
            // This prevents race conditions where extraction runs before OS commits writes
            let _ = file.get_mut().sync_all();
        }
        if let Some(warning) = monitor.check_and_warn(*c) {
            let mut printer = self.printer.borrow_mut();
            writeln!(printer, "{warning}\n")?;
        }
        Ok(())
    }
}
