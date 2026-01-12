//! JSON Stream Parsing Module
//!
//! Functions for parsing NDJSON (newline-delimited JSON)
//! streams from Claude, Codex, and Gemini CLI tools.
//!
//! This module uses serde for JSON parsing, which is ~100x faster
//! than spawning jq for each event.
//!
//! # Module Structure
//!
//! - [`types`] - Shared types and event structures
//! - [`claude`] - Claude CLI output parser
//! - [`codex`] - OpenAI Codex CLI output parser
//! - [`gemini`] - Google Gemini CLI output parser
//!
//! ## Verbosity Levels
//!
//! The parsers respect the configured verbosity level:
//! - **Quiet (0)**: Minimal output, aggressive truncation
//! - **Normal (1)**: Balanced output with moderate truncation
//! - **Verbose (2)**: Default - shows more detail including tool inputs
//! - **Full (3)**: No truncation, show all content
//! - **Debug (4)**: Maximum verbosity, includes raw JSON output

mod claude;
mod codex;
mod gemini;
mod types;

pub(crate) use claude::ClaudeParser;
pub(crate) use codex::CodexParser;
pub(crate) use gemini::GeminiParser;

// Re-export format_tool_input for tests
#[cfg(test)]
pub(crate) use types::format_tool_input;

#[cfg(test)]
mod tests;
