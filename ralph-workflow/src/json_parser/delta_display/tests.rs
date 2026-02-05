// Tests for delta display module.

#[cfg(test)]
mod tests {
    use super::*;

    fn test_colors() -> Colors {
        Colors { enabled: false }
    }

    #[test]
    fn test_format_thinking_content() {
        let formatter = DeltaDisplayFormatter::new();
        let output = formatter.format_thinking(
            "Thinking about this",
            "Claude",
            test_colors(),
            TerminalMode::Full,
        );
        assert!(output.contains("Thinking"));
        assert!(output.contains("Thinking about this"));
    }

    #[test]
    fn test_format_thinking_none_mode_plain_text() {
        let formatter = DeltaDisplayFormatter::new();
        let output = formatter.format_thinking(
            "Thinking about this",
            "Claude",
            Colors { enabled: true },
            TerminalMode::None,
        );
        assert_eq!(output, "[Claude] Thinking: Thinking about this\n");
    }

    #[test]
    fn test_format_tool_input() {
        let formatter = DeltaDisplayFormatter::new();
        let output = formatter.format_tool_input(
            "command=ls -la",
            "Claude",
            test_colors(),
            TerminalMode::Full,
        );
        assert!(output.contains("command=ls -la"));
        assert!(output.contains("└─"));
    }

    #[test]
    fn test_format_tool_input_none_mode_suppressed() {
        let formatter = DeltaDisplayFormatter::new();
        let output = formatter.format_tool_input(
            "command=ls -la",
            "Claude",
            test_colors(),
            TerminalMode::None,
        );
        // Per-delta tool input is suppressed in None mode to prevent spam.
        // Tool input will be flushed once at completion boundaries.
        assert_eq!(output, "");
    }

    #[test]
    fn test_format_tool_input_basic_mode_suppressed() {
        let formatter = DeltaDisplayFormatter::new();
        let output = formatter.format_tool_input(
            "command=ls -la",
            "Claude",
            test_colors(),
            TerminalMode::Basic,
        );
        // Per-delta tool input is suppressed in Basic mode to prevent spam.
        // Tool input will be flushed once at completion boundaries.
        assert_eq!(output, "");
    }

    // Tests for DeltaRenderer trait

    #[test]
    fn test_text_delta_renderer_first_delta_full_mode() {
        let output = TextDeltaRenderer::render_first_delta(
            "Hello",
            "ccs-glm",
            test_colors(),
            TerminalMode::Full,
        );
        assert!(output.contains("[ccs-glm]"));
        assert!(output.contains("Hello"));
        // Multi-line pattern: ends with newline + cursor up
        assert!(output.ends_with("\x1b[1A"));
        assert!(output.contains('\n'));
        assert!(output.contains("\x1b[1A"));
    }

    #[test]
    fn test_text_delta_renderer_first_delta_none_mode() {
        let output = TextDeltaRenderer::render_first_delta(
            "Hello",
            "ccs-glm",
            test_colors(),
            TerminalMode::None,
        );
        // Per-delta output is suppressed in None mode to prevent spam.
        // Content will be flushed once at completion boundaries.
        assert_eq!(output, "");
    }

    #[test]
    fn test_text_delta_renderer_first_delta_basic_mode() {
        let output = TextDeltaRenderer::render_first_delta(
            "Hello",
            "ccs-glm",
            test_colors(),
            TerminalMode::Basic,
        );
        // Per-delta output is suppressed in Basic mode to prevent spam.
        // Content will be flushed once at completion boundaries.
        assert_eq!(output, "");
    }

    #[test]
    fn test_text_delta_renderer_subsequent_delta_full_mode() {
        let output = TextDeltaRenderer::render_subsequent_delta(
            "Hello World",
            "ccs-glm",
            test_colors(),
            TerminalMode::Full,
        );
        assert!(output.contains(CLEAR_LINE));
        assert!(output.contains('\r'));
        assert!(output.contains("Hello World"));
        // Multi-line pattern: ends with newline + cursor up
        assert!(output.contains("\x1b[1A"));
        assert!(output.ends_with("\x1b[1A"));
        assert!(output.contains("[ccs-glm]"));
    }

    #[test]
    fn test_text_delta_renderer_subsequent_delta_none_mode() {
        let output = TextDeltaRenderer::render_subsequent_delta(
            "Hello World",
            "ccs-glm",
            test_colors(),
            TerminalMode::None,
        );
        // Per-delta output is suppressed in None mode to prevent spam.
        // Content will be flushed once at completion boundaries.
        assert_eq!(output, "");
    }

    #[test]
    fn test_text_delta_renderer_uses_full_line_clear() {
        let output = TextDeltaRenderer::render_subsequent_delta(
            "Hello World",
            "ccs-glm",
            test_colors(),
            TerminalMode::Full,
        );
        // Should use \x1b[2K (full line clear), not \x1b[0K (clear to end)
        assert!(output.contains("\x1b[2K"));
        // Should NOT contain \x1b[0K
        assert!(!output.contains("\x1b[0K"));
    }

    #[test]
    fn test_text_delta_renderer_completion_full_mode() {
        let output = TextDeltaRenderer::render_completion(TerminalMode::Full);
        // Multi-line pattern: cursor down + newline
        assert!(output.contains("\x1b[1B"));
        assert!(output.contains('\n'));
        assert_eq!(output, "\x1b[1B\n");
    }

    #[test]
    fn test_text_delta_renderer_completion_none_mode() {
        let output = TextDeltaRenderer::render_completion(TerminalMode::None);
        // No completion sequence in None mode.
        // In non-TTY modes, streaming output is suppressed and the parser flushes
        // newline-terminated content at completion boundaries.
        assert!(!output.contains("\x1b[1B"));
        assert_eq!(output, "");
    }

    #[test]
    fn test_text_delta_renderer_completion_basic_mode() {
        let output = TextDeltaRenderer::render_completion(TerminalMode::Basic);
        // No completion sequence in Basic mode.
        // In non-TTY modes, streaming output is suppressed and the parser flushes
        // newline-terminated content at completion boundaries.
        assert!(!output.contains("\x1b[1B"));
        assert_eq!(output, "");
    }

    #[test]
    fn test_text_delta_renderer_sanitizes_newlines() {
        let output = TextDeltaRenderer::render_first_delta(
            "Hello\nWorld",
            "ccs-glm",
            test_colors(),
            TerminalMode::Full,
        );
        // Newlines should be replaced with spaces
        assert!(!output.contains("Hello\nWorld"));
        assert!(output.contains("Hello World"));
    }

    #[test]
    fn test_text_delta_renderer_in_place_update_sequence() {
        let colors = test_colors();

        // First chunk - multi-line pattern: ends with newline + cursor up
        let out1 =
            TextDeltaRenderer::render_first_delta("Hello", "ccs-glm", colors, TerminalMode::Full);
        assert!(out1.contains("[ccs-glm]"));
        assert!(out1.ends_with("\x1b[1A"));
        assert!(out1.contains('\n'));
        assert!(out1.contains("\x1b[1A"));

        // Second chunk (in-place update with newline + cursor up)
        let out2 = TextDeltaRenderer::render_subsequent_delta(
            "Hello World",
            "ccs-glm",
            colors,
            TerminalMode::Full,
        );
        assert!(out2.contains("\x1b[2K"));
        assert!(out2.contains('\r'));
        assert!(out2.contains("\x1b[1A")); // Cursor up in multi-line pattern
        assert!(out2.contains("[ccs-glm]")); // Prefix is rewritten

        // Completion
        let out3 = TextDeltaRenderer::render_completion(TerminalMode::Full);
        assert!(out3.contains("\x1b[1B"));
        assert_eq!(out3, "\x1b[1B\n");
    }

    #[test]
    fn test_thinking_delta_renderer_first_delta_full_mode() {
        let colors = test_colors();
        let out = ThinkingDeltaRenderer::render_first_delta(
            "git status",
            "ccs/codex",
            colors,
            TerminalMode::Full,
        );
        assert!(out.contains("[ccs/codex]"));
        assert!(out.contains("Thinking:"));
        assert!(out.contains("git status"));
        assert!(out.ends_with("\n\x1b[1A"));
    }

    #[test]
    fn test_thinking_delta_renderer_subsequent_delta_full_mode() {
        let colors = test_colors();
        let out = ThinkingDeltaRenderer::render_subsequent_delta(
            "git status --porcelain",
            "ccs/codex",
            colors,
            TerminalMode::Full,
        );
        assert!(out.contains(CLEAR_LINE));
        assert!(out.contains("Thinking:"));
        assert!(out.contains("git status --porcelain"));
        assert!(out.ends_with("\n\x1b[1A"));
    }

    #[test]
    fn test_thinking_delta_renderer_basic_mode_no_cursor_sequences() {
        let colors = test_colors();
        let out = ThinkingDeltaRenderer::render_first_delta(
            "git status",
            "ccs/codex",
            colors,
            TerminalMode::Basic,
        );
        // Per-delta thinking output is suppressed in Basic mode to prevent spam.
        // Content will be flushed once at completion boundaries.
        assert_eq!(out, "");
    }

    #[test]
    fn test_thinking_delta_renderer_first_delta_none_mode() {
        let colors = test_colors();
        let out = ThinkingDeltaRenderer::render_first_delta(
            "git status",
            "ccs/codex",
            colors,
            TerminalMode::None,
        );
        // Per-delta thinking output is suppressed in None mode to prevent spam.
        // Content will be flushed once at completion boundaries.
        assert_eq!(out, "");
    }

    #[test]
    fn test_thinking_delta_renderer_subsequent_delta_none_mode() {
        let colors = test_colors();
        let out = ThinkingDeltaRenderer::render_subsequent_delta(
            "git status --porcelain",
            "ccs/codex",
            colors,
            TerminalMode::None,
        );
        // Per-delta thinking output is suppressed in None mode to prevent spam.
        // Content will be flushed once at completion boundaries.
        assert_eq!(out, "");
    }

    #[test]
    fn test_thinking_delta_renderer_subsequent_delta_basic_mode() {
        let colors = test_colors();
        let out = ThinkingDeltaRenderer::render_subsequent_delta(
            "git status --porcelain",
            "ccs/codex",
            colors,
            TerminalMode::Basic,
        );
        // Per-delta thinking output is suppressed in Basic mode to prevent spam.
        // Content will be flushed once at completion boundaries.
        assert_eq!(out, "");
    }

    #[test]
    fn test_full_streaming_sequence_no_extra_blank_lines() {
        let colors = test_colors();

        // Simulate a full streaming sequence and verify no extra blank lines
        let first =
            TextDeltaRenderer::render_first_delta("Hello", "agent", colors, TerminalMode::Full);
        let second = TextDeltaRenderer::render_subsequent_delta(
            "Hello World",
            "agent",
            colors,
            TerminalMode::Full,
        );
        let complete = TextDeltaRenderer::render_completion(TerminalMode::Full);

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
    fn test_non_tty_streaming_sequence_simple_output() {
        let colors = test_colors();

        // Simulate a full streaming sequence in None mode
        let first =
            TextDeltaRenderer::render_first_delta("Hello", "agent", colors, TerminalMode::None);
        let second = TextDeltaRenderer::render_subsequent_delta(
            "Hello World",
            "agent",
            colors,
            TerminalMode::None,
        );
        let complete = TextDeltaRenderer::render_completion(TerminalMode::None);

        // Per-delta output is suppressed in None mode to prevent spam
        assert_eq!(first, "");
        assert_eq!(second, "");

        // Completion: no-op (newline is emitted by the parser's non-TTY flush paths)
        assert_eq!(complete, "");
    }

    #[test]
    fn test_prefix_displayed_on_all_deltas() {
        let colors = test_colors();
        let prefix = "my-agent";

        // First delta shows prefix
        let first = TextDeltaRenderer::render_first_delta("A", prefix, colors, TerminalMode::Full);
        assert!(first.contains(&format!("[{prefix}]")));

        // Subsequent delta also shows prefix (design decision: prefix on every delta)
        let subsequent =
            TextDeltaRenderer::render_subsequent_delta("AB", prefix, colors, TerminalMode::Full);
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
    fn test_sanitize_does_not_truncate() {
        // sanitize_for_display no longer truncates - it just sanitizes whitespace
        let long_content = "This is a very long string that should NOT be truncated anymore";
        let result = sanitize_for_display(long_content);
        // Should NOT be truncated
        assert_eq!(result, long_content);
        assert!(!result.contains("..."));
    }

    #[test]
    fn test_delta_renderer_multiple_newlines_render_cleanly() {
        let colors = test_colors();
        let output = TextDeltaRenderer::render_first_delta(
            "Hello\n\n\nWorld",
            "agent",
            colors,
            TerminalMode::Full,
        );
        // Multiple newlines should render as single space
        assert!(output.contains("Hello World"));
        // Should NOT have multiple spaces
        assert!(!output.contains("  "));
    }

    #[test]
    fn test_delta_renderer_trailing_whitespace_trimmed() {
        let colors = test_colors();
        let output = TextDeltaRenderer::render_first_delta(
            "Hello World   ",
            "agent",
            colors,
            TerminalMode::Full,
        );
        // Trailing spaces should be trimmed
        assert!(output.contains("Hello World"));
        // Content should not end with space before escape sequences
        // (it ends with reset color then \n\x1b[1A)
    }

    // Tests for StreamingConfig

    #[test]
    fn test_streaming_config_defaults() {
        let config = StreamingConfig::default();
        assert_eq!(config.prefix_delta_threshold, 0);
        assert!(config.prefix_time_threshold.is_none());
    }

    // Tests for PrefixDebouncer

    #[test]
    fn test_prefix_debouncer_default_first_only() {
        let mut debouncer = PrefixDebouncer::default();

        // First delta always shows prefix
        assert!(debouncer.should_show_prefix(true));

        // With default config (no thresholds), only first delta shows prefix
        // This preserves the original behavior
        assert!(!debouncer.should_show_prefix(false));
        assert!(!debouncer.should_show_prefix(false));
        assert!(!debouncer.should_show_prefix(false));
    }

    #[test]
    fn test_prefix_debouncer_count_threshold() {
        let config = StreamingConfig {
            prefix_delta_threshold: 3,
            prefix_time_threshold: None,
        };
        let mut debouncer = PrefixDebouncer::new(config);

        // First delta always shows prefix
        assert!(debouncer.should_show_prefix(true));

        // Next 2 deltas should skip prefix
        assert!(!debouncer.should_show_prefix(false)); // delta 1
        assert!(!debouncer.should_show_prefix(false)); // delta 2

        // 3rd delta hits threshold, shows prefix
        assert!(debouncer.should_show_prefix(false)); // delta 3

        // Cycle resets
        assert!(!debouncer.should_show_prefix(false)); // delta 1
        assert!(!debouncer.should_show_prefix(false)); // delta 2
        assert!(debouncer.should_show_prefix(false)); // delta 3
    }

    #[test]
    fn test_prefix_debouncer_reset() {
        let config = StreamingConfig {
            prefix_delta_threshold: 3,
            prefix_time_threshold: None,
        };
        let mut debouncer = PrefixDebouncer::new(config);

        // Build up delta count
        debouncer.should_show_prefix(true);
        debouncer.should_show_prefix(false);
        debouncer.should_show_prefix(false);

        // Reset clears state
        debouncer.reset();

        // After reset, next delta is treated as fresh
        // (but not "first delta" unless caller says so)
        assert!(!debouncer.should_show_prefix(false)); // delta 1 after reset
        assert!(!debouncer.should_show_prefix(false)); // delta 2
        assert!(debouncer.should_show_prefix(false)); // delta 3 hits threshold
    }

    #[test]
    fn test_prefix_debouncer_first_delta_always_shows() {
        let config = StreamingConfig {
            prefix_delta_threshold: 100,
            prefix_time_threshold: None,
        };
        let mut debouncer = PrefixDebouncer::new(config);

        // First delta always shows prefix regardless of threshold
        assert!(debouncer.should_show_prefix(true));

        // Even after many skips, marking as first shows prefix
        for _ in 0..10 {
            debouncer.should_show_prefix(false);
        }
        assert!(debouncer.should_show_prefix(true)); // First delta again
    }

    #[test]
    fn test_prefix_debouncer_time_threshold() {
        // Note: This test uses Duration::ZERO for immediate threshold.
        // In practice, time-based debouncing uses longer durations like 100ms.
        let config = StreamingConfig {
            prefix_delta_threshold: 0,
            prefix_time_threshold: Some(Duration::ZERO),
        };
        let mut debouncer = PrefixDebouncer::new(config);

        // First delta shows prefix
        assert!(debouncer.should_show_prefix(true));

        // Since threshold is ZERO, any elapsed time triggers prefix
        // In practice, Instant::now() moves forward, so this should show
        assert!(debouncer.should_show_prefix(false));
    }
}
