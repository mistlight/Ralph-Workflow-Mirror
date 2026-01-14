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
//! 3. **Uses carriage return (`\r`)** to overwrite the previous line, creating an
//!    updating effect that shows the content building up in real-time
//! 4. **Shows prefix** on the first delta event and again on the final non-delta message
//!
//! Example output sequence for streaming "Hello World" in two chunks:
//! ```text
//! [Gemini] Hello\r         (first delta with prefix, no newline)
//! Hello World\r             (second delta overwrites with accumulated text)
//! [Gemini] Hello World\n   (final non-delta message shows complete result)
//! ```

use crate::colors::{Colors, CHECK, CROSS};
use crate::config::Verbosity;
use crate::utils::truncate_text;
use std::cell::{Cell, RefCell};
use std::io::{self, BufRead, Write};
use std::rc::Rc;

use super::health::HealthMonitor;
use super::types::{
    format_tool_input, format_unknown_json_event, ContentType, DeltaAccumulator, GeminiEvent,
};

/// Gemini event parser
pub struct GeminiParser {
    colors: Colors,
    verbosity: Verbosity,
    log_file: Option<String>,
    display_name: String,
    /// Delta accumulator for streaming content
    delta_accumulator: Rc<RefCell<DeltaAccumulator>>,
    /// Track if we're currently streaming delta content
    in_delta_content: Rc<RefCell<Cell<bool>>>,
}

impl GeminiParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self {
            colors,
            verbosity,
            log_file: None,
            display_name: "Gemini".to_string(),
            delta_accumulator: Rc::new(RefCell::new(DeltaAccumulator::new())),
            in_delta_content: Rc::new(RefCell::new(Cell::new(false))),
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
            } => {
                // Clear accumulator on new session
                self.delta_accumulator.borrow_mut().clear();
                let sid = session_id.unwrap_or_else(|| "unknown".to_string());
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
            GeminiEvent::Message {
                role,
                content,
                delta,
            } => {
                let role_str = role.unwrap_or_else(|| "unknown".to_string());
                let is_delta = delta.unwrap_or(false);

                if let Some(text) = content {
                    if is_delta && role_str == "assistant" {
                        // Accumulate delta content
                        let mut acc = self.delta_accumulator.borrow_mut();
                        acc.add_delta(ContentType::Text, "main", &text);
                        // Get accumulated text for streaming display
                        let accumulated_text = acc.get(ContentType::Text, "main").unwrap_or("");

                        // Check if we're already streaming delta content
                        let in_delta_state = self.in_delta_content.borrow();
                        let was_in_delta = in_delta_state.get();
                        drop(in_delta_state);

                        // Only show prefix on the first delta chunk
                        if was_in_delta {
                            // Subsequent chunks: clear line, overwrite with carriage return, show accumulated text without prefix
                            self.in_delta_content.borrow_mut().set(true);
                            return Some(format!("{}\x1b[0K\r{}", c.white(), accumulated_text));
                        }
                        // First chunk: show prefix + text WITHOUT newline (streaming stays on same line)
                        self.in_delta_content.borrow_mut().set(true);
                        return Some(format!(
                            "{}[{}]{} {}{}{}",
                            c.dim(),
                            prefix,
                            c.reset(),
                            c.white(),
                            accumulated_text,
                            c.reset()
                        ));
                    } else if !is_delta && role_str == "assistant" {
                        // Non-delta message - reset streaming state, clear accumulator and show full content
                        let in_delta_state = self.in_delta_content.borrow();
                        let was_in_delta = in_delta_state.get();
                        drop(in_delta_state);
                        self.in_delta_content.borrow_mut().set(false);

                        self.delta_accumulator.borrow_mut().clear();
                        let limit = self.verbosity.truncate_limit("text");
                        let preview = truncate_text(&text, limit);

                        // Add final newline if we were streaming
                        let newline_suffix = if was_in_delta {
                            format!("{}\n", c.reset())
                        } else {
                            String::new()
                        };

                        return Some(format!(
                            "{}[{}]{} {}{}{}{}",
                            c.dim(),
                            prefix,
                            c.reset(),
                            c.white(),
                            preview,
                            newline_suffix,
                            c.reset()
                        ));
                    }
                    // User or other role messages
                    let limit = self.verbosity.truncate_limit("text");
                    let preview = truncate_text(&text, limit);
                    return Some(format!(
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
                    ));
                }
                String::new()
            }
            GeminiEvent::ToolUse {
                tool_name,
                parameters,
                ..
            } => {
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
                    if let Some(ref params) = parameters {
                        let params_str = format_tool_input(params);
                        let limit = self.verbosity.truncate_limit("tool_input");
                        let preview = truncate_text(&params_str, limit);
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
                out
            }
            GeminiEvent::ToolResult { status, output, .. } => {
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
                    if let Some(ref output_text) = output {
                        let limit = self.verbosity.truncate_limit("tool_result");
                        let preview = truncate_text(output_text, limit);
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
                out
            }
            GeminiEvent::Error { message, code, .. } => {
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
            GeminiEvent::Result { status, stats, .. } => {
                let status_result = status.unwrap_or_else(|| "unknown".to_string());
                let is_success = status_result == "success";
                let icon = if is_success { CHECK } else { CROSS };
                let color = if is_success { c.green() } else { c.red() };

                let stats_display = if let Some(s) = stats {
                    let duration_s = s.duration_ms.unwrap_or(0) / 1000;
                    let duration_m = duration_s / 60;
                    let duration_s_rem = duration_s % 60;
                    let input = s.input_tokens.unwrap_or(0);
                    let output = s.output_tokens.unwrap_or(0);
                    let tools = s.tool_calls.unwrap_or(0);
                    format!(
                        "({duration_m}m {duration_s_rem}s, in:{input} out:{output}, {tools} tools)"
                    )
                } else {
                    String::new()
                };

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
            GeminiEvent::Unknown => {
                // Use the generic unknown event formatter for consistent handling
                format_unknown_json_event(line, prefix, c, self.verbosity.is_verbose())
            }
        };

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    /// Check if a Gemini event is a control event (state management with no user output)
    ///
    /// Control events are valid JSON that represent state transitions rather than
    /// user-facing content. They should be tracked separately from "ignored" events
    /// to avoid false health warnings.
    const fn is_control_event(event: &GeminiEvent) -> bool {
        match event {
            // Init event is a control event
            GeminiEvent::Init { .. } => true,
            // Result event is a control event (aggregated stats, no direct user output)
            GeminiEvent::Result { .. } => true,
            _ => false,
        }
    }

    /// Parse a stream of Gemini NDJSON events
    pub(crate) fn parse_stream<R: BufRead, W: Write>(
        &self,
        reader: R,
        mut writer: W,
    ) -> io::Result<()> {
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
                    monitor.record_parsed();
                    write!(writer, "{output}")?;
                    writer.flush()?;
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

        if let Some(ref mut file) = log_writer {
            file.flush()?;
        }
        if let Some(warning) = monitor.check_and_warn(c) {
            writeln!(writer, "{warning}")?;
        }
        Ok(())
    }
}
