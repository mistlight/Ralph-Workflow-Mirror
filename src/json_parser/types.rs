//! Shared types and utilities for JSON parsers.
//!
//! This module contains event types and utility functions used by
//! all the CLI parsers (Claude, Codex, Gemini).

use crate::utils::truncate_text;
use serde::{Deserialize, Serialize};

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
    /// Item type (command_execution, agent_message, reasoning, file_read, etc.)
    #[serde(rename = "type")]
    pub(crate) item_type: Option<String>,
    /// Command text (for command_execution)
    pub(crate) command: Option<String>,
    /// Message/reasoning text (for agent_message, reasoning)
    pub(crate) text: Option<String>,
    /// File path (for file operations)
    pub(crate) path: Option<String>,
    /// Tool name (for mcp_tool_call)
    pub(crate) tool: Option<String>,
    /// Tool arguments (for mcp_tool_call)
    pub(crate) arguments: Option<serde_json::Value>,
    /// Search query (for web_search)
    pub(crate) query: Option<String>,
    /// Plan content (for plan_update)
    pub(crate) plan: Option<String>,
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
    },
    Message {
        role: Option<String>,
        content: Option<String>,
        delta: Option<bool>,
    },
    ToolUse {
        tool_name: Option<String>,
        parameters: Option<serde_json::Value>,
    },
    ToolResult {
        status: Option<String>,
        output: Option<String>,
    },
    Error {
        message: Option<String>,
        code: Option<String>,
    },
    Result {
        status: Option<String>,
        stats: Option<GeminiStats>,
    },
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct GeminiStats {
    pub(crate) input_tokens: Option<u64>,
    pub(crate) output_tokens: Option<u64>,
    pub(crate) duration_ms: Option<u64>,
    pub(crate) tool_calls: Option<u32>,
}

/// Format tool input for display
///
/// Converts JSON input to a human-readable string, showing key parameters.
/// Uses character-safe truncation to handle UTF-8 properly.
pub(crate) fn format_tool_input(input: &serde_json::Value) -> String {
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
