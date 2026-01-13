//! Claude CLI JSON parser.
//!
//! Parses NDJSON output from Claude CLI and formats it for display.

use crate::colors::{Colors, CHECK, CROSS};
use crate::config::Verbosity;
use crate::utils::truncate_text;
use std::io::{self, BufRead, Write};

use super::health::HealthMonitor;
use super::types::{format_tool_input, ClaudeEvent, ContentBlock};

/// Claude event parser
pub(crate) struct ClaudeParser {
    colors: Colors,
    pub(crate) verbosity: Verbosity,
    log_file: Option<String>,
}

impl ClaudeParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self {
            colors,
            verbosity,
            log_file: None,
        }
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

        let output = match event {
            ClaudeEvent::System {
                subtype,
                session_id,
                cwd,
            } => {
                if subtype.as_deref() == Some("init") {
                    let sid = session_id.unwrap_or_else(|| "unknown".to_string());
                    let mut out = format!(
                        "{}[Claude]{} {}Session started{} {}({:.8}...){}\n",
                        c.dim(),
                        c.reset(),
                        c.cyan(),
                        c.reset(),
                        c.dim(),
                        sid,
                        c.reset()
                    );
                    if let Some(cwd) = cwd {
                        out.push_str(&format!(
                            "{}[Claude]{} {}Working dir: {}{}\n",
                            c.dim(),
                            c.reset(),
                            c.dim(),
                            cwd,
                            c.reset()
                        ));
                    }
                    out
                } else {
                    format!(
                        "{}[Claude]{} {}{}{}\n",
                        c.dim(),
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
                                            "{}[Claude]{} {}{}{}\n",
                                            c.dim(),
                                            c.reset(),
                                            c.white(),
                                            preview,
                                            c.reset()
                                        ));
                                    }
                                }
                                ContentBlock::ToolUse { name, input } => {
                                    let tool_name = name.unwrap_or_else(|| "unknown".to_string());
                                    out.push_str(&format!(
                                        "{}[Claude]{} {}Tool{}: {}{}{}\n",
                                        c.dim(),
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
                                                    "{}[Claude]{} {}  └─ {}{}\n",
                                                    c.dim(),
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
                                        let limit = self.verbosity.truncate_limit("tool_result");
                                        let preview = truncate_text(&content, limit);
                                        out.push_str(&format!(
                                            "{}[Claude]{} {}Result:{} {}\n",
                                            c.dim(),
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
                                "{}[Claude]{} {}User{}: {}{}{}\n",
                                c.dim(),
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
                        "{}[Claude]{} {}{} Completed{} {}({}m {}s, {} turns, ${:.4}){}\n",
                        c.dim(),
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
                        "{}[Claude]{} {}{} {}{}: {} {}({}m {}s){}\n",
                        c.dim(),
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
            ClaudeEvent::Unknown => String::new(),
        };

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    /// Parse a stream of Claude NDJSON events
    pub(crate) fn parse_stream<R: BufRead, W: Write>(
        &self,
        reader: R,
        mut writer: W,
    ) -> io::Result<()> {
        let c = &self.colors;
        let monitor = HealthMonitor::new("Claude");

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
            if let Some(ref log_path) = self.log_file {
                if let Ok(mut file) = std::fs::OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(log_path)
                {
                    writeln!(file, "{}", line)?;
                }
            }
        }

        if let Some(warning) = monitor.check_and_warn(c) {
            writeln!(writer, "{}", warning)?;
        }
        Ok(())
    }
}
