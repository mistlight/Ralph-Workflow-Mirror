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
//! 3. **Uses carriage return (`\r`)** to overwrite the previous line, creating an
//!    updating effect that shows the content building up in real-time
//! 4. **Shows prefix** on the first `item.started` event and again on `item.completed`
//!
//! Example output sequence for streaming "Hello World" in two chunks:
//! ```text
//! [Codex] Hello\r          (first chunk with prefix, no newline)
//! Hello World\r              (second chunk overwrites with accumulated text)
//! [Codex] Hello World\n     (item.completed shows final result with prefix)
//! ```

#![expect(clippy::too_many_lines)]
use crate::common::truncate_text;
use crate::config::Verbosity;
use crate::logger::{Colors, CHECK, CROSS};
use std::cell::RefCell;
use std::io::{self, BufRead, Write};
use std::rc::Rc;

use super::delta_display::{DeltaDisplayFormatter, DeltaRenderer, TextDeltaRenderer};
use super::health::HealthMonitor;
use super::streaming_state::StreamingSession;
use super::types::{
    format_tool_input, format_unknown_json_event, CodexEvent, ContentType, DeltaAccumulator,
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
    reasoning_accumulator: Rc<RefCell<DeltaAccumulator>>,
    /// Turn counter for generating synthetic turn IDs
    turn_counter: Rc<RefCell<u64>>,
}

impl CodexParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self {
            colors,
            verbosity,
            log_file: None,
            display_name: "Codex".to_string(),
            streaming_session: Rc::new(RefCell::new(StreamingSession::new())),
            reasoning_accumulator: Rc::new(RefCell::new(DeltaAccumulator::new())),
            turn_counter: Rc::new(RefCell::new(0)),
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
        let c = &self.colors;
        let name = &self.display_name;

        let output = match event {
            CodexEvent::ThreadStarted { thread_id } => {
                let tid = thread_id.unwrap_or_else(|| "unknown".to_string());
                // Set the current message ID for duplicate detection
                self.streaming_session
                    .borrow_mut()
                    .set_current_message_id(Some(tid.clone()));
                format!(
                    "{}[{}]{} {}Thread started{} {}({:.8}...){}\n",
                    c.dim(),
                    name,
                    c.reset(),
                    c.cyan(),
                    c.reset(),
                    c.dim(),
                    tid,
                    c.reset()
                )
            }
            CodexEvent::TurnStarted {} => {
                // Reset streaming state on new turn
                self.streaming_session.borrow_mut().on_message_start();
                self.reasoning_accumulator.borrow_mut().clear();

                // Generate and set synthetic turn ID for duplicate detection
                let turn_id = {
                    let mut counter = self.turn_counter.borrow_mut();
                    let id = format!("turn-{}", *counter);
                    *counter += 1;
                    id
                };
                self.streaming_session
                    .borrow_mut()
                    .set_current_message_id(Some(turn_id));

                format!(
                    "{}[{}]{} {}Turn started{}\n",
                    c.dim(),
                    name,
                    c.reset(),
                    c.blue(),
                    c.reset()
                )
            }
            CodexEvent::TurnCompleted { usage } => {
                // Mark the turn message as complete
                let was_in_block = self.streaming_session.borrow_mut().on_message_stop();

                let (input, output) = usage.map_or((0, 0), |u| {
                    (u.input_tokens.unwrap_or(0), u.output_tokens.unwrap_or(0))
                });

                // Add completion newline if we were in a content block
                let completion = if was_in_block {
                    TextDeltaRenderer::render_completion()
                } else {
                    String::new()
                };

                format!(
                    "{}{}[{}]{} {}{} Turn completed{} {}(in:{} out:{}){}\n",
                    completion,
                    c.dim(),
                    name,
                    c.reset(),
                    c.green(),
                    CHECK,
                    c.reset(),
                    c.dim(),
                    input,
                    output,
                    c.reset()
                )
            }
            CodexEvent::TurnFailed { error } => {
                // Mark the turn message as complete even on failure
                let was_in_block = self.streaming_session.borrow_mut().on_message_stop();

                // Add completion newline if we were in a content block
                let completion = if was_in_block {
                    TextDeltaRenderer::render_completion()
                } else {
                    String::new()
                };

                let err = error.unwrap_or_else(|| "unknown error".to_string());
                format!(
                    "{}{}[{}]{} {}{} Turn failed:{} {}\n",
                    completion,
                    c.dim(),
                    name,
                    c.reset(),
                    c.red(),
                    CROSS,
                    c.reset(),
                    err
                )
            }
            CodexEvent::ItemStarted { item } => {
                if let Some(item) = item {
                    match item.item_type.as_deref() {
                        Some("command_execution") => {
                            let cmd = item.command.clone().unwrap_or_default();
                            let limit = self.verbosity.truncate_limit("command");
                            let preview = truncate_text(&cmd, limit);
                            format!(
                                "{}[{}]{} {}Exec{}: {}{}{}\n",
                                c.dim(),
                                name,
                                c.reset(),
                                c.magenta(),
                                c.reset(),
                                c.white(),
                                preview,
                                c.reset()
                            )
                        }
                        Some("agent_message") => {
                            // For streaming support, accumulate partial content
                            if let Some(ref text) = item.text {
                                let (show_prefix, accumulated_text) = {
                                    let mut session = self.streaming_session.borrow_mut();
                                    let show_prefix = session.on_text_delta_key("agent_msg", text);
                                    // Get accumulated text for streaming display
                                    let accumulated_text = session
                                        .get_accumulated(ContentType::Text, "agent_msg")
                                        .unwrap_or("")
                                        .to_string();
                                    (show_prefix, accumulated_text)
                                };

                                // Use TextDeltaRenderer for consistent rendering across all parsers
                                if show_prefix {
                                    // First delta: use renderer with prefix
                                    return Some(TextDeltaRenderer::render_first_delta(
                                        &accumulated_text,
                                        name,
                                        *c,
                                    ));
                                }
                                // Subsequent deltas: use renderer for in-place update
                                return Some(TextDeltaRenderer::render_subsequent_delta(
                                    &accumulated_text,
                                    *c,
                                ));
                            }
                            // No text yet, show placeholder in non-verbose mode
                            if self.verbosity.is_verbose() {
                                String::new()
                            } else {
                                format!(
                                    "{}[{}]{} {}Thinking...{}\n",
                                    c.dim(),
                                    name,
                                    c.reset(),
                                    c.blue(),
                                    c.reset()
                                )
                            }
                        }
                        Some("reasoning") => {
                            // For streaming support, accumulate reasoning content
                            if let Some(ref text) = item.text {
                                let mut acc = self.reasoning_accumulator.borrow_mut();
                                acc.add_delta(ContentType::Thinking, "reasoning", text);

                                // Show reasoning in real-time using delta display formatter
                                let formatter = DeltaDisplayFormatter::new();
                                return Some(formatter.format_thinking(text, name, *c));
                            }
                            // No reasoning yet
                            if self.verbosity.is_verbose() {
                                format!(
                                    "{}[{}]{} {}Reasoning...{}\n",
                                    c.dim(),
                                    name,
                                    c.reset(),
                                    c.cyan(),
                                    c.reset()
                                )
                            } else {
                                String::new()
                            }
                        }
                        Some("file_read" | "file_write") => {
                            let path = item.path.clone().unwrap_or_default();
                            let action = item.item_type.as_deref().unwrap_or("file");
                            format!(
                                "{}[{}]{} {}{}:{} {}\n",
                                c.dim(),
                                name,
                                c.reset(),
                                c.yellow(),
                                action,
                                c.reset(),
                                path
                            )
                        }
                        Some("mcp_tool_call" | "mcp") => {
                            let tool_name =
                                item.tool.clone().unwrap_or_else(|| "unknown".to_string());
                            let mut out = format!(
                                "{}[{}]{} {}MCP Tool{}: {}{}{}\n",
                                c.dim(),
                                name,
                                c.reset(),
                                c.magenta(),
                                c.reset(),
                                c.bold(),
                                tool_name,
                                c.reset()
                            );
                            // Show tool arguments at Normal+ verbosity
                            if self.verbosity.show_tool_input() {
                                if let Some(ref args) = item.arguments {
                                    let args_str = format_tool_input(args);
                                    let limit = self.verbosity.truncate_limit("tool_input");
                                    let preview = truncate_text(&args_str, limit);
                                    if !preview.is_empty() {
                                        use std::fmt::Write;
                                        let _ = writeln!(
                                            out,
                                            "{}[{}]{} {}  └─ {}{}",
                                            c.dim(),
                                            name,
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
                        Some("web_search") => {
                            let query = item.query.clone().unwrap_or_default();
                            let limit = self.verbosity.truncate_limit("command");
                            let preview = truncate_text(&query, limit);
                            format!(
                                "{}[{}]{} {}Search{}: {}{}{}\n",
                                c.dim(),
                                name,
                                c.reset(),
                                c.cyan(),
                                c.reset(),
                                c.white(),
                                preview,
                                c.reset()
                            )
                        }
                        Some("plan_update") => {
                            format!(
                                "{}[{}]{} {}Updating plan...{}\n",
                                c.dim(),
                                name,
                                c.reset(),
                                c.blue(),
                                c.reset()
                            )
                        }
                        Some(t) => {
                            // Show other types in verbose mode
                            if self.verbosity.is_verbose() {
                                format!(
                                    "{}[{}]{} {}{}:{} {}\n",
                                    c.dim(),
                                    name,
                                    c.reset(),
                                    c.dim(),
                                    t,
                                    c.reset(),
                                    item.path.clone().unwrap_or_default()
                                )
                            } else {
                                String::new()
                            }
                        }
                        None => String::new(),
                    }
                } else {
                    String::new()
                }
            }
            CodexEvent::ItemCompleted { item } => {
                if let Some(item) = item {
                    match item.item_type.as_deref() {
                        Some("agent_message") => {
                            // Check for duplicate final message using message ID or fallback to streaming content check
                            let session = self.streaming_session.borrow();
                            let is_duplicate = session.get_current_message_id().map_or_else(
                                || session.has_any_streamed_content(),
                                |message_id| session.is_duplicate_final_message(message_id),
                            );
                            let was_streaming = session.has_any_streamed_content();
                            drop(session);

                            // Finalize the message (this marks it as displayed)
                            let _was_in_block =
                                self.streaming_session.borrow_mut().on_message_stop();

                            // If this is a duplicate or content was streamed, use TextDeltaRenderer for completion
                            if is_duplicate || was_streaming {
                                return Some(TextDeltaRenderer::render_completion());
                            }

                            // Fallback: show item text if no streaming occurred
                            if let Some(ref text) = item.text {
                                let limit = self.verbosity.truncate_limit("agent_msg");
                                let preview = truncate_text(text, limit);
                                return Some(format!(
                                    "{}[{}]{} {}{}{}\n",
                                    c.dim(),
                                    name,
                                    c.reset(),
                                    c.white(),
                                    preview,
                                    c.reset()
                                ));
                            }
                            String::new()
                        }
                        Some("reasoning") => {
                            // Clear reasoning accumulator on completion
                            let full_reasoning = self
                                .reasoning_accumulator
                                .borrow()
                                .get(ContentType::Thinking, "reasoning")
                                .map(std::string::ToString::to_string);
                            self.reasoning_accumulator
                                .borrow_mut()
                                .clear_key(ContentType::Thinking, "reasoning");

                            // Show reasoning content in verbose mode
                            if self.verbosity.is_verbose() {
                                if let Some(text) = full_reasoning.as_ref().or(item.text.as_ref()) {
                                    let limit = self.verbosity.truncate_limit("text");
                                    let preview = truncate_text(text, limit);
                                    return Some(format!(
                                        "{}[{}]{} {}Thought:{} {}{}{}\n",
                                        c.dim(),
                                        name,
                                        c.reset(),
                                        c.cyan(),
                                        c.reset(),
                                        c.dim(),
                                        preview,
                                        c.reset()
                                    ));
                                }
                            }
                            String::new()
                        }
                        Some("command_execution") => {
                            format!(
                                "{}[{}]{} {}{} Command done{}\n",
                                c.dim(),
                                name,
                                c.reset(),
                                c.green(),
                                CHECK,
                                c.reset()
                            )
                        }
                        Some("file_change" | "file_write") => {
                            let path = item.path.clone().unwrap_or_else(|| "unknown".to_string());
                            format!(
                                "{}[{}]{} {}File{}: {}\n",
                                c.dim(),
                                name,
                                c.reset(),
                                c.yellow(),
                                c.reset(),
                                path
                            )
                        }
                        Some("file_read") => {
                            // Only show file read completion in verbose mode
                            if self.verbosity.is_verbose() {
                                let path =
                                    item.path.clone().unwrap_or_else(|| "unknown".to_string());
                                format!(
                                    "{}[{}]{} {}{} Read:{} {}\n",
                                    c.dim(),
                                    name,
                                    c.reset(),
                                    c.green(),
                                    CHECK,
                                    c.reset(),
                                    path
                                )
                            } else {
                                String::new()
                            }
                        }
                        Some("mcp_tool_call" | "mcp") => {
                            let tool_name = item.tool.clone().unwrap_or_else(|| "tool".to_string());
                            format!(
                                "{}[{}]{} {}{} MCP:{} {} done\n",
                                c.dim(),
                                name,
                                c.reset(),
                                c.green(),
                                CHECK,
                                c.reset(),
                                tool_name
                            )
                        }
                        Some("web_search") => {
                            format!(
                                "{}[{}]{} {}{} Search completed{}\n",
                                c.dim(),
                                name,
                                c.reset(),
                                c.green(),
                                CHECK,
                                c.reset()
                            )
                        }
                        Some("plan_update") => {
                            if self.verbosity.is_verbose() {
                                let limit = self.verbosity.truncate_limit("text");
                                item.plan.as_ref().map_or_else(
                                    || {
                                        format!(
                                            "{}[{}]{} {}{} Plan updated{}\n",
                                            c.dim(),
                                            name,
                                            c.reset(),
                                            c.green(),
                                            CHECK,
                                            c.reset()
                                        )
                                    },
                                    |plan| {
                                        let preview = truncate_text(plan, limit);
                                        format!(
                                            "{}[{}]{} {}Plan:{} {}\n",
                                            c.dim(),
                                            name,
                                            c.reset(),
                                            c.blue(),
                                            c.reset(),
                                            preview
                                        )
                                    },
                                )
                            } else {
                                String::new()
                            }
                        }
                        _ => String::new(),
                    }
                } else {
                    String::new()
                }
            }
            CodexEvent::Error { message, error } => {
                let err = message
                    .or(error)
                    .unwrap_or_else(|| "unknown error".to_string());
                format!(
                    "{}[{}]{} {}{} Error:{} {}\n",
                    c.dim(),
                    name,
                    c.reset(),
                    c.red(),
                    CROSS,
                    c.reset(),
                    err
                )
            }
            CodexEvent::Unknown => {
                // Use the generic unknown event formatter for consistent handling
                format_unknown_json_event(line, name, c, self.verbosity.is_verbose())
            }
        };

        if output.is_empty() {
            None
        } else {
            Some(output)
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
