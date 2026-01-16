//! Tests for Codex JSON parser.

use super::*;
use crate::config::Verbosity;
use crate::logger::Colors;

#[test]
fn test_parse_codex_thread_started() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"thread.started","thread_id":"xyz789"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    assert!(output.unwrap().contains("Thread started"));
}

#[test]
fn test_parse_codex_turn_completed() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"turn.completed","usage":{"input_tokens":100,"output_tokens":50}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    assert!(output.unwrap().contains("Turn completed"));
}

#[test]
fn test_codex_file_operations_shown() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Verbose);
    let json = r#"{"type":"item.started","item":{"type":"file_read","path":"/src/main.rs"}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("file_read"));
    assert!(out.contains("/src/main.rs"));
}

#[test]
fn test_codex_reasoning_event() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Verbose);
    let json = r#"{"type":"item.started","item":{"type":"reasoning","id":"item_1"}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    assert!(output.unwrap().contains("Reasoning"));
}

#[test]
fn test_codex_reasoning_completed_shows_text() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Verbose);
    let json = r#"{"type":"item.completed","item":{"type":"reasoning","id":"item_1","text":"I should analyze this file first"}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Thought"));
    assert!(out.contains("analyze"));
}

#[test]
fn test_codex_mcp_tool_call() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"item.started","item":{"type":"mcp_tool_call","tool":"search_files","arguments":{"query":"main"}}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("MCP Tool"));
    assert!(out.contains("search_files"));
    assert!(out.contains("query=main"));
}

#[test]
fn test_codex_web_search() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json =
        r#"{"type":"item.started","item":{"type":"web_search","query":"rust async tutorial"}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Search"));
    assert!(out.contains("rust async tutorial"));
}

#[test]
fn test_codex_plan_update() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Verbose);
    let json = r#"{"type":"item.started","item":{"type":"plan_update","id":"item_1"}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    assert!(output.unwrap().contains("Updating plan"));
}

#[test]
fn test_codex_turn_completed_with_cached_tokens() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"turn.completed","usage":{"input_tokens":24763,"cached_input_tokens":24448,"output_tokens":122}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Turn completed"));
    assert!(out.contains("in:24763"));
    assert!(out.contains("out:122"));
}

#[test]
fn test_codex_item_with_status() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"item.started","item":{"id":"item_1","type":"command_execution","command":"ls","status":"in_progress"}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Exec"));
    assert!(out.contains("ls"));
}

#[test]
fn test_codex_file_write_completed() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"item.completed","item":{"type":"file_write","path":"/src/main.rs"}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("File"));
    assert!(out.contains("/src/main.rs"));
}

#[test]
fn test_codex_mcp_completed() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"item.completed","item":{"type":"mcp_tool_call","tool":"read_file"}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("MCP"));
    assert!(out.contains("read_file"));
    assert!(out.contains("done"));
}

#[test]
fn test_codex_web_search_completed() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"item.completed","item":{"type":"web_search"}}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
    assert!(output.unwrap().contains("Search completed"));
}

#[test]
fn test_codex_parser_non_json_passthrough() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let output = parser.parse_event("Error: something went wrong");
    assert!(output.is_some());
    assert!(output.unwrap().contains("Error: something went wrong"));
}

/// Test that `with_terminal_mode` method works correctly
#[test]
#[cfg(feature = "test-utils")]
fn test_with_terminal_mode() {
    use crate::json_parser::terminal::TerminalMode;

    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);

    // Verify the parser was created successfully
    let json = r#"{"type":"thread.started","thread_id":"test123"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
}
