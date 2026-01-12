//! Gemini CLI JSON parser.
//!
//! Parses NDJSON output from Gemini CLI and formats it for display.

use crate::colors::{Colors, CHECK, CROSS};
use crate::config::Verbosity;
use crate::utils::truncate_text;
use std::io::{self, BufRead, Write};

use super::types::{format_tool_input, GeminiEvent};

/// Gemini event parser
pub(crate) struct GeminiParser {
    colors: Colors,
    verbosity: Verbosity,
    log_file: Option<String>,
}

impl GeminiParser {
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

    /// Parse and display a single Gemini JSON event
    ///
    /// Returns Some(formatted_output) for valid events, or None for:
    /// - Malformed JSON (non-JSON text passed through if meaningful)
    /// - Unknown event types
    /// - Empty or whitespace-only output
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
        let event: GeminiEvent = match serde_json::from_str(line) {
            Ok(e) => e,
            Err(_) => {
                // Non-JSON line - pass through as-is if meaningful
                let trimmed = line.trim();
                if !trimmed.is_empty() && !trimmed.starts_with('{') {
                    return Some(format!("{}\n", trimmed));
                }
                return None;
            }
        };
        let c = &self.colors;

        let output = match event {
            GeminiEvent::Init {
                session_id, model, ..
            } => {
                let sid = session_id.unwrap_or_else(|| "unknown".to_string());
                let model_str = model.unwrap_or_else(|| "unknown".to_string());
                format!(
                    "{}[Gemini]{} {}Session started{} {}({:.8}..., {}){}\n",
                    c.dim(),
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
                ..
            } => {
                let role_str = role.unwrap_or_else(|| "unknown".to_string());
                if let Some(text) = content {
                    let limit = self.verbosity.truncate_limit("text");
                    let preview = truncate_text(&text, limit);
                    // Show delta indicator if streaming
                    let delta_marker = if delta.unwrap_or(false) { "..." } else { "" };
                    if role_str == "assistant" {
                        format!(
                            "{}[Gemini]{} {}{}{}{}\n",
                            c.dim(),
                            c.reset(),
                            c.white(),
                            preview,
                            delta_marker,
                            c.reset()
                        )
                    } else {
                        format!(
                            "{}[Gemini]{} {}{}:{} {}{}{}\n",
                            c.dim(),
                            c.reset(),
                            c.blue(),
                            role_str,
                            c.reset(),
                            c.dim(),
                            preview,
                            c.reset()
                        )
                    }
                } else {
                    String::new()
                }
            }
            GeminiEvent::ToolUse {
                tool_name,
                parameters,
                ..
            } => {
                let name = tool_name.unwrap_or_else(|| "unknown".to_string());
                let mut out = format!(
                    "{}[Gemini]{} {}Tool{}: {}{}{}\n",
                    c.dim(),
                    c.reset(),
                    c.magenta(),
                    c.reset(),
                    c.bold(),
                    name,
                    c.reset()
                );
                if self.verbosity.show_tool_input() {
                    if let Some(ref params) = parameters {
                        let params_str = format_tool_input(params);
                        let limit = self.verbosity.truncate_limit("tool_input");
                        let preview = truncate_text(&params_str, limit);
                        if !preview.is_empty() {
                            out.push_str(&format!(
                                "{}[Gemini]{} {}  └─ {}{}\n",
                                c.dim(),
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
                    "{}[Gemini]{} {}{} Tool result{}\n",
                    c.dim(),
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
                            "{}[Gemini]{} {}  └─ {}{}\n",
                            c.dim(),
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
                let code_str = code
                    .map(|c| format!(" ({})", c))
                    .unwrap_or_else(String::new);
                format!(
                    "{}[Gemini]{} {}{} Error{}:{} {}\n",
                    c.dim(),
                    c.reset(),
                    c.red(),
                    CROSS,
                    code_str,
                    c.reset(),
                    msg
                )
            }
            GeminiEvent::Result { status, stats, .. } => {
                let status_str = status.unwrap_or_else(|| "unknown".to_string());
                let is_success = status_str == "success";
                let icon = if is_success { CHECK } else { CROSS };
                let color = if is_success { c.green() } else { c.red() };

                let stats_str = if let Some(s) = stats {
                    let duration_s = s.duration_ms.unwrap_or(0) / 1000;
                    let duration_m = duration_s / 60;
                    let duration_s_rem = duration_s % 60;
                    let input = s.input_tokens.unwrap_or(0);
                    let output = s.output_tokens.unwrap_or(0);
                    let tools = s.tool_calls.unwrap_or(0);
                    format!(
                        "({}m {}s, in:{} out:{}, {} tools)",
                        duration_m, duration_s_rem, input, output, tools
                    )
                } else {
                    String::new()
                };

                format!(
                    "{}[Gemini]{} {}{} {}{} {}{}{}\n",
                    c.dim(),
                    c.reset(),
                    color,
                    icon,
                    status_str,
                    c.reset(),
                    c.dim(),
                    stats_str,
                    c.reset()
                )
            }
            GeminiEvent::Unknown => String::new(),
        };

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    /// Parse a stream of Gemini NDJSON events
    pub(crate) fn parse_stream<R: BufRead, W: Write>(
        &self,
        reader: R,
        mut writer: W,
    ) -> io::Result<()> {
        let c = &self.colors;

        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
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

            if let Some(output) = self.parse_event(&line) {
                write!(writer, "{}", output)?;
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
        Ok(())
    }
}
