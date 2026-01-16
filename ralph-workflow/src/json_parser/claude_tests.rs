//! Tests for Claude JSON parser.

use super::*;
use crate::config::Verbosity;
use crate::logger::Colors;

#[test]
fn test_parse_claude_system_init() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"system","subtype":"init","session_id":"abc123"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    assert!(output.unwrap().contains("Session started"));
}

#[test]
fn test_parse_claude_result_success() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"result","subtype":"success","duration_ms":60000,"num_turns":5,"total_cost_usd":0.05}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    assert!(output.unwrap().contains("Completed"));
}

#[test]
fn test_parse_claude_tool_result_object_payload() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_result","content":{"ok":true,"n":1}}]}}"#;
    let output = parser.parse_event(json).unwrap();
    assert!(output.contains("Result"));
    assert!(output.contains("ok"));
}

#[test]
fn test_parse_claude_text_with_unicode() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json =
        r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello 世界! 🌍"}]}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Hello 世界! 🌍"));
}

#[test]
fn test_claude_parser_non_json_passthrough() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
    // Plain text that isn't JSON should be passed through
    let output = parser.parse_event("Hello, this is plain text output");
    assert!(output.is_some());
    assert!(output.unwrap().contains("Hello, this is plain text output"));
}

#[test]
fn test_claude_parser_malformed_json_ignored() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
    // Malformed JSON that looks like JSON should be ignored
    let output = parser.parse_event("{invalid json here}");
    assert!(output.is_none());
}

#[test]
fn test_claude_parser_empty_line_ignored() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
    let output = parser.parse_event("");
    assert!(output.is_none());
    let output2 = parser.parse_event("   ");
    assert!(output2.is_none());
}

/// Test that `content_block_stop` events don't produce blank lines
///
/// This test verifies the fix for ccs-glm blank line issue where
/// `content_block_stop` events were being treated as Unknown events
/// and producing blank output.
#[test]
fn test_content_block_stop_no_output() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#;
    let output = parser.parse_event(json);
    assert!(
        output.is_none(),
        "content_block_stop should produce no output"
    );
}

/// Test that `message_delta` events don't produce blank lines
///
/// This test verifies the fix for ccs-glm blank line issue where
/// `message_delta` events were being treated as Unknown events
/// and producing blank output.
#[test]
fn test_message_delta_no_output() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"end_turn","stop_sequence":null},"usage":{"input_tokens":100,"output_tokens":50}}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_none(), "message_delta should produce no output");
}

/// Test that `content_block_stop` with no index is handled
#[test]
fn test_content_block_stop_no_index() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"stream_event","event":{"type":"content_block_stop"}}"#;
    let output = parser.parse_event(json);
    assert!(
        output.is_none(),
        "content_block_stop without index should produce no output"
    );
}

/// Test complete ccs-glm event sequence
///
/// This test verifies that a typical ccs-glm streaming sequence
/// doesn't produce blank lines from control events.
#[test]
fn test_ccs_glm_event_sequence() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);

    // System init
    let json1 = r#"{"type":"system","subtype":"init","session_id":"test123"}"#;
    let output1 = parser.parse_event(json1);
    assert!(output1.is_some());

    // Message start
    let json2 = r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_123","type":"message","role":"assistant"}}}"#;
    let output2 = parser.parse_event(json2);
    assert!(output2.is_none(), "message_start should produce no output");

    // Content block start
    let json3 = r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#;
    let output3 = parser.parse_event(json3);
    assert!(
        output3.is_none(),
        "content_block_start should produce no output"
    );

    // Content block delta with text
    let json4 = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#;
    let output4 = parser.parse_event(json4);
    assert!(
        output4.is_some(),
        "content_block_delta with text should produce output"
    );

    // Content block stop - should not produce blank line
    let json5 = r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#;
    let output5 = parser.parse_event(json5);
    assert!(
        output5.is_none(),
        "content_block_stop should produce no output"
    );

    // Message delta - should not produce blank line
    let json6 = r#"{"type":"stream_event","event":{"type":"message_delta","delta":{"stop_reason":"end_turn"},"usage":{"input_tokens":100,"output_tokens":5}}}"#;
    let output6 = parser.parse_event(json6);
    assert!(output6.is_none(), "message_delta should produce no output");

    // Message stop
    let json7 = r#"{"type":"stream_event","event":{"type":"message_stop"}}"#;
    let output7 = parser.parse_event(json7);
    // Message stop should produce output (final newline) since we had content
    assert!(
        output7.is_some(),
        "message_stop should produce output after content"
    );
}

/// Test that `with_terminal_mode` method works correctly
#[test]
fn test_with_terminal_mode() {
    use crate::json_parser::terminal::TerminalMode;

    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);

    // Verify the parser was created successfully
    let json = r#"{"type":"assistant","message":{"content":[{"type":"text","text":"Hello"}]}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
}
