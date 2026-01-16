//! JSON Stream Parsing Module
//!
//! Functions for parsing NDJSON (newline-delimited JSON)
//! streams from Claude, Codex, Gemini, and `OpenCode` CLI tools.
//!
//! This module uses serde for JSON parsing, which is ~100x faster
//! than spawning jq for each event.
//!
//! # Module Structure
//!
//! - [`types`] - Shared types and event structures
//! - [`stream_classifier`] - Algorithmic detection of partial vs complete events
//! - [`claude`] - Claude CLI output parser (with streaming support)
//! - [`codex`] - `OpenAI` Codex CLI output parser (with streaming support)
//! - [`gemini`] - Google Gemini CLI output parser (with streaming support)
//! - [`opencode`] - `OpenCode` CLI output parser (with streaming support)
//! - [`health`] - Parser health monitoring and graceful degradation
//!
//! # Streaming Support
//!
//! All parsers now support delta streaming for real-time content display:
//! - **Claude**: Full streaming with `DeltaAccumulator` for text and thinking deltas
//! - **Gemini**: Streaming with delta flag support for message content
//! - **Codex**: Streaming for `agent_message` and reasoning item types
//! - **`OpenCode`**: Streaming for text events
//!
//! In verbose mode, parsers show full accumulated content. In normal mode,
//! they show real-time deltas for immediate feedback.
//!
//! ## Verbosity Levels
//!
//! The parsers respect the configured verbosity level:
//! - **Quiet (0)**: Minimal output, aggressive truncation
//! - **Normal (1)**: Balanced output with moderate truncation, shows real-time deltas
//! - **Verbose (2)**: Default - shows more detail including tool inputs and full accumulated text
//! - **Full (3)**: No truncation, show all content
//! - **Debug (4)**: Maximum verbosity, includes raw JSON output

mod claude;
#[cfg(test)]
mod claude_tests;
mod codex;
#[cfg(test)]
mod codex_tests;
pub mod deduplication;
pub mod delta_display;
mod gemini;
#[cfg(test)]
mod gemini_tests;
pub mod health;
mod incremental_parser;
mod opencode;
#[cfg(test)]
mod opencode_tests;
mod stream_classifier;
pub mod streaming_state;
mod terminal;
mod types;

pub use claude::ClaudeParser;
pub use codex::CodexParser;
pub use gemini::GeminiParser;
pub use opencode::OpenCodeParser;

#[cfg(test)]
mod tests;
