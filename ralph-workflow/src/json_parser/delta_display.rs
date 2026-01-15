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
//! # Prefix Display Strategy
//!
//! Currently, the prefix (e.g., `[ccs-glm]`) is displayed on every delta update.
//! This provides clear visual feedback about which agent is currently streaming.
//!
//! **Design decision**: Keep prefix on every delta for now. The visual clarity
//! outweighs the minor redundancy. Future optimization could reduce prefix
//! display frequency (e.g., only on first delta per block) if terminal bandwidth
//! becomes a concern.

use crate::logger::Colors;

/// ANSI escape sequence for clearing the entire line.
///
/// This is more complete than `\x1b[0K` which only clears to the end of line.
/// Using `\x1b[2K` ensures the entire line is cleared during in-place updates.
pub const CLEAR_LINE: &str = "\x1b[2K";

/// Sanitize content for single-line display.
///
/// This function prepares streamed content for in-place terminal display by:
/// - Replacing newlines with spaces (to prevent artificial line breaks)
/// - Collapsing multiple consecutive whitespace characters into single spaces
/// - Trimming leading and trailing whitespace
///
/// # Arguments
/// * `content` - The raw content to sanitize
///
/// # Returns
/// A sanitized string suitable for single-line display.
fn sanitize_for_display(content: &str) -> String {
    // Replace all whitespace (including \n, \r, \t) with spaces, then collapse multiple spaces
    let mut result = String::with_capacity(content.len());
    let mut prev_was_whitespace = false;

    for ch in content.chars() {
        if ch.is_whitespace() {
            if !prev_was_whitespace {
                result.push(' ');
                prev_was_whitespace = true;
            }
            // Skip consecutive whitespace characters
        } else {
            result.push(ch);
            prev_was_whitespace = false;
        }
    }

    // Trim leading and trailing whitespace
    result.trim().to_string()
}

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
///   - Must end with newline + cursor up (`\n\x1b[1A`)
///   - Shows the accumulated content so far
///
/// - `render_subsequent_delta()`: Called for subsequent deltas
///   - Must include prefix (rewrite entire line)
///   - Must use `\x1b[2K\r` to clear entire line and return to start
///   - Shows the full accumulated content (not just the new delta)
///   - Must end with newline + cursor up (`\n\x1b[1A`)
///
/// - `render_completion()`: Called when streaming completes
///   - Returns cursor down + newline (`\x1b[1B\n`)
///
/// # Example
///
/// ```ignore
/// use crate::json_parser::delta_display::DeltaRenderer;
/// use crate::colors::Colors;
///
/// let colors = Colors { enabled: true };
///
/// // First chunk
/// let output = DeltaRenderer::render_first_delta(
///     "Hello",
///     "ccs-glm",
///     colors
/// );
/// // Output: "[ccs-glm] Hello\n\x1b[1A" (newline + cursor up)
///
/// // Second chunk
/// let output = DeltaRenderer::render_subsequent_delta(
///     "Hello World",
///     "ccs-glm",
///     colors
/// );
/// // Output: "\x1b[2K\r[ccs-glm] Hello World\n\x1b[1A" (clear, rewrite, newline + cursor up)
///
/// // Complete
/// let output = DeltaRenderer::render_completion();
/// // Output: "\x1b[1B\n" (cursor down + newline)
/// ```
pub trait DeltaRenderer {
    /// Render the first delta of a content block.
    ///
    /// This is called when streaming begins for a new content block.
    /// The output should include the prefix and the accumulated content,
    /// ending with newline + cursor up (`\n\x1b[1A`) for in-place updates.
    ///
    /// # Arguments
    /// * `accumulated` - The full accumulated content so far
    /// * `prefix` - The agent prefix (e.g., "ccs-glm")
    /// * `colors` - Terminal colors
    ///
    /// # Returns
    /// A formatted string with prefix and content, ending with `\n\x1b[1A`.
    fn render_first_delta(accumulated: &str, prefix: &str, colors: Colors) -> String;

    /// Render a subsequent delta (in-place update).
    ///
    /// This is called for all deltas after the first. The output should
    /// clear the entire line and rewrite with the prefix and accumulated content.
    ///
    /// # Arguments
    /// * `accumulated` - The full accumulated content so far
    /// * `prefix` - The agent prefix (e.g., "ccs-glm")
    /// * `colors` - Terminal colors
    ///
    /// # Returns
    /// A formatted string with `\x1b[2K\r` prefix, full line rewrite, ending with `\n\x1b[1A`.
    fn render_subsequent_delta(accumulated: &str, prefix: &str, colors: Colors) -> String;

    /// Render the completion of streaming.
    ///
    /// This is called when streaming completes to move cursor down and add newline.
    /// This method ONLY handles cursor state cleanup - it does NOT render content.
    ///
    /// The streamed content is already visible on the terminal from previous deltas.
    /// This method simply positions the cursor correctly for subsequent output.
    ///
    /// # Future Enhancement
    ///
    /// A `render_final_line()` method could be added to render clean output without
    /// cursor control sequences, useful for:
    /// - Log files or non-terminal destinations
    /// - Re-rendering a clean final line after streaming
    /// - Creating output suitable for capture or storage
    ///
    /// # Returns
    /// A string with cursor down + newline (`\x1b[1B\n`).
    fn render_completion() -> String {
        "\x1b[1B\n".to_string()
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
/// ```text
/// [ccs-glm] Hello\n\x1b[1A             <- First chunk: prefix + content + newline + cursor up
/// \x1b[2K\r[ccs-glm] Hello World\n\x1b[1A  <- Second chunk: clear, rewrite, newline, cursor up
/// [ccs-glm] Hello World\n\x1b[1B\n       <- Final: move cursor down + newline
/// ```
///
/// The multi-line pattern is the industry standard used by Rich, Ink, Bubble Tea
/// and other production CLI libraries for clean streaming output.
pub struct TextDeltaRenderer;

impl DeltaRenderer for TextDeltaRenderer {
    fn render_first_delta(accumulated: &str, prefix: &str, colors: Colors) -> String {
        // Sanitize content: replace newlines with spaces and collapse multiple whitespace
        let sanitized = sanitize_for_display(accumulated);

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

    fn render_subsequent_delta(accumulated: &str, prefix: &str, colors: Colors) -> String {
        // Sanitize content: replace newlines with spaces and collapse multiple whitespace
        let sanitized = sanitize_for_display(accumulated);

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
}

/// Delta display formatter
///
/// Formats delta content for user display with consistent styling across all parsers.
pub struct DeltaDisplayFormatter {
    /// Whether to mark partial content visually
    mark_partial: bool,
}

impl DeltaDisplayFormatter {
    /// Create a new formatter with default settings
    pub const fn new() -> Self {
        Self { mark_partial: true }
    }

    /// Format thinking content specifically
    ///
    /// Thinking content has special formatting to distinguish it from regular text.
    pub fn format_thinking(&self, content: &str, prefix: &str, colors: Colors) -> String {
        if self.mark_partial {
            format!(
                "{}[{}]{} {}Thinking: {}{}{}\n",
                colors.dim(),
                prefix,
                colors.reset(),
                colors.dim(),
                colors.cyan(),
                content,
                colors.reset()
            )
        } else {
            format!(
                "{}[{}]{} {}Thinking: {}{}{}\n",
                colors.dim(),
                prefix,
                colors.reset(),
                colors.cyan(),
                colors.reset(),
                content,
                colors.reset()
            )
        }
    }

    /// Format tool input specifically
    ///
    /// Tool input is shown with appropriate styling.
    ///
    /// # Current Behavior
    ///
    /// Every call renders the full `[prefix]   └─ content` pattern.
    /// This provides clarity about which agent's tool is being invoked.
    ///
    /// # Future Enhancement
    ///
    /// For streaming tool inputs with multiple deltas, consider suppressing
    /// the `[prefix]` on continuation lines to reduce visual noise:
    /// - First tool input line: `[prefix] Tool: name`
    /// - Continuation: `           └─ more input` (aligned, no prefix)
    ///
    /// This would require tracking whether the prefix has been displayed
    /// for the current tool block, likely via the streaming session state.
    pub fn format_tool_input(&self, content: &str, prefix: &str, colors: Colors) -> String {
        if self.mark_partial {
            format!(
                "{}[{}]{} {}  └─ {}{}{}\n",
                colors.dim(),
                prefix,
                colors.reset(),
                colors.dim(),
                colors.reset(),
                content,
                colors.reset()
            )
        } else {
            format!(
                "{}[{}]{} {}  └─ {}{}\n",
                colors.dim(),
                prefix,
                colors.reset(),
                colors.reset(),
                content,
                colors.reset()
            )
        }
    }
}

impl Default for DeltaDisplayFormatter {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_colors() -> Colors {
        Colors { enabled: false }
    }

    #[test]
    fn test_format_thinking_content() {
        let formatter = DeltaDisplayFormatter::new();
        let output = formatter.format_thinking("Thinking about this", "Claude", test_colors());
        assert!(output.contains("Thinking"));
        assert!(output.contains("Thinking about this"));
    }

    #[test]
    fn test_format_tool_input() {
        let formatter = DeltaDisplayFormatter::new();
        let output = formatter.format_tool_input("command=ls -la", "Claude", test_colors());
        assert!(output.contains("command=ls -la"));
        assert!(output.contains("└─"));
    }

    // Tests for DeltaRenderer trait

    #[test]
    fn test_text_delta_renderer_first_delta() {
        let output = TextDeltaRenderer::render_first_delta("Hello", "ccs-glm", test_colors());
        assert!(output.contains("[ccs-glm]"));
        assert!(output.contains("Hello"));
        // Multi-line pattern: ends with newline + cursor up
        assert!(output.ends_with("\x1b[1A"));
        assert!(output.contains('\n'));
        assert!(output.contains("\x1b[1A"));
    }

    #[test]
    fn test_text_delta_renderer_subsequent_delta() {
        let output =
            TextDeltaRenderer::render_subsequent_delta("Hello World", "ccs-glm", test_colors());
        assert!(output.contains(CLEAR_LINE));
        assert!(output.contains('\r'));
        assert!(output.contains("Hello World"));
        // Multi-line pattern: ends with newline + cursor up
        assert!(output.contains("\x1b[1A"));
        assert!(output.ends_with("\x1b[1A"));
        assert!(output.contains("[ccs-glm]"));
    }

    #[test]
    fn test_text_delta_renderer_uses_full_line_clear() {
        let output =
            TextDeltaRenderer::render_subsequent_delta("Hello World", "ccs-glm", test_colors());
        // Should use \x1b[2K (full line clear), not \x1b[0K (clear to end)
        assert!(output.contains("\x1b[2K"));
        // Should NOT contain \x1b[0K
        assert!(!output.contains("\x1b[0K"));
    }

    #[test]
    fn test_text_delta_renderer_completion() {
        let output = TextDeltaRenderer::render_completion();
        // Multi-line pattern: cursor down + newline
        assert!(output.contains("\x1b[1B"));
        assert!(output.contains('\n'));
        assert_eq!(output, "\x1b[1B\n");
    }

    #[test]
    fn test_text_delta_renderer_sanitizes_newlines() {
        let output =
            TextDeltaRenderer::render_first_delta("Hello\nWorld", "ccs-glm", test_colors());
        // Newlines should be replaced with spaces
        assert!(!output.contains("Hello\nWorld"));
        assert!(output.contains("Hello World"));
    }

    #[test]
    fn test_text_delta_renderer_in_place_update_sequence() {
        let colors = test_colors();

        // First chunk - multi-line pattern: ends with newline + cursor up
        let out1 = TextDeltaRenderer::render_first_delta("Hello", "ccs-glm", colors);
        assert!(out1.contains("[ccs-glm]"));
        assert!(out1.ends_with("\x1b[1A"));
        assert!(out1.contains('\n'));
        assert!(out1.contains("\x1b[1A"));

        // Second chunk (in-place update with newline + cursor up)
        let out2 = TextDeltaRenderer::render_subsequent_delta("Hello World", "ccs-glm", colors);
        assert!(out2.contains("\x1b[2K"));
        assert!(out2.contains('\r'));
        assert!(out2.contains("\x1b[1A")); // Cursor up in multi-line pattern
        assert!(out2.contains("[ccs-glm]")); // Prefix is rewritten

        // Completion
        let out3 = TextDeltaRenderer::render_completion();
        assert!(out3.contains("\x1b[1B"));
        assert_eq!(out3, "\x1b[1B\n");
    }

    #[test]
    fn test_full_streaming_sequence_no_extra_blank_lines() {
        let colors = test_colors();

        // Simulate a full streaming sequence and verify no extra blank lines
        let first = TextDeltaRenderer::render_first_delta("Hello", "agent", colors);
        let second = TextDeltaRenderer::render_subsequent_delta("Hello World", "agent", colors);
        let complete = TextDeltaRenderer::render_completion();

        // First delta: ends with exactly one \n followed by cursor up
        assert!(first.ends_with("\n\x1b[1A"));
        assert_eq!(first.matches('\n').count(), 1);

        // Subsequent delta: starts with clear+return, ends with \n + cursor up
        assert!(second.starts_with("\x1b[2K\r"));
        assert!(second.ends_with("\n\x1b[1A"));
        assert_eq!(second.matches('\n').count(), 1);

        // Completion: exactly cursor down + one newline
        assert_eq!(complete, "\x1b[1B\n");
        assert_eq!(complete.matches('\n').count(), 1);
    }

    #[test]
    fn test_prefix_displayed_on_all_deltas() {
        let colors = test_colors();
        let prefix = "my-agent";

        // First delta shows prefix
        let first = TextDeltaRenderer::render_first_delta("A", prefix, colors);
        assert!(first.contains(&format!("[{prefix}]")));

        // Subsequent delta also shows prefix (design decision: prefix on every delta)
        let subsequent = TextDeltaRenderer::render_subsequent_delta("AB", prefix, colors);
        assert!(subsequent.contains(&format!("[{prefix}]")));
    }

    // Tests for sanitize_for_display helper function

    #[test]
    fn test_sanitize_collapses_multiple_newlines() {
        let result = sanitize_for_display("Hello\n\nWorld");
        // Multiple newlines should become a single space
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_sanitize_collapses_multiple_spaces() {
        let result = sanitize_for_display("Hello   World");
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_sanitize_mixed_whitespace() {
        let result = sanitize_for_display("Hello\n\n  \t\t  World");
        // All whitespace (newlines, spaces, tabs) collapsed to single space
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_sanitize_trims_leading_trailing_whitespace() {
        let result = sanitize_for_display("  Hello World  ");
        assert_eq!(result, "Hello World");
    }

    #[test]
    fn test_sanitize_only_whitespace() {
        let result = sanitize_for_display("   \n\n   ");
        // Only whitespace content becomes empty string
        assert_eq!(result, "");
    }

    #[test]
    fn test_sanitize_preserves_single_spaces() {
        let result = sanitize_for_display("Hello World Test");
        assert_eq!(result, "Hello World Test");
    }

    #[test]
    fn test_delta_renderer_multiple_newlines_render_cleanly() {
        let colors = test_colors();
        let output = TextDeltaRenderer::render_first_delta("Hello\n\n\nWorld", "agent", colors);
        // Multiple newlines should render as single space
        assert!(output.contains("Hello World"));
        // Should NOT have multiple spaces
        assert!(!output.contains("  "));
    }

    #[test]
    fn test_delta_renderer_trailing_whitespace_trimmed() {
        let colors = test_colors();
        let output = TextDeltaRenderer::render_first_delta("Hello World   ", "agent", colors);
        // Trailing spaces should be trimmed
        assert!(output.contains("Hello World"));
        // Content should not end with space before escape sequences
        // (it ends with reset color then \n\x1b[1A)
    }
}
