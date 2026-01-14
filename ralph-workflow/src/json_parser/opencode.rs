//! `OpenCode` event parser implementation
//!
//! This module handles parsing and displaying `OpenCode` NDJSON event streams.
//!
//! # Streaming Output Behavior
//!
//! This parser implements real-time streaming output for text deltas. When content
//! arrives in multiple chunks (via `text` events), the parser:
//!
//! 1. **Accumulates** text deltas from each chunk into a buffer
//! 2. **Displays** the accumulated text after each chunk
//! 3. **Uses carriage return (`\r`)** to overwrite the previous line, creating an
//!    updating effect that shows the content building up in real-time
//! 4. **Shows prefix** on the first `text` event and again on `step_finish`
//!
//! Example output sequence for streaming "Hello World" in two chunks:
//! ```text
//! [OpenCode] Hello\r       (first text event with prefix, no newline)
//! Hello World\r             (second text event overwrites with accumulated text)
//! [OpenCode] ✓ Step finished... (step_finish shows prefix with newline)
//! ```

#![expect(clippy::too_many_lines)]
#![expect(clippy::items_after_statements)]

use crate::common::truncate_text;
use crate::config::Verbosity;
use crate::logger::{Colors, CHECK, CROSS};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::io::{self, BufRead, Write};
use std::rc::Rc;

use super::delta_display::{DeltaRenderer, TextDeltaRenderer};
use super::health::HealthMonitor;
use super::streaming_state::StreamingSession;
use super::types::{format_tool_input, format_unknown_json_event, ContentType};

/// `OpenCode` event types
///
/// Based on `OpenCode`'s actual NDJSON output format, events include:
/// - `step_start`: Step initialization with snapshot info
/// - `step_finish`: Step completion with reason, cost, tokens
/// - `tool_use`: Tool invocation with tool name, callID, and state (status, input, output)
/// - `text`: Streaming text content
///
/// The top-level structure is: `{ "type": "...", "timestamp": ..., "sessionID": "...", "part": {...} }`
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeEvent {
    #[serde(rename = "type")]
    pub(crate) event_type: String,
    pub(crate) timestamp: Option<u64>,
    #[serde(rename = "sessionID")]
    pub(crate) session_id: Option<String>,
    pub(crate) part: Option<OpenCodePart>,
}

/// Nested part object containing the actual event data
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodePart {
    pub(crate) id: Option<String>,
    #[serde(rename = "sessionID")]
    pub(crate) session_id: Option<String>,
    #[serde(rename = "messageID")]
    pub(crate) message_id: Option<String>,
    #[serde(rename = "type")]
    pub(crate) part_type: Option<String>,
    // For step_start events
    pub(crate) snapshot: Option<String>,
    // For step_finish events
    pub(crate) reason: Option<String>,
    pub(crate) cost: Option<f64>,
    pub(crate) tokens: Option<OpenCodeTokens>,
    // For tool_use events
    #[serde(rename = "callID")]
    pub(crate) call_id: Option<String>,
    pub(crate) tool: Option<String>,
    pub(crate) state: Option<OpenCodeToolState>,
    // For text events
    pub(crate) text: Option<String>,
    // Time info for text events
    pub(crate) time: Option<OpenCodeTime>,
}

/// Tool state containing status, input, and output
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeToolState {
    pub(crate) status: Option<String>,
    pub(crate) input: Option<serde_json::Value>,
    pub(crate) output: Option<serde_json::Value>,
    pub(crate) title: Option<String>,
    pub(crate) metadata: Option<serde_json::Value>,
    pub(crate) time: Option<OpenCodeTime>,
}

/// Token statistics from `step_finish` events
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeTokens {
    pub(crate) input: Option<u64>,
    pub(crate) output: Option<u64>,
    pub(crate) reasoning: Option<u64>,
    pub(crate) cache: Option<OpenCodeCache>,
}

/// Cache statistics
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeCache {
    pub(crate) read: Option<u64>,
    pub(crate) write: Option<u64>,
}

/// Time information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeTime {
    pub(crate) start: Option<u64>,
    pub(crate) end: Option<u64>,
}

/// `OpenCode` event parser
pub struct OpenCodeParser {
    colors: Colors,
    verbosity: Verbosity,
    log_file: Option<String>,
    display_name: String,
    /// Unified streaming session for state tracking
    streaming_session: Rc<RefCell<StreamingSession>>,
}

impl OpenCodeParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self {
            colors,
            verbosity,
            log_file: None,
            display_name: "OpenCode".to_string(),
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

    /// Parse and display a single `OpenCode` JSON event
    ///
    /// The `OpenCode` NDJSON format uses events with:
    /// - `step_start`: Step initialization with snapshot info
    /// - `step_finish`: Step completion with reason, cost, tokens
    /// - `tool_use`: Tool invocation with tool name, callID, and state (status, input, output)
    /// - `text`: Streaming text content
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
        let event: OpenCodeEvent = if let Ok(e) = serde_json::from_str(line) {
            e
        } else {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('{') {
                return Some(format!("{trimmed}\n"));
            }
            return None;
        };
        let c = &self.colors;
        let prefix = &self.display_name;

        let output = match event.event_type.as_str() {
            "step_start" => {
                // Reset streaming state on new step
                self.streaming_session.borrow_mut().on_message_start();

                // Create unique step ID for duplicate detection
                // Use part.message_id if available, otherwise combine session_id + part.id
                let step_id = if let Some(part) = &event.part {
                    if let Some(message_id) = &part.message_id {
                        message_id.clone()
                    } else {
                        let session = event.session_id.as_deref().unwrap_or("unknown");
                        let part_id = part.id.as_deref().unwrap_or("step");
                        format!("{session}:{part_id}")
                    }
                } else {
                    event
                        .session_id
                        .clone()
                        .unwrap_or_else(|| "unknown".to_string())
                };
                self.streaming_session
                    .borrow_mut()
                    .set_current_message_id(Some(step_id));

                let snapshot = event
                    .part
                    .as_ref()
                    .and_then(|p| p.snapshot.as_ref())
                    .map(|s| format!("({s:.8}...)"))
                    .unwrap_or_default();
                format!(
                    "{}[{}]{} {}Step started{} {}{}{}\n",
                    c.dim(),
                    prefix,
                    c.reset(),
                    c.cyan(),
                    c.reset(),
                    c.dim(),
                    snapshot,
                    c.reset()
                )
            }
            "step_finish" => {
                // Check for duplicate final message using message ID or fallback to streaming content check
                let session = self.streaming_session.borrow();
                let is_duplicate = session.get_current_message_id().map_or_else(
                    || session.has_any_streamed_content(),
                    |message_id| session.is_duplicate_final_message(message_id),
                );
                let was_streaming = session.has_any_streamed_content();
                drop(session);

                // Finalize the message (this marks it as displayed)
                let _was_in_block = self.streaming_session.borrow_mut().on_message_stop();

                event.part.as_ref().map_or_else(String::new, |part| {
                    let reason = part.reason.as_deref().unwrap_or("unknown");
                    let cost = part.cost.unwrap_or(0.0);

                    let tokens_str = part.tokens.as_ref().map_or_else(String::new, |tokens| {
                        let input = tokens.input.unwrap_or(0);
                        let output = tokens.output.unwrap_or(0);
                        let reasoning = tokens.reasoning.unwrap_or(0);
                        let cache_read = tokens.cache.as_ref().and_then(|c| c.read).unwrap_or(0);
                        if reasoning > 0 {
                            format!("in:{input} out:{output} reason:{reasoning} cache:{cache_read}")
                        } else if cache_read > 0 {
                            format!("in:{input} out:{output} cache:{cache_read}")
                        } else {
                            format!("in:{input} out:{output}")
                        }
                    });

                    let is_success = reason == "tool-calls" || reason == "end_turn";
                    let icon = if is_success { CHECK } else { CROSS };
                    let color = if is_success { c.green() } else { c.yellow() };

                    // Add final newline if we were streaming text
                    let newline_prefix = if is_duplicate || was_streaming {
                        TextDeltaRenderer::render_completion()
                    } else {
                        String::new()
                    };

                    let mut out = format!(
                        "{}{}[{}]{} {}{} Step finished{} {}({}",
                        newline_prefix,
                        c.dim(),
                        prefix,
                        c.reset(),
                        color,
                        icon,
                        c.reset(),
                        c.dim(),
                        reason
                    );
                    if !tokens_str.is_empty() {
                        use std::fmt::Write;
                        let _ = write!(out, ", {tokens_str}");
                    }
                    if cost > 0.0 {
                        use std::fmt::Write;
                        let _ = write!(out, ", ${cost:.4}");
                    }
                    use std::fmt::Write;
                    let _ = writeln!(out, "){}", c.reset());
                    out
                })
            }
            "tool_use" => {
                event.part.as_ref().map_or_else(String::new, |part| {
                    let tool_name = part.tool.as_deref().unwrap_or("unknown");
                    let status = part
                        .state
                        .as_ref()
                        .and_then(|s| s.status.as_deref())
                        .unwrap_or("pending");
                    let title = part.state.as_ref().and_then(|s| s.title.as_deref());

                    let is_completed = status == "completed";
                    let icon = if is_completed { CHECK } else { '…' };
                    let color = if is_completed { c.green() } else { c.yellow() };

                    let mut out = format!(
                        "{}[{}]{} {}Tool{}: {}{}{} {}{}{}\n",
                        c.dim(),
                        prefix,
                        c.reset(),
                        c.magenta(),
                        c.reset(),
                        c.bold(),
                        tool_name,
                        c.reset(),
                        color,
                        icon,
                        c.reset()
                    );

                    // Show title if available
                    if let Some(t) = title {
                        let limit = self.verbosity.truncate_limit("text");
                        let preview = truncate_text(t, limit);
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

                    // Show tool input at Normal+ verbosity
                    if self.verbosity.show_tool_input() {
                        if let Some(ref state) = part.state {
                            if let Some(ref input_val) = state.input {
                                let input_str = format_tool_input(input_val);
                                let limit = self.verbosity.truncate_limit("tool_input");
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

                    // Show tool output in verbose mode if completed
                    if self.verbosity.is_verbose() && is_completed {
                        if let Some(ref state) = part.state {
                            if let Some(ref output_val) = state.output {
                                let output_str = match output_val {
                                    serde_json::Value::String(s) => s.as_str(),
                                    _ => "",
                                };
                                let output_str = if output_str.is_empty() {
                                    output_val.to_string()
                                } else {
                                    output_str.to_string()
                                };
                                let limit = self.verbosity.truncate_limit("tool_result");
                                let preview = truncate_text(&output_str, limit);
                                if !preview.is_empty() {
                                    use std::fmt::Write;
                                    let _ = writeln!(
                                        out,
                                        "{}[{}]{} {}  └─ Output: {}{}",
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
                    out
                })
            }
            "text" => {
                if let Some(ref part) = event.part {
                    if let Some(ref text) = part.text {
                        // Accumulate streaming text using StreamingSession
                        let (show_prefix, accumulated_text) = {
                            let mut session = self.streaming_session.borrow_mut();
                            let show_prefix = session.on_text_delta_key("main", text);
                            // Get accumulated text for streaming display
                            let accumulated_text = session
                                .get_accumulated(ContentType::Text, "main")
                                .unwrap_or("")
                                .to_string();
                            (show_prefix, accumulated_text)
                        };

                        // Show delta in real-time (both verbose and normal mode)
                        let limit = self.verbosity.truncate_limit("text");
                        let preview = truncate_text(&accumulated_text, limit);

                        // Use TextDeltaRenderer for consistent rendering across all parsers
                        if show_prefix {
                            // First delta: use renderer with prefix
                            return Some(TextDeltaRenderer::render_first_delta(
                                &preview, prefix, *c,
                            ));
                        }
                        // Subsequent deltas: use renderer for in-place update
                        return Some(TextDeltaRenderer::render_subsequent_delta(&preview, *c));
                    }
                }
                String::new()
            }
            _ => {
                // Unknown event type - use the generic formatter in verbose mode
                format_unknown_json_event(line, prefix, c, self.verbosity.is_verbose())
            }
        };

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    /// Check if an `OpenCode` event is a control event (state management with no user output)
    ///
    /// Control events are valid JSON that represent state transitions rather than
    /// user-facing content. They should be tracked separately from "ignored" events
    /// to avoid false health warnings.
    fn is_control_event(event: &OpenCodeEvent) -> bool {
        match event.event_type.as_str() {
            // Step lifecycle events are control events
            "step_start" | "step_finish" => true,
            _ => false,
        }
    }

    /// Check if an `OpenCode` event is a partial/delta event (streaming content displayed incrementally)
    ///
    /// Partial events represent streaming text deltas that are shown to the user
    /// in real-time. These should be tracked separately to avoid inflating "ignored" percentages.
    fn is_partial_event(event: &OpenCodeEvent) -> bool {
        match event.event_type.as_str() {
            // Text events produce streaming content
            "text" => true,
            _ => false,
        }
    }

    /// Parse a stream of `OpenCode` NDJSON events
    pub(crate) fn parse_stream<R: BufRead, W: Write>(
        &self,
        reader: R,
        mut writer: W,
    ) -> io::Result<()> {
        let c = &self.colors;
        let monitor = HealthMonitor::new("OpenCode");
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
                        if let Ok(event) = serde_json::from_str::<OpenCodeEvent>(&line) {
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
                        if let Ok(event) = serde_json::from_str::<OpenCodeEvent>(&line) {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opencode_step_start() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"step_start","timestamp":1768191337567,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06aa45c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"step-start","snapshot":"5d36aa035d4df6edb73a68058733063258114ed5"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Step started"));
        assert!(out.contains("5d36aa03"));
    }

    #[test]
    fn test_opencode_step_finish() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"step_finish","timestamp":1768191347296,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06aca1d001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"step-finish","reason":"tool-calls","snapshot":"5d36aa035d4df6edb73a68058733063258114ed5","cost":0,"tokens":{"input":108,"output":151,"reasoning":0,"cache":{"read":11236,"write":0}}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Step finished"));
        assert!(out.contains("tool-calls"));
        assert!(out.contains("in:108"));
        assert!(out.contains("out:151"));
        assert!(out.contains("cache:11236"));
    }

    #[test]
    fn test_opencode_tool_use_completed() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"tool","callID":"call_8a2985d92e63","tool":"read","state":{"status":"completed","input":{"filePath":"/test/PLAN.md"},"output":"<file>\n00001| # Implementation Plan\n</file>","title":"PLAN.md"}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool"));
        assert!(out.contains("read"));
        assert!(out.contains("✓")); // completed icon
        assert!(out.contains("PLAN.md"));
    }

    #[test]
    fn test_opencode_tool_use_pending() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"tool","callID":"call_8a2985d92e63","tool":"bash","state":{"status":"pending","input":{"command":"ls -la"}}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool"));
        assert!(out.contains("bash"));
        assert!(out.contains("…")); // pending icon (WAIT)
    }

    #[test]
    fn test_opencode_tool_use_shows_input() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"tool","callID":"call_8a2985d92e63","tool":"read","state":{"status":"completed","input":{"filePath":"/Users/test/file.rs"}}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool"));
        assert!(out.contains("read"));
        assert!(out.contains("filePath=/Users/test/file.rs"));
    }

    #[test]
    fn test_opencode_text_event() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"text","timestamp":1768191347231,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac63300","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"text","text":"I'll start by reading the plan and requirements to understand what needs to be implemented.","time":{"start":1768191347226,"end":1768191347226}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("I'll start by reading the plan"));
    }

    #[test]
    fn test_opencode_unknown_event_ignored() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"unknown_event","timestamp":1768191347231,"sessionID":"ses_44f9562d4ffe","part":{}}"#;
        let output = parser.parse_event(json);
        // Unknown events should return None
        assert!(output.is_none());
    }

    #[test]
    fn test_opencode_parser_non_json_passthrough() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let output = parser.parse_event("Error: something went wrong");
        assert!(output.is_some());
        assert!(output.unwrap().contains("Error: something went wrong"));
    }

    #[test]
    fn test_opencode_parser_malformed_json_ignored() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let output = parser.parse_event("{invalid json here}");
        assert!(output.is_none());
    }

    #[test]
    fn test_opencode_step_finish_with_cost() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"step_finish","timestamp":1768191347296,"sessionID":"ses_44f9562d4ffe","part":{"type":"step-finish","reason":"end_turn","cost":0.0025,"tokens":{"input":1000,"output":500,"reasoning":0,"cache":{"read":0,"write":0}}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Step finished"));
        assert!(out.contains("end_turn"));
        assert!(out.contains("$0.0025"));
    }

    #[test]
    fn test_opencode_tool_verbose_shows_output() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Verbose);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"/test.rs"},"output":"fn main() { println!(\"Hello\"); }"}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool"));
        assert!(out.contains("read"));
        assert!(out.contains("Output"));
        assert!(out.contains("fn main"));
    }
}
