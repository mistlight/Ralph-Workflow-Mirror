//! Shared types and utilities for JSON parsers.
//!
//! This module contains event types and utility functions used by
//! all the CLI parsers (Claude, Codex, Gemini).

use crate::common::truncate_text;
use crate::logger::Colors;
use regex::Regex;
use serde::{Deserialize, Serialize};

// Import stream classifier for algorithmic event detection
use super::stream_classifier::{StreamEventClassifier, StreamEventType};

static SECRET_VALUE_RE: std::sync::LazyLock<Option<Regex>> = std::sync::LazyLock::new(|| {
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
        .filter(char::is_ascii_alphanumeric)
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
    /// Streaming event with nested inner events for delta/partial updates
    StreamEvent {
        event: StreamInnerEvent,
    },
    #[serde(other)]
    Unknown,
}

/// Inner events within a Claude `stream_event`
///
/// These events represent the streaming protocol used by Claude CLI
/// when --include-partial-messages is enabled. The streaming protocol
/// uses SSE-style events with deltas for incremental content delivery.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum StreamInnerEvent {
    /// Message start - initialization of a new message stream
    MessageStart { message: Option<AssistantMessage> },
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
    TextDelta { text: Option<String> },
    /// Message stop - completion of the message stream
    MessageStop,
    /// Error event during streaming
    Error { error: Option<StreamError> },
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
pub enum ContentBlockDelta {
    /// Delta for text content blocks
    TextDelta { text: Option<String> },
    /// Delta for tool use content blocks (input streaming)
    ToolUseDelta { tool_use: Option<serde_json::Value> },
    /// Delta for thinking/reasoning content blocks
    ThinkingDelta { thinking: Option<String> },
    #[serde(other)]
    Unknown,
}

/// Error information for streaming errors
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StreamError {
    pub(crate) message: Option<String>,
    pub(crate) code: Option<String>,
}

/// Content type for delta accumulation
///
/// Distinguishes between different types of content that may be streamed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ContentType {
    /// Regular text content
    Text,
    /// Thinking/reasoning content
    Thinking,
    /// Tool input content
    ToolInput,
}

/// Delta accumulator for streaming content
///
/// Tracks partial content across multiple streaming events, accumulating
/// deltas for different content types. Uses a composite key approach
/// to track content by (`content_type`, key).
///
/// Supports both index-based tracking (for parsers with numeric indices)
/// and string-based key tracking (for parsers with string identifiers).
#[derive(Debug, Default, Clone)]
pub struct DeltaAccumulator {
    /// Accumulated content by (`content_type`, key) composite key
    /// Using a String key to support both numeric and string-based identifiers
    buffers: std::collections::HashMap<(ContentType, String), String>,
    /// Track the order of keys for `most_recent` operations
    key_order: Vec<(ContentType, String)>,
}

impl DeltaAccumulator {
    /// Create a new delta accumulator
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Add a text delta for a specific index
    pub(crate) fn add_text_delta(&mut self, index: u64, delta: &str) {
        self.add_delta(ContentType::Text, &index.to_string(), delta);
    }

    /// Add a thinking delta for a specific index
    pub(crate) fn add_thinking_delta(&mut self, index: u64, delta: &str) {
        self.add_delta(ContentType::Thinking, &index.to_string(), delta);
    }

    /// Add a delta for a specific content type and key
    ///
    /// This is the generic method that supports both index-based and
    /// string-based key tracking.
    pub(crate) fn add_delta(&mut self, content_type: ContentType, key: &str, delta: &str) {
        let composite_key = (content_type, key.to_string());
        self.buffers
            .entry(composite_key.clone())
            .and_modify(|buf| buf.push_str(delta))
            .or_insert_with(|| delta.to_string());

        // Track order for most_recent operations
        if !self.key_order.contains(&composite_key) {
            self.key_order.push(composite_key);
        }
    }

    /// Get accumulated content for a specific content type and key
    pub(crate) fn get(&self, content_type: ContentType, key: &str) -> Option<&str> {
        self.buffers
            .get(&(content_type, key.to_string()))
            .map(std::string::String::as_str)
    }

    /// Clear all accumulated content
    pub(crate) fn clear(&mut self) {
        self.buffers.clear();
        self.key_order.clear();
    }

    /// Clear content for a specific index
    pub(crate) fn clear_index(&mut self, index: u64) {
        let index_str = index.to_string();
        for content_type in [
            ContentType::Text,
            ContentType::Thinking,
            ContentType::ToolInput,
        ] {
            let key = (content_type, index_str.clone());
            self.buffers.remove(&key);
            self.key_order.retain(|k| k != &key);
        }
    }

    /// Clear content for a specific content type and key
    pub(crate) fn clear_key(&mut self, content_type: ContentType, key: &str) {
        let composite_key = (content_type, key.to_string());
        self.buffers.remove(&composite_key);
        self.key_order.retain(|k| k != &composite_key);
    }

    /// Check if there is any accumulated content (used in tests)
    #[cfg(test)]
    pub(crate) fn is_empty(&self) -> bool {
        self.buffers.is_empty()
    }
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssistantMessage {
    pub(crate) content: Option<Vec<ContentBlock>>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct UserMessage {
    pub(crate) content: Option<Vec<ContentBlock>>,
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
        content: Option<serde_json::Value>,
    },
    #[serde(other)]
    Unknown,
}

/// Codex event types
///
/// Based on `OpenAI` Codex CLI documentation, events include:
/// - `thread.started`: Thread initialization with `thread_id`
/// - `turn.started`/`turn.completed`/`turn.failed`: Turn lifecycle events
/// - `item.started`/`item.completed`: Item events for commands, file ops, messages, etc.
/// - `error`: Error events
///
/// Item types include: `agent_message`, reasoning, `command_execution`, `file_read`,
/// `file_write`, `file_change`, `mcp_tool_call`, `web_search`, `plan_update`
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
pub struct CodexItem {
    /// Item type (`command_execution`, `agent_message`, reasoning, `file_read`, etc.)
    #[serde(rename = "type")]
    pub(crate) item_type: Option<String>,
    /// Command text (for `command_execution`)
    pub(crate) command: Option<String>,
    /// Message/reasoning text (for `agent_message`, reasoning)
    pub(crate) text: Option<String>,
    /// File path (for file operations)
    pub(crate) path: Option<String>,
    /// Tool name (for `mcp_tool_call`)
    pub(crate) tool: Option<String>,
    /// Tool arguments (for `mcp_tool_call`)
    pub(crate) arguments: Option<serde_json::Value>,
    /// Search query (for `web_search`)
    pub(crate) query: Option<String>,
    /// Plan content (for `plan_update`)
    pub(crate) plan: Option<String>,
}

/// Gemini event types
///
/// Based on Gemini CLI documentation, events include:
/// - `init`: Session initialization with `session_id` and model
/// - `message`: User or assistant messages with content and role
/// - `tool_use`: Tool invocations with tool name, ID, and parameters
/// - `tool_result`: Tool execution results with status and output
/// - `error`: Non-fatal errors and warnings
/// - `result`: Final session outcome with aggregated stats
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum GeminiEvent {
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
pub struct GeminiStats {
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
pub fn format_tool_input(input: &serde_json::Value) -> String {
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

/// Helper function to extract text from nested JSON structures
///
/// This function attempts to extract text content from common nested patterns
/// like `{"text": "..."}`, `{"delta": {"text": "..."}}`, etc.
fn extract_nested_text(value: &serde_json::Value) -> Option<String> {
    // If it's already a string, return it
    if let Some(text) = value.as_str() {
        return Some(text.to_string());
    }

    // If it's an object, look for common text fields
    if let Some(obj) = value.as_object() {
        // Check common text field names
        for field in ["text", "content", "message", "output", "result"] {
            if let Some(val) = obj.get(field) {
                if let Some(text) = val.as_str() {
                    return Some(text.to_string());
                }
            }
        }
    }

    None
}

/// Format an unknown JSON event for display in verbose/debug mode
///
/// This is a generic handler for unknown events that works across all parsers.
/// It uses algorithmic classification to detect delta/partial events vs control events,
/// and extracts key fields from the JSON to provide useful debugging info.
///
/// # Arguments
/// * `line` - The raw JSON line
/// * `parser_name` - Name of the parser for display prefix
/// * `colors` - Colors struct for formatting
/// * `is_verbose` - Whether to show unknown events
///
/// # Returns
/// A formatted string showing the event type and key fields, or an empty string
/// if the JSON couldn't be parsed or verbosity should suppress it.
pub fn format_unknown_json_event(
    line: &str,
    parser_name: &str,
    colors: Colors,
    is_verbose: bool,
) -> String {
    // Try to parse as generic JSON to extract type and key fields
    let value = if let Ok(v) = serde_json::from_str::<serde_json::Value>(line) {
        v
    } else {
        // Only show parsing failure message in verbose mode
        if is_verbose {
            return format!(
                "{}[{}]{} {}Unknown event (invalid JSON)\n",
                colors.dim(),
                parser_name,
                colors.reset(),
                colors.dim()
            );
        }
        return String::new();
    };

    let obj = if let Some(o) = value.as_object() {
        o
    } else {
        if is_verbose {
            return format!(
                "{}[{}]{} {}Unknown event (non-object JSON)\n",
                colors.dim(),
                parser_name,
                colors.reset(),
                colors.dim()
            );
        }
        return String::new();
    };

    // Use stream classifier for algorithmic event detection
    let classifier = StreamEventClassifier::new();
    let classification = classifier.classify(&value);

    // Extract the type field - try both "type" and common variants
    let event_type = obj
        .get("type")
        .and_then(|v| v.as_str())
        .or_else(|| obj.get("event_type").and_then(|v| v.as_str()))
        .unwrap_or_else(|| {
            // Use classifier's detected type name if available
            classification.type_name.as_deref().unwrap_or("unknown")
        });

    // For partial/delta events, try to extract and show content
    let content_info = if classification.event_type == StreamEventType::Partial {
        // Try to extract content from various nested structures
        let extracted_text = if let Some(ref content) = classification.content_field {
            // Content field was found at top level by classifier
            if let Some(val) = obj.get(content) {
                if let Some(text) = val.as_str() {
                    Some(text.to_string())
                } else {
                    // Content field exists but is not a string - try to extract nested text
                    extract_nested_text(val)
                }
            } else {
                None
            }
        } else {
            // No content field found - try to extract from delta field
            obj.get("delta")
                .and_then(extract_nested_text)
                .or_else(|| {
                    // Try nested delta structure: delta.text or delta.content
                    obj.get("delta")
                        .and_then(|d| d.as_object())
                        .and_then(|delta_obj| {
                            // First try delta.text, then delta.content
                            delta_obj
                                .get("text")
                                .or_else(|| delta_obj.get("content"))
                                .and_then(|v| v.as_str())
                                .map(std::string::ToString::to_string)
                        })
                })
                .or_else(|| {
                    // Try other common nested structures
                    obj.get("data").and_then(extract_nested_text)
                })
        };

        extracted_text.map(|text: String| {
            let truncated = if text.len() > 30 {
                format!("{}...", &text[..27.min(text.len())])
            } else {
                text
            };
            format!(" content=\"{truncated}\"")
        })
    } else {
        None
    };

    // Build event type label based on classification
    let type_label = match classification.event_type {
        StreamEventType::Partial => {
            // Show partial events if they have explicit delta indicators in type name OR
            // if they have an actual delta field (not just algorithmically detected).
            // Note: The presence of a delta/partial/chunk field (regardless of whether it's
            // a boolean flag or string content) indicates streaming protocol behavior,
            // so we treat these as partial events to be shown to the user in real-time.
            let type_name_lower = event_type.to_lowercase();
            let has_delta_field = obj.contains_key("delta")
                || obj.contains_key("partial")
                || obj.contains_key("chunk");
            let is_explicit_delta = type_name_lower.contains("delta")
                || type_name_lower.contains("partial")
                || type_name_lower.contains("chunk")
                || has_delta_field;

            if is_verbose {
                format!("Partial event: {event_type}")
            } else if is_explicit_delta {
                // In non-verbose mode, show explicit partial events (they're user content)
                // Extract full content (not truncated) for delta events
                let full_content: Option<String> =
                    if let Some(ref content) = classification.content_field {
                        // Use classifier's detected content field first
                        obj.get(content)
                            .and_then(|v| v.as_str())
                            .map(std::string::ToString::to_string)
                            .or_else(|| {
                                // Content field wasn't a string, try extracting nested text
                                obj.get(content).and_then(extract_nested_text)
                            })
                    } else {
                        // Try delta field (common pattern)
                        obj.get("delta")
                            .and_then(|v| v.as_str())
                            .map(std::string::ToString::to_string)
                            .or_else(|| {
                                // Try nested delta.text or delta.content
                                obj.get("delta").and_then(|d| d.as_object()).and_then(|o| {
                                    o.get("text")
                                        .or_else(|| o.get("content"))
                                        .and_then(|t| t.as_str())
                                        .map(std::string::ToString::to_string)
                                })
                            })
                            .or_else(|| {
                                // Try common text fields at top level
                                for field in ["text", "content", "message"] {
                                    if let Some(val) = obj.get(field) {
                                        if let Some(text) = val.as_str() {
                                            return Some(text.to_string());
                                        }
                                    }
                                }
                                None
                            })
                    };

                if let Some(content) = full_content {
                    if !content.trim().is_empty() {
                        return format!("{content}\n");
                    }
                }
                return String::new();
            } else {
                // Short content that looks like partial - don't show in non-verbose mode
                return String::new();
            }
        }
        StreamEventType::Control => {
            // Control events are state management - don't show in output
            return String::new();
        }
        StreamEventType::Complete => {
            if is_verbose {
                format!("Complete event: {event_type}")
            } else {
                // In non-verbose mode, don't show complete events without content
                return String::new();
            }
        }
    };

    // Extract a few other common fields for context
    let mut fields = Vec::new();
    for key in [
        "subtype",
        "session_id",
        "sessionID",
        "message_id",
        "messageID",
        "index",
        "reason",
        "status",
    ] {
        if let Some(val) = obj.get(key) {
            let val_str = match val {
                serde_json::Value::String(s) => {
                    // Truncate long strings for display
                    if s.len() > 20 {
                        format!("{}...", &s[..17.min(s.len())])
                    } else {
                        s.clone()
                    }
                }
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::Bool(b) => b.to_string(),
                _ => continue,
            };
            fields.push(format!("{key}={val_str}"));
        }
    }

    let mut fields_str = if fields.is_empty() {
        String::new()
    } else {
        format!(" ({})", fields.join(", "))
    };

    // Add content info if available
    if let Some(content) = content_info {
        fields_str.push_str(&content);
    }

    format!(
        "{}[{}]{} {}{}{}{}\n",
        colors.dim(),
        parser_name,
        colors.reset(),
        colors.dim(),
        type_label,
        fields_str,
        colors.reset()
    )
}
