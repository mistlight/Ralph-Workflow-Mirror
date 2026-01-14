//! Unified delta display system for streaming content.
//!
//! This module provides centralized logic for displaying partial vs. complete
//! content consistently across all parsers. It handles visual distinction,
//! real-time streaming display, and automatic transition from delta to complete.

use crate::colors::Colors;

/// Display mode for delta content
///
/// Controls how partial/delta content is presented to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DeltaDisplayMode {
    /// Minimal display - show only deltas, no accumulated content
    Minimal,
    /// Normal display - show deltas in real-time, complete content when available
    Normal,
    /// Show latest accumulated - show "Latest: [accumulated so far]" for streaming
    ShowLatestAccumulated,
    /// Verbose display - always show accumulated content
    Verbose,
}

/// State tracking for content display
///
/// Tracks whether we should show delta or complete content for a specific key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum ContentState {
    /// Content is being accumulated (partial/delta state)
    Accumulating,
    /// Content is complete
    Complete,
}

/// Content display info
///
/// Contains information about how to display specific content.
#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct ContentDisplayInfo {
    /// The content to display
    pub content: String,
    /// Whether this is partial (delta) or complete content
    pub state: ContentState,
    /// The content type for styling
    pub content_type: DisplayContentType,
}

/// Content type for display styling
///
/// Different types of content may be styled differently.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[allow(dead_code)]
pub enum DisplayContentType {
    /// Regular text content
    Text,
    /// Thinking/reasoning content
    Thinking,
    /// Tool input content
    ToolInput,
    /// Error content
    Error,
}

/// Delta display formatter
///
/// Formats delta content for user display with consistent styling across all parsers.
pub struct DeltaDisplayFormatter {
    /// Display mode for delta content
    #[allow(dead_code)]
    display_mode: DeltaDisplayMode,
    /// Whether to mark partial content visually
    mark_partial: bool,
    /// Whether to show delta in real-time
    #[allow(dead_code)]
    show_realtime: bool,
}

impl DeltaDisplayFormatter {
    /// Create a new formatter with default settings
    #[allow(dead_code)]
    pub fn new() -> Self {
        Self {
            display_mode: DeltaDisplayMode::Normal,
            mark_partial: true,
            show_realtime: true,
        }
    }

    /// Create a new formatter with specific display mode
    #[allow(dead_code)]
    pub fn with_mode(display_mode: DeltaDisplayMode) -> Self {
        Self {
            display_mode,
            mark_partial: true,
            show_realtime: true,
        }
    }

    /// Create a new formatter with custom settings
    #[allow(dead_code)]
    pub fn with_settings(display_mode: DeltaDisplayMode, mark_partial: bool, show_realtime: bool) -> Self {
        Self {
            display_mode,
            mark_partial,
            show_realtime,
        }
    }

    /// Format content for display
    ///
    /// # Arguments
    /// * `info` - The content display info
    /// * `prefix` - The parser name prefix (e.g., "Claude", "Codex")
    /// * `colors` - Colors for formatting
    ///
    /// # Returns
    /// Formatted string for display, or None if content should not be shown
    #[allow(dead_code)]
    pub fn format_content(
        &self,
        info: &ContentDisplayInfo,
        prefix: &str,
        colors: &Colors,
    ) -> Option<String> {
        match self.display_mode {
            DeltaDisplayMode::Minimal => {
                // Only show deltas, skip complete content in minimal mode
                if info.state == ContentState::Accumulating && self.show_realtime {
                    Some(self.format_delta_content(&info.content, info.content_type, prefix, colors))
                } else {
                    None
                }
            }
            DeltaDisplayMode::Normal => {
                // Show deltas in real-time, complete content when available
                if info.state == ContentState::Accumulating {
                    if self.show_realtime {
                        Some(self.format_delta_content(&info.content, info.content_type, prefix, colors))
                    } else {
                        // Skip deltas when show_realtime is false
                        None
                    }
                } else {
                    // Always show complete content
                    Some(self.format_complete_content(&info.content, info.content_type, prefix, colors))
                }
            }
            DeltaDisplayMode::ShowLatestAccumulated => {
                // Show "Latest: [accumulated so far]" for streaming content
                if info.state == ContentState::Accumulating {
                    if self.show_realtime {
                        Some(self.format_latest_accumulated(&info.content, info.content_type, prefix, colors))
                    } else {
                        None
                    }
                } else {
                    // Always show complete content
                    Some(self.format_complete_content(&info.content, info.content_type, prefix, colors))
                }
            }
            DeltaDisplayMode::Verbose => {
                // Always show accumulated content
                Some(self.format_complete_content(&info.content, info.content_type, prefix, colors))
            }
        }
    }

    /// Format delta/partial content for display
    ///
    /// Deltas are shown with visual indicators that this is partial content.
    #[allow(dead_code)]
    fn format_delta_content(
        &self,
        content: &str,
        content_type: DisplayContentType,
        prefix: &str,
        colors: &Colors,
    ) -> String {
        let (text_color, prefix_indicator) = match content_type {
            DisplayContentType::Text => (colors.white(), ""),
            DisplayContentType::Thinking => (colors.dim(), "Thinking: "),
            DisplayContentType::ToolInput => (colors.dim(), "Tool input: "),
            DisplayContentType::Error => (colors.red(), "Error: "),
        };

        let partial_marker = if self.mark_partial {
            // Use dimmed text to indicate partial nature
            colors.dim().to_string()
        } else {
            String::new()
        };

        format!(
            "{}[{}]{} {}{}{}{}{}\n",
            colors.dim(),
            prefix,
            colors.reset(),
            partial_marker,
            prefix_indicator,
            text_color,
            content,
            colors.reset()
        )
    }

    /// Format latest accumulated content for display
    ///
    /// Shows "Latest: [accumulated so far]" to provide context during streaming.
    #[allow(dead_code)]
    fn format_latest_accumulated(
        &self,
        content: &str,
        content_type: DisplayContentType,
        prefix: &str,
        colors: &Colors,
    ) -> String {
        let (text_color, prefix_indicator) = match content_type {
            DisplayContentType::Text => (colors.white(), ""),
            DisplayContentType::Thinking => (colors.cyan(), "Thinking: "),
            DisplayContentType::ToolInput => (colors.dim(), "Tool input: "),
            DisplayContentType::Error => (colors.red(), "Error: "),
        };

        format!(
            "{}[{}]{} {}Latest: {}{}{}{}\n",
            colors.dim(),
            prefix,
            colors.reset(),
            colors.dim(),
            prefix_indicator,
            text_color,
            content,
            colors.reset()
        )
    }

    /// Format complete content for display
    ///
    /// Complete content is shown without partial markers.
    #[allow(dead_code)]
    fn format_complete_content(
        &self,
        content: &str,
        content_type: DisplayContentType,
        prefix: &str,
        colors: &Colors,
    ) -> String {
        let (text_color, prefix_indicator) = match content_type {
            DisplayContentType::Text => (colors.white(), ""),
            DisplayContentType::Thinking => (colors.cyan(), "Thought: "),
            DisplayContentType::ToolInput => (colors.dim(), "Tool: "),
            DisplayContentType::Error => (colors.red(), "Error: "),
        };

        format!(
            "{}[{}]{} {}{}{}{}\n",
            colors.dim(),
            prefix,
            colors.reset(),
            prefix_indicator,
            text_color,
            content,
            colors.reset()
        )
    }

    /// Format thinking content specifically
    ///
    /// Thinking content has special formatting to distinguish it from regular text.
    pub fn format_thinking(&self, content: &str, prefix: &str, colors: &Colors) -> String {
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
    pub fn format_tool_input(&self, content: &str, prefix: &str, colors: &Colors) -> String {
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
    fn test_delta_display_formatter_new() {
        let formatter = DeltaDisplayFormatter::new();
        assert_eq!(formatter.display_mode, DeltaDisplayMode::Normal);
        assert!(formatter.mark_partial);
        assert!(formatter.show_realtime);
    }

    #[test]
    fn test_delta_display_formatter_with_mode() {
        let formatter = DeltaDisplayFormatter::with_mode(DeltaDisplayMode::Verbose);
        assert_eq!(formatter.display_mode, DeltaDisplayMode::Verbose);
    }

    #[test]
    fn test_delta_display_formatter_with_settings() {
        let formatter = DeltaDisplayFormatter::with_settings(DeltaDisplayMode::Minimal, false, false);
        assert_eq!(formatter.display_mode, DeltaDisplayMode::Minimal);
        assert!(!formatter.mark_partial);
        assert!(!formatter.show_realtime);
    }

    #[test]
    fn test_format_accumulating_text_normal_mode() {
        let formatter = DeltaDisplayFormatter::new();
        let info = ContentDisplayInfo {
            content: "Hello, world!".to_string(),
            state: ContentState::Accumulating,
            content_type: DisplayContentType::Text,
        };

        let output = formatter.format_content(&info, "Claude", &test_colors());
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Hello, world!"));
        assert!(out.contains("[Claude]"));
    }

    #[test]
    fn test_format_complete_text_normal_mode() {
        let formatter = DeltaDisplayFormatter::new();
        let info = ContentDisplayInfo {
            content: "Complete message".to_string(),
            state: ContentState::Complete,
            content_type: DisplayContentType::Text,
        };

        let output = formatter.format_content(&info, "Claude", &test_colors());
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Complete message"));
    }

    #[test]
    fn test_format_thinking_content() {
        let formatter = DeltaDisplayFormatter::new();
        let output = formatter.format_thinking("Thinking about this", "Claude", &test_colors());
        assert!(output.contains("Thinking"));
        assert!(output.contains("Thinking about this"));
    }

    #[test]
    fn test_format_tool_input() {
        let formatter = DeltaDisplayFormatter::new();
        let output = formatter.format_tool_input("command=ls -la", "Claude", &test_colors());
        assert!(output.contains("command=ls -la"));
        assert!(output.contains("└─"));
    }

    #[test]
    fn test_minimal_mode_shows_only_deltas() {
        let formatter = DeltaDisplayFormatter::with_mode(DeltaDisplayMode::Minimal);

        // Accumulating content should be shown
        let info_delta = ContentDisplayInfo {
            content: "Partial".to_string(),
            state: ContentState::Accumulating,
            content_type: DisplayContentType::Text,
        };
        assert!(formatter.format_content(&info_delta, "Test", &test_colors()).is_some());

        // Complete content should not be shown in minimal mode
        let info_complete = ContentDisplayInfo {
            content: "Complete".to_string(),
            state: ContentState::Complete,
            content_type: DisplayContentType::Text,
        };
        assert!(formatter.format_content(&info_complete, "Test", &test_colors()).is_none());
    }

    #[test]
    fn test_verbose_mode_shows_all_content() {
        let formatter = DeltaDisplayFormatter::with_mode(DeltaDisplayMode::Verbose);

        // Both accumulating and complete content should be shown
        let info_delta = ContentDisplayInfo {
            content: "Partial".to_string(),
            state: ContentState::Accumulating,
            content_type: DisplayContentType::Text,
        };
        assert!(formatter.format_content(&info_delta, "Test", &test_colors()).is_some());

        let info_complete = ContentDisplayInfo {
            content: "Complete".to_string(),
            state: ContentState::Complete,
            content_type: DisplayContentType::Text,
        };
        assert!(formatter.format_content(&info_complete, "Test", &test_colors()).is_some());
    }

    #[test]
    fn test_no_realtime_skips_deltas() {
        let formatter = DeltaDisplayFormatter::with_settings(DeltaDisplayMode::Normal, true, false);

        // Accumulating content should not be shown when show_realtime is false
        let info_delta = ContentDisplayInfo {
            content: "Partial".to_string(),
            state: ContentState::Accumulating,
            content_type: DisplayContentType::Text,
        };
        assert!(formatter.format_content(&info_delta, "Test", &test_colors()).is_none());

        // Complete content should still be shown
        let info_complete = ContentDisplayInfo {
            content: "Complete".to_string(),
            state: ContentState::Complete,
            content_type: DisplayContentType::Text,
        };
        assert!(formatter.format_content(&info_complete, "Test", &test_colors()).is_some());
    }

    #[test]
    fn test_show_latest_accumulated_mode() {
        let formatter = DeltaDisplayFormatter::with_mode(DeltaDisplayMode::ShowLatestAccumulated);

        // Accumulating content should be shown with "Latest:" prefix
        let info_delta = ContentDisplayInfo {
            content: "Partial content".to_string(),
            state: ContentState::Accumulating,
            content_type: DisplayContentType::Text,
        };
        let output = formatter.format_content(&info_delta, "Claude", &test_colors());
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Latest:"));
        assert!(out.contains("Partial content"));

        // Complete content should be shown without "Latest:" prefix
        let info_complete = ContentDisplayInfo {
            content: "Complete message".to_string(),
            state: ContentState::Complete,
            content_type: DisplayContentType::Text,
        };
        let output_complete = formatter.format_content(&info_complete, "Claude", &test_colors());
        assert!(output_complete.is_some());
        let out_complete = output_complete.unwrap();
        assert!(out_complete.contains("Complete message"));
        assert!(!out_complete.contains("Latest:"));
    }

    #[test]
    fn test_show_latest_accumulated_with_thinking() {
        let formatter = DeltaDisplayFormatter::with_mode(DeltaDisplayMode::ShowLatestAccumulated);

        let info = ContentDisplayInfo {
            content: "Thinking about the problem".to_string(),
            state: ContentState::Accumulating,
            content_type: DisplayContentType::Thinking,
        };
        let output = formatter.format_content(&info, "Claude", &test_colors());
        assert!(output.is_some());
        let out = output.unwrap();
        assert!(out.contains("Latest:"));
        assert!(out.contains("Thinking:"));
        assert!(out.contains("Thinking about the problem"));
    }
}
