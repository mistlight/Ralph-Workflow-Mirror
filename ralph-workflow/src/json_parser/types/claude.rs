use serde::{Deserialize, Serialize};

/// Claude event types.
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
    /// Streaming event with nested inner events for delta/partial updates.
    StreamEvent {
        event: StreamInnerEvent,
    },
    #[serde(other)]
    Unknown,
}

/// Inner events within a Claude `stream_event`.
///
/// These events represent the streaming protocol used by Claude CLI
/// when --include-partial-messages is enabled. The streaming protocol
/// uses SSE-style events with deltas for incremental content delivery.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum StreamInnerEvent {
    /// Message start - initialization of a new message stream.
    MessageStart {
        message: Option<AssistantMessage>,
        /// Unique identifier for this message (for deduplication).
        message_id: Option<String>,
    },
    /// Content block start - initialization of a new content block (text, tool use, etc.).
    ContentBlockStart {
        index: Option<u64>,
        content_block: Option<ContentBlock>,
    },
    /// Content block delta - incremental update to a content block.
    ContentBlockDelta {
        index: Option<u64>,
        delta: Option<ContentBlockDelta>,
    },
    /// Text delta - incremental text content update.
    TextDelta { text: Option<String> },
    /// Content block stop - completion of a content block.
    ContentBlockStop { index: Option<u64> },
    /// Message delta - final message metadata (`stop_reason`, usage, etc.).
    MessageDelta {
        delta: Option<MessageDeltaData>,
        usage: Option<MessageUsage>,
    },
    /// Message stop - completion of the message stream.
    MessageStop,
    /// Error event during streaming.
    Error { error: Option<StreamError> },
    /// Ping/keepalive event.
    Ping,
    #[serde(other)]
    Unknown,
}

/// Delta content for streaming updates.
///
/// Represents incremental updates to content blocks during streaming.
#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "type")]
#[serde(rename_all = "snake_case")]
pub enum ContentBlockDelta {
    /// Delta for text content blocks.
    TextDelta { text: Option<String> },
    /// Delta for tool use content blocks (input streaming).
    ToolUseDelta { tool_use: Option<serde_json::Value> },
    /// Delta for thinking/reasoning content blocks.
    ThinkingDelta { thinking: Option<String> },
    #[serde(other)]
    Unknown,
}

/// Error information for streaming errors.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct StreamError {
    pub(crate) message: Option<String>,
    pub(crate) code: Option<String>,
}

/// Message delta data for `message_delta` events.
///
/// Contains final message metadata like `stop_reason` and `stop_sequence`.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageDeltaData {
    pub(crate) stop_reason: Option<String>,
    pub(crate) stop_sequence: Option<u64>,
}

/// Message usage information for `message_delta` events.
///
/// Contains token usage statistics for the completed message.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct MessageUsage {
    /// Number of input tokens.
    #[serde(alias = "input_tokens")]
    pub(crate) input: Option<u64>,
    /// Number of output tokens.
    #[serde(alias = "output_tokens")]
    pub(crate) output: Option<u64>,
    /// Number of cache read input tokens.
    #[serde(alias = "cache_read_input_tokens")]
    pub(crate) cache_read: Option<u64>,
    /// Number of cache creation input tokens.
    #[serde(alias = "cache_creation_input_tokens")]
    pub(crate) cache_creation: Option<u64>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct AssistantMessage {
    #[serde(default)]
    pub(crate) id: Option<String>,
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
