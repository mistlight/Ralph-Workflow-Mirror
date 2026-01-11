//! JSON Stream Parsing Module
//!
//! Functions for parsing NDJSON (newline-delimited JSON)
//! streams from Claude and Codex CLI tools.
//!
//! This module uses serde for JSON parsing, which is ~100x faster
//! than spawning jq for each event.
//!
//! ## Verbosity Levels
//!
//! The parsers respect the configured verbosity level:
//! - **Quiet (0)**: Minimal output, aggressive truncation
//! - **Normal (1)**: Balanced output with moderate truncation
//! - **Verbose (2)**: Default - shows more detail including tool inputs
//! - **Full (3)**: No truncation, show all content
//! - **Debug (4)**: Maximum verbosity, includes raw JSON output

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

/// Format tool input for display
///
/// Converts JSON input to a human-readable string, showing key parameters.
/// Uses character-safe truncation to handle UTF-8 properly.
fn format_tool_input(input: &serde_json::Value) -> String {
    match input {
        serde_json::Value::Object(map) => {
            let parts: Vec<String> = map
                .iter()
                .map(|(k, v)| {
                    let val_str = match v {
                        serde_json::Value::String(s) => {
                            // Use character-safe truncation for strings
                            truncate_text(s, 100)
                        }
                        serde_json::Value::Number(n) => n.to_string(),
                        serde_json::Value::Bool(b) => b.to_string(),
                        serde_json::Value::Null => "null".to_string(),
                        serde_json::Value::Array(arr) => format!("[{} items]", arr.len()),
                        serde_json::Value::Object(_) => "{...}".to_string(),
                    };
                    format!("{}={}", k, val_str)
                })
                .collect();
            parts.join(", ")
        }
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
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
    pub fn parse_stream<R: BufRead, W: Write>(&self, reader: R, mut writer: W) -> io::Result<()> {
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

    #[test]
    fn test_format_tool_input_object() {
        let input = serde_json::json!({
            "file_path": "/path/to/file.rs",
            "content": "hello world"
        });
        let result = format_tool_input(&input);
        assert!(result.contains("file_path=/path/to/file.rs"));
        assert!(result.contains("content=hello world"));
    }

    #[test]
    fn test_format_tool_input_truncates_long_strings() {
        let long_content = "x".repeat(150);
        let input = serde_json::json!({
            "content": long_content
        });
        let result = format_tool_input(&input);
        assert!(result.contains("..."));
        assert!(result.len() < 150);
    }

    #[test]
    fn test_format_tool_input_handles_arrays() {
        let input = serde_json::json!({
            "files": ["a.rs", "b.rs", "c.rs"]
        });
        let result = format_tool_input(&input);
        assert!(result.contains("files=[3 items]"));
    }

    #[test]
    fn test_format_tool_input_handles_nested_objects() {
        let input = serde_json::json!({
            "options": {"key": "value"}
        });
        let result = format_tool_input(&input);
        assert!(result.contains("options={...}"));
    }

    #[test]
    fn test_tool_use_shows_input_in_verbose_mode() {
        let verbose_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Verbose);
        let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.rs"}}]}}"#;
        let output = verbose_parser.parse_event(json).unwrap();
        assert!(output.contains("Tool"));
        assert!(output.contains("Read"));
        assert!(output.contains("file_path=/test.rs"));
    }

    #[test]
    fn test_tool_use_shows_input_in_normal_mode() {
        // Tool inputs are now shown at Normal level for better usability
        let normal_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.rs"}}]}}"#;
        let output = normal_parser.parse_event(json).unwrap();
        assert!(output.contains("Tool"));
        assert!(output.contains("Read"));
        // Tool inputs are now visible at Normal level
        assert!(output.contains("file_path=/test.rs"));
    }

    #[test]
    fn test_tool_use_hides_input_in_quiet_mode() {
        // Only Quiet mode hides tool inputs
        let quiet_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Quiet);
        let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.rs"}}]}}"#;
        let output = quiet_parser.parse_event(json).unwrap();
        assert!(output.contains("Tool"));
        assert!(output.contains("Read"));
        // In Quiet mode, input details should not be shown
        assert!(!output.contains("file_path=/test.rs"));
    }

    #[test]
    fn test_debug_verbosity_is_recognized() {
        let debug_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Debug);
        // Debug mode should be detectable via is_debug()
        assert!(debug_parser.verbosity.is_debug());
    }

    #[test]
    fn test_codex_file_operations_shown() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Verbose);
        let json = r#"{"type":"item.started","item":{"type":"file_read","path":"/src/main.rs"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("file_read"));
        assert!(out.contains("/src/main.rs"));
    }

    #[test]
    fn test_format_tool_input_unicode_safe() {
        // Ensure Unicode characters don't cause panics
        let unicode_content = "日本語".to_string() + &"x".repeat(200);
        let input = serde_json::json!({
            "content": unicode_content
        });
        // Should not panic and should truncate properly
        let result = format_tool_input(&input);
        assert!(result.contains("..."));
        assert!(result.contains("日本語"));
    }

    #[test]
    fn test_parse_claude_text_with_unicode() {
        let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello 世界! 🌍"}]}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Hello 世界! 🌍"));
    }
}
