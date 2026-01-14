//! Claude CLI JSON parser.
//!
//! Parses NDJSON output from Claude CLI and formats it for display.

use crate::colors::{Colors, CHECK, CROSS};
use crate::config::Verbosity;
use crate::utils::truncate_text;
use std::cell::RefCell;
use std::io::{self, BufRead, Write};
use std::rc::Rc;

use super::health::HealthMonitor;
use super::types::{
    format_tool_input, ClaudeEvent, ContentBlock, ContentBlockDelta, DeltaAccumulator,
    StreamInnerEvent,
};

/// Claude event parser
pub(crate) struct ClaudeParser {
    colors: Colors,
    pub(crate) verbosity: Verbosity,
    log_file: Option<String>,
    display_name: String,
    /// Delta accumulator for streaming content
    delta_accumulator: Rc<RefCell<DeltaAccumulator>>,
}

impl ClaudeParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self {
            colors,
            verbosity,
            log_file: None,
            display_name: "Claude".to_string(),
            delta_accumulator: Rc::new(RefCell::new(DeltaAccumulator::new())),
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

    /// Parse and display a single Claude JSON event
    ///
    /// Returns Some(formatted_output) for valid events, or None for:
    /// - Malformed JSON (logged at debug level)
    /// - Unknown event types
    /// - Empty or whitespace-only output
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
        let event: ClaudeEvent = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => {
                // Non-JSON line - could be raw text output from agent
                // Pass through as-is if it looks like real output (not empty)
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('{') {
                    return Some(format!("{}\n", trimmed));
                }
                return None;
            }
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
                        out.push_str(&format!(
                            "{}[{}]{} {}Working dir: {}{}\n",
                            c.dim(),
                            prefix,
                            c.reset(),
                            c.dim(),
                            cwd,
                            c.reset()
                        ));
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
                let mut out = String::new();
                if let Some(msg) = message {
                    if let Some(content) = msg.content {
                        for block in content {
                            match block {
                                ContentBlock::Text { text } => {
                                    if let Some(text) = text {
                                        let limit = self.verbosity.truncate_limit("text");
                                        let preview = truncate_text(&text, limit);
                                        out.push_str(&format!(
                                            "{}[{}]{} {}{}{}\n",
                                            c.dim(),
                                            prefix,
                                            c.reset(),
                                            c.white(),
                                            preview,
                                            c.reset()
                                        ));
                                    }
                                }
                                ContentBlock::ToolUse { name: tool, input } => {
                                    let tool_name = tool.unwrap_or_else(|| "unknown".to_string());
                                    out.push_str(&format!(
                                        "{}[{}]{} {}Tool{}: {}{}{}\n",
                                        c.dim(),
                                        prefix,
                                        c.reset(),
                                        c.magenta(),
                                        c.reset(),
                                        c.bold(),
                                        tool_name,
                                        c.reset(),
                                    ));
                                    // Show tool input details at Normal and above (not just Verbose)
                                    // Tool inputs provide crucial context for understanding agent actions
                                    if self.verbosity.show_tool_input() {
                                        if let Some(ref input_val) = input {
                                            let input_str = format_tool_input(input_val);
                                            let limit = self.verbosity.truncate_limit("tool_input");
                                            let preview = truncate_text(&input_str, limit);
                                            if !preview.is_empty() {
                                                out.push_str(&format!(
                                                    "{}[{}]{} {}  └─ {}{}\n",
                                                    c.dim(),
                                                    prefix,
                                                    c.reset(),
                                                    c.dim(),
                                                    preview,
                                                    c.reset()
                                                ));
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
                                        let limit = self.verbosity.truncate_limit("tool_result");
                                        let preview = truncate_text(&content_str, limit);
                                        out.push_str(&format!(
                                            "{}[{}]{} {}Result:{} {}\n",
                                            c.dim(),
                                            prefix,
                                            c.reset(),
                                            c.dim(),
                                            c.reset(),
                                            preview
                                        ));
                                    }
                                }
                                ContentBlock::Unknown => {}
                            }
                        }
                    }
                }
                out
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
                    out.push_str(&format!(
                        "\n{}Result summary:{}\n{}{}{}\n",
                        c.bold(),
                        c.reset(),
                        c.dim(),
                        preview,
                        c.reset()
                    ));
                }
                out
            }
            ClaudeEvent::StreamEvent { event } => {
                // Handle streaming events for delta/partial updates
                self.parse_stream_event(event)
            }
            ClaudeEvent::Unknown => {
                // In verbose/debug mode, show information about unknown events
                if self.verbosity.is_verbose() {
                    self.format_unknown_event(line)
                } else {
                    String::new()
                }
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
    /// - ContentBlockStart: Initialize new content blocks
    /// - ContentBlockDelta/TextDelta: Accumulate and display incrementally
    /// - Error: Display appropriately
    fn parse_stream_event(&self, event: StreamInnerEvent) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;
        let mut acc = self.delta_accumulator.borrow_mut();

        match event {
            StreamInnerEvent::MessageStart { .. } => {
                // Clear accumulator on new message
                acc.clear();
                String::new()
            }
            StreamInnerEvent::ContentBlockStart {
                index: Some(index), ..
            } => {
                // Initialize a new content block at this index
                acc.clear_index(index);
                String::new()
            }
            StreamInnerEvent::ContentBlockStart { .. } => String::new(),
            StreamInnerEvent::ContentBlockDelta {
                index: Some(index),
                delta: Some(delta),
            } => match delta {
                ContentBlockDelta::TextDelta { text: Some(text) } => {
                    // Accumulate and display the text delta
                    acc.add_text_delta(index, &text);
                    // In verbose mode, show the full accumulated text so far
                    if self.verbosity.is_verbose() {
                        if let Some(full_text) = acc.get_text(&index) {
                            return format!(
                                "{}[{}]{} {}{}{}\n",
                                c.dim(),
                                prefix,
                                c.reset(),
                                c.white(),
                                full_text,
                                c.reset()
                            );
                        }
                    }
                    // Otherwise, just show the delta (real-time streaming)
                    format!(
                        "{}[{}]{} {}{}{}\n",
                        c.dim(),
                        prefix,
                        c.reset(),
                        c.white(),
                        text,
                        c.reset()
                    )
                }
                ContentBlockDelta::ThinkingDelta { thinking: Some(text) } => {
                    // Accumulate thinking content
                    acc.add_thinking_delta(index, &text);
                    // Display thinking in a different style
                    format!(
                        "{}[{}]{} {}Thinking: {}{}\n",
                        c.dim(),
                        prefix,
                        c.reset(),
                        c.dim(),
                        text,
                        c.reset()
                    )
                }
                ContentBlockDelta::ToolUseDelta { .. } => {
                    // Tool use deltas are less common, show minimal info
                    String::new()
                }
                _ => String::new(),
            },
            StreamInnerEvent::ContentBlockDelta { .. } => String::new(),
            StreamInnerEvent::TextDelta { text: Some(text) } => {
                // Standalone text delta (not part of content block)
                // Display incrementally for real-time feedback
                format!(
                    "{}[{}]{} {}{}{}\n",
                    c.dim(),
                    prefix,
                    c.reset(),
                    c.white(),
                    text,
                    c.reset()
                )
            }
            StreamInnerEvent::TextDelta { .. } => String::new(),
            StreamInnerEvent::MessageStop => {
                // Message complete - we could show final accumulated state here
                // For now, just clear the accumulator
                acc.clear();
                String::new()
            }
            StreamInnerEvent::Error {
                error: Some(err), ..
            } => {
                let msg = err.message.unwrap_or_else(|| "Unknown streaming error".to_string());
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
            StreamInnerEvent::Error { .. } => String::new(),
            StreamInnerEvent::Ping => String::new(),
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

    /// Format an unknown event for display in verbose/debug mode
    ///
    /// Extracts key fields from unknown events to provide useful debugging info
    /// without exposing potentially sensitive data.
    fn format_unknown_event(&self, line: &str) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        // Try to parse as generic JSON to extract type and key fields
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(line) {
            if let Some(obj) = value.as_object() {
                // Extract the type field
                let event_type = obj.get("type").and_then(|v| v.as_str()).unwrap_or("unknown");

                // Extract a few other common fields for context
                let mut fields = Vec::new();
                for key in ["subtype", "session_id", "message_id", "index"] {
                    if let Some(val) = obj.get(key) {
                        let val_str = match val {
                            serde_json::Value::String(s) => {
                                // Truncate long strings for display
                                if s.len() > 20 {
                                    format!("{}...", &s[..17])
                                } else {
                                    s.clone()
                                }
                            }
                            serde_json::Value::Number(n) => n.to_string(),
                            serde_json::Value::Bool(b) => b.to_string(),
                            _ => continue,
                        };
                        fields.push(format!("{}={}", key, val_str));
                    }
                }

                let fields_str = if fields.is_empty() {
                    String::new()
                } else {
                    format!(" ({})", fields.join(", "))
                };

                return format!(
                    "{}[{}]{} {}Unknown event: {}{}{}\n",
                    c.dim(),
                    prefix,
                    c.reset(),
                    c.dim(),
                    event_type,
                    fields_str,
                    c.reset()
                );
            }
        }

        // Fallback: just note it was an unknown event
        format!(
            "[{}]{} Unknown event\n",
            prefix,
            c.reset()
        )
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

            if trimmed.starts_with('{')
                && serde_json::from_str::<serde_json::Value>(trimmed).is_err()
            {
                monitor.record_parse_error();
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

            match self.parse_event(&line) {
                Some(output) => {
                    monitor.record_parsed();
                    write!(writer, "{}", output)?;
                }
                None => {
                    monitor.record_ignored();
                }
            }

            // Log raw JSON to file if configured
            if let Some(ref mut file) = log_writer {
                writeln!(file, "{}", line)?;
            }
        }

        if let Some(ref mut file) = log_writer {
            file.flush()?;
        }
        if let Some(warning) = monitor.check_and_warn(c) {
            writeln!(writer, "{}", warning)?;
        }
        Ok(())
    }
}
