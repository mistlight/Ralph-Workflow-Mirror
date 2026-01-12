//! Codex CLI JSON parser.
//!
//! Parses NDJSON output from OpenAI Codex CLI and formats it for display.

use crate::colors::{Colors, CHECK, CROSS};
use crate::config::Verbosity;
use crate::utils::truncate_text;
use std::io::{self, BufRead, Write};

use super::types::{format_tool_input, CodexEvent};

/// Codex event parser
pub(crate) struct CodexParser {
    colors: Colors,
    verbosity: Verbosity,
    log_file: Option<String>,
}

impl CodexParser {
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

        let output = match event {
            CodexEvent::ThreadStarted { thread_id } => {
                let tid = thread_id.unwrap_or_else(|| "unknown".to_string());
                format!(
                    "{}[Codex]{} {}Thread started{} {}({:.8}...){}\n",
                    c.dim(),
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
                    "{}[Codex]{} {}Turn started{}\n",
                    c.dim(),
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
                    "{}[Codex]{} {}{} Turn completed{} {}(in:{} out:{}){}\n",
                    c.dim(),
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
                    "{}[Codex]{} {}{} Turn failed:{} {}\n",
                    c.dim(),
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
                                "{}[Codex]{} {}Exec{}: {}{}{}\n",
                                c.dim(),
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
                                    "{}[Codex]{} {}Thinking...{}\n",
                                    c.dim(),
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
                                    "{}[Codex]{} {}Reasoning...{}\n",
                                    c.dim(),
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
                                "{}[Codex]{} {}{}:{} {}\n",
                                c.dim(),
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
                                "{}[Codex]{} {}MCP Tool{}: {}{}{}\n",
                                c.dim(),
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
                                            "{}[Codex]{} {}  └─ {}{}\n",
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
                        Some("web_search") => {
                            let query = item.query.clone().unwrap_or_default();
                            let limit = self.verbosity.truncate_limit("command");
                            let preview = truncate_text(&query, limit);
                            format!(
                                "{}[Codex]{} {}Search{}: {}{}{}\n",
                                c.dim(),
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
                                "{}[Codex]{} {}Updating plan...{}\n",
                                c.dim(),
                                c.reset(),
                                c.blue(),
                                c.reset()
                            )
                        }
                        Some(t) => {
                            // Show other types in verbose mode
                            if self.verbosity.is_verbose() {
                                format!(
                                    "{}[Codex]{} {}{}:{} {}\n",
                                    c.dim(),
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
                                    "{}[Codex]{} {}{}{}\n",
                                    c.dim(),
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
                                        "{}[Codex]{} {}Thought:{} {}{}{}\n",
                                        c.dim(),
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
                                "{}[Codex]{} {}{} Command done{}\n",
                                c.dim(),
                                c.reset(),
                                c.green(),
                                CHECK,
                                c.reset()
                            )
                        }
                        Some("file_change") | Some("file_write") => {
                            let path = item.path.clone().unwrap_or_else(|| "unknown".to_string());
                            format!(
                                "{}[Codex]{} {}File{}: {}\n",
                                c.dim(),
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
                                    "{}[Codex]{} {}{} Read:{} {}\n",
                                    c.dim(),
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
                                "{}[Codex]{} {}{} MCP:{} {} done\n",
                                c.dim(),
                                c.reset(),
                                c.green(),
                                CHECK,
                                c.reset(),
                                tool_name
                            )
                        }
                        Some("web_search") => {
                            format!(
                                "{}[Codex]{} {}{} Search completed{}\n",
                                c.dim(),
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
                                        "{}[Codex]{} {}Plan:{} {}\n",
                                        c.dim(),
                                        c.reset(),
                                        c.blue(),
                                        c.reset(),
                                        preview
                                    )
                                } else {
                                    format!(
                                        "{}[Codex]{} {}{} Plan updated{}\n",
                                        c.dim(),
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
                    "{}[Codex]{} {}{} Error:{} {}\n",
                    c.dim(),
                    c.reset(),
                    c.red(),
                    CROSS,
                    c.reset(),
                    err
                )
            }
            CodexEvent::Unknown => String::new(),
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
