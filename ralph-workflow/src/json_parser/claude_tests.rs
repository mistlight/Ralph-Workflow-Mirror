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
