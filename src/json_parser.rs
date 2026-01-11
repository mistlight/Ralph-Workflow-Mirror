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
pub(crate) enum ClaudeEvent {
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
pub(crate) struct AssistantMessage {
    pub(crate) content: Option<Vec<ContentBlock>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct UserMessage {
    pub(crate) content: Option<Vec<ContentBlock>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub(crate) enum ContentBlock {
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
///
/// Based on OpenAI Codex CLI documentation, events include:
/// - `thread.started`: Thread initialization with thread_id
/// - `turn.started`/`turn.completed`/`turn.failed`: Turn lifecycle events
/// - `item.started`/`item.completed`: Item events for commands, file ops, messages, etc.
/// - `error`: Error events
///
/// Item types include: agent_message, reasoning, command_execution, file_read,
/// file_write, file_change, mcp_tool_call, web_search, plan_update
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub(crate) enum CodexEvent {
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
pub(crate) struct CodexUsage {
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    /// Cached input tokens (for prompt caching)
    pub(crate) cached_input_tokens: Option<u64>,
}

/// Codex item structure
///
/// Items represent individual operations performed by Codex:
/// - `command_execution`: Shell command execution
/// - `agent_message`: Text response from the agent
/// - `reasoning`: Internal reasoning/thinking content
/// - `file_read`/`file_write`/`file_change`: File operations
/// - `mcp_tool_call`: Model Context Protocol tool invocations
/// - `web_search`: Web search operations
/// - `plan_update`: Changes to execution plan
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct CodexItem {
    /// Unique identifier for this item
    pub(crate) id: Option<String>,
    /// Item type (command_execution, agent_message, reasoning, file_read, etc.)
    #[serde(rename = "type")]
    pub(crate) item_type: Option<String>,
    /// Command text (for command_execution)
    pub(crate) command: Option<String>,
    /// Message/reasoning text (for agent_message, reasoning)
    pub(crate) text: Option<String>,
    /// File path (for file operations)
    pub(crate) path: Option<String>,
    /// Item status (in_progress, completed, etc.)
    pub(crate) status: Option<String>,
    /// Tool name (for mcp_tool_call)
    pub(crate) tool: Option<String>,
    /// Tool arguments (for mcp_tool_call)
    pub(crate) arguments: Option<serde_json::Value>,
    /// Search query (for web_search)
    pub(crate) query: Option<String>,
    /// Plan content (for plan_update)
    pub(crate) plan: Option<String>,
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
pub(crate) struct ClaudeParser {
    colors: Colors,
    verbosity: Verbosity,
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
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
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

/// Gemini event types
///
/// Based on Gemini CLI documentation, events include:
/// - `init`: Session initialization with session_id and model
/// - `message`: User or assistant messages with content and role
/// - `tool_use`: Tool invocations with tool name, ID, and parameters
/// - `tool_result`: Tool execution results with status and output
/// - `error`: Non-fatal errors and warnings
/// - `result`: Final session outcome with aggregated stats
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub(crate) enum GeminiEvent {
    Init {
        session_id: Option<String>,
        model: Option<String>,
        timestamp: Option<String>,
    },
    Message {
        role: Option<String>,
        content: Option<String>,
        delta: Option<bool>,
        timestamp: Option<String>,
    },
    ToolUse {
        tool_name: Option<String>,
        tool_id: Option<String>,
        parameters: Option<serde_json::Value>,
        timestamp: Option<String>,
    },
    ToolResult {
        tool_id: Option<String>,
        status: Option<String>,
        output: Option<String>,
        timestamp: Option<String>,
    },
    Error {
        message: Option<String>,
        code: Option<String>,
        timestamp: Option<String>,
    },
    Result {
        status: Option<String>,
        stats: Option<GeminiStats>,
        timestamp: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct GeminiStats {
    pub(crate) total_tokens: Option<u64>,
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) tool_calls: Option<u32>,
}

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
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
        let event: GeminiEvent = serde_json::from_str(line).ok()?;
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
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
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

    #[test]
    fn test_codex_reasoning_event() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Verbose);
        let json = r#"{"type":"item.started","item":{"type":"reasoning","id":"item_1"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        assert!(output.unwrap().contains("Reasoning"));
    }

    #[test]
    fn test_codex_reasoning_completed_shows_text() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Verbose);
        let json = r#"{"type":"item.completed","item":{"type":"reasoning","id":"item_1","text":"I should analyze this file first"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Thought"));
        assert!(out.contains("analyze"));
    }

    #[test]
    fn test_codex_mcp_tool_call() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"item.started","item":{"type":"mcp_tool_call","tool":"search_files","arguments":{"query":"main"}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("MCP Tool"));
        assert!(out.contains("search_files"));
        assert!(out.contains("query=main"));
    }

    #[test]
    fn test_codex_web_search() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json =
            r#"{"type":"item.started","item":{"type":"web_search","query":"rust async tutorial"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Search"));
        assert!(out.contains("rust async tutorial"));
    }

    #[test]
    fn test_codex_plan_update() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Verbose);
        let json = r#"{"type":"item.started","item":{"type":"plan_update","id":"item_1"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        assert!(output.unwrap().contains("Updating plan"));
    }

    #[test]
    fn test_codex_turn_completed_with_cached_tokens() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"turn.completed","usage":{"input_tokens":24763,"cached_input_tokens":24448,"output_tokens":122}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Turn completed"));
        assert!(out.contains("in:24763"));
        assert!(out.contains("out:122"));
    }

    #[test]
    fn test_codex_item_with_status() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"ls","status":"in_progress"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Exec"));
        assert!(out.contains("ls"));
    }

    #[test]
    fn test_codex_file_write_completed() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json =
            r#"{"type":"item.completed","item":{"type":"file_write","path":"/src/main.rs"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("File"));
        assert!(out.contains("/src/main.rs"));
    }

    #[test]
    fn test_codex_mcp_completed() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json =
            r#"{"type":"item.completed","item":{"type":"mcp_tool_call","tool":"read_file"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("MCP"));
        assert!(out.contains("read_file"));
        assert!(out.contains("done"));
    }

    #[test]
    fn test_codex_web_search_completed() {
        let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"item.completed","item":{"type":"web_search"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        assert!(output.unwrap().contains("Search completed"));
    }

    // Gemini parser tests
    #[test]
    fn test_gemini_init_event() {
        let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"init","timestamp":"2025-10-10T12:00:00.000Z","session_id":"abc123","model":"gemini-2.0-flash-exp"}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Session started"));
        assert!(out.contains("gemini-2.0-flash-exp"));
    }

    #[test]
    fn test_gemini_message_assistant() {
        let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"message","role":"assistant","content":"Here are the files...","timestamp":"2025-10-10T12:00:04.000Z"}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Here are the files"));
    }

    #[test]
    fn test_gemini_message_user() {
        let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"message","role":"user","content":"List files in current directory","timestamp":"2025-10-10T12:00:01.000Z"}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("user"));
        assert!(out.contains("List files"));
    }

    #[test]
    fn test_gemini_tool_use() {
        let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","tool_name":"Bash","tool_id":"bash-123","parameters":{"command":"ls -la"},"timestamp":"2025-10-10T12:00:02.000Z"}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool"));
        assert!(out.contains("Bash"));
        assert!(out.contains("command=ls -la"));
    }

    #[test]
    fn test_gemini_tool_result_success() {
        let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Verbose);
        let json = r#"{"type":"tool_result","tool_id":"bash-123","status":"success","output":"file1.txt\nfile2.txt","timestamp":"2025-10-10T12:00:03.000Z"}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool result"));
        assert!(out.contains("file1.txt"));
    }

    #[test]
    fn test_gemini_tool_result_error() {
        let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_result","tool_id":"bash-123","status":"error","output":"command not found","timestamp":"2025-10-10T12:00:03.000Z"}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool result"));
    }

    #[test]
    fn test_gemini_error_event() {
        let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"error","message":"Rate limit exceeded","code":"429","timestamp":"2025-10-10T12:00:05.000Z"}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Error"));
        assert!(out.contains("Rate limit exceeded"));
        assert!(out.contains("429"));
    }

    #[test]
    fn test_gemini_result_success() {
        let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"result","status":"success","stats":{"total_tokens":250,"input_tokens":50,"output_tokens":200,"duration_ms":3000,"tool_calls":1},"timestamp":"2025-10-10T12:00:05.000Z"}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("success"));
        assert!(out.contains("in:50"));
        assert!(out.contains("out:200"));
        assert!(out.contains("1 tools"));
    }

    #[test]
    fn test_gemini_message_delta() {
        let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"message","role":"assistant","content":"Streaming","delta":true,"timestamp":"2025-10-10T12:00:04.000Z"}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Streaming"));
        assert!(out.contains("..."));
    }

    #[test]
    fn test_gemini_unknown_event() {
        let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"unknown_event_type","data":"something"}"#;
        let output = parser.parse_event(json);
        // Unknown events should return None (empty output)
        assert!(output.is_none());
    }
}
