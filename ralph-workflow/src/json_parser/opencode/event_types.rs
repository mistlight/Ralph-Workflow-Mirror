// OpenCode event type definitions.
//
// Based on OpenCode's NDJSON output format (run.ts lines 146-201).

use serde::{Deserialize, Serialize};

/// `OpenCode` event types
///
/// Based on `OpenCode`'s actual NDJSON output format (`run.ts` lines 146-201), events include:
/// - `step_start`: Step initialization with snapshot info
/// - `step_finish`: Step completion with reason, cost, tokens
/// - `tool_use`: Tool invocation with tool name, callID, and state (status, input, output)
/// - `text`: Streaming text content
/// - `error`: Session/API error events (from `session.error` in run.ts)
///
/// The top-level structure is: `{ "type": "...", "timestamp": ..., "sessionID": "...", "part": {...} }`
/// For error events: `{ "type": "error", "timestamp": ..., "sessionID": "...", "error": {...} }`
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeEvent {
    #[serde(rename = "type")]
    pub(crate) event_type: String,
    pub(crate) timestamp: Option<u64>,
    #[serde(rename = "sessionID")]
    pub(crate) session_id: Option<String>,
    pub(crate) part: Option<OpenCodePart>,
    /// Error information for error events (from `session.error` in run.ts line 201)
    pub(crate) error: Option<OpenCodeError>,
}

/// Error information from error events
///
/// From `run.ts` lines 192-202, error events contain:
/// - `name`: Error type name
/// - `data`: Optional additional error data (may contain `message` field)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeError {
    /// Error type name
    pub(crate) name: Option<String>,
    /// Error message (direct or extracted from data.message)
    pub(crate) message: Option<String>,
    /// Additional error data (may contain `message` field)
    pub(crate) data: Option<serde_json::Value>,
}

/// Nested part object containing the actual event data
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodePart {
    pub(crate) id: Option<String>,
    #[serde(rename = "sessionID")]
    pub(crate) session_id: Option<String>,
    #[serde(rename = "messageID")]
    pub(crate) message_id: Option<String>,
    #[serde(rename = "type")]
    pub(crate) part_type: Option<String>,
    // For step_start events
    pub(crate) snapshot: Option<String>,
    // For step_finish events
    pub(crate) reason: Option<String>,
    pub(crate) cost: Option<f64>,
    pub(crate) tokens: Option<OpenCodeTokens>,
    // For tool_use events
    #[serde(rename = "callID")]
    pub(crate) call_id: Option<String>,
    pub(crate) tool: Option<String>,
    pub(crate) state: Option<OpenCodeToolState>,
    // For text events
    pub(crate) text: Option<String>,
    // Time info for text events
    pub(crate) time: Option<OpenCodeTime>,
}

/// Tool state containing status, input, and output
///
/// From `message-v2.ts` lines 221-287, the state is a discriminated union based on `status`:
/// - `pending`: Tool call received, waiting to execute (`input`, `raw`)
/// - `running`: Tool is executing (`input`, `title?`, `metadata?`, `time.start`)
/// - `completed`: Tool finished successfully (`input`, `output`, `title`, `metadata`, `time`)
/// - `error`: Tool failed (`input`, `error`, `metadata?`, `time`)
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeToolState {
    /// Status: "pending", "running", "completed", or "error"
    pub(crate) status: Option<String>,
    /// Tool input parameters (tool-specific, e.g., `filePath` for read, `command` for bash)
    pub(crate) input: Option<serde_json::Value>,
    /// Tool output (only present when status is "completed")
    pub(crate) output: Option<serde_json::Value>,
    /// Human-readable title/description (e.g., filename for read operations)
    pub(crate) title: Option<String>,
    /// Additional metadata from tool execution
    pub(crate) metadata: Option<serde_json::Value>,
    /// Timing information
    pub(crate) time: Option<OpenCodeTime>,
    /// Error message (only present when status is "error")
    pub(crate) error: Option<String>,
}

/// Token statistics from `step_finish` events
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeTokens {
    pub(crate) input: Option<u64>,
    pub(crate) output: Option<u64>,
    pub(crate) reasoning: Option<u64>,
    pub(crate) cache: Option<OpenCodeCache>,
}

/// Cache statistics
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeCache {
    pub(crate) read: Option<u64>,
    pub(crate) write: Option<u64>,
}

/// Time information
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OpenCodeTime {
    pub(crate) start: Option<u64>,
    pub(crate) end: Option<u64>,
}
