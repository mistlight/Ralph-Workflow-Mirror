use serde::{Deserialize, Serialize};

/// Gemini event types.
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
