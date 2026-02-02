//! Shared types and utilities for NDJSON stream parsers.
//!
//! This module defines the event types emitted by AI agent CLIs during streaming
//! execution. Each agent (Claude, Codex, Gemini, OpenCode) outputs NDJSON (newline-delimited
//! JSON) with agent-specific event schemas that get normalized into these types.

mod accumulator;
mod claude;
mod codex;
mod formatting;
mod gemini;

pub use accumulator::{ContentType, DeltaAccumulator};
pub use claude::{
    AssistantMessage, ClaudeEvent, ContentBlock, ContentBlockDelta, MessageDeltaData, MessageUsage,
    StreamError, StreamInnerEvent, UserMessage,
};
pub use codex::{CodexEvent, CodexItem, CodexUsage};
pub use formatting::{format_tool_input, format_unknown_json_event};
pub use gemini::{GeminiEvent, GeminiStats};
