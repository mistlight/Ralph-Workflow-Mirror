//! JSON Stream Parsing Module
//!
//! Functions for parsing NDJSON (newline-delimited JSON)
//! streams from Claude and Codex CLI tools.
//!
//! This module uses serde for JSON parsing, which is ~100x faster
//! than spawning jq for each event.

use crate::colors::{Colors, CHECK, CROSS};
use crate::config::Verbosity;
use crate::utils::truncate_text;
use serde::{Deserialize, Serialize};
use std::io::{self, BufRead, Write};

/// Claude event types
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ClaudeEvent {
    System {
        subtype: Option<String>,
        session_id: Option<String>,
        cwd: Option<String>,
    },
    Assistant {
        message: Option<AssistantMessage>,
    },
    User {
        message: Option<UserMessage>,
    },
    Result {
        subtype: Option<String>,
        duration_ms: Option<u64>,
        total_cost_usd: Option<f64>,
        num_turns: Option<u32>,
        result: Option<String>,
        error: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssistantMessage {
    pub content: Option<Vec<ContentBlock>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserMessage {
    pub content: Option<Vec<ContentBlock>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ContentBlock {
    Text {
        text: Option<String>,
    },
    ToolUse {
        name: Option<String>,
        input: Option<serde_json::Value>,
    },
    ToolResult {
        content: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// Codex event types
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum CodexEvent {
    #[serde(rename = "thread.started")]
    ThreadStarted { thread_id: Option<String> },
    #[serde(rename = "turn.started")]
    TurnStarted {},
    #[serde(rename = "turn.completed")]
    TurnCompleted { usage: Option<CodexUsage> },
    #[serde(rename = "turn.failed")]
    TurnFailed { error: Option<String> },
    #[serde(rename = "item.started")]
    ItemStarted { item: Option<CodexItem> },
    #[serde(rename = "item.completed")]
    ItemCompleted { item: Option<CodexItem> },
    Error {
        message: Option<String>,
        error: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodexUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CodexItem {
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    pub command: Option<String>,
    pub text: Option<String>,
    pub path: Option<String>,
}

/// Claude event parser
pub struct ClaudeParser {
    colors: Colors,
    verbosity: Verbosity,
    log_file: Option<String>,
}

impl ClaudeParser {
    pub fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self {
            colors,
            verbosity,
            log_file: None,
        }
    }

    pub fn with_log_file(mut self, path: &str) -> Self {
        self.log_file = Some(path.to_string());
        self
    }

    /// Parse and display a single Claude JSON event
    pub fn parse_event(&self, line: &str) -> Option<String> {
        let event: ClaudeEvent = serde_json::from_str(line).ok()?;
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
                                    let key = input
                                        .and_then(|v| {
                                            v.as_object().and_then(|o| o.keys().next().cloned())
                                        })
                                        .map(|k| format!(" ({})", k))
                                        .unwrap_or_default();
                                    out.push_str(&format!(
                                        "{}[Claude]{} {}Tool{}: {}{}{}{}\n",
                                        c.dim(),
                                        c.reset(),
                                        c.magenta(),
                                        c.reset(),
                                        c.bold(),
                                        tool_name,
                                        c.reset(),
                                        key
                                    ));
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
    pub fn parse_stream<R: BufRead, W: Write>(&self, reader: R, mut writer: W) -> io::Result<()> {
        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                continue;
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

/// Codex event parser
pub struct CodexParser {
    colors: Colors,
    verbosity: Verbosity,
    log_file: Option<String>,
}

impl CodexParser {
    pub fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self {
            colors,
            verbosity,
            log_file: None,
        }
    }

    pub fn with_log_file(mut self, path: &str) -> Self {
        self.log_file = Some(path.to_string());
        self
    }

    /// Parse and display a single Codex JSON event
    pub fn parse_event(&self, line: &str) -> Option<String> {
        let event: CodexEvent = serde_json::from_str(line).ok()?;
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
                            let cmd = item.command.unwrap_or_default();
                            let limit = self.verbosity.truncate_limit("command");
                            let preview = truncate_text(&cmd, limit);
                            format!(
                                "{}[Codex]{} {}Exec{}: {}{}{}\n",
                                c.dim(),
                                c.reset(),
                                c.magenta(),
                                c.reset(),
                                c.dim(),
                                preview,
                                c.reset()
                            )
                        }
                        Some("agent_message") => {
                            format!(
                                "{}[Codex]{} {}Thinking...{}\n",
                                c.dim(),
                                c.reset(),
                                c.blue(),
                                c.reset()
                            )
                        }
                        Some(t) => {
                            format!(
                                "{}[Codex]{} {}{}{}\n",
                                c.dim(),
                                c.reset(),
                                c.dim(),
                                t,
                                c.reset()
                            )
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
                            if let Some(text) = item.text {
                                let limit = self.verbosity.truncate_limit("agent_msg");
                                let preview = truncate_text(&text, limit);
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
                        Some("file_change") => {
                            let path = item.path.unwrap_or_else(|| "unknown".to_string());
                            format!(
                                "{}[Codex]{} {}File{}: {}\n",
                                c.dim(),
                                c.reset(),
                                c.yellow(),
                                c.reset(),
                                path
                            )
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
    pub fn parse_stream<R: BufRead, W: Write>(&self, reader: R, mut writer: W) -> io::Result<()> {
        for line in reader.lines() {
            let line = line?;
            if line.is_empty() {
                continue;
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_claude_system_init() {
        let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"system","subtype":"init","session_id":"abc123"}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        assert!(output.unwrap().contains("Session started"));
    }

    #[test]
    fn test_parse_claude_result_success() {
        let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"result","subtype":"success","duration_ms":60000,"num_turns":5,"total_cost_usd":0.05}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        assert!(output.unwrap().contains("Completed"));
    }

    #[test]
    fn test_parse_codex_thread_started() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"thread.started","thread_id":"xyz789"}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        assert!(output.unwrap().contains("Thread started"));
    }

    #[test]
    fn test_parse_codex_turn_completed() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        assert!(output.unwrap().contains("Turn completed"));
    }

    #[test]
    fn test_verbosity_affects_output() {
        let quiet_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Quiet);
        let full_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Full);

        let long_text = "a".repeat(200);
        let json = format!(
            r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{}"}}]}}}}"#,
            long_text
        );

        let quiet_output = quiet_parser.parse_event(&json).unwrap();
        let full_output = full_parser.parse_event(&json).unwrap();

        // Quiet output should be truncated (shorter)
        assert!(quiet_output.len() < full_output.len());
    }
}
