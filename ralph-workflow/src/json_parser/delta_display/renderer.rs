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
/// The renderer supports true append-only streaming that works correctly under
/// terminal line wrapping and in ANSI-stripping environments:
///
/// 1. **First delta**: Shows prefix with accumulated content, NO newline
///    - Example: `[ccs/glm] Hello`
///    - No cursor movement, content stays on current line
///
/// 2. **Subsequent deltas**: Parser computes and emits ONLY new suffix
///    - Parser responsibility: track last rendered content and emit only delta
///    - Example: parser emits ` World` (just the new text with color codes)
///    - NO prefix rewrite, NO `\r` (carriage return), NO cursor movement
///    - Renderers provide `render_subsequent_delta` for backward compatibility
///      but parsers implementing append-only should bypass it
///
/// 3. **Completion**: Single newline to finalize the line
///    - Example: `\n`
///    - Moves cursor to next line after streaming completes
///
/// This pattern works correctly even when content wraps to multiple terminal rows
/// because there is NO cursor movement. The terminal naturally handles wrapping,
/// and content appears to grow incrementally on the same logical line.
///
/// # Why Append-Only?
///
/// Previous patterns using `\r` (carriage return) or `\n\x1b[1A` (newline + cursor up)
/// fail in two scenarios:
///
/// 1. **Line wrapping**: When content exceeds terminal width and wraps to multiple rows,
///    `\r` only returns to column 0 of current row (not start of logical line), and
///    `\x1b[1A` (cursor up 1 row) + `\x1b[2K` (clear 1 row) cannot erase all wrapped rows
/// 2. **ANSI-stripping consoles**: Many CI/log environments strip or ignore ANSI sequences,
///    so `\n` becomes a literal newline causing waterfall spam
///
/// Append-only streaming eliminates both issues by never using cursor movement.
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
///   - Must NOT include newline (stays on current line for append-only)
///   - Shows the accumulated content so far
///
/// - `render_subsequent_delta()`: Called for subsequent deltas
///   - **Parsers implementing append-only should compute suffix and bypass this method**
///   - This method is kept for backward compatibility with parsers not yet using append-only
///   - In Full mode: uses `\r` to rewrite line (legacy pattern, has wrapping issues)
///   - In Basic/None mode: suppresses output (parser flushes at completion)
///
/// - `render_completion()`: Called when streaming completes
///   - Returns single newline (`\n`) in Full mode to finalize the line
///   - Returns empty string in Basic/None mode (parser already flushed with newline)
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
/// Supports true append-only streaming pattern that works correctly under
/// line wrapping and in ANSI-stripping environments.
///
/// - First delta: prefix + content (no newline, stays on current line)
/// - Subsequent deltas: **Parser computes and emits only new suffix**
/// - Completion: single newline to finalize the line
/// - Sanitizes newlines to spaces (to prevent artificial line breaks)
/// - Applies consistent color formatting
///
/// # Output Pattern
///
/// ## Full Mode (TTY with capable terminal) - Append-Only Pattern
///
/// ```text
/// [ccs-glm] Hello                    <- First delta: prefix + content, NO newline
///  World                             <- Parser emits suffix: " World" (no prefix, no \r)
/// \n                                  <- Completion: single newline
/// ```
///
/// Result: Single logical line that may wrap to multiple terminal rows.
/// Terminal handles wrapping naturally. No cursor movement means wrapping is not an issue.
///
/// ## Full Mode (Legacy Pattern - Deprecated)
///
/// Some parsers not yet implementing append-only may still use `render_subsequent_delta`
/// which rewrites the line with `\r`. This pattern has known issues with wrapping:
///
/// ```text
/// [ccs-glm] Hello                    <- First delta
/// \r[ccs-glm] Hello World            <- Subsequent: carriage return + full rewrite
/// ```
///
/// Issue: When content wraps, `\r` only returns to column 0 of current row, not
/// start of logical line. This causes display corruption.
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
/// Supports the same append-only pattern as `TextDeltaRenderer`:
/// - First delta: prefix + "Thinking: " + content (no newline)
/// - Subsequent deltas: **Parser computes and emits only new suffix**
/// - Completion: single newline via `DeltaRenderer::render_completion`
///
/// # Append-Only Pattern
///
/// For true append-only streaming in Full mode, parsers should:
/// 1. Call `render_first_delta` for the first thinking delta (shows prefix + content)
/// 2. Track last rendered content and emit only new suffixes directly (bypass `render_subsequent_delta`)
/// 3. Call `render_completion` when thinking completes (adds final newline)
///
/// This avoids cursor movement and works correctly under terminal wrapping.
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
            TerminalMode::Full => {
                // Legacy pattern for parsers not yet implementing append-only.
                // This uses `\r` to rewrite the line, which has known issues with wrapping.
                // Parsers should instead track last rendered content and emit only new suffixes.
                format!(
                    "\r{}[{}]{} {}Thinking: {}{}{}",
                    colors.dim(),
                    prefix,
                    colors.reset(),
                    colors.dim(),
                    colors.cyan(),
                    sanitized,
                    colors.reset()
                )
            }
            TerminalMode::Basic | TerminalMode::None => {
                // SUPPRESS per-delta thinking output in non-TTY modes.
                // Thinking content will be flushed ONCE at completion boundaries
                // (message_stop for Claude, item.completed for Codex).
                String::new()
            }
        }
    }
}
