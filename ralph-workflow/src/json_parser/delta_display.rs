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
//! - First chunk shows prefix with accumulated content using newline + cursor up
//! - Subsequent chunks update in-place (clear line, rewrite with prefix, newline, cursor up)
//! - Final cursor down + newline on completion only
//!
//! # In-Place Updates
//!
//! The terminal escape sequences used for in-place updates:
//! - `\x1b[2K` - Clears the entire line (not just to end like `\x1b[0K`)
//! - `\r` - Returns cursor to the beginning of the line
//! - `\x1b[1A` - Moves cursor up one line
//! - `\x1b[1B` - Moves cursor down one line
//!
//! This ensures that previous content is completely erased before displaying
//! the updated content, preventing visual artifacts.
//!
//! # Multi-Line In-Place Update Pattern
//!
//! The renderer uses a multi-line pattern with cursor positioning for terminal compatibility:
//!
//! ```text
//! [ccs-glm] Hello\n\x1b[1A       <- First chunk: newline + cursor up
//! \x1b[2K\r[ccs-glm] Hello World\n\x1b[1A  <- Second chunk: rewrite, newline, cursor up
//! \x1b[1B[ccs-glm] Hello World\n  <- Final: cursor down + final output
//! ```
//!
//! This pattern ensures:
//! - Content is immediately visible (newline flushes terminal buffer)
//! - Subsequent updates rewrite the previous line (cursor up + clear)
//! - Works across all terminals supporting basic ANSI escape codes

use crate::logger::Colors;

/// ANSI escape sequence for clearing the entire line.
///
/// This is more complete than `\x1b[0K` which only clears to the end of line.
/// Using `\x1b[2K` ensures the entire line is cleared during in-place updates.
pub const CLEAR_LINE: &str = "\x1b[2K";

/// Renderer for streaming delta content.
///
/// This trait defines the contract for rendering streaming deltas consistently
/// across all parsers. Implementations must ensure:
///
/// 1. **First chunk**: Shows prefix with accumulated content, newline + cursor up
/// 2. **Subsequent chunks**: Clear line, rewrite with prefix and accumulated content, newline, cursor up
/// 3. **Completion**: Cursor down + newline when streaming completes
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
///   - Must end with newline + cursor up
///
/// - `render_completion()`: Called when streaming completes
///   - Adds a cursor down + newline to move cursor to next line
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
/// // Output: "\x1b[2K\r[ccs-glm] Hello World\n\x1b[1A" (clear, rewrite, newline, cursor up)
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
    ///
    /// # Returns
    /// A string with cursor down + newline.
    fn render_completion() -> String {
        "\x1b[1B\n".to_string()
    }
}

/// Default implementation of `DeltaRenderer` for text content.
///
/// This implementation follows the multi-line rendering pattern for terminal compatibility:
/// - Prefix and content appear on their own line with cursor up
/// - Content updates in-place using clear, carriage return, newline, and cursor up
/// - Sanitizes newlines to spaces (to prevent artificial line breaks)
/// - Uses ANSI escape codes for in-place updates with full line clear
/// - Applies consistent color formatting
///
/// # Output Pattern
///
/// ```text
/// [ccs-glm] Hello\n\x1b[1A       <- First chunk: newline + cursor up
/// \x1b[2K\r[ccs-glm] Hello World\n\x1b[1A  <- Second chunk: clear, rewrite, newline, cursor up
/// \x1b[1B[ccs-glm] Hello World\n  <- Final: cursor down + newline
/// ```
///
/// The multi-line pattern ensures compatibility across all terminals that support basic ANSI
/// escape codes, avoiding issues with single-line carriage return rendering on some terminals.
pub struct TextDeltaRenderer;

impl DeltaRenderer for TextDeltaRenderer {
    fn render_first_delta(accumulated: &str, prefix: &str, colors: Colors) -> String {
        // Sanitize embedded newlines to spaces to prevent artificial line breaks
        let sanitized = accumulated.replace('\n', " ");

        // Multi-line pattern: newline + cursor up for terminal compatibility
        // This ensures content is immediately visible while allowing rewrite
        format!(
            "{}[{}]{} {}{}{}\n{}{}",
            colors.dim(),
            prefix,
            colors.reset(),
            colors.white(),
            sanitized,
            colors.reset(),
            CLEAR_LINE,
            "\x1b[1A"
        )
    }

    fn render_subsequent_delta(accumulated: &str, prefix: &str, colors: Colors) -> String {
        // Sanitize embedded newlines to spaces
        let sanitized = accumulated.replace('\n', " ");

        // Clear line, rewrite with prefix and accumulated content, newline, cursor up
        // This creates true in-place update using 2-line pattern
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
        // NEW BEHAVIOR: Should contain newline and cursor up for 2-line pattern
        assert!(output.contains('\n'));
        assert!(output.contains("\x1b[1A"));
        assert!(output.ends_with("\x1b[1A"));
    }

    #[test]
    fn test_text_delta_renderer_subsequent_delta() {
        let output =
            TextDeltaRenderer::render_subsequent_delta("Hello World", "ccs-glm", test_colors());
        assert!(output.contains(CLEAR_LINE));
        assert!(output.contains('\r'));
        assert!(output.contains("Hello World"));
        // Should contain cursor up for 2-line pattern
        assert!(output.contains("\x1b[1A"));
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
        // NEW BEHAVIOR: Should contain cursor down
        assert!(output.contains("\x1b[1B"));
        assert!(output.contains('\n'));
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

        // First chunk - NEW BEHAVIOR: newline + cursor up
        let out1 = TextDeltaRenderer::render_first_delta("Hello", "ccs-glm", colors);
        assert!(out1.contains("[ccs-glm]"));
        assert!(out1.contains('\n'));
        assert!(out1.contains("\x1b[1A"));

        // Second chunk (in-place update with cursor up)
        let out2 = TextDeltaRenderer::render_subsequent_delta("Hello World", "ccs-glm", colors);
        assert!(out2.contains("\x1b[2K"));
        assert!(out2.contains("\x1b[1A")); // Has CURSOR_UP for 2-line pattern
        assert!(out2.contains('\r'));
        assert!(out2.contains("[ccs-glm]")); // Prefix is rewritten

        // Completion
        let out3 = TextDeltaRenderer::render_completion();
        assert!(out3.contains("\x1b[1B"));
    }
}
