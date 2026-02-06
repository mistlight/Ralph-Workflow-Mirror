// Delta renderer trait and implementations.
//
// Contains the DeltaRenderer trait and TextDeltaRenderer implementation.
//
// # CCS Spam Prevention Architecture
//
// This module implements a three-layer approach to prevent repeated prefixed lines
// for streaming deltas in non-TTY modes (logs, CI output):
//
// ## Layer 1: Suppression at Renderer Level
//
// Delta renderers (`TextDeltaRenderer`, `ThinkingDeltaRenderer`) return empty strings
// in `TerminalMode::Basic` and `TerminalMode::None` for both `render_first_delta` and
// `render_subsequent_delta`. This prevents per-delta spam at the source.
//
// ## Layer 2: Accumulation in StreamingSession
//
// `StreamingSession` (in `streaming_state/session`) accumulates all content by
// (ContentType, index) across deltas. This state is preserved across all delta
// events for a single message.
//
// ## Layer 3: Flush at Completion Boundaries
//
// Parser layer (ClaudeParser, CodexParser) flushes accumulated content ONCE at
// completion boundaries:
// - ClaudeParser: `handle_message_stop` (in `claude/delta_handling.rs`)
// - CodexParser: `item.completed` handlers (in `codex/event_handlers/*.rs`)
//
// This ensures:
// - **Full mode (TTY)**: In-place updates work normally with cursor positioning
// - **Basic/None modes**: One prefixed line per content block, regardless of delta count
//
// ## Validation
//
// Comprehensive regression tests validate this architecture:
// - `ccs_delta_spam_systematic_reproduction.rs`: NEW systematic reproduction test (all delta types, both parsers, both modes)
// - `ccs_all_delta_types_spam_reproduction.rs`: 1000+ deltas per block
// - `ccs_streaming_spam_all_deltas.rs`: All delta types (text/thinking/tool)
// - `ccs_nuclear_full_log_regression.rs`: Real production logs (12,000+ deltas)
// - `ccs_streaming_edge_cases.rs`: Edge cases (empty deltas, rapid transitions)
// - `codex_reasoning_spam_regression.rs`: Original Codex reasoning fix

/// Renderer for streaming delta content.
///
/// This trait defines the contract for rendering streaming deltas consistently
/// across all parsers using the append-only pattern.
///
/// # Append-Only Pattern (Full Mode)
///
/// 1. **First delta**: Shows prefix with accumulated content, NO newline
///    - Example: `[ccs/glm] Hello`
///
/// 2. **Subsequent deltas**: Carriage return + rewrite full line
///    - Example: `\r[ccs/glm] Hello World`
///
/// 3. **Completion**: Single newline to finalize the line
///    - Example: `\n`
///
/// This pattern works correctly even when content wraps to multiple terminal rows,
/// because `\r` (carriage return) returns to column 0 of the current row without
/// assuming anything about the number of rows occupied.
///
/// # Non-TTY Modes (Basic/None)
///
/// Per-delta output is suppressed. Content is flushed ONCE at completion boundaries
/// by the parser layer to prevent spam in logs and CI output.
///
/// # Rendering Rules
///
/// - `render_first_delta()`: Called for the first delta of a content block
///   - Must include prefix
///   - Must end with newline + cursor up (`\n\x1b[1A`) for in-place updates (in Full mode)
///   - Shows the accumulated content so far
///
/// - `render_subsequent_delta()`: Called for subsequent deltas
///   - Must include prefix (rewrite entire line)
///   - Must use `\x1b[2K\r` to clear entire line and return to start (in Full mode)
///   - Shows the full accumulated content (not just the new delta)
///   - Must end with newline + cursor up (`\n\x1b[1A`) (in Full mode)
///
/// - `render_completion()`: Called when streaming completes
///   - Returns cursor down + newline (`\x1b[1B\n`) in Full mode
///   - Returns simple newline in Basic/None mode
///
/// # Terminal Mode Awareness
///
/// The renderer automatically adapts output based on terminal capability:
/// - **Full mode**: Uses cursor positioning for in-place updates
/// - **Basic mode**: Uses colors but simple line output (no cursor positioning)
/// - **None mode**: Plain text output (no ANSI sequences)
///
/// # Example
///
/// ```ignore
/// use crate::json_parser::delta_display::DeltaRenderer;
/// use crate::logger::Colors;
/// use crate::json_parser::TerminalMode;
///
/// let colors = Colors { enabled: true };
/// let terminal_mode = TerminalMode::detect();
///
/// // First chunk
/// let output = DeltaRenderer::render_first_delta(
///     "Hello",
///     "ccs-glm",
///     colors,
///     terminal_mode
/// );
///
/// // Second chunk
/// let output = DeltaRenderer::render_subsequent_delta(
///     "Hello World",
///     "ccs-glm",
///     colors,
///     terminal_mode
/// );
///
/// // Complete
/// let output = DeltaRenderer::render_completion(terminal_mode);
/// ```
pub trait DeltaRenderer {
    /// Render the first delta of a content block.
    ///
    /// This is called when streaming begins for a new content block.
    /// The output should include the prefix and the accumulated content.
    ///
    /// # Arguments
    /// * `accumulated` - The full accumulated content so far
    /// * `prefix` - The agent prefix (e.g., "ccs-glm")
    /// * `colors` - Terminal colors
    /// * `terminal_mode` - The detected terminal capability mode
    ///
    /// # Returns
    /// A formatted string with prefix and content. In Full mode, ends with `\n\x1b[1A`.
    /// In Basic/None modes, returns empty string (per-delta output suppressed).
    fn render_first_delta(
        accumulated: &str,
        prefix: &str,
        colors: Colors,
        terminal_mode: TerminalMode,
    ) -> String;

    /// Render a subsequent delta (in-place update).
    ///
    /// This is called for all deltas after the first. The output should
    /// clear the entire line and rewrite with the prefix and accumulated content
    /// in Full mode, or append content in Basic/None mode.
    ///
    /// # Arguments
    /// * `accumulated` - The full accumulated content so far
    /// * `prefix` - The agent prefix (e.g., "ccs-glm")
    /// * `colors` - Terminal colors
    /// * `terminal_mode` - The detected terminal capability mode
    ///
    /// # Returns
    /// A formatted string with prefix and content. In Full mode, ends with `\n\x1b[1A`.
    /// In Basic/None modes, returns empty string (per-delta output suppressed).
    fn render_subsequent_delta(
        accumulated: &str,
        prefix: &str,
        colors: Colors,
        terminal_mode: TerminalMode,
    ) -> String;

    /// Render the completion of streaming.
    ///
    /// This is called when streaming completes to finalize the line.
    /// In Full mode with append-only pattern, this emits a single newline to complete the line.
    ///
    /// The streamed content is already visible on the terminal from previous deltas.
    /// This method simply adds the final newline for proper line termination.
    ///
    /// # Arguments
    /// * `terminal_mode` - The detected terminal capability mode
    ///
    /// # Returns
    /// A string with appropriate completion sequence for the terminal mode.
    fn render_completion(terminal_mode: TerminalMode) -> String {
        match terminal_mode {
            TerminalMode::Full => "\n".to_string(), // Single newline at end for append-only pattern
            // In non-TTY modes, streamed output is suppressed and the parser flushes
            // newline-terminated content at completion boundaries. Returning a newline here
            // would add an extra blank line if a caller invokes `render_completion`.
            TerminalMode::Basic | TerminalMode::None => String::new(),
        }
    }
}

/// Default implementation of `DeltaRenderer` for text content.
///
/// This implementation follows the multi-line rendering pattern used by production CLIs:
/// - Prefix and content on same line ending with newline + cursor up
/// - Content updates in-place using clear, rewrite, and newline + cursor up
/// - Sanitizes newlines to spaces (to prevent artificial line breaks)
/// - Uses ANSI escape codes for in-place updates with full line clear
/// - Applies consistent color formatting
///
/// # Output Pattern
///
/// ## Full Mode (TTY with capable terminal)
///
/// ```text
/// [ccs-glm] Hello\n\x1b[1A             <- First chunk: prefix + content + newline + cursor up
/// \x1b[2K\r[ccs-glm] Hello World\n\x1b[1A  <- Second chunk: clear, rewrite, newline, cursor up
/// [ccs-glm] Hello World\n\x1b[1B\n       <- Final: move cursor down + newline
/// ```
///
/// ## Basic/None Mode (non-TTY logs)
///
/// In non-TTY modes, per-delta output is suppressed to avoid repeated prefixed
/// lines for partial updates. The parser is responsible for flushing the final
/// accumulated content once at a completion boundary (e.g. `message_stop`).
///
/// ```text
/// [ccs-glm] Hello World\n
/// ```
///
/// # CCS Spam Prevention (Bug Fix)
///
/// This implementation prevents repeated prefixed lines for CCS agents (ccs/codex,
/// ccs/glm) in non-TTY modes. The spam fix is validated with comprehensive regression
/// tests that simulate real-world streaming scenarios:
///
/// - **Ultra-extreme delta counts:** Tests verify no spam with 1000+ deltas per content block
/// - **Multi-turn sessions:** Validates 3+ turns with 200+ deltas each (600+ total)
/// - **All delta types:** Covers text deltas, thinking deltas, and tool input deltas
/// - **Real-world logs:** Tests with production logs containing 12,596 total deltas
///
/// The multi-line pattern (in-place updates) is the industry standard used by
/// Rich, Ink, Bubble Tea, and other production CLI libraries for clean streaming
/// output.
///
/// See comprehensive regression tests:
/// - `tests/integration_tests/ccs_delta_spam_systematic_reproduction.rs` (NEW: systematic reproduction & verification)
/// - `tests/integration_tests/ccs_all_delta_types_spam_reproduction.rs` (ultra-comprehensive edge case coverage)
/// - `tests/integration_tests/ccs_extreme_streaming_regression.rs` (500+ deltas per block)
/// - `tests/integration_tests/ccs_streaming_spam_all_deltas.rs` (all delta types)
/// - `tests/integration_tests/ccs_real_world_log_regression.rs` (production log with 12,596 deltas)
/// - `tests/integration_tests/codex_reasoning_spam_regression.rs` (original reasoning fix)
pub struct TextDeltaRenderer;

impl DeltaRenderer for TextDeltaRenderer {
    fn render_first_delta(
        accumulated: &str,
        prefix: &str,
        colors: Colors,
        terminal_mode: TerminalMode,
    ) -> String {
        // Sanitize content: replace newlines with spaces and collapse multiple whitespace
        // NOTE: No truncation here - allow full content to accumulate during streaming
        let sanitized = sanitize_for_display(accumulated);

        match terminal_mode {
            TerminalMode::Full => {
                // Append-only pattern: prefix + content, NO NEWLINE
                // This allows content to grow on same line without wrapping issues
                format!(
                    "{}[{}]{} {}{}{}",
                    colors.dim(),
                    prefix,
                    colors.reset(),
                    colors.white(),
                    sanitized,
                    colors.reset()
                )
            }
            TerminalMode::Basic | TerminalMode::None => {
                // SUPPRESS per-delta output in non-TTY modes to prevent spam.
                // The accumulated content will be rendered ONCE at completion boundaries
                // (message_stop, content_block_stop) by the parser layer.
                // This prevents repeated prefixed lines in logs and CI output.
                String::new()
            }
        }
    }

    fn render_subsequent_delta(
        accumulated: &str,
        prefix: &str,
        colors: Colors,
        terminal_mode: TerminalMode,
    ) -> String {
        // Sanitize content: replace newlines with spaces and collapse multiple whitespace
        // NOTE: No truncation here - allow full content to accumulate during streaming
        let sanitized = sanitize_for_display(accumulated);

        match terminal_mode {
            TerminalMode::Full => {
                // Append-only: compute diff and emit only NEW content
                // Use carriage return to go back to start, rewrite full line
                // This is the classic "progressive line update" pattern
                format!(
                    "\r{}[{}]{} {}{}{}",
                    colors.dim(),
                    prefix,
                    colors.reset(),
                    colors.white(),
                    sanitized,
                    colors.reset()
                )
            }
            TerminalMode::Basic | TerminalMode::None => {
                // SUPPRESS per-delta output in non-TTY modes to prevent spam.
                // The accumulated content will be rendered ONCE at completion boundaries
                // (message_stop, content_block_stop) by the parser layer.
                // This prevents repeated prefixed lines in logs and CI output.
                String::new()
            }
        }
    }
}

/// Renderer for streaming thinking deltas.
///
/// This uses the same multi-line in-place update pattern as `TextDeltaRenderer` in `TerminalMode::Full`
/// so the caller can finalize the line with `DeltaRenderer::render_completion`.
///
/// # CCS Spam Prevention (Bug Fix)
///
/// Like `TextDeltaRenderer`, this implementation suppresses per-delta output in non-TTY modes
/// to prevent repeated "[ccs/codex] Thinking:" and "[ccs/glm] Thinking:" lines in logs.
/// The fix is validated with ultra-extreme streaming tests (1000+ thinking deltas).
///
/// See comprehensive regression tests:
/// - `tests/integration_tests/ccs_delta_spam_systematic_reproduction.rs` (NEW: systematic reproduction test)
/// - `tests/integration_tests/ccs_all_delta_types_spam_reproduction.rs` (1000+ deltas, rapid succession, interleaved blocks)
/// - `tests/integration_tests/ccs_extreme_streaming_regression.rs` (500+ deltas per block)
/// - `tests/integration_tests/ccs_streaming_spam_all_deltas.rs` (all delta types)
/// - `tests/integration_tests/codex_reasoning_spam_regression.rs` (original reasoning fix)
pub struct ThinkingDeltaRenderer;

impl DeltaRenderer for ThinkingDeltaRenderer {
    fn render_first_delta(
        accumulated: &str,
        prefix: &str,
        colors: Colors,
        terminal_mode: TerminalMode,
    ) -> String {
        let sanitized = sanitize_for_display(accumulated);

        match terminal_mode {
            TerminalMode::Full => format!(
                "{}[{}]{} {}Thinking: {}{}{}",
                colors.dim(),
                prefix,
                colors.reset(),
                colors.dim(),
                colors.cyan(),
                sanitized,
                colors.reset()
            ),
            TerminalMode::Basic | TerminalMode::None => {
                // SUPPRESS per-delta thinking output in non-TTY modes.
                // Thinking content will be flushed ONCE at completion boundaries
                // (message_stop for Claude, item.completed for Codex).
                String::new()
            }
        }
    }

    fn render_subsequent_delta(
        accumulated: &str,
        prefix: &str,
        colors: Colors,
        terminal_mode: TerminalMode,
    ) -> String {
        let sanitized = sanitize_for_display(accumulated);

        match terminal_mode {
            TerminalMode::Full => format!(
                "\r{}[{}]{} {}Thinking: {}{}{}",
                colors.dim(),
                prefix,
                colors.reset(),
                colors.dim(),
                colors.cyan(),
                sanitized,
                colors.reset()
            ),
            TerminalMode::Basic | TerminalMode::None => {
                // SUPPRESS per-delta thinking output in non-TTY modes.
                // Thinking content will be flushed ONCE at completion boundaries
                // (message_stop for Claude, item.completed for Codex).
                String::new()
            }
        }
    }
}
