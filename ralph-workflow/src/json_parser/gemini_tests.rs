//! Tests for Gemini JSON parser.

use super::*;
use crate::config::Verbosity;
use crate::logger::Colors;

#[test]
fn test_gemini_init_event() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"init","timestamp":"2025-10-10T12:00:00.000Z","session_id":"abc123","model":"gemini-2.0-flash-exp"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Session started"));
    assert!(out.contains("gemini-2.0-flash-exp"));
}

#[test]
fn test_gemini_message_assistant() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"message","role":"assistant","content":"Here are the files...","timestamp":"2025-10-10T12:00:04.000Z"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Here are the files"));
}

#[test]
fn test_gemini_message_user() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"message","role":"user","content":"List files in current directory","timestamp":"2025-10-10T12:00:01.000Z"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("user"));
    assert!(out.contains("List files"));
}

#[test]
fn test_gemini_tool_use() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"tool_use","tool_name":"Bash","tool_id":"bash-123","parameters":{"command":"ls -la"},"timestamp":"2025-10-10T12:00:02.000Z"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Tool"));
    assert!(out.contains("Bash"));
    assert!(out.contains("command=ls -la"));
}

#[test]
fn test_gemini_tool_result_success() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Verbose);
    let json = r#"{"type":"tool_result","tool_id":"bash-123","status":"success","output":"file1.txt\nfile2.txt","timestamp":"2025-10-10T12:00:03.000Z"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Tool result"));
    assert!(out.contains("file1.txt"));
}

#[test]
fn test_gemini_tool_result_error() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"tool_result","tool_id":"bash-123","status":"error","output":"command not found","timestamp":"2025-10-10T12:00:03.000Z"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Tool result"));
}

#[test]
fn test_gemini_error_event() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"error","message":"Rate limit exceeded","code":"429","timestamp":"2025-10-10T12:00:05.000Z"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Error"));
    assert!(out.contains("Rate limit exceeded"));
    assert!(out.contains("429"));
}

#[test]
fn test_gemini_result_success() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"result","status":"success","stats":{"total_tokens":250,"input_tokens":50,"output_tokens":200,"duration_ms":3000,"tool_calls":1},"timestamp":"2025-10-10T12:00:05.000Z"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("success"));
    assert!(out.contains("in:50"));
    assert!(out.contains("out:200"));
    assert!(out.contains("1 tools"));
}

#[test]
fn test_gemini_message_delta() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"message","role":"assistant","content":"Streaming","delta":true,"timestamp":"2025-10-10T12:00:04.000Z"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Streaming"));
    // Delta content displays naturally without "..." marker
}

#[test]
fn test_gemini_unknown_event() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"unknown_event_type","data":"something"}"#;
    let output = parser.parse_event(json);
    // Unknown events should return None (empty output)
    assert!(output.is_none());
}

#[test]
fn test_gemini_parser_non_json_passthrough() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
    let output = parser.parse_event("Warning: rate limit approaching");
    assert!(output.is_some());
    assert!(output.unwrap().contains("Warning: rate limit approaching"));
}

/// Test that `with_terminal_mode` method works correctly
#[test]
fn test_with_terminal_mode() {
    use crate::json_parser::terminal::TerminalMode;

    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);

    // Verify the parser was created successfully
    let json = r#"{"type":"message","role":"assistant","content":"Hello"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
}
