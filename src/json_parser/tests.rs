//! Tests for JSON parsers.

use super::*;
use crate::colors::Colors;
use crate::config::Verbosity;

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
        r#"{{"type":"assistant","message":{{"content":[{{"type":"text","text":"{}"}}]}}}}"#,
        long_text
    );

    let quiet_output = quiet_parser.parse_event(&json).unwrap();
    let full_output = full_parser.parse_event(&json).unwrap();

    // Quiet output should be truncated (shorter)
    assert!(quiet_output.len() < full_output.len());
}

#[test]
fn test_format_tool_input_object() {
    let input = serde_json::json!({
        "file_path": "/path/to/file.rs",
        "content": "hello world"
    });
    let result = format_tool_input(&input);
    assert!(result.contains("file_path=/path/to/file.rs"));
    assert!(result.contains("content=hello world"));
}

#[test]
fn test_format_tool_input_truncates_long_strings() {
    let long_content = "x".repeat(150);
    let input = serde_json::json!({
        "content": long_content
    });
    let result = format_tool_input(&input);
    assert!(result.contains("..."));
    assert!(result.len() < 150);
}

#[test]
fn test_format_tool_input_handles_arrays() {
    let input = serde_json::json!({
        "files": ["a.rs", "b.rs", "c.rs"]
    });
    let result = format_tool_input(&input);
    assert!(result.contains("files=[3 items]"));
}

#[test]
fn test_format_tool_input_handles_nested_objects() {
    let input = serde_json::json!({
        "options": {"key": "value"}
    });
    let result = format_tool_input(&input);
    assert!(result.contains("options={...}"));
}

#[test]
fn test_format_tool_input_redacts_sensitive_keys() {
    let fake_key = format!("sk-{}", "a".repeat(24));
    let input = serde_json::json!({
        "api_key": fake_key,
        "access_token": format!("{}{}", "sk-", "b".repeat(24)),
        "Authorization": format!("Bearer {}", format!("{}{}", "sk-", "c".repeat(24))),
        "file_path": "/safe/path.rs"
    });
    let result = format_tool_input(&input);
    assert!(result.contains("api_key=<redacted>"));
    assert!(result.contains("access_token=<redacted>"));
    assert!(result.contains("Authorization=<redacted>"));
    assert!(result.contains("file_path=/safe/path.rs"));
    assert!(!result.contains("sk-"));
}

#[test]
fn test_format_tool_input_redacts_secret_like_string_values() {
    let fake_key = format!("{}{}", "sk-", "d".repeat(24));
    let input = serde_json::json!({
        "query": format!("please use {} for this", fake_key)
    });
    let result = format_tool_input(&input);
    assert!(result.contains("query=<redacted>"));
    assert!(!result.contains("sk-"));
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
fn test_format_tool_input_unicode_safe() {
    // Ensure Unicode characters don't cause panics
    let unicode_content = "日本語".to_string() + &"x".repeat(200);
    let input = serde_json::json!({
        "content": unicode_content
    });
    // Should not panic and should truncate properly
    let result = format_tool_input(&input);
    assert!(result.contains("..."));
    assert!(result.contains("日本語"));
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
    assert!(out.contains("..."));
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
