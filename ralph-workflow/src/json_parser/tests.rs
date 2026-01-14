//! Tests for JSON parsers.

use super::*;
use crate::colors::Colors;
use crate::config::Verbosity;
use std::cell::RefCell;
use std::io::{self, Cursor, Write};

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
    let verbose_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Verbose);
    let json = r#"{"type":"assistant","message":{"content":[{"type":"tool_use","name":"Read","input":{"file_path":"/test.rs"}}]}}"#;
    let output = verbose_parser.parse_event(json).unwrap();
    assert!(output.contains("Tool"));
    assert!(output.contains("Read"));
    assert!(output.contains("file_path=/test.rs"));
}

#[test]
fn test_tool_use_shows_input_in_normal_mode() {
    // Tool inputs are now shown at Normal level for better usability
    let normal_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
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
        .with_display_name("ccs-glm");
    let json = r#"{"type":"system","subtype":"init","session_id":"abc123"}"#;
    let output = parser.parse_event(json).unwrap();
    assert!(output.contains("[ccs-glm]"));
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
fn test_parse_opencode_tool_output_object_payload() {
    let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Verbose);
    let json = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"/test.rs"},"output":{"ok":true,"bytes":123}}}}"#;
    let output = parser.parse_event(json).unwrap();
    assert!(output.contains("Output"));
    assert!(output.contains("ok"));
}

#[test]
fn test_debug_verbosity_is_recognized() {
    let debug_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Debug);
    // Debug mode should be detectable via is_debug()
    assert!(debug_parser.verbosity.is_debug());
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

// Gemini parser tests
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

// Tests for JSON parser robustness - malformed line handling

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

#[test]
fn test_codex_parser_non_json_passthrough() {
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
    let output = parser.parse_event("Error: something went wrong");
    assert!(output.is_some());
    assert!(output.unwrap().contains("Error: something went wrong"));
}

#[test]
fn test_gemini_parser_non_json_passthrough() {
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
    let output = parser.parse_event("Warning: rate limit approaching");
    assert!(output.is_some());
    assert!(output.unwrap().contains("Warning: rate limit approaching"));
}

// Test for DeltaAccumulator
#[test]
fn test_delta_accumulator_text() {
    let mut acc = super::types::DeltaAccumulator::new();
    acc.add_text_delta(0, "Hello, ");
    acc.add_text_delta(0, "World!");

    assert_eq!(
        acc.get(super::types::ContentType::Text, "0"),
        Some("Hello, World!")
    );
    assert!(!acc.is_empty());
}

#[test]
fn test_delta_accumulator_thinking() {
    let mut acc = super::types::DeltaAccumulator::new();
    acc.add_thinking_delta(0, "Let me think...");
    acc.add_thinking_delta(0, " Done.");

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
    acc.add_text_delta(0, "Some text");
    assert!(!acc.is_empty());

    acc.clear();
    assert!(acc.is_empty());
    assert_eq!(acc.get(super::types::ContentType::Text, "0"), None);
}

#[test]
fn test_delta_accumulator_clear_key() {
    let mut acc = super::types::DeltaAccumulator::new();
    acc.add_text_delta(0, "Text 0");
    acc.add_text_delta(1, "Text 1");

    acc.clear_key(super::types::ContentType::Text, "0");
    assert_eq!(acc.get(super::types::ContentType::Text, "0"), None);
    assert_eq!(
        acc.get(super::types::ContentType::Text, "1"),
        Some("Text 1")
    );
}

#[test]
fn test_format_unknown_json_event_control_event() {
    let colors = Colors { enabled: false };
    // Control events should not show output even in verbose mode
    let json = r#"{"type":"message_start","message":{"id":"msg_123"}}"#;
    let output = super::types::format_unknown_json_event(json, "Claude", &colors, true);
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
    let output = super::types::format_unknown_json_event(json, "Claude", &colors, false);
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
    let output = super::types::format_unknown_json_event(json, "Claude", &colors, true);
    // Should show "Partial event:" in verbose mode
    assert!(!output.is_empty());
    assert!(output.contains("Partial event"));
}

#[test]
fn test_format_unknown_json_event_complete_event_verbose() {
    let colors = Colors { enabled: false };
    // Complete events only show in verbose mode
    let json = r#"{"type":"message","content":"This is a complete message with substantial content that should be displayed as is."}"#;
    let output = super::types::format_unknown_json_event(json, "Claude", &colors, true);
    assert!(!output.is_empty());
    assert!(output.contains("Complete event"));
}

#[test]
fn test_format_unknown_json_event_complete_event_normal() {
    let colors = Colors { enabled: false };
    // Complete events should not show in non-verbose mode if no explicit content
    let json = r#"{"type":"status","status":"ok"}"#;
    let output = super::types::format_unknown_json_event(json, "Claude", &colors, false);
    // Should return empty in non-verbose mode for complete events without content
    assert!(output.is_empty());
}

#[test]
fn test_format_unknown_json_event_with_explicit_delta_flag() {
    let colors = Colors { enabled: false };
    // Events with explicit delta: true should be detected as partial
    let json = r#"{"type":"message","delta":true,"content":"partial content"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, true);
    assert!(!output.is_empty());
    assert!(output.contains("Partial event"));
}

#[test]
fn test_format_unknown_json_event_error_control() {
    let colors = Colors { enabled: false };
    // Error events are control events and should not show
    let json = r#"{"type":"error","message":"Something went wrong"}"#;
    let output = super::types::format_unknown_json_event(json, "Claude", &colors, true);
    // Error events are control events - should return empty
    assert!(output.is_empty());
}

#[test]
fn test_format_unknown_json_event_delta_shows_content_in_normal_mode() {
    let colors = Colors { enabled: false };
    // Delta events should show their content even in non-verbose mode
    let json = r#"{"type":"content_block_delta","delta":{"text":"Hello World"}}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
    // Should show the delta content in normal mode
    assert!(!output.is_empty());
    assert!(output.contains("Hello World"));
}

#[test]
fn test_format_unknown_json_event_partial_with_delta_field() {
    let colors = Colors { enabled: false };
    // Events with delta field should show content even without explicit type name
    let json = r#"{"type":"chunk","delta":"some partial content"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
    // Should show content because delta field is present
    assert!(!output.is_empty());
    assert!(output.contains("some partial content"));
}

#[test]
fn test_format_unknown_json_event_partial_with_text_field() {
    let colors = Colors { enabled: false };
    // Partial events with text field should show content
    let json = r#"{"type":"partial","text":"streaming text here"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
    // Should show content
    assert!(!output.is_empty());
    assert!(output.contains("streaming text here"));
}

#[test]
fn test_format_unknown_json_event_nested_delta_text() {
    let colors = Colors { enabled: false };
    // Nested delta.text should be extracted
    let json = r#"{"type":"update","delta":{"text":"nested content"}}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
    // Should show the nested delta text content
    assert!(!output.is_empty());
    assert!(output.contains("nested content"));
}

// Edge case tests for format_unknown_json_event

#[test]
fn test_format_unknown_json_event_deeply_nested_content() {
    let colors = Colors { enabled: false };
    // Test deeply nested content extraction
    // The current implementation extracts from delta.text or content fields
    // but not from arbitrary nested paths
    let json = r#"{"type":"delta","delta":{"text":"deep nested text"}}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
    // Should extract content from nested delta.text
    assert!(!output.is_empty());
    assert!(output.contains("deep nested text"));
}

#[test]
fn test_format_unknown_json_event_array_content() {
    let colors = Colors { enabled: false };
    // Test content extraction from arrays
    let json = r#"{"type":"message","content":["item1","item2","item3"]}"#;
    let _output = super::types::format_unknown_json_event(json, "Test", &colors, false);
    // Arrays should be handled gracefully (content field exists but isn't a string)
    // The function should not crash
}

#[test]
fn test_format_unknown_json_event_empty_delta() {
    let colors = Colors { enabled: false };
    // Test delta with empty string content
    let json = r#"{"type":"content_block_delta","delta":{"text":""}}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
    // Empty content should not show output
    assert!(output.is_empty() || output.trim().is_empty());
}

#[test]
fn test_format_unknown_json_event_whitespace_only_delta() {
    let colors = Colors { enabled: false };
    // Test delta with whitespace-only content
    let json = r#"{"type":"content_block_delta","delta":{"text":"   "}}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
    // Whitespace-only content should not show output
    assert!(output.is_empty() || output.trim().is_empty());
}

#[test]
fn test_format_unknown_json_event_unicode_delta_content() {
    let colors = Colors { enabled: false };
    // Test Unicode content in delta
    let json = r#"{"type":"delta","text":"Hello 世界 🌍"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
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
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
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
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
    // Null content should be handled gracefully
    assert!(output.is_empty());
}

#[test]
fn test_format_unknown_json_event_numeric_content_field() {
    let colors = Colors { enabled: false };
    // Test event with numeric content (not a string)
    let json = r#"{"type":"metric","content":12345}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, true);
    // Numeric content should be shown in verbose mode
    assert!(!output.is_empty());
}

#[test]
fn test_format_unknown_json_event_boolean_delta_flag() {
    let colors = Colors { enabled: false };
    // Test explicit boolean delta flag
    let json = r#"{"type":"chunk","delta":true,"content":"test"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
    // Should detect delta: true and show content
    assert!(!output.is_empty());
    assert!(output.contains("test"));
}

#[test]
fn test_format_unknown_json_event_special_characters_in_content() {
    let colors = Colors { enabled: false };
    // Test special characters that might cause issues
    let json = r#"{"type":"delta","text":"Line1\nLine2\tTabbed\"Quoted"}"#;
    let output = super::types::format_unknown_json_event(json, "Test", &colors, false);
    // Should handle special characters properly
    assert!(!output.is_empty());
}

#[test]
fn test_format_unknown_json_event_very_long_content() {
    let colors = Colors { enabled: false };
    // Test very long content doesn't cause issues
    let long_text = "a".repeat(10000);
    let json = format!(r#"{{"type":"delta","text":"{long_text}"}}"#);
    let output = super::types::format_unknown_json_event(&json, "Test", &colors, false);
    // Should handle long content without crashing
    assert!(!output.is_empty());
    // Content should be shown in full for deltas (not truncated)
}

// Tests for stream classifier edge cases

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

// Tests for partial event tracking in health monitoring

#[test]
fn test_claude_parser_tracks_partial_events_in_health_monitoring() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);

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
    use crate::colors::Colors;

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
    use crate::colors::Colors;

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

    // Reset and test with actual parse errors
    monitor.reset();

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

// Test for verbose mode streaming fix - ensures accumulated text output
#[test]
fn test_verbose_mode_streaming_no_duplicate_lines() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Verbose);

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

    // After the fix, streaming should show accumulated text on the same line:
    // [Claude] warning: unu\r           (first chunk with prefix)
    // warning: unused\r                (second chunk overwriting with accumulated text)
    // warning: unused vari\r            (third chunk overwriting with accumulated text)
    // warning: unused variable\n        (final chunk + message_stop adds newline)

    // The output should contain carriage returns for overwriting
    assert!(
        output.contains('\r'),
        "Should contain carriage returns for overwriting"
    );

    // Should only have ONE prefix (not multiple)
    let prefix_count = output.matches("[Claude]").count();
    assert_eq!(
        prefix_count, 1,
        "Should have exactly 1 prefix, not multiple duplicates"
    );

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

// Test that normal and verbose mode show the same delta content
#[test]
fn test_normal_and_verbose_mode_show_same_deltas() {
    let normal_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
    let verbose_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Verbose);

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

// Regression test for delta text with embedded newlines
// Ensures that newlines within delta text don't cause artificial line breaks
// that would result in duplicate prefixes being added to each line
#[test]
fn test_delta_with_embedded_newline_displays_inline() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);

    // Simulate a delta that contains a newline character within the text
    // For example: "Now I understand\n1. In src/..."
    let json = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Now I understand\n1. In src/"}}}"#;

    let output = parser.parse_event(json);

    assert!(output.is_some());
    let out = output.unwrap();

    // The newline should be replaced with a space to prevent artificial line breaks
    // This ensures we don't get duplicate prefixes like:
    // [Claude] Now I understand
    // [Claude] 1. In src/
    // Instead we should get a single line:
    // [Claude] Now I understand 1. In src/
    assert!(out.contains("Now I understand"));
    assert!(out.contains("1. In src/"));

    // Verify that the output doesn't have an actual newline in the delta text portion
    // (there should only be one line from the prefix+text, not two)
    assert_eq!(
        out.lines().count(),
        1,
        "Delta with embedded newline should produce a single output line"
    );
}

// Integration test for real-time streaming behavior
// Verifies that flush() is called after each streaming write to ensure
// output is displayed immediately rather than being buffered

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
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);

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
    let parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);

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
    let parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);

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

#[test]
fn test_opencode_streaming_flushes_after_write() {
    let parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);

    // Simulate streaming tool_use events
    let input = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool":"read","state":{"status":"started","input":{"filePath":"/test.rs"}}}}
{"type":"tool_use","timestamp":1768191346713,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","type":"tool","tool","read","state":{"status":"completed","input":{"filePath":"/test.rs"}}}}"#;

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
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Verbose);

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

    // Should only have ONE prefix at the start
    let prefix_count = output.matches("[Claude]").count();
    assert_eq!(
        prefix_count, 1,
        "Should have exactly 1 prefix, not multiple duplicates"
    );

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
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);

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
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);

    // Single chunk scenario - content arrives all at once
    let input = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Complete message"}}}
{"type":"stream_event","event":{"type":"message_stop"}}"#;

    let reader = Cursor::new(input);
    let mut writer = Vec::new();

    parser.parse_stream(reader, &mut writer).unwrap();
    let output = String::from_utf8(writer).unwrap();

    // Should have exactly one prefix
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
#[test]
fn test_streaming_very_long_text() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);

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
    // Should contain the accumulated text (streaming accumulates before truncation)
    assert!(
        output.len() >= long_chunk.len(),
        "Output should contain at least the first chunk"
    );
}

/// Test streaming with special characters in text
/// Verifies that special characters (quotes, unicode, etc.) are handled correctly
#[test]
fn test_streaming_special_characters() {
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);

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
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);

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

    // Should have exactly one prefix despite many chunks
    let prefix_count = output.matches("[Claude]").count();
    assert_eq!(prefix_count, 1, "Rapid chunks should have exactly 1 prefix");

    // Should contain carriage returns for overwriting
    assert!(
        output.contains('\r'),
        "Rapid chunks should use carriage returns"
    );

    // Verify content from multiple chunks is present
    assert!(output.contains("chunk0"), "Should contain first chunk");
    assert!(output.contains("chunk9"), "Should contain last chunk");
}

/// Test streaming with only whitespace chunks
/// Verifies that whitespace-only chunks don't produce spurious output
#[test]
fn test_streaming_whitespace_only_chunks() {
    use std::io::Cursor;
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);

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
    let parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);

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
/// Verifies that all parsers (`Claude`, `Codex`, `Gemini`, `OpenCode`) handle streaming consistently
#[test]
fn test_streaming_consistency_across_parsers() {
    use std::io::Cursor;

    // Test Claude parser
    let claude_parser = ClaudeParser::new(Colors { enabled: false }, Verbosity::Normal);
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
    assert_eq!(
        claude_output.matches("[Claude]").count(),
        1,
        "Claude should have 1 prefix"
    );

    // Test Codex parser
    // Note: Codex shows prefix on first item.started AND on item.completed (2 prefixes total)
    // This is different from Claude which shows prefix only at the start
    let codex_parser = CodexParser::new(Colors { enabled: false }, Verbosity::Normal);
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
    // Codex shows prefix on first item.started and on item.completed (2 prefixes)
    assert_eq!(
        codex_output.matches("[Codex]").count(),
        2,
        "Codex shows prefix on start and completion"
    );

    // Test Gemini parser
    // Note: Gemini shows prefix on first delta AND on final non-delta message (2 prefixes total)
    let gemini_parser = GeminiParser::new(Colors { enabled: false }, Verbosity::Normal);
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
    // Gemini shows prefix on first delta and on final non-delta message (2 prefixes)
    assert_eq!(
        gemini_output.matches("[Gemini]").count(),
        2,
        "Gemini shows prefix on first delta and final message"
    );

    // Test OpenCode parser
    // Note: OpenCode shows prefix on first text event AND on step_finish (2 prefixes total)
    let opencode_parser = OpenCodeParser::new(Colors { enabled: false }, Verbosity::Normal);
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
    // OpenCode shows prefix on first text event and on step_finish (2 prefixes)
    assert_eq!(
        opencode_output.matches("[OpenCode]").count(),
        2,
        "OpenCode shows prefix on first text and step_finish"
    );
}
