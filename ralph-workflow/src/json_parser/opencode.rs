//! `OpenCode` event parser implementation
//!
//! This module handles parsing and displaying `OpenCode` NDJSON event streams.
//!
//! # Source Code Reference
//!
//! This parser is based on analysis of the OpenCode source code from:
//! - **Repository**: <https://github.com/anomalyco/opencode>
//! - **Key source files**:
//!   - `/packages/opencode/src/cli/cmd/run.ts` - NDJSON output generation
//!   - `/packages/opencode/src/session/message-v2.ts` - Message part type definitions
//!
//! # NDJSON Event Format
//!
//! OpenCode outputs NDJSON (newline-delimited JSON) events via `--format json`.
//! Each event has the structure:
//!
//! ```json
//! {
//!   "type": "step_start" | "step_finish" | "tool_use" | "text" | "error",
//!   "timestamp": 1768191337567,
//!   "sessionID": "ses_44f9562d4ffe",
//!   ...event-specific data (usually in "part" field)
//! }
//! ```
//!
//! From `run.ts` lines 146-201, the event types are generated as:
//! ```typescript
//! outputJsonEvent("tool_use", { part })    // Tool invocations
//! outputJsonEvent("step_start", { part })  // Step initialization
//! outputJsonEvent("step_finish", { part }) // Step completion
//! outputJsonEvent("text", { part })        // Streaming text content
//! outputJsonEvent("error", { error })      // Error events
//! ```
//!
//! # Part Type Definitions
//!
//! ## StepStartPart (`message-v2.ts` lines 194-200)
//!
//! ```typescript
//! {
//!   id: string,
//!   sessionID: string,
//!   messageID: string,
//!   type: "step-start",
//!   snapshot: string | undefined  // Git commit hash for state snapshot
//! }
//! ```
//!
//! ## StepFinishPart (`message-v2.ts` lines 202-219)
//!
//! ```typescript
//! {
//!   id: string,
//!   sessionID: string,
//!   messageID: string,
//!   type: "step-finish",
//!   reason: string,               // "tool-calls", "end_turn", etc.
//!   snapshot: string | undefined,
//!   cost: number,                 // Cost in USD
//!   tokens: {
//!     input: number,
//!     output: number,
//!     reasoning: number,
//!     cache: { read: number, write: number }
//!   }
//! }
//! ```
//!
//! ## TextPart (`message-v2.ts` lines 62-77)
//!
//! ```typescript
//! {
//!   id: string,
//!   sessionID: string,
//!   messageID: string,
//!   type: "text",
//!   text: string,
//!   synthetic?: boolean,
//!   ignored?: boolean,
//!   time?: { start: number, end?: number },
//!   metadata?: Record<string, any>
//! }
//! ```
//!
//! ## ToolPart (`message-v2.ts` lines 289-298)
//!
//! ```typescript
//! {
//!   id: string,
//!   sessionID: string,
//!   messageID: string,
//!   type: "tool",
//!   callID: string,
//!   tool: string,  // Tool name: "read", "bash", "write", "edit", "glob", "grep", etc.
//!   state: ToolState,
//!   metadata?: Record<string, any>
//! }
//! ```
//!
//! ## ToolState Variants (`message-v2.ts` lines 221-287)
//!
//! The `state` field is a discriminated union based on `status`:
//!
//! ### Pending (`status: "pending"`)
//! ```typescript
//! { status: "pending", input: Record<string, any>, raw: string }
//! ```
//!
//! ### Running (`status: "running"`)
//! ```typescript
//! {
//!   status: "running",
//!   input: Record<string, any>,
//!   title?: string,
//!   metadata?: Record<string, any>,
//!   time: { start: number }
//! }
//! ```
//!
//! ### Completed (`status: "completed"`)
//! ```typescript
//! {
//!   status: "completed",
//!   input: Record<string, any>,
//!   output: string,
//!   title: string,
//!   metadata: Record<string, any>,
//!   time: { start: number, end: number, compacted?: number },
//!   attachments?: FilePart[]
//! }
//! ```
//!
//! ### Error (`status: "error"`)
//! ```typescript
//! {
//!   status: "error",
//!   input: Record<string, any>,
//!   error: string,
//!   metadata?: Record<string, any>,
//!   time: { start: number, end: number }
//! }
//! ```
//!
//! ## Tool Input Parameters
//!
//! The `state.input` object contains tool-specific parameters:
//!
//! | Tool    | Input Fields                                         |
//! |---------|-----------------------------------------------------|
//! | `read`  | `{ filePath: string, offset?: number, limit?: number }` |
//! | `bash`  | `{ command: string, timeout?: number }`              |
//! | `write` | `{ filePath: string, content: string }`              |
//! | `edit`  | `{ filePath: string, ... }`                          |
//! | `glob`  | `{ pattern: string, path?: string }`                 |
//! | `grep`  | `{ pattern: string, path?: string, include?: string }` |
//! | `fetch` | `{ url: string, format?: string, timeout?: number }` |
//!
//! From `run.ts` line 168, the title fallback is:
//! ```typescript
//! const title = part.state.title ||
//!   (Object.keys(part.state.input).length > 0 ? JSON.stringify(part.state.input) : "Unknown")
//! ```
//!
//! # Streaming Output Behavior
//!
//! This parser implements real-time streaming output for text deltas. When content
//! arrives in multiple chunks (via `text` events), the parser:
//!
//! 1. **Accumulates** text deltas from each chunk into a buffer
//! 2. **Displays** the accumulated text after each chunk
//! 3. **Uses carriage return (`\r`) and line clearing (`\x1b[2K`)** to rewrite the entire line,
//!    creating an updating effect that shows the content building up in real-time
//! 4. **Shows prefix on every delta**, rewriting the entire line each time (industry standard)
//!
//! Example output sequence for streaming "Hello World" in two chunks:
//! ```text
//! [OpenCode] Hello\r       (first text event with prefix, no newline)
//! \x1b[2K\r[OpenCode] Hello World\r  (second text event clears line, rewrites with accumulated)
//! [OpenCode] ✓ Step finished... (step_finish shows prefix with newline)
//! ```
//!
//! # Single-Line Pattern
//!
//! The renderer uses a single-line pattern with carriage return for in-place updates.
//! This is the industry standard for streaming CLIs (used by Rich, Ink, Bubble Tea).
//!
//! Each delta rewrites the entire line with prefix, ensuring that:
//! - The user always sees the prefix
//! - Content updates in-place without visual artifacts
//! - Terminal state is clean and predictable

use crate::common::truncate_text;
use crate::config::Verbosity;
use crate::logger::{Colors, CHECK, CROSS};
use serde::{Deserialize, Serialize};
use std::cell::RefCell;
use std::fmt::Write as _;
use std::io::{self, BufRead, Write};
use std::path::Path;
use std::rc::Rc;

use super::delta_display::{DeltaRenderer, TextDeltaRenderer};
use super::health::HealthMonitor;
#[cfg(feature = "test-utils")]
use super::health::StreamingQualityMetrics;
use super::printer::SharedPrinter;
use super::streaming_state::StreamingSession;
use super::terminal::TerminalMode;
use super::types::{format_tool_input, format_unknown_json_event, ContentType};

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

/// `OpenCode` event parser
pub struct OpenCodeParser {
    colors: Colors,
    verbosity: Verbosity,
    /// Relative path to log file (if logging enabled)
    log_path: Option<std::path::PathBuf>,
    display_name: String,
    /// Unified streaming session for state tracking
    streaming_session: Rc<RefCell<StreamingSession>>,
    /// Terminal mode for output formatting
    terminal_mode: RefCell<TerminalMode>,
    /// Whether to show streaming quality metrics
    show_streaming_metrics: bool,
    /// Output printer for capturing or displaying output
    printer: SharedPrinter,
}

impl OpenCodeParser {
    pub(crate) fn new(colors: Colors, verbosity: Verbosity) -> Self {
        Self::with_printer(colors, verbosity, super::printer::shared_stdout())
    }

    /// Create a new `OpenCodeParser` with a custom printer.
    ///
    /// # Arguments
    ///
    /// * `colors` - Colors for terminal output
    /// * `verbosity` - Verbosity level for output
    /// * `printer` - Shared printer for output
    ///
    /// # Returns
    ///
    /// A new `OpenCodeParser` instance
    pub(crate) fn with_printer(
        colors: Colors,
        verbosity: Verbosity,
        printer: SharedPrinter,
    ) -> Self {
        let verbose_warnings = matches!(verbosity, Verbosity::Debug);
        let streaming_session = StreamingSession::new().with_verbose_warnings(verbose_warnings);

        // Use the printer's is_terminal method to validate it's connected correctly
        let _printer_is_terminal = printer.borrow().is_terminal();

        Self {
            colors,
            verbosity,
            log_path: None,
            display_name: "OpenCode".to_string(),
            streaming_session: Rc::new(RefCell::new(streaming_session)),
            terminal_mode: RefCell::new(TerminalMode::detect()),
            show_streaming_metrics: false,
            printer,
        }
    }

    pub(crate) const fn with_show_streaming_metrics(mut self, show: bool) -> Self {
        self.show_streaming_metrics = show;
        self
    }

    pub(crate) fn with_display_name(mut self, display_name: &str) -> Self {
        self.display_name = display_name.to_string();
        self
    }

    pub(crate) fn with_log_file(mut self, path: &str) -> Self {
        self.log_path = Some(std::path::PathBuf::from(path));
        self
    }

    #[cfg(test)]
    pub fn with_terminal_mode(self, mode: TerminalMode) -> Self {
        *self.terminal_mode.borrow_mut() = mode;
        self
    }

    /// Create a new parser with a test printer.
    ///
    /// This is the primary entry point for integration tests that need
    /// to capture parser output for verification.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_printer_for_test(
        colors: Colors,
        verbosity: Verbosity,
        printer: SharedPrinter,
    ) -> Self {
        Self::with_printer(colors, verbosity, printer)
    }

    /// Set the log file path for testing.
    ///
    /// This allows tests to verify log file content after parsing.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn with_log_file_for_test(mut self, path: &str) -> Self {
        self.log_path = Some(std::path::PathBuf::from(path));
        self
    }

    /// Parse a stream for testing purposes.
    ///
    /// This exposes the internal `parse_stream` method for integration tests.
    #[cfg(any(test, feature = "test-utils"))]
    pub fn parse_stream_for_test<R: std::io::BufRead>(
        &self,
        reader: R,
        workspace: &dyn crate::workspace::Workspace,
    ) -> std::io::Result<()> {
        self.parse_stream(reader, workspace)
    }

    /// Get a shared reference to the printer.
    ///
    /// This allows tests, monitoring, and other code to access the printer after parsing
    /// to verify output content, check for duplicates, or capture output for analysis.
    /// Only available with the `test-utils` feature.
    ///
    /// # Returns
    ///
    /// A clone of the shared printer reference (`Rc<RefCell<dyn Printable>>`)
    #[cfg(feature = "test-utils")]
    pub fn printer(&self) -> SharedPrinter {
        Rc::clone(&self.printer)
    }

    /// Get streaming quality metrics from the current session.
    ///
    /// This provides insight into the deduplication and streaming quality of the
    /// parsing session. Only available with the `test-utils` feature.
    ///
    /// # Returns
    ///
    /// A copy of the streaming quality metrics from the internal `StreamingSession`.
    #[cfg(feature = "test-utils")]
    pub fn streaming_metrics(&self) -> StreamingQualityMetrics {
        self.streaming_session
            .borrow()
            .get_streaming_quality_metrics()
    }

    /// Parse and display a single `OpenCode` JSON event
    ///
    /// From OpenCode source (`run.ts` lines 146-201), the NDJSON format uses events with:
    /// - `step_start`: Step initialization with snapshot info
    /// - `step_finish`: Step completion with reason, cost, tokens
    /// - `tool_use`: Tool invocation with tool name, callID, and state (status, input, output)
    /// - `text`: Streaming text content
    /// - `error`: Session/API error events
    pub(crate) fn parse_event(&self, line: &str) -> Option<String> {
        let event: OpenCodeEvent = if let Ok(e) = serde_json::from_str(line) {
            e
        } else {
            let trimmed = line.trim();
            if !trimmed.is_empty() && !trimmed.starts_with('{') {
                return Some(format!("{trimmed}\n"));
            }
            return None;
        };
        let c = &self.colors;
        let prefix = &self.display_name;

        let output = match event.event_type.as_str() {
            "step_start" => self.format_step_start_event(&event),
            "step_finish" => self.format_step_finish_event(&event),
            "tool_use" => self.format_tool_use_event(&event),
            "text" => self.format_text_event(&event),
            "error" => self.format_error_event(&event, line),
            _ => {
                // Unknown event type - use the generic formatter in verbose mode
                format_unknown_json_event(line, prefix, *c, self.verbosity.is_verbose())
            }
        };

        if output.is_empty() {
            None
        } else {
            Some(output)
        }
    }

    /// Format a `step_start` event
    fn format_step_start_event(&self, event: &OpenCodeEvent) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        // Create unique step ID for duplicate detection
        // Use part.message_id if available, otherwise combine session_id + part.id
        let step_id = event.part.as_ref().map_or_else(
            || {
                event
                    .session_id
                    .clone()
                    .unwrap_or_else(|| "unknown".to_string())
            },
            |part| {
                part.message_id.as_ref().map_or_else(
                    || {
                        let session = event.session_id.as_deref().unwrap_or("unknown");
                        let part_id = part.id.as_deref().unwrap_or("step");
                        format!("{session}:{part_id}")
                    },
                    std::clone::Clone::clone,
                )
            },
        );

        // Defensive: OpenCode can emit duplicate `step_start` events for the same message.
        // Suppress duplicates to avoid spamming and to avoid resetting streaming state mid-step.
        if self
            .streaming_session
            .borrow()
            .get_current_message_id()
            .is_some_and(|current| current == step_id)
        {
            return String::new();
        }

        // Reset streaming state on new step
        self.streaming_session.borrow_mut().on_message_start();
        self.streaming_session
            .borrow_mut()
            .set_current_message_id(Some(step_id));

        let snapshot = event
            .part
            .as_ref()
            .and_then(|p| p.snapshot.as_ref())
            .map(|s| format!("({s:.8}...)"))
            .unwrap_or_default();
        format!(
            "{}[{}]{} {}Step started{} {}{}{}\n",
            c.dim(),
            prefix,
            c.reset(),
            c.cyan(),
            c.reset(),
            c.dim(),
            snapshot,
            c.reset()
        )
    }

    /// Format a `step_finish` event
    fn format_step_finish_event(&self, event: &OpenCodeEvent) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        // Check for duplicate final message using message ID or fallback to streaming content check
        let session = self.streaming_session.borrow();
        let is_duplicate = session.get_current_message_id().map_or_else(
            || session.has_any_streamed_content(),
            |message_id| session.is_duplicate_final_message(message_id),
        );
        let was_streaming = session.has_any_streamed_content();
        let metrics = session.get_streaming_quality_metrics();
        drop(session);

        // Finalize the message (this marks it as displayed)
        let _was_in_block = self.streaming_session.borrow_mut().on_message_stop();

        event.part.as_ref().map_or_else(String::new, |part| {
            let reason = part.reason.as_deref().unwrap_or("unknown");
            let cost = part.cost.unwrap_or(0.0);

            let tokens_str = part.tokens.as_ref().map_or_else(String::new, |tokens| {
                let input = tokens.input.unwrap_or(0);
                let output = tokens.output.unwrap_or(0);
                let reasoning = tokens.reasoning.unwrap_or(0);
                let cache_read = tokens.cache.as_ref().and_then(|c| c.read).unwrap_or(0);
                if reasoning > 0 {
                    format!("in:{input} out:{output} reason:{reasoning} cache:{cache_read}")
                } else if cache_read > 0 {
                    format!("in:{input} out:{output} cache:{cache_read}")
                } else {
                    format!("in:{input} out:{output}")
                }
            });

            let is_success = reason == "tool-calls" || reason == "end_turn";
            let icon = if is_success { CHECK } else { CROSS };
            let color = if is_success { c.green() } else { c.yellow() };

            // Add final newline if we were streaming text
            let terminal_mode = *self.terminal_mode.borrow();
            let newline_prefix = if is_duplicate || was_streaming {
                let completion = TextDeltaRenderer::render_completion(terminal_mode);
                let show_metrics = (self.verbosity.is_debug() || self.show_streaming_metrics)
                    && metrics.total_deltas > 0;
                if show_metrics {
                    format!("{}\n{}", completion, metrics.format(*c))
                } else {
                    completion
                }
            } else {
                String::new()
            };

            let mut out = format!(
                "{}{}[{}]{} {}{} Step finished{} {}({}",
                newline_prefix,
                c.dim(),
                prefix,
                c.reset(),
                color,
                icon,
                c.reset(),
                c.dim(),
                reason
            );
            if !tokens_str.is_empty() {
                let _ = write!(out, ", {tokens_str}");
            }
            if cost > 0.0 {
                let _ = write!(out, ", ${cost:.4}");
            }
            let _ = writeln!(out, "){}", c.reset());
            out
        })
    }

    /// Format a `tool_use` event
    ///
    /// Based on OpenCode source (`run.ts` lines 163-174, `message-v2.ts` lines 221-287):
    /// - Shows tool name with status-specific icon and color
    /// - Status handling: pending (…), running (►), completed (✓), error (✗)
    /// - Title/description when available (from `state.title`)
    /// - Tool-specific input formatting based on tool type
    /// - Tool output/results shown at Normal+ verbosity
    /// - Error messages shown in red when status is "error"
    fn format_tool_use_event(&self, event: &OpenCodeEvent) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        event.part.as_ref().map_or_else(String::new, |part| {
            let tool_name = part.tool.as_deref().unwrap_or("unknown");
            let status = part
                .state
                .as_ref()
                .and_then(|s| s.status.as_deref())
                .unwrap_or("pending");
            let title = part.state.as_ref().and_then(|s| s.title.as_deref());

            // Status-specific icon and color based on ToolState variants from message-v2.ts
            // Statuses: "pending", "running", "completed", "error"
            let (icon, color) = match status {
                "completed" => (CHECK, c.green()),
                "error" => (CROSS, c.red()),
                "running" => ('►', c.cyan()),
                _ => ('…', c.yellow()), // "pending" or unknown
            };

            let mut out = format!(
                "{}[{}]{} {}Tool{}: {}{}{} {}{}{}\n",
                c.dim(),
                prefix,
                c.reset(),
                c.magenta(),
                c.reset(),
                c.bold(),
                tool_name,
                c.reset(),
                color,
                icon,
                c.reset()
            );

            // Show title if available (from state.title)
            if let Some(t) = title {
                let limit = self.verbosity.truncate_limit("text");
                let preview = truncate_text(t, limit);
                let _ = writeln!(
                    out,
                    "{}[{}]{} {}  └─ {}{}",
                    c.dim(),
                    prefix,
                    c.reset(),
                    c.dim(),
                    preview,
                    c.reset()
                );
            }

            // Show tool input at Normal+ verbosity with tool-specific formatting
            if self.verbosity.show_tool_input() {
                if let Some(ref state) = part.state {
                    if let Some(ref input_val) = state.input {
                        let input_str = Self::format_tool_specific_input(tool_name, input_val);
                        let limit = self.verbosity.truncate_limit("tool_input");
                        let preview = truncate_text(&input_str, limit);
                        if !preview.is_empty() {
                            let _ = writeln!(
                                out,
                                "{}[{}]{} {}  └─ {}{}",
                                c.dim(),
                                prefix,
                                c.reset(),
                                c.dim(),
                                preview,
                                c.reset()
                            );
                        }
                    }
                }
            }

            // Show error message when status is "error"
            if status == "error" {
                if let Some(ref state) = part.state {
                    if let Some(ref error_msg) = state.error {
                        let limit = self.verbosity.truncate_limit("tool_result");
                        let preview = truncate_text(error_msg, limit);
                        let _ = writeln!(
                            out,
                            "{}[{}]{} {}  └─ {}Error:{} {}{}{}",
                            c.dim(),
                            prefix,
                            c.reset(),
                            c.red(),
                            c.bold(),
                            c.reset(),
                            c.red(),
                            preview,
                            c.reset()
                        );
                    }
                }
            }

            // Show tool output at Normal+ verbosity when completed
            // (Changed from verbose-only to match OpenCode's interactive mode behavior)
            if self.verbosity.show_tool_input() && status == "completed" {
                if let Some(ref state) = part.state {
                    if let Some(ref output_val) = state.output {
                        let output_str = match output_val {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        if !output_str.is_empty() {
                            let limit = self.verbosity.truncate_limit("tool_result");
                            // Format multi-line output with proper indentation
                            self.format_tool_output(&mut out, &output_str, limit, prefix, *c);
                        }
                    }
                }
            }
            out
        })
    }

    /// Format tool output with proper multi-line handling
    ///
    /// For single-line outputs, shows inline. For multi-line outputs (like file contents),
    /// shows only the first few lines as a preview.
    fn format_tool_output(
        &self,
        out: &mut String,
        output: &str,
        limit: usize,
        prefix: &str,
        c: Colors,
    ) {
        use crate::config::truncation::MAX_OUTPUT_LINES;

        let lines: Vec<&str> = output.lines().collect();
        let is_multiline = lines.len() > 1;

        if is_multiline {
            // Multi-line output: show header then first few lines
            let _ = writeln!(
                out,
                "{}[{}]{} {}  └─ Output:{}",
                c.dim(),
                prefix,
                c.reset(),
                c.cyan(),
                c.reset()
            );

            let mut chars_used = 0;
            let indent = format!("{}[{}]{}     ", c.dim(), prefix, c.reset());

            for (lines_shown, line) in lines.iter().enumerate() {
                // Stop if we've shown enough lines OR exceeded char limit
                if lines_shown >= MAX_OUTPUT_LINES || chars_used + line.len() > limit {
                    let remaining = lines.len() - lines_shown;
                    if remaining > 0 {
                        let _ = writeln!(out, "{}{}...({} more lines)", indent, c.dim(), remaining);
                    }
                    break;
                }
                let _ = writeln!(out, "{}{}{}{}", indent, c.dim(), line, c.reset());
                chars_used += line.len() + 1;
            }
        } else {
            // Single-line output: show inline
            let preview = truncate_text(output, limit);
            if !preview.is_empty() {
                let _ = writeln!(
                    out,
                    "{}[{}]{} {}  └─ Output:{} {}",
                    c.dim(),
                    prefix,
                    c.reset(),
                    c.cyan(),
                    c.reset(),
                    preview
                );
            }
        }
    }

    /// Format tool input based on tool type
    ///
    /// From OpenCode source, each tool has specific input fields:
    /// - `read`: `filePath`, `offset?`, `limit?`
    /// - `bash`: `command`, `timeout?`
    /// - `write`: `filePath`, `content`
    /// - `edit`: `filePath`, ...
    /// - `glob`: `pattern`, `path?`
    /// - `grep`: `pattern`, `path?`, `include?`
    /// - `fetch`: `url`, `format?`, `timeout?`
    fn format_tool_specific_input(tool_name: &str, input: &serde_json::Value) -> String {
        let obj = match input.as_object() {
            Some(o) => o,
            None => return format_tool_input(input),
        };

        match tool_name {
            "read" | "view" => {
                // Primary: filePath, optional: offset, limit
                let file_path = obj.get("filePath").and_then(|v| v.as_str()).unwrap_or("");
                let mut result = file_path.to_string();
                if let Some(offset) = obj.get("offset").and_then(|v| v.as_u64()) {
                    result.push_str(&format!(" (offset: {offset})"));
                }
                if let Some(limit) = obj.get("limit").and_then(|v| v.as_u64()) {
                    result.push_str(&format!(" (limit: {limit})"));
                }
                result
            }
            "bash" => {
                // Primary: command
                obj.get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            }
            "write" => {
                // Primary: filePath (don't show content in summary)
                let file_path = obj.get("filePath").and_then(|v| v.as_str()).unwrap_or("");
                let content_len = obj
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                if content_len > 0 {
                    format!("{file_path} ({content_len} bytes)")
                } else {
                    file_path.to_string()
                }
            }
            "edit" => {
                // Primary: filePath
                obj.get("filePath")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            }
            "glob" => {
                // Primary: pattern, optional: path
                let pattern = obj.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                let path = obj.get("path").and_then(|v| v.as_str());
                if let Some(p) = path {
                    format!("{pattern} in {p}")
                } else {
                    pattern.to_string()
                }
            }
            "grep" => {
                // Primary: pattern, optional: path, include
                let pattern = obj.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                let mut result = format!("/{pattern}/");
                if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
                    result.push_str(&format!(" in {path}"));
                }
                if let Some(include) = obj.get("include").and_then(|v| v.as_str()) {
                    result.push_str(&format!(" ({include})"));
                }
                result
            }
            "fetch" | "webfetch" => {
                // Primary: url, optional: format
                let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let format = obj.get("format").and_then(|v| v.as_str());
                if let Some(f) = format {
                    format!("{url} ({f})")
                } else {
                    url.to_string()
                }
            }
            "todowrite" | "todoread" => {
                // Show count of todos if available
                if let Some(todos) = obj.get("todos").and_then(|v| v.as_array()) {
                    format!("{} items", todos.len())
                } else {
                    format_tool_input(input)
                }
            }
            _ => {
                // Fallback to generic formatting
                format_tool_input(input)
            }
        }
    }

    /// Format a `text` event
    fn format_text_event(&self, event: &OpenCodeEvent) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        if let Some(ref part) = event.part {
            if let Some(ref text) = part.text {
                // Accumulate streaming text using StreamingSession
                let (show_prefix, accumulated_text) = {
                    let mut session = self.streaming_session.borrow_mut();
                    let show_prefix = session.on_text_delta_key("main", text);
                    // Get accumulated text for streaming display
                    let accumulated_text = session
                        .get_accumulated(ContentType::Text, "main")
                        .unwrap_or("")
                        .to_string();
                    (show_prefix, accumulated_text)
                };

                // Show delta in real-time (both verbose and normal mode)
                let limit = self.verbosity.truncate_limit("text");
                let preview = truncate_text(&accumulated_text, limit);

                // Use TextDeltaRenderer for consistent rendering across all parsers
                let terminal_mode = *self.terminal_mode.borrow();
                if show_prefix {
                    // First delta: use renderer with prefix
                    return TextDeltaRenderer::render_first_delta(
                        &preview,
                        prefix,
                        *c,
                        terminal_mode,
                    );
                }
                // Subsequent deltas: use renderer for in-place update
                return TextDeltaRenderer::render_subsequent_delta(
                    &preview,
                    prefix,
                    *c,
                    terminal_mode,
                );
            }
        }
        String::new()
    }

    /// Format an `error` event
    ///
    /// From OpenCode source (`run.ts` lines 192-202), error events are emitted for session errors:
    /// ```typescript
    /// if (event.type === "session.error") {
    ///   let err = String(props.error.name)
    ///   if ("data" in props.error && props.error.data && "message" in props.error.data) {
    ///     err = String(props.error.data.message)
    ///   }
    ///   outputJsonEvent("error", { error: props.error })
    /// }
    /// ```
    fn format_error_event(&self, event: &OpenCodeEvent, raw_line: &str) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        // Try to extract error message from the event
        let error_msg = event.error.as_ref().map_or_else(
            || {
                // Fallback: try to extract from raw JSON
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(raw_line) {
                    json.get("error")
                        .and_then(|e| {
                            // Try data.message first (as in run.ts)
                            e.get("data")
                                .and_then(|d| d.get("message"))
                                .and_then(|m| m.as_str())
                                .map(String::from)
                                // Then try direct message
                                .or_else(|| {
                                    e.get("message").and_then(|m| m.as_str()).map(String::from)
                                })
                                // Then try name
                                .or_else(|| {
                                    e.get("name").and_then(|n| n.as_str()).map(String::from)
                                })
                        })
                        .unwrap_or_else(|| "Unknown error".to_string())
                } else {
                    "Unknown error".to_string()
                }
            },
            |err| {
                // Try data.message first (as in run.ts)
                err.data
                    .as_ref()
                    .and_then(|d| d.get("message"))
                    .and_then(|m| m.as_str())
                    .map(String::from)
                    // Then try direct message
                    .or_else(|| err.message.clone())
                    // Then try name
                    .or_else(|| err.name.clone())
                    .unwrap_or_else(|| "Unknown error".to_string())
            },
        );

        let limit = self.verbosity.truncate_limit("text");
        let preview = truncate_text(&error_msg, limit);

        format!(
            "{}[{}]{} {}{} Error:{} {}{}{}\n",
            c.dim(),
            prefix,
            c.reset(),
            c.red(),
            CROSS,
            c.reset(),
            c.red(),
            preview,
            c.reset()
        )
    }

    /// Check if an `OpenCode` event is a control event (state management with no user output)
    ///
    /// Control events are valid JSON that represent state transitions rather than
    /// user-facing content. They should be tracked separately from "ignored" events
    /// to avoid false health warnings.
    fn is_control_event(event: &OpenCodeEvent) -> bool {
        match event.event_type.as_str() {
            // Step lifecycle events are control events
            "step_start" | "step_finish" => true,
            _ => false,
        }
    }

    /// Check if an `OpenCode` event is a partial/delta event (streaming content displayed incrementally)
    ///
    /// Partial events represent streaming text deltas that are shown to the user
    /// in real-time. These should be tracked separately to avoid inflating "ignored" percentages.
    fn is_partial_event(event: &OpenCodeEvent) -> bool {
        match event.event_type.as_str() {
            // Text events produce streaming content
            "text" => true,
            _ => false,
        }
    }

    /// Parse a stream of `OpenCode` NDJSON events
    pub(crate) fn parse_stream<R: BufRead>(
        &self,
        mut reader: R,
        workspace: &dyn crate::workspace::Workspace,
    ) -> io::Result<()> {
        use super::incremental_parser::IncrementalNdjsonParser;

        let c = &self.colors;
        let monitor = HealthMonitor::new("OpenCode");
        // Accumulate log content in memory, write to workspace at the end
        let logging_enabled = self.log_path.is_some();
        let mut log_buffer: Vec<u8> = Vec::new();

        // Use incremental parser for true real-time streaming
        // This processes JSON as soon as it's complete, not waiting for newlines
        let mut incremental_parser = IncrementalNdjsonParser::new();
        let mut byte_buffer = Vec::new();

        loop {
            // Read available bytes
            byte_buffer.clear();
            let chunk = reader.fill_buf()?;
            if chunk.is_empty() {
                break;
            }

            // Process all bytes immediately
            byte_buffer.extend_from_slice(chunk);
            let consumed = chunk.len();
            reader.consume(consumed);

            // Feed bytes to incremental parser
            let json_events = incremental_parser.feed(&byte_buffer);

            // Process each complete JSON event immediately
            for line in json_events {
                let trimmed = line.trim();
                if trimmed.is_empty() {
                    continue;
                }

                if self.verbosity.is_debug() {
                    let mut printer = self.printer.borrow_mut();
                    writeln!(
                        printer,
                        "{}[DEBUG]{} {}{}{}",
                        c.dim(),
                        c.reset(),
                        c.dim(),
                        &line,
                        c.reset()
                    )?;
                    printer.flush()?;
                }

                // Parse the event once - parse_event handles malformed JSON by returning None
                match self.parse_event(&line) {
                    Some(output) => {
                        // Check if this is a partial/delta event (streaming content)
                        if trimmed.starts_with('{') {
                            if let Ok(event) = serde_json::from_str::<OpenCodeEvent>(&line) {
                                if Self::is_partial_event(&event) {
                                    monitor.record_partial_event();
                                } else {
                                    monitor.record_parsed();
                                }
                            } else {
                                monitor.record_parsed();
                            }
                        } else {
                            monitor.record_parsed();
                        }
                        // Write output to printer
                        let mut printer = self.printer.borrow_mut();
                        write!(printer, "{output}")?;
                        printer.flush()?;
                    }
                    None => {
                        // Check if this was a control event (state management with no user output)
                        if trimmed.starts_with('{') {
                            if let Ok(event) = serde_json::from_str::<OpenCodeEvent>(&line) {
                                if Self::is_control_event(&event) {
                                    monitor.record_control_event();
                                } else {
                                    // Valid JSON but not a control event - track as unknown
                                    monitor.record_unknown_event();
                                }
                            } else {
                                // Failed to deserialize - track as parse error
                                monitor.record_parse_error();
                            }
                        } else {
                            monitor.record_ignored();
                        }
                    }
                }

                if logging_enabled {
                    writeln!(log_buffer, "{line}")?;
                }
            }
        }

        // Handle any remaining buffered data when the stream ends.
        // Only process if it's valid JSON - incomplete buffered data should be skipped.
        if let Some(remaining) = incremental_parser.finish() {
            let trimmed = remaining.trim();
            if !trimmed.is_empty()
                && trimmed.starts_with('{')
                && serde_json::from_str::<OpenCodeEvent>(&remaining).is_ok()
            {
                // Process the remaining event
                if let Some(output) = self.parse_event(&remaining) {
                    monitor.record_parsed();
                    let mut printer = self.printer.borrow_mut();
                    write!(printer, "{output}")?;
                    printer.flush()?;
                }
                // Write to log buffer
                if logging_enabled {
                    writeln!(log_buffer, "{remaining}")?;
                }
            }
        }

        // Write accumulated log content to workspace
        if let Some(log_path) = &self.log_path {
            workspace.append_bytes(log_path, &log_buffer)?;
        }

        // OpenCode models may emit XML directly in text output (without using tools to write
        // `.agent/tmp/*.xml`). Capture `<ralph-commit>...</ralph-commit>` from the accumulated
        // text stream and write it to the standard commit artifact path so the commit phase can
        // validate it via file-based extraction.
        if let Some(accumulated) = self
            .streaming_session
            .borrow()
            .get_accumulated(ContentType::Text, "main")
        {
            if let Some(xml) =
                crate::files::llm_output_extraction::xml_extraction::extract_xml_commit(accumulated)
            {
                workspace.create_dir_all(Path::new(".agent/tmp"))?;
                workspace.write(
                    Path::new(crate::files::llm_output_extraction::file_based_extraction::paths::COMMIT_MESSAGE_XML),
                    &xml,
                )?;
            }
        }
        if let Some(warning) = monitor.check_and_warn(*c) {
            let mut printer = self.printer.borrow_mut();
            writeln!(printer, "{warning}")?;
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_opencode_step_start() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"step_start","timestamp":1768191337567,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06aa45c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"step-start","snapshot":"5d36aa035d4df6edb73a68058733063258114ed5"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Step started"));
        assert!(out.contains("5d36aa03"));
    }

    #[test]
    fn test_opencode_step_start_dedupes_duplicate_starts_for_same_message_id() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"step_start","timestamp":1768191337567,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06aa45c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"step-start","snapshot":"5d36aa035d4df6edb73a68058733063258114ed5"}}"#;

        let first = parser.parse_event(json);
        assert!(first.is_some());
        assert!(first.unwrap().contains("Step started"));

        // Defensive behavior: OpenCode can emit duplicate step_start events; we should not spam.
        let second = parser.parse_event(json);
        assert!(second.is_none());
    }

    #[test]
    fn test_opencode_step_finish() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"step_finish","timestamp":1768191347296,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06aca1d001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"step-finish","reason":"tool-calls","snapshot":"5d36aa035d4df6edb73a68058733063258114ed5","cost":0,"tokens":{"input":108,"output":151,"reasoning":0,"cache":{"read":11236,"write":0}}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Step finished"));
        assert!(out.contains("tool-calls"));
        assert!(out.contains("in:108"));
        assert!(out.contains("out:151"));
        assert!(out.contains("cache:11236"));
    }

    #[test]
    fn test_opencode_tool_use_completed() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"tool","callID":"call_8a2985d92e63","tool":"read","state":{"status":"completed","input":{"filePath":"/test/PLAN.md"},"output":"<file>\n00001| # Implementation Plan\n</file>","title":"PLAN.md"}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool"));
        assert!(out.contains("read"));
        assert!(out.contains("✓")); // completed icon
        assert!(out.contains("PLAN.md"));
    }

    #[test]
    fn test_opencode_tool_use_pending() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"tool","callID":"call_8a2985d92e63","tool":"bash","state":{"status":"pending","input":{"command":"ls -la"}}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool"));
        assert!(out.contains("bash"));
        assert!(out.contains("…")); // pending icon (WAIT)
    }

    #[test]
    fn test_opencode_tool_use_shows_input() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"tool","callID":"call_8a2985d92e63","tool":"read","state":{"status":"completed","input":{"filePath":"/Users/test/file.rs"}}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool"));
        assert!(out.contains("read"));
        assert!(out.contains("/Users/test/file.rs"));
    }

    #[test]
    fn test_opencode_text_event() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"text","timestamp":1768191347231,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac63300","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"text","text":"I'll start by reading the plan and requirements to understand what needs to be implemented.","time":{"start":1768191347226,"end":1768191347226}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("I'll start by reading the plan"));
    }

    #[test]
    fn test_opencode_unknown_event_ignored() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"unknown_event","timestamp":1768191347231,"sessionID":"ses_44f9562d4ffe","part":{}}"#;
        let output = parser.parse_event(json);
        // Unknown events should return None
        assert!(output.is_none());
    }

    #[test]
    fn test_opencode_parser_non_json_passthrough() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let output = parser.parse_event("Error: something went wrong");
        assert!(output.is_some());
        assert!(output.unwrap().contains("Error: something went wrong"));
    }

    #[test]
    fn test_opencode_parser_malformed_json_ignored() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let output = parser.parse_event("{invalid json here}");
        assert!(output.is_none());
    }

    #[test]
    fn test_opencode_step_finish_with_cost() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"step_finish","timestamp":1768191347296,"sessionID":"ses_44f9562d4ffe","part":{"type":"step-finish","reason":"end_turn","cost":0.0025,"tokens":{"input":1000,"output":500,"reasoning":0,"cache":{"read":0,"write":0}}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Step finished"));
        assert!(out.contains("end_turn"));
        assert!(out.contains("$0.0025"));
    }

    #[test]
    fn test_opencode_tool_verbose_shows_output() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Verbose);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"/test.rs"},"output":"fn main() { println!(\"Hello\"); }"}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool"));
        assert!(out.contains("read"));
        assert!(out.contains("Output"));
        assert!(out.contains("fn main"));
    }

    #[test]
    fn test_opencode_tool_running_status() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool":"bash","state":{"status":"running","input":{"command":"npm test"},"time":{"start":1768191346712}}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool"));
        assert!(out.contains("bash"));
        assert!(out.contains("►")); // running icon
    }

    #[test]
    fn test_opencode_tool_error_status() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool":"bash","state":{"status":"error","input":{"command":"invalid_cmd"},"error":"Command not found: invalid_cmd","time":{"start":1768191346712,"end":1768191346800}}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Tool"));
        assert!(out.contains("bash"));
        assert!(out.contains("✗")); // error icon
        assert!(out.contains("Error"));
        assert!(out.contains("Command not found"));
    }

    #[test]
    fn test_opencode_error_event() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"error","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","error":{"name":"APIError","message":"Rate limit exceeded"}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Error"));
        assert!(out.contains("Rate limit exceeded"));
    }

    #[test]
    fn test_opencode_error_event_with_data_message() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        // Error with data.message (as in run.ts lines 197-199)
        let json = r#"{"type":"error","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","error":{"name":"ProviderError","data":{"message":"Invalid API key"}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Error"));
        assert!(out.contains("Invalid API key"));
    }

    #[test]
    fn test_opencode_tool_bash_formatting() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"type":"tool","tool":"bash","state":{"status":"completed","input":{"command":"git status"},"output":"On branch main","title":"git status"}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("bash"));
        assert!(out.contains("git status"));
    }

    #[test]
    fn test_opencode_tool_glob_formatting() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"type":"tool","tool":"glob","state":{"status":"completed","input":{"pattern":"**/*.rs","path":"src"},"output":"found 10 files","title":"**/*.rs"}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("glob"));
        assert!(out.contains("**/*.rs"));
        assert!(out.contains("in src"));
    }

    #[test]
    fn test_opencode_tool_grep_formatting() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"type":"tool","tool":"grep","state":{"status":"completed","input":{"pattern":"TODO","path":"src","include":"*.rs"},"output":"3 matches","title":"TODO"}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("grep"));
        assert!(out.contains("/TODO/"));
        assert!(out.contains("in src"));
        assert!(out.contains("(*.rs)"));
    }

    #[test]
    fn test_opencode_tool_write_formatting() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"type":"tool","tool":"write","state":{"status":"completed","input":{"filePath":"test.txt","content":"Hello World"},"output":"wrote 11 bytes","title":"test.txt"}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("write"));
        assert!(out.contains("test.txt"));
        assert!(out.contains("11 bytes"));
    }

    #[test]
    fn test_opencode_tool_read_with_offset_limit() {
        let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
        let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"large.txt","offset":100,"limit":50},"output":"content...","title":"large.txt"}}}"#;
        let output = parser.parse_event(json);
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("read"));
        assert!(out.contains("large.txt"));
        assert!(out.contains("offset: 100"));
        assert!(out.contains("limit: 50"));
    }
}
