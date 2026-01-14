//! Claude CLI JSON parser.
//!
//! Parses NDJSON output from Claude CLI and formats it for display.
//!
//! # Streaming Output Behavior
//!
//! This parser implements real-time streaming output for text deltas. When content
//! arrives in multiple chunks (via `content_block_delta` events), the parser:
//!
//! 1. **Accumulates** text deltas from each chunk into a buffer
//! 2. **Displays** the accumulated text after each chunk
//! 3. **Uses carriage return (`\r`)** to overwrite the previous line, creating an
//!    updating effect that shows the content building up in real-time
//! 4. **Shows prefix only once** at the start of streaming, avoiding duplicate
//!    prefixes on each line
//!
//! Example output sequence for streaming "Hello World" in two chunks:
//! ```text
//! [Claude] Hello\r          (first chunk with prefix, no newline)
//! Hello World\r              (second chunk overwrites with accumulated text)
//! Hello World\n              (message_stop adds final newline)
//! ```
//!
//! This pattern is consistent across all parsers (Claude, Codex, Gemini, `OpenCode`)
//! with variations in when the prefix is shown based on each format's event structure.

#![expect(clippy::too_many_lines)]
#![expect(clippy::items_after_statements)]

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
    format_tool_input, format_unknown_json_event, ClaudeEvent, ContentBlock, ContentBlockDelta,
    ContentType, StreamInnerEvent,
};

/// Claude event parser
///
/// Note: This parser is designed for single-threaded use only.
/// The internal state uses `Rc<RefCell<>>` for convenience, not for thread safety.
/// Do not share this parser across threads.
pub struct ClaudeParser {
    colors: Colors,
    pub(crate) verbosity: Verbosity,
    log_file: Option<String>,
    display_name: String,
    /// Unified streaming session tracker
    /// Provides single source of truth for streaming state across all content types
    streaming_session: Rc<RefCell<StreamingSession>>,
}

impl ClaudeParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self {
            colors,
            verbosity,
            log_file: None,
            display_name: "Claude".to_string(),
            streaming_session: Rc::new(RefCell::new(StreamingSession::new())),
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

    /// Check if this parser is handling a GLM agent.
    ///
    /// GLM agents are known to send snapshot-style content when deltas are expected,
    /// so we apply stricter validation and automatic conversion for them.
    fn is_glm_agent(&self) -> bool {
        // GLM agents are identified by display names containing "glm" or "ccs-glm"
        let name = self.display_name.to_lowercase();
        name.contains("glm") || name.contains("ccs")
    }

    /// Parse and display a single Claude JSON event
    ///
    /// Returns `Some(formatted_output)` for valid events, or None for:
    /// - Malformed JSON (logged at debug level)
    /// - Unknown event types
    /// - Empty or whitespace-only output
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
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
            } => {
                if subtype.as_deref() == Some("init") {
                    let sid = session_id.unwrap_or_else(|| "unknown".to_string());
                    let mut out = format!(
                        "{}[{}]{} {}Session started{} {}({:.8}...){}\n",
                        c.dim(),
                        prefix,
                        c.reset(),
                        c.cyan(),
                        c.reset(),
                        c.dim(),
                        sid,
                        c.reset()
                    );
                    if let Some(cwd) = cwd {
                        use std::fmt::Write;
                        let _ = writeln!(
                            out,
                            "{}[{}]{} {}Working dir: {}{}",
                            c.dim(),
                            prefix,
                            c.reset(),
                            c.dim(),
                            cwd,
                            c.reset()
                        );
                    }
                    out
                } else {
                    format!(
                        "{}[{}]{} {}{}{}\n",
                        c.dim(),
                        prefix,
                        c.reset(),
                        c.cyan(),
                        subtype.unwrap_or_else(|| "system".to_string()),
                        c.reset()
                    )
                }
            }
            ClaudeEvent::Assistant { message } => {
                // CRITICAL FIX: When ANY content has been streamed via deltas,
                // the Assistant event should NOT display it again.
                // The Assistant event represents the "complete" message, but if we've
                // already shown the streaming deltas, showing it again causes duplication.
                let session = self.streaming_session.borrow();

                // If ANY content was streamed for this message, skip the entire display
                // This prevents duplicate text AND duplicate tool use events
                if session.has_any_streamed_content() {
                    drop(session);
                    String::new()
                } else {
                    drop(session);
                    let mut out = String::new();
                    if let Some(msg) = message {
                        if let Some(content) = msg.content {
                            for block in content {
                                match block {
                                    ContentBlock::Text { text } => {
                                        if let Some(text) = text {
                                            let limit = self.verbosity.truncate_limit("text");
                                            let preview = truncate_text(&text, limit);
                                            use std::fmt::Write;
                                            let _ = writeln!(
                                                out,
                                                "{}[{}]{} {}{}{}",
                                                c.dim(),
                                                prefix,
                                                c.reset(),
                                                c.white(),
                                                preview,
                                                c.reset()
                                            );
                                        }
                                    }
                                    ContentBlock::ToolUse { name: tool, input } => {
                                        let tool_name =
                                            tool.unwrap_or_else(|| "unknown".to_string());
                                        use std::fmt::Write;
                                        let _ = writeln!(
                                            out,
                                            "{}[{}]{} {}Tool{}: {}{}{}",
                                            c.dim(),
                                            prefix,
                                            c.reset(),
                                            c.magenta(),
                                            c.reset(),
                                            c.bold(),
                                            tool_name,
                                            c.reset(),
                                        );
                                        // Show tool input details at Normal and above (not just Verbose)
                                        // Tool inputs provide crucial context for understanding agent actions
                                        if self.verbosity.show_tool_input() {
                                            if let Some(ref input_val) = input {
                                                let input_str = format_tool_input(input_val);
                                                let limit =
                                                    self.verbosity.truncate_limit("tool_input");
                                                let preview = truncate_text(&input_str, limit);
                                                if !preview.is_empty() {
                                                    use std::fmt::Write;
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
                                    }
                                    ContentBlock::ToolResult { content } => {
                                        if let Some(content) = content {
                                            let content_str = match content {
                                                serde_json::Value::String(s) => s,
                                                other => other.to_string(),
                                            };
                                            let limit =
                                                self.verbosity.truncate_limit("tool_result");
                                            let preview = truncate_text(&content_str, limit);
                                            use std::fmt::Write;
                                            let _ = writeln!(
                                                out,
                                                "{}[{}]{} {}Result:{} {}",
                                                c.dim(),
                                                prefix,
                                                c.reset(),
                                                c.dim(),
                                                c.reset(),
                                                preview
                                            );
                                        }
                                    }
                                    ContentBlock::Unknown => {}
                                }
                            }
                        }
                    }
                    out
                }
            }
            ClaudeEvent::User { message } => {
                if let Some(msg) = message {
                    if let Some(content) = msg.content {
                        if let Some(ContentBlock::Text { text: Some(text) }) = content.first() {
                            let limit = self.verbosity.truncate_limit("user");
                            let preview = truncate_text(text, limit);
                            return Some(format!(
                                "{}[{}]{} {}User{}: {}{}{}\n",
                                c.dim(),
                                prefix,
                                c.reset(),
                                c.blue(),
                                c.reset(),
                                c.dim(),
                                preview,
                                c.reset()
                            ));
                        }
                    }
                }
                String::new()
            }
            ClaudeEvent::Result {
                subtype,
                duration_ms,
                total_cost_usd,
                num_turns,
                result,
                error,
            } => {
                let duration_s = duration_ms.unwrap_or(0) / 1000;
                let duration_m = duration_s / 60;
                let duration_s_rem = duration_s % 60;
                let cost = total_cost_usd.unwrap_or(0.0);
                let turns = num_turns.unwrap_or(0);

                let mut out = if subtype.as_deref() == Some("success") {
                    format!(
                        "{}[{}]{} {}{} Completed{} {}({}m {}s, {} turns, ${:.4}){}\n",
                        c.dim(),
                        prefix,
                        c.reset(),
                        c.green(),
                        CHECK,
                        c.reset(),
                        c.dim(),
                        duration_m,
                        duration_s_rem,
                        turns,
                        cost,
                        c.reset()
                    )
                } else {
                    let err = error.unwrap_or_else(|| "unknown error".to_string());
                    format!(
                        "{}[{}]{} {}{} {}{}: {} {}({}m {}s){}\n",
                        c.dim(),
                        prefix,
                        c.reset(),
                        c.red(),
                        CROSS,
                        subtype.unwrap_or_else(|| "error".to_string()),
                        c.reset(),
                        err,
                        c.dim(),
                        duration_m,
                        duration_s_rem,
                        c.reset()
                    )
                };

                if let Some(result) = result {
                    let limit = self.verbosity.truncate_limit("result");
                    let preview = truncate_text(&result, limit);
                    use std::fmt::Write;
                    let _ = writeln!(
                        out,
                        "\n{}Result summary:{}\n{}{}{}",
                        c.bold(),
                        c.reset(),
                        c.dim(),
                        preview,
                        c.reset()
                    );
                }
                out
            }
            ClaudeEvent::StreamEvent { event } => {
                // Handle streaming events for delta/partial updates
                self.parse_stream_event(event)
            }
            ClaudeEvent::Unknown => {
                // Use the generic unknown event formatter for consistent handling
                // In verbose mode, this will show the event type and key fields
                // In normal mode, this returns empty string
                format_unknown_json_event(line, prefix, c, self.verbosity.is_verbose())
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
    /// - Error: Display appropriately
    ///
    /// Returns String for display content, empty String for control events.
    fn parse_stream_event(&self, event: StreamInnerEvent) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;
        let mut session = self.streaming_session.borrow_mut();

        match event {
            StreamInnerEvent::MessageStart { .. } => {
                // Clear session state on new message
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
                    ContentBlock::ToolUse {
                        name: _,
                        input: Some(i),
                    } => {
                        // Initialize tool input accumulator
                        let input_str = if let serde_json::Value::String(s) = &i {
                            s.clone()
                        } else {
                            format_tool_input(i)
                        };
                        session.on_tool_input_delta(index, &input_str);
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
            } => match delta {
                ContentBlockDelta::TextDelta { text: Some(text) } => {
                    // Check for snapshot-as-delta bug (GLM sending full accumulated content)
                    // If detected, extract only the delta portion
                    let index_str = index.to_string();
                    let is_glm = self.is_glm_agent();
                    let text_to_process = if session.is_likely_snapshot(&text, &index_str) {
                        // Snapshot detected - log warning and extract delta
                        if is_glm {
                            eprintln!(
                                "GLM contract violation: Detected snapshot-as-delta for index {index}. \
                                This is a known GLM streaming bug. Automatically converting to delta. \
                                Previous: {:?}, Received (first 100 chars): {:?}",
                                session.get_accumulated(ContentType::Text, &index_str),
                                &text.chars().take(100).collect::<String>()
                            );
                        } else {
                            eprintln!(
                                "Warning: Detected snapshot-as-delta for index {index}. \
                                Converting to delta. Previous: {:?}, Received: {:?}",
                                session.get_accumulated(ContentType::Text, &index_str),
                                text
                            );
                        }
                        match session.get_delta_from_snapshot(&text, &index_str) {
                            Ok(delta) => delta,
                            Err(e) => {
                                // Snapshot extraction failed - fall back to original text.
                                // This preserves content on false positives, though it may cause
                                // some duplication. Better to duplicate than to lose data.
                                eprintln!(
                                    "Warning: Snapshot extraction failed: {e}. \
                                     Falling back to original text to prevent data loss. \
                                     May cause some duplication.",
                                );
                                &text
                            }
                        }
                    } else {
                        // Genuine delta - use as-is
                        &text
                    };

                    // Use StreamingSession to track state and determine prefix display
                    let show_prefix = session.on_text_delta(index, text_to_process);

                    // Get accumulated text for streaming display
                    let accumulated_text = session
                        .get_accumulated(ContentType::Text, &index_str)
                        .unwrap_or("");

                    // Use TextDeltaRenderer for consistent rendering
                    if show_prefix {
                        TextDeltaRenderer::render_first_delta(accumulated_text, prefix, *c)
                    } else {
                        TextDeltaRenderer::render_subsequent_delta(accumulated_text, *c)
                    }
                }
                ContentBlockDelta::ThinkingDelta {
                    thinking: Some(text),
                } => {
                    // Track thinking deltas
                    session.on_thinking_delta(index, &text);
                    // Display thinking with visual distinction
                    Self::formatter().format_thinking(text.as_str(), prefix, *c)
                }
                ContentBlockDelta::ToolUseDelta {
                    tool_use: Some(tool_delta),
                } => {
                    // Handle tool input streaming
                    // Extract the tool input from the delta
                    let input_str =
                        tool_delta
                            .get("input")
                            .map_or_else(String::new, |input| match input {
                                serde_json::Value::String(s) => s.clone(),
                                other => format_tool_input(other),
                            });

                    if input_str.is_empty() {
                        String::new()
                    } else {
                        // Accumulate tool input
                        session.on_tool_input_delta(index, &input_str);

                        // Show partial tool input in real-time
                        let formatter = DeltaDisplayFormatter::new();
                        formatter.format_tool_input(&input_str, prefix, *c)
                    }
                }
                _ => String::new(),
            },
            #[expect(clippy::match_same_arms)]
            StreamInnerEvent::ContentBlockDelta { .. } | StreamInnerEvent::Ping => String::new(),
            StreamInnerEvent::TextDelta { text: Some(text) } => {
                // Standalone text delta (not part of content block)
                // Use default index "0" for standalone text
                let default_index = 0u64;
                let default_index_str = "0";

                // Check for snapshot-as-delta bug
                let text_to_process = if session.is_likely_snapshot(&text, default_index_str) {
                    eprintln!(
                        "Warning: Detected snapshot-as-delta for standalone text. Converting to delta."
                    );
                    match session.get_delta_from_snapshot(&text, default_index_str) {
                        Ok(delta) => delta,
                        Err(e) => {
                            // Snapshot extraction failed - fall back to original text.
                            // This preserves content on false positives, though it may cause
                            // some duplication. Better to duplicate than to lose data.
                            eprintln!(
                                "Warning: Snapshot extraction failed: {e}. \
                                 Falling back to original text to prevent data loss. \
                                 May cause some duplication.",
                            );
                            &text
                        }
                    }
                } else {
                    &text
                };

                let show_prefix = session.on_text_delta(default_index, text_to_process);
                let accumulated_text = session
                    .get_accumulated(ContentType::Text, default_index_str)
                    .unwrap_or("");

                // Use TextDeltaRenderer for consistent rendering
                if show_prefix {
                    TextDeltaRenderer::render_first_delta(accumulated_text, prefix, *c)
                } else {
                    TextDeltaRenderer::render_subsequent_delta(accumulated_text, *c)
                }
            }
            StreamInnerEvent::MessageStop => {
                // Message complete - add final newline if we were in a content block
                let was_in_block = session.on_message_stop();
                if was_in_block {
                    format!("{}{}", c.reset(), TextDeltaRenderer::render_completion())
                } else {
                    String::new()
                }
            }
            StreamInnerEvent::Error {
                error: Some(err), ..
            } => {
                let msg = err
                    .message
                    .unwrap_or_else(|| "Unknown streaming error".to_string());
                format!(
                    "{}[{}]{} {}Error: {}{}\n",
                    c.dim(),
                    prefix,
                    c.reset(),
                    c.red(),
                    msg,
                    c.reset()
                )
            }
            StreamInnerEvent::TextDelta { text: None } | StreamInnerEvent::Error { error: None } => String::new(),
            StreamInnerEvent::Unknown => {
                // Unknown stream event - in debug mode, log it
                if self.verbosity.is_debug() {
                    format!(
                        "{}[{}]{} {}Unknown streaming event{}\n",
                        c.dim(),
                        prefix,
                        c.reset(),
                        c.dim(),
                        c.reset()
                    )
                } else {
                    String::new()
                }
            }
        }
    }

    /// Check if a Claude event is a control event (state management with no user output)
    ///
    /// Control events are valid JSON that represent state transitions rather than
    /// user-facing content. They should be tracked separately from "ignored" events
    /// to avoid false health warnings.
    const fn is_control_event(event: &ClaudeEvent) -> bool {
        match event {
            // Stream events that are control events
            ClaudeEvent::StreamEvent { event } => matches!(
                event,
                StreamInnerEvent::MessageStart { .. }
                    | StreamInnerEvent::ContentBlockStart { .. }
                    | StreamInnerEvent::MessageStop
                    | StreamInnerEvent::Ping
            ),
            _ => false,
        }
    }

    /// Check if a Claude event is a partial/delta event (streaming content displayed incrementally)
    ///
    /// Partial events represent streaming content deltas (text deltas, thinking deltas,
    /// tool input deltas) that are shown to the user in real-time. These should be
    /// tracked separately to avoid inflating "ignored" percentages.
    const fn is_partial_event(event: &ClaudeEvent) -> bool {
        match event {
            // Stream events that produce incremental content
            ClaudeEvent::StreamEvent { event } => matches!(
                event,
                StreamInnerEvent::ContentBlockDelta { .. } | StreamInnerEvent::TextDelta { .. }
            ),
            _ => false,
        }
    }

    /// Get a shared delta display formatter
    const fn formatter() -> DeltaDisplayFormatter {
        DeltaDisplayFormatter::new()
    }

    /// Parse a stream of Claude NDJSON events
    pub(crate) fn parse_stream<R: BufRead, W: Write>(
        &self,
        reader: R,
        mut writer: W,
    ) -> io::Result<()> {
        let c = &self.colors;
        let monitor = HealthMonitor::new("Claude");
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
                        if let Ok(event) = serde_json::from_str::<ClaudeEvent>(&line) {
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
                    // Control events are valid JSON that return empty output but aren't "ignored"
                    if trimmed.starts_with('{') {
                        if let Ok(event) = serde_json::from_str::<ClaudeEvent>(&line) {
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
