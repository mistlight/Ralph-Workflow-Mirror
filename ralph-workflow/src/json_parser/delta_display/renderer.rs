// Delta renderer trait and implementations.
//
// Contains the DeltaRenderer trait and TextDeltaRenderer implementation.

/// Renderer for streaming delta content.
///
/// This trait defines the contract for rendering streaming deltas consistently
/// across all parsers. Implementations must ensure:
///
/// 1. **First chunk**: Shows prefix with accumulated content, ending with newline + cursor up
/// 2. **Subsequent chunks**: Clear line, rewrite with prefix and accumulated content, newline + cursor up
/// 3. **Completion**: Move cursor down + newline when streaming completes
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
    /// A formatted string with prefix and content.
    fn render_subsequent_delta(
        accumulated: &str,
        prefix: &str,
        colors: Colors,
        terminal_mode: TerminalMode,
    ) -> String;

    /// Render the completion of streaming.
    ///
    /// This is called when streaming completes to move cursor down and add newline.
    /// This method ONLY handles cursor state cleanup - it does NOT render content.
    ///
    /// The streamed content is already visible on the terminal from previous deltas.
    /// This method simply positions the cursor correctly for subsequent output.
    ///
    /// # Arguments
    /// * `terminal_mode` - The detected terminal capability mode
    ///
    /// # Returns
    /// A string with appropriate cursor sequence for the terminal mode.
    fn render_completion(terminal_mode: TerminalMode) -> String {
        match terminal_mode {
            TerminalMode::Full => "\x1b[1B\n".to_string(),
            TerminalMode::Basic | TerminalMode::None => "\n".to_string(),
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
/// ## Basic/None Mode (colors only or plain text)
///
/// ```text
/// [ccs-glm] Hello\n                      <- First chunk: simple line output
/// [ccs-glm] Hello World\n                <- Second chunk: full content (no in-place update)
///                                       <- Final: just a newline
/// ```
///
/// The multi-line pattern is the industry standard used by Rich, Ink, Bubble Tea
/// and other production CLI libraries for clean streaming output.
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
                // Multi-line pattern: end with newline + cursor up for in-place updates
                // This forces terminal output flush and positions cursor for rewrite
                format!(
                    "{}[{}]{} {}{}{}\n\x1b[1A",
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
                // Clear line, rewrite with prefix and accumulated content, end with newline + cursor up
                // This creates in-place update using multi-line pattern
                format!(
                    "{CLEAR_LINE}\r{}[{}]{} {}{}{}\n\x1b[1A",
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
                "{}[{}]{} {}Thinking: {}{}{}\n\x1b[1A",
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
                "{CLEAR_LINE}\r{}[{}]{} {}Thinking: {}{}{}\n\x1b[1A",
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
