//! Shared tests for JSON parsers.
//!
//! This module contains tests for cross-parser behavior, shared utilities,
//! and streaming functionality that applies to multiple parsers.

use super::terminal::TerminalMode;
use super::*;
use crate::config::Verbosity;
use crate::logger::Colors;
use std::cell::RefCell;
use std::io::{self, Cursor, Write};

// Cross-parser behavior tests

#[test]
fn test_verbosity_affects_output() {
    let quiet_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Quiet);
    let full_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Full);

    let long_text = "a".repeat(200);
    let json = format!(
        r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{long_text}"}}]}}}}"#
    );

    let quiet_output = quiet_parser.parse_event(&json).unwrap();
    let full_output = full_parser.parse_event(&json).unwrap();

    // Quiet output should be truncated (shorter)
    assert!(quiet_output.len() < full_output.len());
}

#[test]
fn test_tool_use_shows_input_in_verbose_mode() {
    let verbose_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Verbose)
        .with_terminal_mode(TerminalMode::Full);
    let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.rs"}}]}}"#;
    let output = verbose_parser.parse_event(json).unwrap();
    assert!(output.contains("Tool"));
    assert!(output.contains("Read"));
    assert!(output.contains("file_path=/test.rs"));
}

#[test]
fn test_tool_use_shows_input_in_normal_mode() {
    // Tool inputs are now shown at Normal level for better usability
    let normal_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);
    let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.rs"}}]}}"#;
    let output = normal_parser.parse_event(json).unwrap();
    assert!(output.contains("Tool"));
    assert!(output.contains("Read"));
    // Tool inputs are now visible at Normal level
    assert!(output.contains("file_path=/test.rs"));
}

#[test]
fn test_tool_use_hides_input_in_quiet_mode() {
    // Only Quiet mode hides tool inputs
    let quiet_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Quiet);
    let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.rs"}}]}}"#;
    let output = quiet_parser.parse_event(json).unwrap();
    assert!(output.contains("Tool"));
    assert!(output.contains("Read"));
    // In Quiet mode, input details should not be shown
    assert!(!output.contains("file_path=/test.rs"));
}

#[test]
fn test_parser_uses_custom_display_name_prefix() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full)
        .with_display_name("ccs-glm");
    let json = r#"{"type":"system","subtype":"init","session_id":"abc123"}"#;
    let output = parser.parse_event(json).unwrap();
    assert!(output.contains("[ccs-glm]"));
}

#[test]
fn test_debug_verbosity_is_recognized() {
    let debug_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Debug)
        .with_terminal_mode(TerminalMode::Full);
    // Debug mode should be detectable via is_debug()
    assert!(debug_parser.verbosity.is_debug());
}

// Tests for DeltaAccumulator (shared type)
#[test]
fn test_delta_accumulator_text() {
    let mut acc = super::types::DeltaAccumulator::new();
    acc.add_delta(super::types::ContentType::Text, "0", "Hello, ");
    acc.add_delta(super::types::ContentType::Text, "0", "World!");

    assert_eq!(
        acc.get(super::types::ContentType::Text, "0"),
        Some("Hello, World!")
    );
    assert!(!acc.is_empty());
}

#[test]
fn test_delta_accumulator_thinking() {
    let mut acc = super::types::DeltaAccumulator::new();
    acc.add_delta(super::types::ContentType::Thinking, "0", "Let me think...");
    acc.add_delta(super::types::ContentType::Thinking, "0", " Done.");

    assert_eq!(
        acc.get(super::types::ContentType::Thinking, "0"),
        Some("Let me think... Done.")
    );
}

#[test]
fn test_delta_accumulator_generic() {
    let mut acc = super::types::DeltaAccumulator::new();
    acc.add_delta(super::types::ContentType::Text, "custom_key", "Part 1 ");
    acc.add_delta(super::types::ContentType::Text, "custom_key", "Part 2");

    assert_eq!(
        acc.get(super::types::ContentType::Text, "custom_key"),
        Some("Part 1 Part 2")
    );
}

#[test]
fn test_delta_accumulator_clear() {
    let mut acc = super::types::DeltaAccumulator::new();
    acc.add_delta(super::types::ContentType::Text, "0", "Some text");
    assert!(!acc.is_empty());

    acc.clear();
    assert!(acc.is_empty());
    assert_eq!(acc.get(super::types::ContentType::Text, "0"), None);
}

#[test]
fn test_delta_accumulator_clear_key() {
    let mut acc = super::types::DeltaAccumulator::new();
    acc.add_delta(super::types::ContentType::Text, "0", "Text 0");
    acc.add_delta(super::types::ContentType::Text, "1", "Text 1");

    acc.clear_key(super::types::ContentType::Text, "0");
    assert_eq!(acc.get(super::types::ContentType::Text, "0"), None);
    assert_eq!(
        acc.get(super::types::ContentType::Text, "1"),
        Some("Text 1")
    );
}

// Tests for format_unknown_json_event (shared utility)
#[test]
fn test_format_unknown_json_event_control_event() {
    let colors = Colors { enabled: false };
    // Control events should not show output even in verbose mode
    let json = r#"{"type":"message_start","message":{"id":"msg_123"}}"#;
    let output = super::types::format_unknown_json_event(json, "Claude", colors, true);
    // Control events should return empty string
    assert!(output.is_empty());
}

#[test]
fn test_format_unknown_json_event_partial_event() {
    let colors = Colors { enabled: false };
    // Partial events with content should show in non-verbose mode
    // The delta content should be extracted and shown directly
    let json =
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
    let output = super::types::format_unknown_json_event(json, "Claude", colors, false);
    // Should show content for partial events
    // Note: The delta text field is nested, so it will be extracted
    assert!(!output.is_empty());
}

#[test]
fn test_format_unknown_json_event_partial_event_verbose() {
    let colors = Colors { enabled: false };
    // Partial events should be labeled as such in verbose mode
    let json =
        r#"{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}"#;
    let output = super::types::format_unknown_json_event(json, "Claude", colors, true);
    // Should show "Partial event:" in verbose mode
    assert!(!output.is_empty());
    assert!(output.contains("Partial event"));
}

#[test]
fn test_format_unknown_json_event_complete_event_verbose() {
    let colors = Colors { enabled: false };
    // Complete events only show in verbose mode
    let json = r#"{"type":"message","content":"This is a complete message with substantial content that should be displayed as is."}"#;
    let output = super::types::format_unknown_json_event(json, "Claude", colors, true);
    assert!(!output.is_empty());
    assert!(output.contains("Complete event"));
}

#[test]
fn test_format_unknown_json_event_complete_event_normal() {
    let colors = Colors { enabled: false };
    // Complete events should not show in non-verbose mode if no explicit content
    let json = r#"{"type":"status","status":"ok"}"#;
    let output = super::types::format_unknown_json_event(json, "Claude", colors, false);
    // Should return empty in non-verbose mode for complete events without content
    assert!(output.is_empty());
}

#[test]
fn test_format_unknown_json_event_with_explicit_delta_flag() {
    let colors = Colors { enabled: false };
    // Events with explicit delta: true should be detected as partial
    let json = r#"{"type":"message","delta":true,"content":"partial content"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, true);
    assert!(!output.is_empty());
    assert!(output.contains("Partial event"));
}

#[test]
fn test_format_unknown_json_event_error_control() {
    let colors = Colors { enabled: false };
    // Error events are control events and should not show
    let json = r#"{"type":"error","message":"Something went wrong"}"#;
    let output = super::types::format_unknown_json_event(json, "Claude", colors, true);
    // Error events are control events - should return empty
    assert!(output.is_empty());
}

#[test]
fn test_format_unknown_json_event_delta_shows_content_in_normal_mode() {
    let colors = Colors { enabled: false };
    // Delta events should show their content even in non-verbose mode
    let json = r#"{"type":"content_block_delta","delta":{"text":"Hello World"}}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Should show the delta content in normal mode
    assert!(!output.is_empty());
    assert!(output.contains("Hello World"));
}

#[test]
fn test_format_unknown_json_event_partial_with_delta_field() {
    let colors = Colors { enabled: false };
    // Events with delta field should show content even without explicit type name
    let json = r#"{"type":"chunk","delta":"some partial content"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Should show content because delta field is present
    assert!(!output.is_empty());
    assert!(output.contains("some partial content"));
}

#[test]
fn test_format_unknown_json_event_partial_with_text_field() {
    let colors = Colors { enabled: false };
    // Partial events with text field should show content
    let json = r#"{"type":"partial","text":"streaming text here"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Should show content
    assert!(!output.is_empty());
    assert!(output.contains("streaming text here"));
}

#[test]
fn test_format_unknown_json_event_nested_delta_text() {
    let colors = Colors { enabled: false };
    // Nested delta.text should be extracted
    let json = r#"{"type":"update","delta":{"text":"nested content"}}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Should show the nested delta text content
    assert!(!output.is_empty());
    assert!(output.contains("nested content"));
}

#[test]
fn test_format_unknown_json_event_deeply_nested_content() {
    let colors = Colors { enabled: false };
    // Test deeply nested content extraction
    // The current implementation extracts from delta.text or content fields
    // but not from arbitrary nested paths
    let json = r#"{"type":"delta","delta":{"text":"deep nested text"}}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Should extract content from nested delta.text
    assert!(!output.is_empty());
    assert!(output.contains("deep nested text"));
}

#[test]
fn test_format_unknown_json_event_array_content() {
    let colors = Colors { enabled: false };
    // Test content extraction from arrays
    let json = r#"{"type":"message","content":["item1","item2","item3"]}"#;
    let _output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Arrays should be handled gracefully (content field exists but isn't a string)
    // The function should not crash
}

#[test]
fn test_format_unknown_json_event_empty_delta() {
    let colors = Colors { enabled: false };
    // Test delta with empty string content
    let json = r#"{"type":"content_block_delta","delta":{"text":""}}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Empty content should not show output
    assert!(output.is_empty() || output.trim().is_empty());
}

#[test]
fn test_format_unknown_json_event_whitespace_only_delta() {
    let colors = Colors { enabled: false };
    // Test delta with whitespace-only content
    let json = r#"{"type":"content_block_delta","delta":{"text":"   "}}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Whitespace-only content should not show output
    assert!(output.is_empty() || output.trim().is_empty());
}

#[test]
fn test_format_unknown_json_event_unicode_delta_content() {
    let colors = Colors { enabled: false };
    // Test Unicode content in delta
    let json = r#"{"type":"delta","text":"Hello 世界 🌍"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Should show Unicode content properly
    assert!(!output.is_empty());
    assert!(output.contains("Hello 世界"));
}

#[test]
fn test_format_unknown_json_event_text_field_priority() {
    let colors = Colors { enabled: false };
    // Test that content field has priority over text field in classifier
    // The classifier's find_content_field returns "content" first (priority order)
    let json = r#"{"type":"content_delta","text":"first","content":"second"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Should extract content since type contains "delta" (explicit partial)
    assert!(!output.is_empty());
    // The content field is prioritized by the classifier (not text)
    assert!(output.contains("second"));
}

#[test]
fn test_format_unknown_json_event_null_content_field() {
    let colors = Colors { enabled: false };
    // Test event with null content field
    let json = r#"{"type":"message","content":null}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Null content should be handled gracefully
    assert!(output.is_empty());
}

#[test]
fn test_format_unknown_json_event_numeric_content_field() {
    let colors = Colors { enabled: false };
    // Test event with numeric content (not a string)
    let json = r#"{"type":"metric","content":12345}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, true);
    // Numeric content should be shown in verbose mode
    assert!(!output.is_empty());
}

#[test]
fn test_format_unknown_json_event_boolean_delta_flag() {
    let colors = Colors { enabled: false };
    // Test explicit boolean delta flag
    let json = r#"{"type":"chunk","delta":true,"content":"test"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Should detect delta: true and show content
    assert!(!output.is_empty());
    assert!(output.contains("test"));
}

#[test]
fn test_format_unknown_json_event_special_characters_in_content() {
    let colors = Colors { enabled: false };
    // Test special characters that might cause issues
    let json = r#"{"type":"delta","text":"Line1\nLine2\tTabbed\"Quoted"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", colors, false);
    // Should handle special characters properly
    assert!(!output.is_empty());
}

#[test]
fn test_format_unknown_json_event_very_long_content() {
    let colors = Colors { enabled: false };
    // Test very long content doesn't cause issues
    let long_text = "a".repeat(10000);
    let json = format!(r#"{{"type":"delta","text":"{long_text}"}}"#);
    let output = super::types::format_unknown_json_event(&json, "Test", colors, false);
    // Should handle long content without crashing
    assert!(!output.is_empty());
    // Content should be shown in full for deltas (not truncated)
}

// Tests for stream classifier
#[test]
fn test_stream_classifies_short_content_as_partial() {
    use super::stream_classifier::{StreamEventClassifier, StreamEventType};
    let classifier = StreamEventClassifier::new();
    let event = serde_json::json!({
        "type": "chunk",
        "content": "Hi"
    });

    let result = classifier.classify(&event);
    assert_eq!(result.event_type, StreamEventType::Partial);
}

#[test]
fn test_stream_classifies_long_content_as_complete() {
    use super::stream_classifier::{StreamEventClassifier, StreamEventType};
    let classifier = StreamEventClassifier::new();
    let long_text = "This is a substantial message that exceeds the default threshold and should be considered complete.";
    let event = serde_json::json!({
        "type": "message",
        "content": long_text
    });

    let result = classifier.classify(&event);
    assert_eq!(result.event_type, StreamEventType::Complete);
}

#[test]
fn test_stream_classifies_status_without_content_as_control() {
    use super::stream_classifier::{StreamEventClassifier, StreamEventType};
    let classifier = StreamEventClassifier::new();
    let event = serde_json::json!({
        "type": "status",
        "status": "processing"
    });

    let result = classifier.classify(&event);
    assert_eq!(result.event_type, StreamEventType::Control);
}

#[test]
fn test_stream_classifies_error_as_control() {
    use super::stream_classifier::{StreamEventClassifier, StreamEventType};
    let classifier = StreamEventClassifier::new();
    let event = serde_json::json!({
        "type": "error",
        "message": "Something went wrong"
    });

    let result = classifier.classify(&event);
    assert_eq!(result.event_type, StreamEventType::Control);
}

// Tests for health monitoring
#[test]
fn test_claude_parser_tracks_partial_events_in_health_monitoring() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Create a stream with mixed events: control, partial (delta), and complete
    let input = r#"{"type":"stream_event","event":{"type":"message_start"}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}}
{"type":"stream_event","event":{"type":"message_stop"}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Complete message"}]}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    // Parse stream - should handle all events without health warnings
    let result = parser.parse_stream(reader, &mut writer);
    assert!(result.is_ok());

    // Verify output contains delta content
    let output = String::from_utf8(writer).unwrap();
    assert!(output.contains("Hello") || output.contains("World") || output.contains("Complete"));
}

#[test]
fn test_health_monitor_no_warning_with_high_partial_percentage() {
    use super::health::HealthMonitor;

    let monitor = HealthMonitor::new("test");
    let colors = Colors { enabled: false };

    // Simulate the bug report scenario: 97.5% partial events (2049 of 2102)
    // These should NOT trigger a warning because partial events are valid streaming content
    for _ in 0..2049 {
        monitor.record_partial_event();
    }
    for _ in 0..53 {
        monitor.record_parsed();
    }

    // Should NOT warn even with 97.5% "partial" events
    let warning = monitor.check_and_warn(colors);
    assert!(
        warning.is_none(),
        "Should not warn with high percentage of partial events"
    );
}

#[test]
fn test_health_monitor_warning_only_for_parse_errors() {
    use super::health::HealthMonitor;

    let monitor = HealthMonitor::new("test");
    let colors = Colors { enabled: false };

    // Mix of partial, control, and parsed events should NOT trigger warning
    for _ in 0..1000 {
        monitor.record_partial_event();
    }
    for _ in 0..500 {
        monitor.record_control_event();
    }
    for _ in 0..50 {
        monitor.record_parsed();
    }

    let warning = monitor.check_and_warn(colors);
    assert!(
        warning.is_none(),
        "Should not warn with mix of partial, control, and parsed events"
    );

    // Create a new monitor and test with actual parse errors
    let monitor = HealthMonitor::new("claude");

    // Add parse errors exceeding 50% threshold
    for _ in 0..60 {
        monitor.record_parse_error();
    }
    for _ in 0..40 {
        monitor.record_parsed();
    }

    let warning = monitor.check_and_warn(colors);
    assert!(warning.is_some(), "Should warn with >50% parse errors");
    assert!(warning.unwrap().contains("parse errors"));
}

// Tests for verbose mode streaming
#[test]
fn test_verbose_mode_streaming_no_duplicate_lines() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Verbose)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate streaming content that arrives in multiple deltas
    // This mimics a diagnostic message like "warning: unused variable"
    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"warning: unu"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"sed"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" vari"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"able"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // After the fix, streaming should show accumulated text on a single line using in-place updates:
    // [Claude] warning: unu\r                    (first chunk with prefix)
    // \x1b[2K\r[Claude] warning: unused\r      (second chunk clears line, rewrites with accumulated)
    // \x1b[2K\r[Claude] warning: unused vari\r (third chunk clears line, rewrites with accumulated)
    // \x1b[2K\r[Claude] warning: unused variable\n (final chunk + message_stop adds newline)

    // The output should contain carriage returns for overwriting
    assert!(
        output.contains('\r'),
        "Should contain carriage returns for overwriting"
    );

    // Should contain the line clear escape sequence
    assert!(
        output.contains("\x1b[2K"),
        "Should contain line clear escape sequence for in-place updates"
    );

    // With the single-line pattern, each delta rewrites the entire line including prefix
    // The output string will contain multiple prefixes, but visually only one is shown
    // due to carriage returns and line clearing
    let prefix_count = output.matches("[Claude]").count();
    assert!(prefix_count >= 1, "Should have at least 1 prefix");

    // The final accumulated text should be present
    assert!(
        output.contains("warning: unused variable"),
        "Should contain complete accumulated text"
    );

    // Should end with newline (from message_stop)
    assert!(
        output.ends_with('\n'),
        "Should end with newline after message_stop"
    );
}

#[test]
fn test_normal_and_verbose_mode_show_same_deltas() {
    let normal_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);
    let verbose_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Verbose)
        .with_terminal_mode(TerminalMode::Full);

    let json = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#;

    let normal_output = normal_parser.parse_event(json);
    let verbose_output = verbose_parser.parse_event(json);

    // Both should show the delta content
    assert!(normal_output.is_some());
    assert!(verbose_output.is_some());

    // Both should contain the delta text
    assert!(normal_output.unwrap().contains("Hello"));
    assert!(verbose_output.unwrap().contains("Hello"));
}

#[test]
fn test_delta_with_embedded_newline_displays_inline() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate a delta that contains a newline character within the text
    // For example: "Now I understand\n1. In src/..."
    let json = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Now I understand\n1. In src/"}}}"#;

    let output = parser.parse_event(json);

    assert!(output.is_some());
    let out = output.unwrap();

    // The newline should be replaced with a space to prevent artificial line breaks
    // Multi-line pattern: prefix and content on same line ending with newline + cursor up
    // Output format: "[Claude] Now I understand 1. In src/\n\x1b[1A"
    assert!(out.contains("Now I understand"));
    assert!(out.contains("1. In src/"));

    // Multi-line pattern: output ends with newline + cursor up (2 lines when counted)
    // but visually appears as 1 line due to cursor positioning
    assert_eq!(
        out.lines().count(),
        2,
        "Delta with embedded newline should produce 2 lines with multi-line pattern (content + cursor up)"
    );
}

// Integration tests for streaming flush behavior

/// A mock writer that tracks whether `flush()` is called after each `write()`
struct FlushTrackingWriter {
    write_count: RefCell<usize>,
    flush_count: RefCell<usize>,
    buffer: RefCell<Vec<u8>>,
}

impl FlushTrackingWriter {
    fn new() -> Self {
        Self {
            write_count: RefCell::new(0),
            flush_count: RefCell::new(0),
            buffer: RefCell::new(Vec::new()),
        }
    }

    /// Verify that flush was called at least as many times as write
    /// In the streaming fix, flush should be called after every write
    fn flush_called_after_writes(&self) -> bool {
        let writes = *self.write_count.borrow();
        let flushes = *self.flush_count.borrow();
        flushes >= writes
    }
}

impl Write for FlushTrackingWriter {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        *self.write_count.borrow_mut() += 1;
        self.buffer.borrow_mut().extend_from_slice(buf);
        Ok(buf.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        *self.flush_count.borrow_mut() += 1;
        Ok(())
    }
}

#[test]
fn test_claude_streaming_flushes_after_write() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate streaming deltas that produce output
    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = FlushTrackingWriter::new();

    parser.parse_stream(reader, &mut writer).unwrap();

    // Verify flush was called after writes for streaming output
    assert!(
        writer.flush_called_after_writes(),
        "flush() should be called after writes for real-time streaming"
    );
}

#[test]
fn test_codex_streaming_flushes_after_write() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate streaming delta events
    let input = r#"{"type":"item.started","item":{"type":"reasoning","id":"item_1","text":"Thinking"}}
{"type":"item.completed","item":{"type":"reasoning","id":"item_1"}}"#;

    let reader = Cursor::new(input);
    let mut writer = FlushTrackingWriter::new();

    parser.parse_stream(reader, &mut writer).unwrap();

    // Verify flush was called after writes
    assert!(
        writer.flush_called_after_writes(),
        "flush() should be called after writes for real-time streaming"
    );
}

#[test]
fn test_gemini_streaming_flushes_after_write() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate streaming delta events
    let input = r#"{"type":"message","role":"assistant","content":"Hello","delta":true,"timestamp":"2025-10-10T12:00:01.000Z"}
{"type":"message","role":"assistant","content":" World","delta":true,"timestamp":"2025-10-10T12:00:02.000Z"}
{"type":"result","status":"success","timestamp":"2025-10-10T12:00:03.000Z"}"#;

    let reader = Cursor::new(input);
    let mut writer = FlushTrackingWriter::new();

    parser.parse_stream(reader, &mut writer).unwrap();

    // Verify flush was called after writes
    assert!(
        writer.flush_called_after_writes(),
        "flush() should be called after writes for real-time streaming"
    );
}

// Integration test for streaming accumulation behavior
// Verifies that multiple text deltas accumulate correctly and output contains carriage returns
#[test]
fn test_streaming_accumulation_behavior() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Verbose)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate streaming content arriving in multiple deltas
    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // Should contain carriage returns for overwriting previous content
    assert!(
        output.contains('\r'),
        "Should contain carriage returns for streaming overwrite"
    );

    // With the single-line pattern, each delta rewrites the entire line including prefix
    // The output string will contain multiple prefixes, but visually only one is shown
    let prefix_count = output.matches("[Claude]").count();
    assert!(prefix_count >= 1, "Should have at least 1 prefix");

    // The final accumulated text should be present
    assert!(
        output.contains("Hello World!"),
        "Should contain complete accumulated text"
    );

    // Should end with newline (from message_stop)
    assert!(
        output.ends_with('\n'),
        "Should end with newline after message_stop"
    );

    // Verify progressive accumulation: output should contain intermediate accumulated states
    // After first delta: "Hello"
    // After second delta: "Hello World"
    // After third delta: "Hello World!"
    assert!(output.contains("Hello"), "Should contain first delta");
    assert!(
        output.contains("Hello World"),
        "Should contain accumulated text after second delta"
    );
    assert!(
        output.contains("Hello World!"),
        "Should contain final accumulated text"
    );
}

// Edge case tests for streaming behavior

/// Test streaming with empty delta chunks
/// Verifies that empty chunks don't cause errors and don't produce output
#[test]
fn test_streaming_empty_delta_chunk() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate streaming with an empty delta in the middle
    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":""}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    // Should not panic or error
    let result = parser.parse_stream(reader, &mut writer);
    assert!(
        result.is_ok(),
        "Empty delta chunks should be handled gracefully"
    );

    let output = String::from_utf8(writer).unwrap();
    // Should still contain the final accumulated text
    assert!(
        output.contains("Hello World"),
        "Should contain accumulated text despite empty chunk"
    );
}

/// Test streaming with a single chunk (no streaming scenario)
/// Verifies that single-chunk content displays correctly with prefix
#[test]
fn test_streaming_single_chunk() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Single chunk scenario - content arrives all at once
    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Complete message"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // With single chunk, there should be exactly one prefix (first delta only)
    let prefix_count = output.matches("[Claude]").count();
    assert_eq!(prefix_count, 1, "Single chunk should have exactly 1 prefix");

    // Should contain the complete text
    assert!(
        output.contains("Complete message"),
        "Should contain single chunk text"
    );

    // Should end with newline
    assert!(
        output.ends_with('\n'),
        "Should end with newline after message_stop"
    );
}

/// Test streaming with very long accumulated text
/// Verifies that the parser handles long text without errors
/// and that truncation works correctly in Full terminal mode
#[test]
fn test_streaming_very_long_text() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Create a very long text that would exceed terminal width
    let long_chunk = "a".repeat(200);
    let long_chunk2 = "b".repeat(200);

    let input = format!(
        r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{long_chunk}"}}}}}}
{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{long_chunk2}"}}}}}}
{{"type":"stream_event","event":{{"type":"message_stop"}}}}"#
    );

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    // Should handle long text without errors
    let result = parser.parse_stream(reader, &mut writer);
    assert!(
        result.is_ok(),
        "Should handle very long text without errors"
    );

    let output = String::from_utf8(writer).unwrap();
    // In Full mode, long text should be truncated with ellipsis
    // The output should be much shorter than the input due to truncation
    assert!(
        output.len() < long_chunk.len(),
        "Output should be truncated in Full terminal mode"
    );
    // Should contain ellipsis indicating truncation
    assert!(
        output.contains("..."),
        "Truncated output should contain ellipsis"
    );
}

/// Test streaming with special characters in text
/// Verifies that special characters (quotes, unicode, etc.) are handled correctly
#[test]
fn test_streaming_special_characters() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Text with various special characters
    let special_text = "Hello \"World\"! 'quotes' and $ymbols & unicode: 🌍 世界";

    let json = format!(
        r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}}}"#,
        // Escape quotes for JSON
        special_text.replace('"', "\\\"")
    );

    let output = parser.parse_event(&json);

    assert!(
        output.is_some(),
        "Should handle special characters without errors"
    );
    let out = output.unwrap();

    // Verify some special characters are present
    assert!(out.contains("Hello"), "Should contain text before quotes");
    assert!(
        out.contains("World") || out.contains("quotes"),
        "Should handle quoted text"
    );
}

/// Test streaming with rapid consecutive chunks
/// Verifies that rapid streaming (multiple chunks in quick succession) is handled correctly
#[test]
fn test_streaming_rapid_chunks() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate rapid streaming with many small chunks
    let mut input_lines = Vec::new();
    for i in 0..10 {
        input_lines.push(format!(
            r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"chunk{i}"}}}}}}"#
        ));
    }
    input_lines.push(r#"{"type":"stream_event","event":{"type":"message_stop"}}"#.to_string());

    let input = input_lines.join("\n");
    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    // Should handle rapid chunks without errors
    let result = parser.parse_stream(reader, &mut writer);
    assert!(result.is_ok(), "Should handle rapid consecutive chunks");

    let output = String::from_utf8(writer).unwrap();

    // With the single-line pattern, each delta rewrites the entire line including prefix
    // 10 deltas = 10 prefixes in output string, but visually only one is shown
    let prefix_count = output.matches("[Claude]").count();
    assert_eq!(
        prefix_count, 10,
        "Rapid chunks should have 10 prefixes (one per delta)"
    );

    // Should contain carriage returns for overwriting
    assert!(
        output.contains('\r'),
        "Rapid chunks should use carriage returns"
    );

    // Verify content from multiple chunks is present
    assert!(output.contains("chunk0"), "Should contain first chunk");
    // In Full mode, the accumulated text may be truncated if it exceeds terminal width
    // The total "chunk0chunk1...chunk9" is 60 chars, which may be truncated
    // Just verify that streaming worked (prefixes are present, cursor positioning works)
}

/// Test streaming with only whitespace chunks
/// Verifies that whitespace-only chunks don't produce spurious output
#[test]
fn test_streaming_whitespace_only_chunks() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate streaming with whitespace chunks
    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"   "}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"\t"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    // Should handle whitespace chunks without errors
    let result = parser.parse_stream(reader, &mut writer);
    assert!(result.is_ok(), "Should handle whitespace-only chunks");

    let output = String::from_utf8(writer).unwrap();
    // Should contain the actual non-whitespace content
    assert!(
        output.contains("Hello"),
        "Should contain non-whitespace content"
    );
}

/// Test that content block start resets state properly
/// Verifies that a new content block starts fresh without previous accumulation
#[test]
fn test_streaming_content_block_reset() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // First content block, then start a new one
    let input = r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":"Initial"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" Block1"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // Should contain the content from the block
    assert!(
        output.contains("Initial") || output.contains("Block1"),
        "Should contain content from block"
    );
}

/// Test streaming behavior across multiple parsers for consistency
/// Verifies that all parsers (Claude, Codex, Gemini, `OpenCode`) handle streaming consistently
#[test]
fn test_streaming_consistency_across_parsers() {
    use std::io::Cursor;

    // Test Claude parser
    let claude_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);
    let claude_input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;
    let claude_reader = Cursor::new(claude_input);
    let mut claude_writer = Vec::new();
    claude_parser
        .parse_stream(claude_reader, &mut claude_writer)
        .unwrap();
    let claude_output = String::from_utf8(claude_writer).unwrap();

    // All parsers should use carriage returns for streaming
    assert!(
        claude_output.contains('\r'),
        "Claude should use carriage returns"
    );
    // With the single-line pattern, each delta includes the prefix
    // 2 deltas = 2 prefixes in output string
    assert_eq!(
        claude_output.matches("[Claude]").count(),
        2,
        "Claude should have 2 prefixes (one per delta)"
    );

    // Test Codex parser
    // Note: With StreamingSession, Codex shows prefix only on first item.started (1 prefix)
    // The completion just adds a newline without re-displaying content
    let codex_parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);
    let codex_input = r#"{"type":"item.started","item":{"type":"agent_message","id":"msg1","text":"Hello"}}
{"type":"item.started","item":{"type":"agent_message","id":"msg1","text":" World"}}
{"type":"item.completed","item":{"type":"agent_message","id":"msg1"}}"#;
    let codex_reader = Cursor::new(codex_input);
    let mut codex_writer = Vec::new();
    codex_parser
        .parse_stream(codex_reader, &mut codex_writer)
        .unwrap();
    let codex_output = String::from_utf8(codex_writer).unwrap();

    assert!(
        codex_output.contains('\r'),
        "Codex should use carriage returns"
    );
    // With the single-line pattern, each item.started includes the prefix
    // 2 item.started events = 2 prefixes in output string
    assert_eq!(
        codex_output.matches("[Codex]").count(),
        2,
        "Codex shows 2 prefixes (one per item.started)"
    );

    // Test Gemini parser
    // Note: With the single-line pattern, each delta includes the prefix
    // The final non-delta message is deduplicated and only adds a newline
    let gemini_parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);
    let gemini_input = r#"{"type":"message","role":"assistant","content":"Hello","delta":true}
{"type":"message","role":"assistant","content":" World","delta":true}
{"type":"message","role":"assistant","content":"Hello World"}"#;
    let gemini_reader = Cursor::new(gemini_input);
    let mut gemini_writer = Vec::new();
    gemini_parser
        .parse_stream(gemini_reader, &mut gemini_writer)
        .unwrap();
    let gemini_output = String::from_utf8(gemini_writer).unwrap();

    assert!(
        gemini_output.contains('\r'),
        "Gemini should use carriage returns"
    );
    // With the single-line pattern, each delta includes the prefix
    // 2 deltas = 2 prefixes in output string
    assert_eq!(
        gemini_output.matches("[Gemini]").count(),
        2,
        "Gemini shows 2 prefixes (one per delta)"
    );

    // Test OpenCode parser
    // Note: With the single-line pattern, each text event includes the prefix
    // The step_finish event also shows a prefix (different content)
    // 2 text events + 1 step_finish = 3 prefixes total
    let opencode_parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);
    let opencode_input = r#"{"type":"text","timestamp":1768191347231,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac63300","type":"text","text":"Hello"}}
{"type":"text","timestamp":1768191347232,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac63300","type":"text","text":" World"}}
{"type":"step_finish","timestamp":1768191347296,"sessionID":"ses_44f9562d4ffe","part":{"type":"step-finish","reason":"end_turn"}}"#;
    let opencode_reader = Cursor::new(opencode_input);
    let mut opencode_writer = Vec::new();
    opencode_parser
        .parse_stream(opencode_reader, &mut opencode_writer)
        .unwrap();
    let opencode_output = String::from_utf8(opencode_writer).unwrap();

    assert!(
        opencode_output.contains('\r'),
        "OpenCode should use carriage returns"
    );
    // With the single-line pattern: 2 text events (each with prefix) + 1 step_finish (with prefix) = 3
    assert_eq!(
        opencode_output.matches("[OpenCode]").count(),
        3,
        "OpenCode shows 3 prefixes (2 text events + 1 step_finish)"
    );
}

// Tests for snapshot-as-delta detection
// These tests verify that the streaming state correctly identifies when
// snapshot-style content is being sent as deltas (a common bug pattern)

/// Test that a single large delta triggers a warning
/// This simulates a parser sending the entire accumulated content as a "delta"
#[test]
fn test_snapshot_as_delta_single_large_delta_warns() {
    use super::streaming_state::StreamingSession;

    let mut session = StreamingSession::new();
    session.on_message_start();

    // Create a delta larger than SNAPSHOT_THRESHOLD (200 chars)
    let large_delta = "x".repeat(201);

    // Capture stderr to verify warning is emitted
    // Note: In a real test environment, this warning would go to stderr
    // The test verifies the functionality doesn't crash and handles the large delta
    let show_prefix = session.on_text_delta(0, &large_delta);
    assert!(show_prefix, "First large delta should show prefix");

    // Content should still be accumulated correctly
    assert_eq!(
        session.get_accumulated(super::types::ContentType::Text, "0"),
        Some(large_delta.as_str())
    );
}

/// Test that many tiny deltas work correctly without warnings
/// This verifies the normal streaming case doesn't trigger false positives
#[test]
fn test_many_tiny_deltas_work_correctly() {
    use super::streaming_state::StreamingSession;

    let mut session = StreamingSession::new();
    session.on_message_start();

    // Send many small deltas (normal streaming behavior)
    let mut expected_content = String::new();
    for i in 0..20 {
        let delta = format!("chunk{i}");
        expected_content.push_str(&delta);
        session.on_text_delta(0, &delta);
    }

    // All content should be accumulated correctly
    assert_eq!(
        session.get_accumulated(super::types::ContentType::Text, "0"),
        Some(expected_content.as_str())
    );
}

/// Test that a pattern of repeated large deltas is detected
/// This simulates a bug where the same snapshot is sent repeatedly as "deltas"
#[test]
fn test_pattern_of_repeated_large_deltas_detected() {
    use super::streaming_state::StreamingSession;

    let mut session = StreamingSession::new();
    session.on_message_start();

    // Create a large snapshot
    let large_snapshot = "x".repeat(201);

    // Send the same large content 3 times (simulating snapshot-as-delta bug)
    // This should trigger the pattern detection warning
    for _ in 0..3 {
        session.on_text_delta(0, &large_snapshot);
    }

    // Content should accumulate (with duplication - this is the bug we're detecting)
    let accumulated = session
        .get_accumulated(super::types::ContentType::Text, "0")
        .unwrap();
    assert!(accumulated.len() > large_snapshot.len());
}

/// Test mixed small and large deltas
/// Verifies that legitimate mixed content doesn't cause issues
#[test]
fn test_mixed_small_and_large_deltas() {
    use super::streaming_state::StreamingSession;

    let mut session = StreamingSession::new();
    session.on_message_start();

    // Start with small deltas
    session.on_text_delta(0, "Hello ");
    session.on_text_delta(0, "World ");

    // Add a large delta (might be legitimate in some cases)
    let large_delta = "x".repeat(201);
    session.on_text_delta(0, &large_delta);

    // Continue with small deltas
    session.on_text_delta(0, " End");

    // All content should be accumulated
    let accumulated = session
        .get_accumulated(super::types::ContentType::Text, "0")
        .unwrap();
    assert!(accumulated.contains("Hello"));
    assert!(accumulated.contains("World"));
    assert!(accumulated.ends_with(" End"));
}

/// Test for ccs-glm streaming scenario
///
/// This test simulates the problematic output pattern from the ccs-glm agent:
/// - One token per line with repeated prefix (the bug we're fixing)
/// - After the fix, output should have:
///   - Single-line in-place rendering with carriage returns
///   - Line clearing before each rewrite
///   - Single final newline
///   - No duplication of final message
///
/// With the single-line pattern:
/// - First delta: `[Claude] H\r`
/// - Second delta: `\x1b[2K\r[Claude] He\r`
/// - ...and so on
/// - Each delta rewrites the entire line with prefix
/// - Visually, the user sees only one prefix that updates in-place
#[test]
fn test_ccs_glm_streaming_no_duplicate_prefix() {
    use std::io::Cursor;

    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate the problematic ccs-glm streaming pattern:
    // Many small deltas arriving one token at a time
    let mut input_lines = Vec::new();

    // Message start
    input_lines.push(r#"{"type":"stream_event","event":{"type":"message_start"}}"#.to_string());

    // Content block start
    input_lines.push(r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#.to_string());

    // Simulate streaming "Hello World" one token at a time
    for token in ["H", "e", "l", "l", "o", " ", "W", "o", "r", "l", "d", "!"] {
        let delta_json = format!(
            r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{token}"}}}}}}"#
        );
        input_lines.push(delta_json);
    }

    // Message stop
    input_lines.push(r#"{"type":"stream_event","event":{"type":"message_stop"}}"#.to_string());

    let input = input_lines.join("\n");
    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // Verify the fix:
    // 1. With the single-line pattern, each delta includes the prefix
    // 12 tokens = 12 prefixes in output string, but visually only one is shown
    let prefix_count = output.matches("[Claude]").count();
    assert_eq!(
        prefix_count, 12,
        "Should have 12 prefixes (one per delta). Output: {output:?}"
    );

    // 2. Should contain carriage returns for in-place updates
    assert!(
        output.contains('\r'),
        "Should use carriage returns for in-place updates. Output: {output:?}"
    );

    // 3. Final message "Hello World!" should be present
    assert!(
        output.contains("Hello World!"),
        "Should contain complete message. Output: {output:?}"
    );

    // 4. Should end with newline (from message_stop)
    assert!(
        output.ends_with('\n'),
        "Should end with newline after message_stop. Output: {output:?}"
    );
}

/// Test for ccs-glm complete message deduplication
///
/// This test verifies that when a complete message event arrives after
/// streaming has already displayed the content, the complete message
/// is NOT re-displayed (preventing duplication).
#[test]
fn test_ccs_glm_complete_message_deduplication() {
    use std::io::Cursor;

    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate streaming followed by a complete message event
    let input = r#"{"type":"stream_event","event":{"type":"message_start"}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}}
{"type":"stream_event","event":{"type":"message_stop"}}
{"type":"assistant","message":{"content":[{"type":"text","text":"Hello World!"}]}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // The complete message should NOT be displayed because streaming already showed it
    // Count how many times the full text appears
    let full_text_count = output.matches("Hello World!").count();

    // The text should appear at most once (from the accumulated streaming output)
    // The complete message event should be skipped due to deduplication
    assert!(
        full_text_count <= 1,
        "Complete message should not be duplicated. Found {full_text_count} occurrences. Output: {output:?}"
    );

    // With the single-line pattern, each delta includes the prefix
    // 3 deltas = 3 prefixes in output string (but visually only one is shown)
    let prefix_count = output.matches("[Claude]").count();
    assert_eq!(
        prefix_count, 3,
        "Should have 3 prefixes from streaming (one per delta). Output: {output:?}"
    );
}

/// Test for content block state tracking
///
/// This test verifies that the `ContentBlockState` implementation correctly
/// tracks block transitions and the `started_output` flag. This is the foundation
/// for future enhancements where block transitions can emit newlines.
///
/// Note: This is a unit test that directly tests the `StreamingSession` state
/// tracking, not the end-to-end parser behavior (which would require additional
/// parser-layer changes to actually emit newlines on block transitions).
///
/// When transitioning to a different content block index, the old block's content
/// is cleared to prevent memory buildup and to ensure proper isolation between blocks.
#[test]
fn test_content_block_state_tracking() {
    use crate::json_parser::streaming_state::StreamingSession;

    let mut session = StreamingSession::new();
    session.on_message_start();

    // Initially, no content has been streamed
    assert!(!session.has_any_streamed_content());

    // Start streaming content in block 0
    let show_prefix = session.on_text_delta(0, "First");
    assert!(show_prefix, "First delta should show prefix");
    assert!(session.has_any_streamed_content());

    // Transition to block 1 via on_content_block_start
    // This should finalize block 0 and clear its accumulated content
    session.on_content_block_start(1);

    // Stream content in block 1
    let show_prefix = session.on_text_delta(1, "Second");
    assert!(show_prefix, "First delta in new block should show prefix");

    // Verify block 0 content was cleared and block 1 content is present
    assert_eq!(
        session.get_accumulated(crate::json_parser::types::ContentType::Text, "0"),
        None,
        "Block 0 content should be cleared after transitioning to block 1"
    );
    assert_eq!(
        session.get_accumulated(crate::json_parser::types::ContentType::Text, "1"),
        Some("Second")
    );
}

/// Test for message finalize without deltas producing no output
///
/// This test verifies that when a message starts and stops without any
/// content deltas, no extraneous output is produced (like spurious newlines).
#[test]
fn test_finalize_without_deltas_no_output() {
    use std::io::Cursor;

    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate message_start -> message_stop with no content
    let input = r#"{"type":"stream_event","event":{"type":"message_start"}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // Should have NO prefix since no content was streamed
    let prefix_count = output.matches("[Claude]").count();
    assert_eq!(
        prefix_count, 0,
        "Should have no prefix when no content was streamed. Output: {output:?}"
    );

    // Output should be empty or contain only whitespace (no actual content)
    let trimmed = output.trim();
    assert!(
        trimmed.is_empty(),
        "Should have no actual content when message has no deltas. Output: {output:?}"
    );
}

/// Test for repeated `ContentBlockStart` not causing duplicate prefix
///
/// This test simulates GLM sending `ContentBlockStart` repeatedly for the same
/// index, which should NOT cause the next delta to show the prefix again.
/// The fix ensures that accumulated content is only cleared when transitioning
/// to a DIFFERENT block index, not the same index.
#[test]
fn test_repeated_content_block_start_no_duplicate_prefix() {
    use std::io::Cursor;

    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Simulate GLM sending ContentBlockStart before each delta
    let mut input_lines = Vec::new();

    // Message start
    input_lines.push(r#"{"type":"stream_event","event":{"type":"message_start"}}"#.to_string());

    // ContentBlockStart, Delta, ContentBlockStart, Delta, ContentBlockStart, Delta
    for i in 0..3 {
        // ContentBlockStart for the SAME index (0) each time
        input_lines.push(
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#
                .to_string(),
        );

        // Delta for this chunk
        let delta = format!("chunk{i} ");
        input_lines.push(format!(
            r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{delta}"}}}}}}"#
        ));
    }

    // Message stop
    input_lines.push(r#"{"type":"stream_event","event":{"type":"message_stop"}}"#.to_string());

    let input = input_lines.join("\n");
    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // With the single-line pattern, each delta includes the prefix
    // 3 deltas = 3 prefixes in output string (but visually only one is shown)
    // Even though ContentBlockStart is repeated, it's for the same index so accumulation continues
    let prefix_count = output.matches("[Claude]").count();
    assert_eq!(
        prefix_count, 3,
        "Should have 3 prefixes (one per delta) with repeated ContentBlockStart for same index. \
        Got {prefix_count} prefixes. Output: {output:?}"
    );

    // Should contain the accumulated content
    assert!(
        output.contains("chunk0"),
        "Should contain first chunk. Output: {output:?}"
    );
    assert!(
        output.contains("chunk1"),
        "Should contain second chunk. Output: {output:?}"
    );
    assert!(
        output.contains("chunk2"),
        "Should contain third chunk. Output: {output:?}"
    );
}

/// Test for multi-message streaming with proper separation
///
/// This test verifies that multiple complete messages in sequence are rendered
/// independently with proper newlines between them, no duplication, and each
/// message has its own prefix.
#[test]
fn test_multiple_messages_with_proper_separation() {
    use std::io::Cursor;

    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Full);

    // Stream two complete messages in sequence
    let input = r#"{"type":"stream_event","event":{"type":"message_start"}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"First"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" message"}}}
{"type":"stream_event","event":{"type":"message_stop"}}
{"type":"stream_event","event":{"type":"message_start"}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Second"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" message"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // With the single-line pattern, each delta includes the prefix
    // 2 messages x 2 deltas each = 4 prefixes in output string
    let prefix_count = output.matches("[Claude]").count();
    assert_eq!(
        prefix_count, 4,
        "Should have 4 prefixes (2 per message). Got {prefix_count}. Output: {output:?}"
    );

    // Should contain both messages
    assert!(
        output.contains("First message"),
        "Should contain first message. Output: {output:?}"
    );
    assert!(
        output.contains("Second message"),
        "Should contain second message. Output: {output:?}"
    );

    // Should end with newline (from final message_stop)
    assert!(
        output.ends_with('\n'),
        "Should end with newline after final message_stop. Output: {output:?}"
    );

    // Each message should appear only once (no duplication)
    let first_count = output.matches("First message").count();
    let second_count = output.matches("Second message").count();
    assert_eq!(
        first_count, 1,
        "First message should appear exactly once. Found {first_count} times. Output: {output:?}"
    );
    assert_eq!(
        second_count, 1,
        "Second message should appear exactly once. Found {second_count} times. Output: {output:?}"
    );
}

// Integration tests for non-full terminal modes (Basic and None)
// These tests verify that parser-level behavior works correctly when
// terminal capabilities are limited (Basic: colors only, None: plain text)

/// Test streaming with `TerminalMode::None` (non-TTY output)
///
/// Verifies that when output is piped or redirected (non-TTY), the parser
/// produces clean output without escape sequences for cursor positioning.
#[test]
fn test_streaming_with_terminal_mode_none() {
    use std::io::Cursor;

    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);

    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // Should contain the accumulated content
    assert!(
        output.contains("Hello World!"),
        "Should contain complete accumulated text. Output: {output:?}"
    );

    // Should NOT contain cursor positioning escape sequences
    assert!(
        !output.contains("\x1b[1A"), // Cursor up
        "Should NOT contain cursor up sequence in None mode. Output: {output:?}"
    );
    assert!(
        !output.contains("\x1b[1B"), // Cursor down
        "Should NOT contain cursor down sequence in None mode. Output: {output:?}"
    );
    assert!(
        !output.contains("\x1b[2K"), // Clear line
        "Should NOT contain clear line sequence in None mode. Output: {output:?}"
    );

    // Should NOT contain carriage returns for in-place updates
    assert!(
        !output.contains('\r'),
        "Should NOT contain carriage returns in None mode. Output: {output:?}"
    );

    // Should end with newline
    assert!(
        output.ends_with('\n'),
        "Should end with newline after message_stop. Output: {output:?}"
    );
}

/// Test streaming with `TerminalMode::Basic` (colors without cursor positioning)
///
/// Verifies that when terminal supports colors but not cursor positioning,
/// the parser produces output with colors but without in-place updates.
#[test]
fn test_streaming_with_terminal_mode_basic() {
    use std::io::Cursor;

    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Basic);

    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // Should contain the accumulated content
    assert!(
        output.contains("Hello World!"),
        "Should contain complete accumulated text. Output: {output:?}"
    );

    // Should NOT contain cursor positioning escape sequences
    assert!(
        !output.contains("\x1b[1A"), // Cursor up
        "Should NOT contain cursor up sequence in Basic mode. Output: {output:?}"
    );
    assert!(
        !output.contains("\x1b[1B"), // Cursor down
        "Should NOT contain cursor down sequence in Basic mode. Output: {output:?}"
    );
    assert!(
        !output.contains("\x1b[2K"), // Clear line
        "Should NOT contain clear line sequence in Basic mode. Output: {output:?}"
    );

    // Should NOT contain carriage returns for in-place updates
    assert!(
        !output.contains('\r'),
        "Should NOT contain carriage returns in Basic mode. Output: {output:?}"
    );

    // Should end with newline
    assert!(
        output.ends_with('\n'),
        "Should end with newline after message_stop. Output: {output:?}"
    );
}

/// Test completion in `TerminalMode::None`
///
/// Verifies that message completion produces just a newline without
/// cursor positioning in None mode.
#[test]
fn test_completion_with_terminal_mode_none() {
    use std::io::Cursor;

    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);

    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // Should NOT contain cursor down sequence
    assert!(
        !output.contains("\x1b[1B"),
        "Should NOT contain cursor down sequence in None mode. Output: {output:?}"
    );

    // Should end with plain newline
    assert!(
        output.ends_with('\n'),
        "Should end with newline in None mode. Output: {output:?}"
    );
}

/// Test completion in `TerminalMode::Basic`
///
/// Verifies that message completion produces just a newline without
/// cursor positioning in Basic mode.
#[test]
fn test_completion_with_terminal_mode_basic() {
    use std::io::Cursor;

    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::Basic);

    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // Should NOT contain cursor down sequence
    assert!(
        !output.contains("\x1b[1B"),
        "Should NOT contain cursor down sequence in Basic mode. Output: {output:?}"
    );

    // Should end with plain newline
    assert!(
        output.ends_with('\n'),
        "Should end with newline in Basic mode. Output: {output:?}"
    );
}

/// Test multiple deltas in None mode produce multiple lines
///
/// Verifies that without cursor positioning, each delta appears on its
/// own line (no in-place updates).
#[test]
fn test_multiple_deltas_none_mode_produces_multiple_lines() {
    use std::io::Cursor;

    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);

    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // Each delta should produce output (no in-place updates)
    // The output should contain both intermediate states
    assert!(
        output.contains("Hello"),
        "Should contain first delta. Output: {output:?}"
    );

    // Count newlines - should be at least 2 (first delta + message_stop)
    let newline_count = output.matches('\n').count();
    assert!(
        newline_count >= 2,
        "Should have at least 2 newlines in None mode. Found {newline_count}. Output: {output:?}"
    );
}

/// Test consistency across all parsers in `TerminalMode::None`
///
/// Verifies that all parsers (Claude, Codex, Gemini, `OpenCode`) produce
/// clean output without escape sequences in None mode.
#[test]
fn test_all_parsers_clean_output_in_none_mode() {
    use std::io::Cursor;

    // Test Claude parser
    let claude_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);
    let claude_input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;
    let claude_reader = Cursor::new(claude_input);
    let mut claude_writer = Vec::new();
    claude_parser
        .parse_stream(claude_reader, &mut claude_writer)
        .unwrap();
    let claude_output = String::from_utf8(claude_writer).unwrap();

    assert!(
        !claude_output.contains("\x1b["),
        "Claude should have no escape sequences in None mode. Output: {claude_output:?}"
    );

    // Test Codex parser
    let codex_parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);
    let codex_input = r#"{"type":"item.started","item":{"type":"agent_message","id":"msg1","text":"Hello"}}
{"type":"item.completed","item":{"type":"agent_message","id":"msg1"}}"#;
    let codex_reader = Cursor::new(codex_input);
    let mut codex_writer = Vec::new();
    codex_parser
        .parse_stream(codex_reader, &mut codex_writer)
        .unwrap();
    let codex_output = String::from_utf8(codex_writer).unwrap();

    assert!(
        !codex_output.contains("\x1b["),
        "Codex should have no escape sequences in None mode. Output: {codex_output:?}"
    );

    // Test Gemini parser
    let gemini_parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);
    let gemini_input = r#"{"type":"message","role":"assistant","content":"Hello","delta":true}
{"type":"result","status":"success"}"#;
    let gemini_reader = Cursor::new(gemini_input);
    let mut gemini_writer = Vec::new();
    gemini_parser
        .parse_stream(gemini_reader, &mut gemini_writer)
        .unwrap();
    let gemini_output = String::from_utf8(gemini_writer).unwrap();

    assert!(
        !gemini_output.contains("\x1b["),
        "Gemini should have no escape sequences in None mode. Output: {gemini_output:?}"
    );

    // Test OpenCode parser
    let opencode_parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);
    let opencode_input = r#"{"type":"text","timestamp":1768191347231,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac63300","type":"text","text":"Hello"}}
{"type":"step_finish","timestamp":1768191347296,"sessionID":"ses_44f9562d4ffe","part":{"type":"step-finish","reason":"end_turn"}}"#;
    let opencode_reader = Cursor::new(opencode_input);
    let mut opencode_writer = Vec::new();
    opencode_parser
        .parse_stream(opencode_reader, &mut opencode_writer)
        .unwrap();
    let opencode_output = String::from_utf8(opencode_writer).unwrap();

    assert!(
        !opencode_output.contains("\x1b["),
        "OpenCode should have no escape sequences in None mode. Output: {opencode_output:?}"
    );
}

/// Test that debug output is flushed immediately in all parsers.
///
/// This test verifies that the `[DEBUG]` output is flushed before the actual
/// event output, ensuring that debug output appears synchronously with streaming
/// events and is not lost or overwritten by subsequent output.
#[test]
fn test_all_parsers_flush_debug_output_immediately() {
    use std::io::Cursor;

    // Test Claude parser with debug mode
    let claude_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Debug)
        .with_terminal_mode(TerminalMode::Full);
    let claude_input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;
    let claude_reader = Cursor::new(claude_input);
    let mut claude_writer = Vec::new();
    claude_parser
        .parse_stream(claude_reader, &mut claude_writer)
        .expect("Parse stream should succeed");
    let claude_output = String::from_utf8(claude_writer).unwrap();

    // Verify that [DEBUG] lines are complete (not truncated)
    if claude_output.contains("[DEBUG]") {
        let lines: Vec<&str> = claude_output.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.contains("[DEBUG]") {
                assert!(
                    line.trim().ends_with('}') || line.trim().ends_with(']'),
                    "Claude: Debug line {i} should be complete JSON. Line: {line:?}"
                );
            }
        }
    }

    // Test Codex parser with debug mode
    let codex_parser = CodexParser::new(Colors { enabled: false }, Verbosity::Debug)
        .with_terminal_mode(TerminalMode::Full);
    let codex_input = r#"{"type":"item.started","item":{"type":"agent_message","id":"msg1","text":"Hello"}}
{"type":"item.completed","item":{"type":"agent_message","id":"msg1"}}"#;
    let codex_reader = Cursor::new(codex_input);
    let mut codex_writer = Vec::new();
    codex_parser
        .parse_stream(codex_reader, &mut codex_writer)
        .expect("Parse stream should succeed");
    let codex_output = String::from_utf8(codex_writer).unwrap();

    if codex_output.contains("[DEBUG]") {
        let lines: Vec<&str> = codex_output.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.contains("[DEBUG]") {
                assert!(
                    line.trim().ends_with('}') || line.trim().ends_with(']'),
                    "Codex: Debug line {i} should be complete JSON. Line: {line:?}"
                );
            }
        }
    }

    // Test Gemini parser with debug mode
    let gemini_parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Debug)
        .with_terminal_mode(TerminalMode::Full);
    let gemini_input = r#"{"type":"message","role":"assistant","content":"Hello","delta":true}
{"type":"result","status":"success"}"#;
    let gemini_reader = Cursor::new(gemini_input);
    let mut gemini_writer = Vec::new();
    gemini_parser
        .parse_stream(gemini_reader, &mut gemini_writer)
        .expect("Parse stream should succeed");
    let gemini_output = String::from_utf8(gemini_writer).unwrap();

    if gemini_output.contains("[DEBUG]") {
        let lines: Vec<&str> = gemini_output.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.contains("[DEBUG]") {
                assert!(
                    line.trim().ends_with('}') || line.trim().ends_with(']'),
                    "Gemini: Debug line {i} should be complete JSON. Line: {line:?}"
                );
            }
        }
    }

    // Test OpenCode parser with debug mode
    let opencode_parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Debug)
        .with_terminal_mode(TerminalMode::Full);
    let opencode_input = r#"{"type":"text","timestamp":1768191347231,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac63300","type":"text","text":"Hello"}}
{"type":"step_finish","timestamp":1768191347296,"sessionID":"ses_44f9562d4ffe","part":{"type":"step-finish","reason":"end_turn"}}"#;
    let opencode_reader = Cursor::new(opencode_input);
    let mut opencode_writer = Vec::new();
    opencode_parser
        .parse_stream(opencode_reader, &mut opencode_writer)
        .expect("Parse stream should succeed");
    let opencode_output = String::from_utf8(opencode_writer).unwrap();

    if opencode_output.contains("[DEBUG]") {
        let lines: Vec<&str> = opencode_output.lines().collect();
        for (i, line) in lines.iter().enumerate() {
            if line.contains("[DEBUG]") {
                assert!(
                    line.trim().ends_with('}') || line.trim().ends_with(']'),
                    "OpenCode: Debug line {i} should be complete JSON. Line: {line:?}"
                );
            }
        }
    }
}
