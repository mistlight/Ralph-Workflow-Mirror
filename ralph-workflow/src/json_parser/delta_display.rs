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
//! - First chunk shows prefix with accumulated content
//! - Subsequent chunks update in-place (no prefix, carriage return)
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
/// 1. **First chunk**: Shows prefix with accumulated content, no trailing newline
/// 2. **Subsequent chunks**: Updates in-place with `\x1b[2K\r` (clear entire line + carriage return), no prefix
/// 3. **Completion**: Final newline added when streaming completes
///
/// # Rendering Rules
///
/// - `render_first_delta()`: Called for the first delta of a content block
///   - Must include prefix
///   - Must NOT include trailing newline (stays on same line for in-place updates)
///   - Shows the accumulated content so far
///
/// - `render_subsequent_delta()`: Called for subsequent deltas
///   - Must NOT include prefix
///   - Must use `\x1b[2K\r` to clear entire line and return to start
///   - Shows the full accumulated content (not just the new delta)
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
/// // Output: "[ccs-glm] Hello" (no newline)
///
/// // Second chunk
/// let output = DeltaRenderer::render_subsequent_delta(
///     "Hello World",
///     colors
/// );
/// // Output: "\x1b[2K\rHello World" (no newline, in-place update with full line clear)
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
    /// but no trailing newline (to allow in-place updates).
    ///
    /// # Arguments
    /// * `accumulated` - The full accumulated content so far
    /// * `prefix` - The agent prefix (e.g., "ccs-glm")
    /// * `colors` - Terminal colors
    ///
    /// # Returns
    /// A formatted string with prefix and content, no trailing newline.
    fn render_first_delta(accumulated: &str, prefix: &str, colors: Colors) -> String;

    /// Render a subsequent delta (in-place update).
    ///
    /// This is called for all deltas after the first. The output should
    /// clear the entire line and overwrite with the accumulated content.
    ///
    /// # Arguments
    /// * `accumulated` - The full accumulated content so far
    /// * `colors` - Terminal colors
    ///
    /// # Returns
    /// A formatted string with `\x1b[2K\r` prefix and content, no trailing newline.
    fn render_subsequent_delta(accumulated: &str, colors: Colors) -> String;

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
/// This implementation follows the standard rendering rules:
/// - Sanitizes newlines to spaces (to prevent artificial line breaks)
/// - Uses ANSI escape codes for in-place updates with full line clear
/// - Applies consistent color formatting
pub struct TextDeltaRenderer;

impl DeltaRenderer for TextDeltaRenderer {
    fn render_first_delta(accumulated: &str, prefix: &str, colors: Colors) -> String {
        // Sanitize embedded newlines to spaces to prevent artificial line breaks
        let sanitized = accumulated.replace('\n', " ");

        // Write prefix on its own line, then content below (no newline on content)
        // This creates a stable prefix line with in-place content updates below it
        format!(
            "{}[{}]{}{}\n{}{}{}",
            colors.dim(),
            prefix,
            colors.reset(),
            colors.reset(), // Extra reset to ensure clean state
            colors.white(),
            sanitized,
            colors.reset()
        )
    }

    fn render_subsequent_delta(accumulated: &str, colors: Colors) -> String {
        // Sanitize embedded newlines to spaces
        let sanitized = accumulated.replace('\n', " ");

        // Clear entire line, carriage return, show accumulated content
        // The prefix is on the line above (from render_first_delta), so we only update content
        // Using \x1b[2K (clear entire line) instead of \x1b[0K (clear to end)
        format!("{CLEAR_LINE}\r{}{}", colors.white(), sanitized)
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
        // First delta should NOT have trailing newline
        assert!(!output.ends_with('\n'));
    }

    #[test]
    fn test_text_delta_renderer_subsequent_delta() {
        let output = TextDeltaRenderer::render_subsequent_delta("Hello World", test_colors());
        // Should contain carriage return and FULL line clear
        assert!(output.contains(CLEAR_LINE));
        assert!(output.contains('\r'));
        assert!(output.contains("Hello World"));
        // Subsequent delta should NOT have trailing newline
        assert!(!output.ends_with('\n'));
        // Should NOT contain prefix
        assert!(!output.contains("[ccs-glm]"));
    }

    #[test]
    fn test_text_delta_renderer_uses_full_line_clear() {
        let output = TextDeltaRenderer::render_subsequent_delta("Hello World", test_colors());
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

        // First chunk
        let out1 = TextDeltaRenderer::render_first_delta("Hello", "ccs-glm", colors);
        assert!(out1.contains("[ccs-glm]"));
        assert!(!out1.ends_with('\n'));

        // Second chunk (in-place update)
        let out2 = TextDeltaRenderer::render_subsequent_delta("Hello World", colors);
        assert!(out2.contains("\x1b[2K\r"));
        assert!(!out2.contains("[ccs-glm]"));
        assert!(!out2.ends_with('\n'));

        // Completion
        let out3 = TextDeltaRenderer::render_completion();
        assert_eq!(out3, "\n");
    }
}
