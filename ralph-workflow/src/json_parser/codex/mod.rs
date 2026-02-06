//! Codex CLI JSON parser.
//!
//! Parses NDJSON output from `OpenAI` Codex CLI and formats it for display.
//!
//! # Streaming Output Behavior
//!
//! This parser implements real-time streaming output for text deltas. When content
//! arrives in multiple chunks (via `item.started` events with `agent_message` type),
//! the parser:
//!
//! 1. **Accumulates** text deltas from each chunk into a buffer
//! 2. **Displays** the accumulated text after each chunk
//! 3. **Uses carriage return (`\r`) and line clearing (`\x1b[2K`)** to rewrite the entire line,
//!    creating an updating effect that shows the content building up in real-time
//! 4. **Shows prefix on every delta**, rewriting the entire line each time (industry standard)
//!
//! Example output sequence for streaming "Hello World" in two chunks:
//! ```text
//! [Codex] Hello\r          (first chunk with prefix, no newline)
//! \x1b[2K\r[Codex] Hello World\r  (second chunk clears line, rewrites with accumulated)
//! [Codex] Hello World\n     (item.completed shows final result with prefix)
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

mod event_handlers;

use crate::config::Verbosity;
use crate::logger::Colors;
use crate::workspace::Workspace;
use std::cell::RefCell;
use std::io::{self, BufRead, Write};
use std::path::PathBuf;
use std::rc::Rc;

use super::health::HealthMonitor;
#[cfg(any(test, feature = "test-utils"))]
use super::health::StreamingQualityMetrics;
use super::printer::SharedPrinter;
use super::streaming_state::StreamingSession;
use super::terminal::TerminalMode;
use super::types::{format_unknown_json_event, CodexEvent};

use event_handlers::{
    handle_error, handle_item_completed, handle_item_started, handle_thread_started,
    handle_turn_completed, handle_turn_failed, handle_turn_started, EventHandlerContext,
};

include!("parser.rs");
include!("event_parsing.rs");
include!("stream_parsing.rs");
