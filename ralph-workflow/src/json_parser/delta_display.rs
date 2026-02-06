//! Unified delta display system for streaming content.
//!
//! This module provides centralized logic for displaying partial vs. complete
//! content consistently across all parsers. It handles visual distinction,
//! real-time streaming display, and automatic transition from delta to complete.
//!
//! # Streaming Architecture: Append-Only Pattern
//!
//! This module implements ChatGPT-style real-time streaming using an append-only
//! pattern in Full mode (TTY with ANSI support):
//!
//! ## Append-Only Pattern (Full Mode)
//!
//! ```text
//! [ccs/glm] Hello           <- First delta: prefix + content, NO newline
//! \r[ccs/glm] Hello World   <- Second delta: \r (carriage return) + full line
//! \r[ccs/glm] Hello World!  <- Third delta: \r + full line
//! \n                         <- Completion: single newline
//! ```
//!
//! ## Why Append-Only?
//!
//! Previous implementations used `\n\x1b[1A` (newline + cursor up) for in-place
//! updates. This pattern has critical flaws:
//!
//! 1. **Line wrapping breaks in-place updates**
//!    - When content exceeds terminal width, it wraps to multiple rows
//!    - `\x1b[2K` (clear line) only clears the current row, not wrapped rows above
//!    - `\x1b[1A` (cursor up 1) assumes single-row content, incorrect when wrapped
//!    - Result: multi-line waterfall effect instead of in-place update
//!
//! 2. **ANSI-stripping consoles see literal newlines**
//!    - Many CI/log consoles strip ANSI sequences
//!    - The `\n` from `\n\x1b[1A` becomes a real visible newline
//!    - Cursor positioning is ignored, creating repeated prefixed lines
//!
//! 3. **Complexity and fragility**
//!    - Width-aware multi-row updates require tracking terminal dimensions
//!    - Different terminals handle wrapping differently
//!    - Difficult to test all edge cases
//!
//! ## Append-Only Advantages
//!
//! 1. **Works with wrapping**: `\r` returns to column 0 of current row, regardless
//!    of how many rows the content occupies
//! 2. **No ANSI cursor positioning during streaming**: Only uses `\r` (carriage return)
//!    which works even if ANSI is partially supported
//! 3. **Simple and robust**: One pattern works for all terminal widths and content lengths
//! 4. **Real-time streaming**: Each delta immediately updates the visible line
//!
//! ## Terminal Mode Behavior
//!
//! The renderer automatically detects terminal capability and adjusts output:
//!
//! - **Full mode (TTY)**: Append-only streaming with `\r` for in-place updates
//!   - Real-time per-delta output using carriage return pattern
//!   - Single `\n` at completion to finalize the line
//!   - Works correctly even when content wraps to multiple rows
//!
//! - **Basic/None mode (logs, CI)**: Per-delta output suppressed, flush once at completion
//!   - No per-delta output during streaming (prevents spam in logs)
//!   - Accumulated content flushed ONCE at completion boundaries
//!   - No ANSI sequences in None mode (pipes, redirects, CI environments)
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
