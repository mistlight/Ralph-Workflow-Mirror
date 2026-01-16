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
//! 3. **Uses carriage return (`\r`) and line clearing (`\x1b[2K`)** to rewrite the entire line,
//!    creating an updating effect that shows the content building up in real-time
//! 4. **Shows prefix on every delta**, rewriting the entire line each time (industry standard)
//!
//! Example output sequence for streaming "Hello World" in two chunks:
//! ```text
//! [Claude] Hello\r          (first chunk with prefix, no newline)
//! \x1b[2K\r[Claude] Hello World\r  (second chunk clears line, rewrites with accumulated)
//! [Claude] Hello World\n    (message_stop adds final newline)
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
//!
//! This pattern is consistent across all parsers (Claude, Codex, Gemini, `OpenCode`)
//! with variations in when the prefix is shown based on each format's event structure.

use crate::common::truncate_text;
use crate::config::Verbosity;
use crate::logger::{Colors, CHECK, CROSS};
use std::cell::RefCell;
use std::fmt::Write as _;
use std::io::{self, BufRead, Write};
use std::rc::Rc;

use super::delta_display::{DeltaDisplayFormatter, DeltaRenderer, TextDeltaRenderer};
use super::health::HealthMonitor;
use super::streaming_state::StreamingSession;
use super::terminal::TerminalMode;
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
    /// Terminal mode for output formatting
    /// Detected at parse time and cached for performance
    terminal_mode: RefCell<TerminalMode>,
    /// Whether to show streaming quality metrics
    show_streaming_metrics: bool,
}

impl ClaudeParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        let verbose_warnings = matches!(verbosity, Verbosity::Debug);
        let streaming_session = StreamingSession::new().with_verbose_warnings(verbose_warnings);
        Self {
            colors,
            verbosity,
            log_file: None,
            display_name: "Claude".to_string(),
            streaming_session: Rc::new(RefCell::new(streaming_session)),
            terminal_mode: RefCell::new(TerminalMode::detect()),
            show_streaming_metrics: false,
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
    pub(crate) fn with_terminal_mode(self, mode: TerminalMode) -> Self {
        *self.terminal_mode.borrow_mut() = mode;
        self
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
                message: _,
                message_id,
            } => {
                // Set message ID for tracking and clear session state on new message
                session.set_current_message_id(message_id);
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

    /// Format a system event
    fn format_system_event(
        &self,
        subtype: Option<&String>,
        session_id: Option<String>,
        cwd: Option<String>,
    ) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        if subtype.map(std::string::String::as_str) == Some("init") {
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
                subtype.map_or("system", |s| s.as_str()),
                c.reset()
            )
        }
    }

    /// Extract text content from a message for hash-based deduplication.
    fn extract_text_content_for_hash(
        message: Option<&crate::json_parser::types::AssistantMessage>,
    ) -> Option<String> {
        message?.content.as_ref().map(|content| {
            content
                .iter()
                .filter_map(|block| {
                    if let ContentBlock::Text { text } = block {
                        text.as_deref()
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>()
                .join("")
        })
    }

    /// Check if this assistant message is a duplicate of already-streamed content.
    fn is_duplicate_assistant_message(
        &self,
        message: Option<&crate::json_parser::types::AssistantMessage>,
    ) -> bool {
        let session = self.streaming_session.borrow();
        let text_content_for_hash = Self::extract_text_content_for_hash(message);

        session.get_current_message_id().map_or_else(
            || {
                // Try hash-based deduplication first (more precise)
                if let Some(text_content) = text_content_for_hash {
                    if !text_content.is_empty() {
                        return session.is_duplicate_by_hash(&text_content);
                    }
                }
                // Fallback to coarse check
                session.has_any_streamed_content()
            },
            |message_id| session.is_duplicate_final_message(message_id),
        )
    }

    /// Format a text content block for assistant output.
    fn format_text_block(&self, out: &mut String, text: &str, prefix: &str, colors: Colors) {
        let limit = self.verbosity.truncate_limit("text");
        let preview = truncate_text(text, limit);
        let _ = writeln!(
            out,
            "{}[{}]{} {}{}{}",
            colors.dim(),
            prefix,
            colors.reset(),
            colors.white(),
            preview,
            colors.reset()
        );
    }

    /// Format a tool use content block for assistant output.
    fn format_tool_use_block(
        &self,
        out: &mut String,
        tool: Option<&String>,
        input: Option<&serde_json::Value>,
        prefix: &str,
        colors: Colors,
    ) {
        let tool_name = tool.cloned().unwrap_or_else(|| "unknown".to_string());
        let _ = writeln!(
            out,
            "{}[{}]{} {}Tool{}: {}{}{}",
            colors.dim(),
            prefix,
            colors.reset(),
            colors.magenta(),
            colors.reset(),
            colors.bold(),
            tool_name,
            colors.reset(),
        );

        // Show tool input details at Normal and above (not just Verbose)
        // Tool inputs provide crucial context for understanding agent actions
        if self.verbosity.show_tool_input() {
            if let Some(input_val) = input {
                let input_str = format_tool_input(input_val);
                let limit = self.verbosity.truncate_limit("tool_input");
                let preview = truncate_text(&input_str, limit);
                if !preview.is_empty() {
                    let _ = writeln!(
                        out,
                        "{}[{}]{} {}  └─ {}{}",
                        colors.dim(),
                        prefix,
                        colors.reset(),
                        colors.dim(),
                        preview,
                        colors.reset()
                    );
                }
            }
        }
    }

    /// Format a tool result content block for assistant output.
    fn format_tool_result_block(
        &self,
        out: &mut String,
        content: &serde_json::Value,
        prefix: &str,
        colors: Colors,
    ) {
        let content_str = match content {
            serde_json::Value::String(s) => s.clone(),
            other => other.to_string(),
        };
        let limit = self.verbosity.truncate_limit("tool_result");
        let preview = truncate_text(&content_str, limit);
        let _ = writeln!(
            out,
            "{}[{}]{} {}Result:{} {}",
            colors.dim(),
            prefix,
            colors.reset(),
            colors.dim(),
            colors.reset(),
            preview
        );
    }

    /// Format all content blocks from an assistant message.
    fn format_content_blocks(
        &self,
        out: &mut String,
        content: &[ContentBlock],
        prefix: &str,
        colors: Colors,
    ) {
        for block in content {
            match block {
                ContentBlock::Text { text } => {
                    if let Some(text) = text {
                        self.format_text_block(out, text, prefix, colors);
                    }
                }
                ContentBlock::ToolUse { name, input } => {
                    self.format_tool_use_block(out, name.as_ref(), input.as_ref(), prefix, colors);
                }
                ContentBlock::ToolResult { content } => {
                    if let Some(content) = content {
                        self.format_tool_result_block(out, content, prefix, colors);
                    }
                }
                ContentBlock::Unknown => {}
            }
        }
    }

    /// Format an assistant event
    fn format_assistant_event(
        &self,
        message: Option<crate::json_parser::types::AssistantMessage>,
    ) -> String {
        // CRITICAL FIX: When ANY content has been streamed via deltas,
        // the Assistant event should NOT display it again.
        // The Assistant event represents the "complete" message, but if we've
        // already shown the streaming deltas, showing it again causes duplication.
        if self.is_duplicate_assistant_message(message.as_ref()) {
            return String::new();
        }

        let mut out = String::new();
        if let Some(msg) = message {
            if let Some(content) = msg.content {
                self.format_content_blocks(&mut out, &content, &self.display_name, self.colors);
            }
        }
        out
    }

    /// Format a user event
    fn format_user_event(&self, message: Option<crate::json_parser::types::UserMessage>) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        if let Some(msg) = message {
            if let Some(content) = msg.content {
                if let Some(ContentBlock::Text { text: Some(text) }) = content.first() {
                    let limit = self.verbosity.truncate_limit("user");
                    let preview = truncate_text(text, limit);
                    return format!(
                        "{}[{}]{} {}User{}: {}{}{}\n",
                        c.dim(),
                        prefix,
                        c.reset(),
                        c.blue(),
                        c.reset(),
                        c.dim(),
                        preview,
                        c.reset()
                    );
                }
            }
        }
        String::new()
    }

    /// Format a result event
    fn format_result_event(
        &self,
        subtype: Option<String>,
        duration_ms: Option<u64>,
        total_cost_usd: Option<f64>,
        num_turns: Option<u32>,
        result: Option<String>,
        error: Option<String>,
    ) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        let duration_total_secs = duration_ms.unwrap_or(0) / 1000;
        let duration_m = duration_total_secs / 60;
        let duration_s_rem = duration_total_secs % 60;
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

    /// Handle content block delta events
    fn handle_content_block_delta(
        &self,
        session: &mut std::cell::RefMut<'_, StreamingSession>,
        index: u64,
        delta: ContentBlockDelta,
    ) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        match delta {
            ContentBlockDelta::TextDelta { text: Some(text) } => {
                // Check for snapshot-as-delta bug (GLM sending full accumulated content)
                // If detected, extract only the delta portion
                let index_str = index.to_string();
                let text_to_process = if session.is_likely_snapshot(&text, &index_str) {
                    // Snapshot detected - extract delta (auto-corrects GLM/CCS quirk)
                    session
                        .get_delta_from_snapshot(&text, &index_str)
                        .unwrap_or(&text)
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

                // Skip rendering if accumulated content is unchanged (prevents visual repetition)
                if session.should_skip_render(ContentType::Text, &index_str) {
                    return String::new();
                }

                // Use TextDeltaRenderer for consistent rendering
                let terminal_mode = *self.terminal_mode.borrow();
                let output = if show_prefix {
                    TextDeltaRenderer::render_first_delta(
                        accumulated_text,
                        prefix,
                        *c,
                        terminal_mode,
                    )
                } else {
                    TextDeltaRenderer::render_subsequent_delta(
                        accumulated_text,
                        prefix,
                        *c,
                        terminal_mode,
                    )
                };

                // Mark this content as rendered
                session.mark_rendered(ContentType::Text, &index_str);

                output
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
        }
    }

    /// Handle text delta events
    fn handle_text_delta(
        &self,
        session: &mut std::cell::RefMut<'_, StreamingSession>,
        text: &str,
    ) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        // Standalone text delta (not part of content block)
        // Use default index "0" for standalone text
        let default_index = 0u64;
        let default_index_str = "0";

        // Check for snapshot-as-delta bug
        let text_to_process = if session.is_likely_snapshot(text, default_index_str) {
            // Snapshot detected - extract delta (auto-corrects GLM/CCS quirk)
            session
                .get_delta_from_snapshot(text, default_index_str)
                .unwrap_or(text)
        } else {
            text
        };

        let show_prefix = session.on_text_delta(default_index, text_to_process);
        let accumulated_text = session
            .get_accumulated(ContentType::Text, default_index_str)
            .unwrap_or("");

        // Skip rendering if accumulated content is unchanged (prevents visual repetition)
        if session.should_skip_render(ContentType::Text, default_index_str) {
            return String::new();
        }

        // Use TextDeltaRenderer for consistent rendering across all parsers
        let terminal_mode = *self.terminal_mode.borrow();
        let output = if show_prefix {
            // First delta - use the renderer with prefix
            TextDeltaRenderer::render_first_delta(accumulated_text, prefix, *c, terminal_mode)
        } else {
            // Subsequent delta - use renderer for in-place update
            TextDeltaRenderer::render_subsequent_delta(accumulated_text, prefix, *c, terminal_mode)
        };

        // Mark this content as rendered
        session.mark_rendered(ContentType::Text, default_index_str);

        output
    }

    /// Handle message stop events
    fn handle_message_stop(&self, session: &mut std::cell::RefMut<'_, StreamingSession>) -> String {
        let c = &self.colors;

        // Message complete - add final newline if we were in a content block
        // OR if any content was streamed (handles edge cases where block state
        // may not have been set but content was still streamed)
        let metrics = session.get_streaming_quality_metrics();
        let was_in_block = session.on_message_stop();
        let had_content = session.has_any_streamed_content();
        if was_in_block || had_content {
            // Use TextDeltaRenderer for completion - adds final newline
            let terminal_mode = *self.terminal_mode.borrow();
            let completion = format!(
                "{}{}",
                c.reset(),
                TextDeltaRenderer::render_completion(terminal_mode)
            );
            // Show streaming quality metrics in debug mode or when flag is set
            let show_metrics = (self.verbosity.is_debug() || self.show_streaming_metrics)
                && metrics.total_deltas > 0;
            if show_metrics {
                format!("{}\n{}", completion, metrics.format(*c))
            } else {
                completion
            }
        } else {
            String::new()
        }
    }

    /// Handle error events
    fn handle_error_event(&self, err: crate::json_parser::types::StreamError) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

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

    /// Handle unknown events
    fn handle_unknown_event(&self) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

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
                    | StreamInnerEvent::ContentBlockStop { .. }
                    | StreamInnerEvent::MessageDelta { .. }
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
        mut reader: R,
        mut writer: W,
    ) -> io::Result<()> {
        use super::incremental_parser::IncrementalNdjsonParser;

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
                    writeln!(
                        writer,
                        "{}[DEBUG]{} {}{}{}",
                        c.dim(),
                        c.reset(),
                        c.dim(),
                        &line,
                        c.reset()
                    )?;
                    writer.flush()?;
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
