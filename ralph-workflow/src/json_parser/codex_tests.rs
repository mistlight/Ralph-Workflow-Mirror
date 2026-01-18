//! Tests for Codex JSON parser.

use super::*;
use crate::config::Verbosity;
use crate::files::result_extraction::extract_last_result;
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
#[cfg(test)]
fn test_with_terminal_mode() {
    use crate::json_parser::terminal::TerminalMode;

    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_terminal_mode(TerminalMode::None);

    // Verify the parser was created successfully
    let json = r#"{"type":"thread.started","thread_id":"test123"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_some());
}

/// Test that Codex parser handles Result events correctly.
///
/// Result events are synthetic events written by the parser (not from Codex CLI itself)
/// to enable content extraction. This test verifies that the Result event variant
/// is properly handled.
#[test]
fn test_codex_result_event() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json =
        r#"{"type":"result","result":"This is the accumulated content from agent_message items"}"#;
    let output = parser.parse_event(json);
    // Result events are control events that don't produce output in normal mode
    assert!(output.is_none() || output.unwrap().is_empty());
}

/// Test that Codex parser handles Result events in debug mode.
///
/// In debug mode, result events should be displayed for troubleshooting.
#[test]
fn test_codex_result_event_debug_mode() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Debug);
    let json = r#"{"type":"result","result":"Debug result content"}"#;
    let output = parser.parse_event(json);
    // In debug mode, result events should be shown
    assert!(output.is_some());
    let out = output.unwrap();
    assert!(out.contains("Debug result content"));
}

/// Test that synthetic result events can be extracted from log files.
///
/// This test simulates the full flow: Codex parser writes events to log file,
/// including the synthetic result event, and then extraction retrieves it.
#[test]
fn test_codex_synthetic_result_event_extraction() {
    use std::io::Write;

    let temp_dir = tempfile::TempDir::new().unwrap();
    let log_path = temp_dir.path().join("codex_test.log");

    // Create a Codex parser with log file
    let _parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal)
        .with_log_file(log_path.to_str().unwrap());

    // Simulate Codex events being written to the log file
    let mut file = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_path)
        .unwrap();

    // Write typical Codex events
    writeln!(
        file,
        r#"{{"type":"thread.started","thread_id":"thread123"}}"#
    )
    .unwrap();
    writeln!(file, r#"{{"type":"turn.started"}}"#).unwrap();

    // Write agent_message items (simulating streaming content)
    writeln!(
        file,
        r#"{{"type":"item.started","item":{{"type":"agent_message","text":"I'll craft a prioritized checklist"}}}}"#
    )
    .unwrap();
    writeln!(
        file,
        r#"{{"type":"item.completed","item":{{"type":"agent_message","text":"I'll craft a prioritized checklist with about ten items"}}}}"#
    )
    .unwrap();

    // Write turn completed event
    writeln!(
        file,
        r#"{{"type":"turn.completed","usage":{{"input_tokens":100,"output_tokens":50}}}}"#
    )
    .unwrap();

    // The parser should have written a synthetic result event
    // Let's manually write one to simulate what the parser does
    writeln!(
        file,
        r#"{{"type":"result","result":"I'll craft a prioritized checklist with about ten items across four levels."}}"#
    )
    .unwrap();

    drop(file);

    // Verify extraction can find the result event
    let result = extract_last_result(&log_path).unwrap();
    assert!(
        result.is_some(),
        "Expected to find result event from Codex parser"
    );
    let result_content = result.unwrap();
    assert!(result_content.contains("prioritized checklist"));
}

/// Test that Codex parser correctly identifies Result events as control events.
///
/// Control events are state management events that don't produce user output.
#[test]
fn test_codex_result_event_is_control_event() {
    // Result events should be classified as control events
    // This means they won't produce output in normal mode
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let json = r#"{"type":"result","result":"test content"}"#;
    let output = parser.parse_event(json);
    assert!(output.is_none() || output.unwrap().is_empty());
}
