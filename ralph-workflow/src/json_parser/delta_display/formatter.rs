// Delta display formatter.
//
// Contains the DeltaDisplayFormatter for consistent styling across parsers.

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
    pub fn format_tool_input(
        &self,
        content: &str,
        prefix: &str,
        colors: Colors,
        terminal_mode: crate::json_parser::terminal::TerminalMode,
    ) -> String {
        use crate::json_parser::terminal::TerminalMode;

        match terminal_mode {
            TerminalMode::Full => {
                // In Full mode, render tool input deltas as they arrive for real-time feedback
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
            TerminalMode::Basic | TerminalMode::None => {
                // SUPPRESS per-delta tool input in non-TTY modes.
                // Tool input will be rendered ONCE at tool completion or message_stop.
                String::new()
            }
        }
    }
}

impl Default for DeltaDisplayFormatter {
    fn default() -> Self {
        Self::new()
    }
}
