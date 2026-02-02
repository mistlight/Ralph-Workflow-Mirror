//! Unified delta display system for streaming content.
//!
//! This module provides centralized logic for displaying partial vs. complete
//! content consistently across all parsers. It handles visual distinction,
//! real-time streaming display, and automatic transition from delta to complete.
//!
//! # `DeltaRenderer` Trait
//!
//! The `DeltaRenderer` trait defines a consistent interface for rendering
//! streaming deltas across all parsers. Implementations must ensure:
//! - First chunk shows prefix with accumulated content ending with carriage return
//! - Subsequent chunks update in-place (clear line, rewrite with prefix, carriage return)
//! - Final output adds newline when streaming completes
//!
//! # In-Place Updates
//!
//! The terminal escape sequences used for in-place updates:
//! - `\x1b[2K` - Clears the entire line (not just to end like `\x1b[0K`)
//! - `\r` - Returns cursor to the beginning of the line
//!
//! This ensures that previous content is completely erased before displaying
//! the updated content, preventing visual artifacts.
//!
//! # Multi-Line In-Place Update Pattern
//!
//! The renderer uses a multi-line pattern with cursor positioning for in-place updates.
//! This is the industry standard for streaming CLIs (used by Rich, Ink, Bubble Tea).
//!
//! ```text
//! [ccs-glm] Hello\n\x1b[1A             <- First chunk: prefix + content + newline + cursor up
//! \x1b[2K\r[ccs-glm] Hello World\n\x1b[1A  <- Second chunk: clear, rewrite, newline, cursor up
//! [ccs-glm] Hello World\n\x1b[1B\n       <- Final: move cursor down + newline
//! ```
//!
//! This pattern ensures:
//! - Newline forces immediate terminal output buffer flush
//! - Cursor positioning provides reliable in-place updates
//! - Production-quality rendering used by major CLI libraries
//!
//! # Terminal Mode Detection
//!
//! The renderer automatically detects terminal capability and adjusts output:
//! - **Full mode**: Uses cursor positioning for in-place updates (TTY with capable terminal)
//! - **Basic mode**: Uses colors but no cursor positioning (e.g., `TERM=dumb`)
//! - **None mode**: No ANSI sequences (pipes, redirects, CI environments)
//!
//! # Prefix Display Strategy
//!
//! The prefix (e.g., `[ccs-glm]`) is displayed on every delta update by default.
//! This provides clear visual feedback about which agent is currently streaming.
//!
//! ## Prefix Debouncing
//!
//! For scenarios where prefix repetition creates visual noise (e.g., character-by-character
//! streaming), a `PrefixDebouncer` can be used to control prefix display frequency.
//! It supports both delta-count-based and time-based strategies:
//!
//! - **Count-based**: Show prefix every N deltas (default: every delta)
//! - **Time-based**: Show prefix after M milliseconds since last prefix
//!
//! The debouncer is opt-in; the default behavior shows prefix on every delta.

use crate::json_parser::terminal::TerminalMode;
use crate::logger::Colors;

#[cfg(test)]
use std::time::{Duration, Instant};

// Display utilities (constants and sanitize function)
include!("delta_display/display_utils.rs");

// Prefix debouncing (test-only)
include!("delta_display/debouncer.rs");

// Delta renderer trait and implementation
include!("delta_display/renderer.rs");

// Delta display formatter
include!("delta_display/formatter.rs");

// Tests
include!("delta_display/tests.rs");
