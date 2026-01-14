//! Shared types and utilities for JSON parsers.
//!
//! This module contains event types and utility functions used by
//! all the CLI parsers (Claude, Codex, Gemini).

use crate::utils::truncate_text;
use once_cell::sync::Lazy;
use regex::Regex;
use serde::{Deserialize, Serialize};

static SECRET_VALUE_RE: Lazy<Option<Regex>> = Lazy::new(|| {
    // Keep this intentionally conservative to reduce false positives in normal text.
    // Primary goal: avoid leaking common API key formats to stdout/logs.
    Regex::new(
        r"(?xi)
        \bsk-[a-z0-9]{16,}\b          # OpenAI-style keys
        | \bghp_[a-z0-9]{20,}\b       # GitHub PATs
        | \bxox[baprs]-[a-z0-9-]{10,}\b # Slack tokens
        ",
    )
    .ok()
});

fn is_sensitive_key(key: &str) -> bool {
    let normalized = key
        .to_lowercase()
        .chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .collect::<String>();

    // Common sensitive key patterns. We intentionally use `contains` to catch variants like:
    // access_token, apiKey, openai_api_key, githubToken, bearerToken, etc.
    normalized.contains("token")
        || normalized.contains("apikey")
        || normalized.contains("secret")
        || normalized.contains("password")
        || normalized == "authorization"
        || normalized.contains("bearer")
}

fn looks_like_secret_value(value: &str) -> bool {
    SECRET_VALUE_RE
        .as_ref()
        .is_some_and(|re| re.is_match(value))
}

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
    /// Streaming event with nested inner events for delta/partial updates
    StreamEvent {
        event: StreamInnerEvent,
    },
    #[serde(other)]
    Unknown,
}

/// Inner events within a Claude stream_event
///
/// These events represent the streaming protocol used by Claude CLI
/// when --include-partial-messages is enabled. The streaming protocol
/// uses SSE-style events with deltas for incremental content delivery.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub(crate) enum StreamInnerEvent {
    /// Message start - initialization of a new message stream
    MessageStart {
        message: Option<AssistantMessage>,
    },
    /// Content block start - initialization of a new content block (text, tool use, etc.)
    ContentBlockStart {
        index: Option<u64>,
        content_block: Option<ContentBlock>,
    },
    /// Content block delta - incremental update to a content block
    ContentBlockDelta {
        index: Option<u64>,
        delta: Option<ContentBlockDelta>,
    },
    /// Text delta - incremental text content update
    TextDelta {
        text: Option<String>,
    },
    /// Message stop - completion of the message stream
    MessageStop,
    /// Error event during streaming
    Error {
        error: Option<StreamError>,
    },
    /// Ping/keepalive event
    Ping,
    #[serde(other)]
    Unknown,
}

/// Delta content for streaming updates
///
/// Represents incremental updates to content blocks during streaming.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub(crate) enum ContentBlockDelta {
    /// Delta for text content blocks
    TextDelta {
        text: Option<String>,
    },
    /// Delta for tool use content blocks (input streaming)
    ToolUseDelta {
        tool_use: Option<serde_json::Value>,
    },
    /// Delta for thinking/reasoning content blocks
    ThinkingDelta {
        thinking: Option<String>,
    },
    #[serde(other)]
    Unknown,
}

/// Error information for streaming errors
#[derive(Debug, Clone, Deserialize, Serialize)]
pub(crate) struct StreamError {
    pub(crate) message: Option<String>,
    pub(crate) code: Option<String>,
}

/// Delta accumulator for streaming content
///
/// Tracks partial content across multiple streaming events, accumulating
/// deltas for text and other content types. Uses a simple key-based approach
/// to track content by (event_type, index).
#[derive(Debug, Default, Clone)]
pub(crate) struct DeltaAccumulator {
    /// Accumulated text content for each stream index
    text_buffers: std::collections::HashMap<u64, String>,
    /// Accumulated thinking/reasoning content
    thinking_buffers: std::collections::HashMap<u64, String>,
}

impl DeltaAccumulator {
    /// Create a new delta accumulator
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Add a text delta for a specific index
    pub(crate) fn add_text_delta(&mut self, index: u64, delta: &str) {
        self.text_buffers
            .entry(index)
            .and_modify(|buf| buf.push_str(delta))
            .or_insert_with(|| delta.to_string());
    }

    /// Add a thinking delta for a specific index
    pub(crate) fn add_thinking_delta(&mut self, index: u64, delta: &str) {
        self.thinking_buffers
            .entry(index)
            .and_modify(|buf| buf.push_str(delta))
            .or_insert_with(|| delta.to_string());
    }

    /// Get the accumulated text for a specific index
    pub(crate) fn get_text(&self, index: &u64) -> Option<&str> {
        self.text_buffers.get(index).map(|s| s.as_str())
    }

    /// Get the accumulated thinking for a specific index
    pub(crate) fn get_thinking(&self, index: &u64) -> Option<&str> {
        self.thinking_buffers.get(index).map(|s| s.as_str())
    }

    /// Get the most recent text (highest index)
    pub(crate) fn get_most_recent_text(&self) -> Option<&str> {
        self.text_buffers
            .iter()
            .max_by_key(|(k, _)| *k)
            .map(|(_, s)| s.as_str())
    }

    /// Clear all accumulated content
    pub(crate) fn clear(&mut self) {
        self.text_buffers.clear();
        self.thinking_buffers.clear();
    }

    /// Clear content for a specific index
    pub(crate) fn clear_index(&mut self, index: u64) {
        self.text_buffers.remove(&index);
        self.thinking_buffers.remove(&index);
    }

    /// Check if there is any accumulated content
    pub(crate) fn is_empty(&self) -> bool {
        self.text_buffers.is_empty() && self.thinking_buffers.is_empty()
    }
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
        content: Option<serde_json::Value>,
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

fn format_tool_value(key: Option<&str>, value: &serde_json::Value) -> String {
    if let Some(k) = key {
        if is_sensitive_key(k) {
            return "<redacted>".to_string();
        }
    }

    match value {
        serde_json::Value::String(s) => {
            if looks_like_secret_value(s) {
                "<redacted>".to_string()
            } else {
                // Use character-safe truncation for strings
                truncate_text(s, 100)
            }
        }
        serde_json::Value::Number(n) => n.to_string(),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => "null".to_string(),
        serde_json::Value::Array(arr) => format!("[{} items]", arr.len()),
        serde_json::Value::Object(_) => "{...}".to_string(),
    }
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
                    let val_str = format_tool_value(Some(k.as_str()), v);
                    format!("{k}={val_str}")
                })
                .collect();
            parts.join(", ")
        }
        serde_json::Value::String(_) => format_tool_value(None, input),
        other => other.to_string(),
    }
}
