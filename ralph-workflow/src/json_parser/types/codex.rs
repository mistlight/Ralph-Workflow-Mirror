use serde::{Deserialize, Serialize};

/// Codex event types.
///
/// Based on `OpenAI` Codex CLI documentation, events include:
/// - `thread.started`: Thread initialization with `thread_id`
/// - `turn.started`/`turn.completed`/`turn.failed`: Turn lifecycle events
/// - `item.started`/`item.completed`: Item events for commands, file ops, messages, etc.
/// - `error`: Error events
/// - `result`: Synthetic result event written by the parser (not from Codex CLI itself)
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
    /// Result event containing aggregated content from `agent_message` items.
    /// This is a synthetic event written by the parser to enable content extraction.
    #[serde(rename = "result")]
    Result { result: Option<String> },
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

/// Codex item structure.
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
    /// Item type (`command_execution`, `agent_message`, reasoning, `file_read`, etc.).
    #[serde(rename = "type")]
    pub item_type: Option<String>,
    /// Command text (for `command_execution`).
    pub command: Option<String>,
    /// Message/reasoning text (for `agent_message`, reasoning).
    pub text: Option<String>,
    /// File path (for file operations).
    pub path: Option<String>,
    /// Tool name (for `mcp_tool_call`).
    pub tool: Option<String>,
    /// Tool arguments (for `mcp_tool_call`).
    pub arguments: Option<serde_json::Value>,
    /// Search query (for `web_search`).
    pub query: Option<String>,
    /// Plan content (for `plan_update`).
    pub plan: Option<String>,
}
