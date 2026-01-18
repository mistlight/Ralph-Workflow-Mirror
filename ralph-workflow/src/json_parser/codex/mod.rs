//! Codex CLI JSON parser.
//!
//! Parses NDJSON output from `OpenAI` Codex CLI and formats it for display.
//!
//! # Streaming Output Behavior
//!
//! This parser implements real-time streaming output for text deltas. When content
//! arrives in multiple chunks (via `item.started` events with `agent_message` type),
//! the parser:
//!
//! 1. **Accumulates** text deltas from each chunk into a buffer
//! 2. **Displays** the accumulated text after each chunk
//! 3. **Uses carriage return (`\r`) and line clearing (`\x1b[2K`)** to rewrite the entire line,
//!    creating an updating effect that shows the content building up in real-time
//! 4. **Shows prefix on every delta**, rewriting the entire line each time (industry standard)
//!
//! Example output sequence for streaming "Hello World" in two chunks:
//! ```text
//! [Codex] Hello\r          (first chunk with prefix, no newline)
//! \x1b[2K\r[Codex] Hello World\r  (second chunk clears line, rewrites with accumulated)
//! [Codex] Hello World\n     (item.completed shows final result with prefix)
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

mod event_handlers;

use crate::config::Verbosity;
use crate::logger::Colors;
use std::cell::RefCell;
use std::io::{self, BufRead, Write};
use std::rc::Rc;

use super::health::HealthMonitor;
use super::health::StreamingQualityMetrics;
use super::printer::SharedPrinter;
use super::streaming_state::StreamingSession;
use super::terminal::TerminalMode;
use super::types::{format_unknown_json_event, CodexEvent};

use event_handlers::{
    handle_error, handle_item_completed, handle_item_started, handle_thread_started,
    handle_turn_completed, handle_turn_failed, handle_turn_started, EventHandlerContext,
};

/// Codex event parser
pub struct CodexParser {
    colors: Colors,
    verbosity: Verbosity,
    log_file: Option<String>,
    display_name: String,
    /// Unified streaming session for state tracking
    streaming_session: Rc<RefCell<StreamingSession>>,
    /// Delta accumulator for reasoning content (which uses special display)
    /// Note: We keep this for reasoning only, as it uses `DeltaDisplayFormatter`
    reasoning_accumulator: Rc<RefCell<super::types::DeltaAccumulator>>,
    /// Turn counter for generating synthetic turn IDs
    turn_counter: Rc<RefCell<u64>>,
    /// Terminal mode for output formatting
    terminal_mode: RefCell<TerminalMode>,
    /// Whether to show streaming quality metrics
    show_streaming_metrics: bool,
    /// Output printer for capturing or displaying output
    printer: SharedPrinter,
}

impl CodexParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self::with_printer(colors, verbosity, super::printer::shared_stdout())
    }

    /// Create a new `CodexParser` with a custom printer.
    ///
    /// # Arguments
    ///
    /// * `colors` - Colors for terminal output
    /// * `verbosity` - Verbosity level for output
    /// * `printer` - Shared printer for output
    ///
    /// # Returns
    ///
    /// A new `CodexParser` instance
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
            display_name: "Codex".to_string(),
            streaming_session: Rc::new(RefCell::new(streaming_session)),
            reasoning_accumulator: Rc::new(RefCell::new(super::types::DeltaAccumulator::new())),
            turn_counter: Rc::new(RefCell::new(0)),
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

    /// Get a shared reference to the printer.
    ///
    /// This allows tests, monitoring, and other code to access the printer after parsing
    /// to verify output content, check for duplicates, or capture output for analysis.
    ///
    /// # Returns
    ///
    /// A clone of the shared printer reference (`Rc<RefCell<dyn Printable>>`)
    #[cfg_attr(any(debug_assertions, test, feature = "monitoring"), allow(dead_code))]
    pub fn printer(&self) -> SharedPrinter {
        Rc::clone(&self.printer)
    }

    /// Get streaming quality metrics from the current session.
    ///
    /// This provides insight into the deduplication and streaming quality of the
    /// parsing session.
    ///
    /// # Returns
    ///
    /// A copy of the streaming quality metrics from the internal `StreamingSession`.
    #[cfg_attr(any(debug_assertions, test, feature = "monitoring"), allow(dead_code))]
    pub fn streaming_metrics(&self) -> StreamingQualityMetrics {
        self.streaming_session
            .borrow()
            .get_streaming_quality_metrics()
    }

    /// Parse and display a single Codex JSON event
    ///
    /// Returns `Some(formatted_output)` for valid events, or None for:
    /// - Malformed JSON (non-JSON text passed through if meaningful)
    /// - Unknown event types
    /// - Empty or whitespace-only output
    #[allow(clippy::too_many_lines)]
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
        let event: CodexEvent = if let Ok(e) = serde_json::from_str(line) {
            e
        } else {
            // Non-JSON line - pass through as-is if meaningful
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('{') {
                return Some(format!("{trimmed}\n"));
            }
            return None;
        };

        let ctx = EventHandlerContext {
            colors: &self.colors,
            verbosity: self.verbosity,
            display_name: &self.display_name,
            streaming_session: &self.streaming_session,
            reasoning_accumulator: &self.reasoning_accumulator,
            terminal_mode: *self.terminal_mode.borrow(),
            show_streaming_metrics: self.show_streaming_metrics,
        };

        match event {
            CodexEvent::ThreadStarted { thread_id } => {
                let output = handle_thread_started(&ctx, thread_id);
                if output.is_empty() {
                    None
                } else {
                    Some(output)
                }
            }
            CodexEvent::TurnStarted {} => {
                // Generate and set synthetic turn ID for duplicate detection
                let turn_id = {
                    let mut counter = self.turn_counter.borrow_mut();
                    let id = format!("turn-{}", *counter);
                    *counter += 1;
                    id
                };
                let output = handle_turn_started(&ctx, turn_id);
                if output.is_empty() {
                    None
                } else {
                    Some(output)
                }
            }
            CodexEvent::TurnCompleted { usage } => {
                let output = handle_turn_completed(&ctx, usage);
                if output.is_empty() {
                    None
                } else {
                    Some(output)
                }
            }
            CodexEvent::TurnFailed { error } => {
                let output = handle_turn_failed(&ctx, error);
                if output.is_empty() {
                    None
                } else {
                    Some(output)
                }
            }
            CodexEvent::ItemStarted { item } => handle_item_started(&ctx, item.as_ref()),
            CodexEvent::ItemCompleted { item } => handle_item_completed(&ctx, item.as_ref()),
            CodexEvent::Error { message, error } => {
                let output = handle_error(&ctx, message, error);
                if output.is_empty() {
                    None
                } else {
                    Some(output)
                }
            }
            CodexEvent::Result { result } => {
                // Result events are synthetic control events that are logged to the file
                // In debug mode, also show them on console for troubleshooting
                if self.verbosity.is_debug() {
                    result.map(|content| {
                        let limit = self.verbosity.truncate_limit("result");
                        let preview = crate::common::truncate_text(&content, limit);
                        format!(
                            "{}[{}]{} {}Result:{} {}{}{}\n",
                            self.colors.dim(),
                            self.display_name,
                            self.colors.reset(),
                            self.colors.green(),
                            self.colors.reset(),
                            self.colors.dim(),
                            preview,
                            self.colors.reset()
                        )
                    })
                } else {
                    None
                }
            }
            CodexEvent::Unknown => {
                // Use the generic unknown event formatter for consistent handling
                let output = format_unknown_json_event(
                    line,
                    &self.display_name,
                    self.colors,
                    self.verbosity.is_verbose(),
                );
                if output.is_empty() {
                    None
                } else {
                    Some(output)
                }
            }
        }
    }

    /// Check if a Codex event is a control event (state management with no user output)
    ///
    /// Control events are valid JSON that represent state transitions rather than
    /// user-facing content. They should be tracked separately from "ignored" events
    /// to avoid false health warnings.
    fn is_control_event(event: &CodexEvent) -> bool {
        match event {
            // Turn lifecycle events are control events
            CodexEvent::ThreadStarted { .. }
            | CodexEvent::TurnStarted { .. }
            | CodexEvent::TurnCompleted { .. }
            | CodexEvent::TurnFailed { .. }
            | CodexEvent::Result { .. } => true,
            // Item started/completed events are control events for certain item types
            CodexEvent::ItemStarted { item } => {
                item.as_ref().and_then(|i| i.item_type.as_deref()) == Some("plan_update")
            }
            CodexEvent::ItemCompleted { item } => {
                item.as_ref().and_then(|i| i.item_type.as_deref()) == Some("plan_update")
            }
            _ => false,
        }
    }

    /// Check if a Codex event is a partial/delta event (streaming content displayed incrementally)
    ///
    /// Partial events represent streaming content deltas (agent messages, reasoning)
    /// that are shown to the user in real-time. These should be tracked separately
    /// to avoid inflating "ignored" percentages.
    fn is_partial_event(event: &CodexEvent) -> bool {
        match event {
            // Item started events for agent_message and reasoning produce streaming content
            CodexEvent::ItemStarted { item: Some(item) } => matches!(
                item.item_type.as_deref(),
                Some("agent_message" | "reasoning")
            ),
            _ => false,
        }
    }

    /// Write a synthetic result event to the log file with accumulated content.
    ///
    /// This is called when a `TurnCompleted` event is encountered to ensure
    /// that the extraction process can find the aggregated content.
    ///
    /// # Important: Persistence Guarantees
    ///
    /// This function flushes the `BufWriter` after writing. The final `sync_all()`
    /// on the underlying `File` happens at the end of `parse_stream()` to ensure
    /// all data is committed to physical storage.
    fn write_synthetic_result_event(
        file: &mut impl std::io::Write,
        accumulated: &str,
    ) -> io::Result<()> {
        let result_event = CodexEvent::Result {
            result: Some(accumulated.to_string()),
        };
        let json = serde_json::to_string(&result_event)?;
        writeln!(file, "{json}")?;
        file.flush()?;
        Ok(())
    }

    /// Process a single JSON event line during parsing.
    ///
    /// This helper method handles the common logic for processing parsed JSON events,
    /// including debug output, event parsing, health monitoring, and log writing.
    /// It's used both for events from the streaming parser and for any remaining
    /// buffered data at the end of the stream.
    ///
    /// # Arguments
    ///
    /// * `line` - The JSON line to process
    /// * `monitor` - The health monitor to record parsing metrics (mut needed for `record_*` methods)
    /// * `log_writer` - Optional log file writer
    ///
    /// # Returns
    ///
    /// `Ok(true)` if the line was successfully processed, `Ok(false)` if the line
    /// was empty or skipped, or `Err` if an IO error occurred.
    #[allow(clippy::needless_pass_by_ref_mut)] // The monitor methods mutate internal counters
    fn process_event_line(
        &self,
        line: &str,
        monitor: &mut HealthMonitor,
        log_writer: &mut Option<std::io::BufWriter<std::fs::File>>,
    ) -> io::Result<bool> {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            return Ok(false);
        }

        if self.verbosity.is_debug() {
            let mut printer = self.printer.borrow_mut();
            writeln!(
                printer,
                "{}[DEBUG]{} {}{}{}",
                self.colors.dim(),
                self.colors.reset(),
                self.colors.dim(),
                line,
                self.colors.reset()
            )?;
            printer.flush()?;
        }

        // Parse the event once for both display/logic and synthetic result writing
        let parsed_event = if trimmed.starts_with('{') {
            serde_json::from_str::<CodexEvent>(trimmed).ok()
        } else {
            None
        };

        // Check if this is a turn.completed event using the parsed event
        let is_turn_completed = parsed_event
            .as_ref()
            .is_some_and(|e| matches!(e, CodexEvent::TurnCompleted { .. }));

        match self.parse_event(line) {
            Some(output) => {
                if let Some(event) = &parsed_event {
                    if Self::is_partial_event(event) {
                        monitor.record_partial_event();
                    } else {
                        monitor.record_parsed();
                    }
                } else {
                    monitor.record_parsed();
                }
                let mut printer = self.printer.borrow_mut();
                write!(printer, "{output}")?;
                printer.flush()?;
            }
            None => {
                if let Some(event) = &parsed_event {
                    if Self::is_control_event(event) {
                        monitor.record_control_event();
                    } else {
                        monitor.record_unknown_event();
                    }
                } else {
                    monitor.record_ignored();
                }
            }
        }

        if let Some(ref mut file) = log_writer {
            writeln!(file, "{line}")?;
            // Write synthetic result event on turn.completed to ensure content is captured
            // This handles the normal case where the stream completes properly
            if is_turn_completed {
                if let Some(accumulated) = self
                    .streaming_session
                    .borrow()
                    .get_accumulated(super::types::ContentType::Text, "agent_msg")
                {
                    // Propagate the error to ensure the result event is written
                    Self::write_synthetic_result_event(file, accumulated)?;
                    // Sync to disk to ensure result event is persisted immediately
                    file.get_mut().sync_all()?;
                }
            }
        }

        Ok(true)
    }

    /// Parse a stream of Codex NDJSON events
    pub(crate) fn parse_stream<R: BufRead>(&self, mut reader: R) -> io::Result<()> {
        use super::incremental_parser::IncrementalNdjsonParser;

        let mut monitor = HealthMonitor::new("Codex");
        let mut log_writer = self.log_file.as_ref().and_then(|log_path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .ok()
                .map(std::io::BufWriter::new)
        });

        let mut incremental_parser = IncrementalNdjsonParser::new();
        let mut byte_buffer = Vec::new();
        // Track whether we've written a synthetic result event for the current turn
        let mut result_written_for_current_turn = false;

        loop {
            byte_buffer.clear();
            let chunk = reader.fill_buf()?;
            if chunk.is_empty() {
                break;
            }
            let consumed = chunk.len();
            byte_buffer.extend_from_slice(chunk);
            reader.consume(consumed);

            for line in incremental_parser.feed(&byte_buffer) {
                // Check if this is a turn.completed event before processing
                let is_turn_completed = line.trim().starts_with('{')
                    && serde_json::from_str::<CodexEvent>(line.trim())
                        .ok()
                        .is_some_and(|e| matches!(e, CodexEvent::TurnCompleted { .. }));

                self.process_event_line(&line, &mut monitor, &mut log_writer)?;

                // Track result event writes
                if is_turn_completed {
                    result_written_for_current_turn = true;
                }
            }
        }

        // Handle any remaining buffered data when the stream ends.
        // This is important for cases where the last JSON line doesn't have a
        // trailing newline or is otherwise incomplete.
        if let Some(remaining) = incremental_parser.finish() {
            self.process_event_line(&remaining, &mut monitor, &mut log_writer)?;
        }

        // Ensure accumulated content is written even if turn.completed was not received
        // This handles the case where the stream ends unexpectedly
        if let Some(ref mut file) = log_writer {
            if !result_written_for_current_turn {
                if let Some(accumulated) = self
                    .streaming_session
                    .borrow()
                    .get_accumulated(super::types::ContentType::Text, "agent_msg")
                {
                    // Write the synthetic result event for any accumulated content
                    Self::write_synthetic_result_event(file, accumulated)?;
                }
            }
            file.flush()?;
            // Ensure data is written to disk before continuing
            // This prevents race conditions where extraction runs before OS commits writes
            file.get_mut().sync_all()?;
        }
        if let Some(warning) = monitor.check_and_warn(self.colors) {
            let mut printer = self.printer.borrow_mut();
            writeln!(printer, "{warning}")?;
        }
        Ok(())
    }
}
