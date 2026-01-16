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
}

impl CodexParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        let verbose_warnings = matches!(verbosity, Verbosity::Debug);
        let streaming_session = StreamingSession::new().with_verbose_warnings(verbose_warnings);
        Self {
            colors,
            verbosity,
            log_file: None,
            display_name: "Codex".to_string(),
            streaming_session: Rc::new(RefCell::new(streaming_session)),
            reasoning_accumulator: Rc::new(RefCell::new(super::types::DeltaAccumulator::new())),
            turn_counter: Rc::new(RefCell::new(0)),
            terminal_mode: RefCell::new(TerminalMode::detect()),
        }
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
    pub(crate) fn with_terminal_mode(self, mode: TerminalMode) -> Self {
        *self.terminal_mode.borrow_mut() = mode;
        self
    }

    /// Parse and display a single Codex JSON event
    ///
    /// Returns `Some(formatted_output)` for valid events, or None for:
    /// - Malformed JSON (non-JSON text passed through if meaningful)
    /// - Unknown event types
    /// - Empty or whitespace-only output
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
            | CodexEvent::TurnFailed { .. } => true,
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

    /// Parse a stream of Codex NDJSON events
    pub(crate) fn parse_stream<R: BufRead, W: Write>(
        &self,
        reader: R,
        mut writer: W,
    ) -> io::Result<()> {
        let c = &self.colors;
        let monitor = HealthMonitor::new("Codex");
        let mut log_writer = self.log_file.as_ref().and_then(|log_path| {
            std::fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(log_path)
                .ok()
                .map(std::io::BufWriter::new)
        });

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            // In debug mode, also show the raw JSON
            if self.verbosity.is_debug() {
                writeln!(
                    writer,
                    "{}[DEBUG]{} {}{}{}",
                    c.dim(),
                    c.reset(),
                    c.dim(),
                    &line,
                    c.reset()
                )?;
            }

            // Parse the event once - parse_event handles malformed JSON by returning None
            match self.parse_event(&line) {
                Some(output) => {
                    // Check if this is a partial/delta event (streaming content)
                    if trimmed.starts_with('{') {
                        if let Ok(event) = serde_json::from_str::<CodexEvent>(&line) {
                            if Self::is_partial_event(&event) {
                                monitor.record_partial_event();
                            } else {
                                monitor.record_parsed();
                            }
                        } else {
                            monitor.record_parsed();
                        }
                    } else {
                        monitor.record_parsed();
                    }
                    write!(writer, "{output}")?;
                    writer.flush()?;
                }
                None => {
                    // Check if this was a control event (state management with no user output)
                    if trimmed.starts_with('{') {
                        if let Ok(event) = serde_json::from_str::<CodexEvent>(&line) {
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

        if let Some(ref mut file) = log_writer {
            file.flush()?;
        }
        if let Some(warning) = monitor.check_and_warn(*c) {
            writeln!(writer, "{warning}")?;
        }
        Ok(())
    }
}
