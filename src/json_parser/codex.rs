//! Codex CLI JSON parser.
//!
//! Parses NDJSON output from OpenAI Codex CLI and formats it for display.

use crate::colors::{Colors, CHECK, CROSS};
use crate::config::Verbosity;
use crate::utils::truncate_text;
use std::io::{self, BufRead, Write};

use super::health::HealthMonitor;
use super::types::{format_tool_input, format_unknown_json_event, CodexEvent};

/// Codex event parser
pub(crate) struct CodexParser {
    colors: Colors,
    verbosity: Verbosity,
    log_file: Option<String>,
    display_name: String,
}

impl CodexParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self {
            colors,
            verbosity,
            log_file: None,
            display_name: "Codex".to_string(),
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
    /// Returns Some(formatted_output) for valid events, or None for:
    /// - Malformed JSON (non-JSON text passed through if meaningful)
    /// - Unknown event types
    /// - Empty or whitespace-only output
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
        let event: CodexEvent = match serde_json::from_str(line) {
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
        let name = &self.display_name;

        let output = match event {
            CodexEvent::ThreadStarted { thread_id } => {
                let tid = thread_id.unwrap_or_else(|| "unknown".to_string());
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
                let (input, output) = usage
                    .map(|u| (u.input_tokens.unwrap_or(0), u.output_tokens.unwrap_or(0)))
                    .unwrap_or((0, 0));
                format!(
                    "{}[{}]{} {}{} Turn completed{} {}(in:{} out:{}){}\n",
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
                let err = error.unwrap_or_else(|| "unknown error".to_string());
                format!(
                    "{}[{}]{} {}{} Turn failed:{} {}\n",
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
                            // Show "Thinking..." only in non-verbose mode
                            // In verbose mode, we'll show the actual message in ItemCompleted
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
                            // Show reasoning/thinking in verbose mode
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
                        Some("file_read") | Some("file_write") => {
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
                        Some("mcp_tool_call") | Some("mcp") => {
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
                                        out.push_str(&format!(
                                            "{}[{}]{} {}  └─ {}{}\n",
                                            c.dim(),
                                            name,
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
                            if let Some(ref text) = item.text {
                                let limit = self.verbosity.truncate_limit("agent_msg");
                                let preview = truncate_text(text, limit);
                                format!(
                                    "{}[{}]{} {}{}{}\n",
                                    c.dim(),
                                    name,
                                    c.reset(),
                                    c.white(),
                                    preview,
                                    c.reset()
                                )
                            } else {
                                String::new()
                            }
                        }
                        Some("reasoning") => {
                            // Show reasoning content in verbose mode
                            if self.verbosity.is_verbose() {
                                if let Some(ref text) = item.text {
                                    let limit = self.verbosity.truncate_limit("text");
                                    let preview = truncate_text(text, limit);
                                    format!(
                                        "{}[{}]{} {}Thought:{} {}{}{}\n",
                                        c.dim(),
                                        name,
                                        c.reset(),
                                        c.cyan(),
                                        c.reset(),
                                        c.dim(),
                                        preview,
                                        c.reset()
                                    )
                                } else {
                                    String::new()
                                }
                            } else {
                                String::new()
                            }
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
                        Some("file_change") | Some("file_write") => {
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
                        Some("mcp_tool_call") | Some("mcp") => {
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
                                if let Some(ref plan) = item.plan {
                                    let limit = self.verbosity.truncate_limit("text");
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
                                } else {
                                    format!(
                                        "{}[{}]{} {}{} Plan updated{}\n",
                                        c.dim(),
                                        name,
                                        c.reset(),
                                        c.green(),
                                        CHECK,
                                        c.reset()
                                    )
                                }
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
                    // Check if this was valid JSON but an unknown event type
                    if trimmed.starts_with('{') {
                        monitor.record_unknown_event();
                    } else {
                        monitor.record_ignored();
                    }
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
