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
//! - First chunk shows prefix with accumulated content on a single line
//! - Subsequent chunks update in-place (clear line, rewrite with prefix, carriage return)
//! - Final newline on completion only
//!
//! # In-Place Updates
//!
//! The terminal escape sequence `\x1b[2K\r` is used for in-place updates:
//! - `\x1b[2K` - Clears the entire line (not just to end like `\x1b[0K`)
//! - `\r` - Returns cursor to the beginning of the line
//!
//! This ensures that previous content is completely erased before displaying
//! the updated content, preventing visual artifacts.
//!
//! # Single-Line Pattern
//!
//! The renderer uses a single-line pattern with carriage return for in-place updates.
//! This is the industry standard for streaming CLIs (used by Rich, Ink, Bubble Tea).
//!
//! ```text
//! [ccs-glm] Hello\r          <- First chunk with prefix and carriage return
//! \x1b[2K\r[ccs-glm] Hello World\r  <- Second chunk clears line, rewrites with accumulated
//! [ccs-glm] Hello World\n    <- Final output with newline
//! ```

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
/// 1. **First chunk**: Shows prefix with accumulated content on single line, ends with `\r`
/// 2. **Subsequent chunks**: Clear line, rewrite with prefix and accumulated content, end with `\r`
/// 3. **Completion**: Final newline added when streaming completes
///
/// # Rendering Rules
///
/// - `render_first_delta()`: Called for the first delta of a content block
///   - Must include prefix
///   - Must end with `\r` (carriage return, not newline)
///   - Shows the accumulated content so far
///
/// - `render_subsequent_delta()`: Called for subsequent deltas
///   - Must include prefix (rewrite entire line)
///   - Must use `\x1b[2K\r` to clear entire line and return to start
///   - Shows the full accumulated content (not just the new delta)
///   - Must end with `\r`
///
/// - `render_completion()`: Called when streaming completes
///   - Adds a final newline to move cursor to next line
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
/// // Output: "[ccs-glm] Hello\r" (no newline, just carriage return)
///
/// // Second chunk
/// let output = DeltaRenderer::render_subsequent_delta(
///     "Hello World",
///     "ccs-glm",
///     colors
/// );
/// // Output: "\x1b[2K\r[ccs-glm] Hello World\r" (clear, rewrite full line)
///
/// // Complete
/// let output = DeltaRenderer::render_completion();
/// // Output: "\n"
/// ```
pub trait DeltaRenderer {
    /// Render the first delta of a content block.
    ///
    /// This is called when streaming begins for a new content block.
    /// The output should include the prefix and the accumulated content,
    /// ending with `\r` for in-place updates.
    ///
    /// # Arguments
    /// * `accumulated` - The full accumulated content so far
    /// * `prefix` - The agent prefix (e.g., "ccs-glm")
    /// * `colors` - Terminal colors
    ///
    /// # Returns
    /// A formatted string with prefix and content, ending with `\r`.
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
    /// A formatted string with `\x1b[2K\r` prefix, full line rewrite, ending with `\r`.
    fn render_subsequent_delta(accumulated: &str, prefix: &str, colors: Colors) -> String;

    /// Render the completion of streaming.
    ///
    /// This is called when streaming completes to add a final newline.
    ///
    /// # Returns
    /// A string with just a newline character.
    fn render_completion() -> String {
        "\n".to_string()
    }
}

/// Default implementation of `DeltaRenderer` for text content.
///
/// This implementation follows the single-line rendering pattern:
/// - Prefix and content appear on the same line
/// - Content updates in-place using carriage return
/// - Sanitizes newlines to spaces (to prevent artificial line breaks)
/// - Uses ANSI escape codes for in-place updates with full line clear
/// - Applies consistent color formatting
///
/// # Output Pattern
///
/// ```text
/// [ccs-glm] Hello\r          <- First chunk with prefix and carriage return
/// \x1b[2K\r[ccs-glm] Hello World\r  <- Second chunk clears line, rewrites with accumulated
/// [ccs-glm] Hello World\n    <- Final output with newline
/// ```
///
/// This is the industry standard pattern used by production CLIs like Rich, Ink, and Bubble Tea.
pub struct TextDeltaRenderer;

impl DeltaRenderer for TextDeltaRenderer {
    fn render_first_delta(accumulated: &str, prefix: &str, colors: Colors) -> String {
        // Sanitize embedded newlines to spaces to prevent artificial line breaks
        let sanitized = accumulated.replace('\n', " ");

        // Single-line pattern: prefix and content on same line, ending with \r
        // This allows subsequent deltas to rewrite the entire line
        format!(
            "{}[{}]{} {}{}{}\r",
            colors.dim(),
            prefix,
            colors.reset(),
            colors.white(),
            sanitized,
            colors.reset()
        )
    }

    fn render_subsequent_delta(accumulated: &str, prefix: &str, colors: Colors) -> String {
        // Sanitize embedded newlines to spaces
        let sanitized = accumulated.replace('\n', " ");

        // Clear entire line, carriage return, rewrite with prefix and accumulated content
        // This creates true in-place update on a single line
        format!(
            "{CLEAR_LINE}\r{}[{}]{} {}{}{}\r",
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
        // NEW BEHAVIOR: First delta ends with \r (carriage return), not \n
        assert!(output.ends_with('\r'));
        // Should have exactly 1 carriage return
        let cr_count = output.matches('\r').count();
        assert_eq!(cr_count, 1, "Should have 1 carriage return");
    }

    #[test]
    fn test_text_delta_renderer_subsequent_delta() {
        let output =
            TextDeltaRenderer::render_subsequent_delta("Hello World", "ccs-glm", test_colors());
        // Should contain carriage return and FULL line clear
        assert!(output.contains(CLEAR_LINE));
        assert!(output.contains('\r'));
        assert!(output.contains("Hello World"));
        // Subsequent delta should NOT have trailing newline
        assert!(!output.ends_with('\n'));
        // Should contain prefix (rewrite entire line)
        assert!(output.contains("[ccs-glm]"));
        // Should NOT contain CURSOR_UP (no longer needed for single-line pattern)
        assert!(!output.contains("\x1b[1A"));
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
        assert_eq!(output, "\n");
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

        // First chunk - NEW BEHAVIOR: single line with prefix and content, ending with \r
        let out1 = TextDeltaRenderer::render_first_delta("Hello", "ccs-glm", colors);
        assert!(out1.contains("[ccs-glm]"));
        // First delta ends with \r (carriage return)
        assert!(out1.ends_with('\r'));

        // Second chunk (in-place update without cursor up)
        let out2 = TextDeltaRenderer::render_subsequent_delta("Hello World", "ccs-glm", colors);
        assert!(out2.contains("\x1b[2K"));
        assert!(!out2.contains("\x1b[1A")); // No CURSOR_UP
        assert!(out2.contains('\r'));
        assert!(out2.contains("[ccs-glm]")); // Prefix is rewritten
        assert!(!out2.ends_with('\n'));

        // Completion
        let out3 = TextDeltaRenderer::render_completion();
        assert_eq!(out3, "\n");
    }
}
