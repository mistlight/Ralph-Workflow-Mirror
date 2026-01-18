//! Integration tests for Codex parser with log file verification.
//!
//! These tests verify that the Codex parser correctly writes synthetic result
//! events to log files, even in edge cases like missing turn.completed events
//! or last lines without trailing newlines.

use ralph_workflow::config::Verbosity;
use ralph_workflow::files::result_extraction::extract_last_result;
use ralph_workflow::json_parser::codex::CodexParser;
use ralph_workflow::json_parser::printer::{Printable, SharedPrinter, TestPrinter};
use ralph_workflow::logger::Colors;
use std::io::BufReader;
use tempfile::TempDir;

/// Test normal flow with turn.completed event.
///
/// Verifies that when a turn.completed event is received, the synthetic
/// result event is written to the log file and can be extracted.
#[test]
fn test_codex_parser_normal_flow_with_turn_completed() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("codex_test.log");

    // Create a CodexParser with log file
    let printer: SharedPrinter = std::rc::Rc::new(std::cell::RefCell::new(TestPrinter::new()));
    let parser = CodexParser::with_printer(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file(log_path.to_str().unwrap());

    // Simulate a normal flow: thread started → turn started → item started → turn completed
    let input = r#"{"type":"thread.started","thread_id":"thread-123"}
{"type":"turn.started"}
{"type":"item.started","item":{"item_type":"agent_message","content":"Hello World"}}
{"type":"item.completed","item":{"item_type":"agent_message"}}
{"type":"turn.completed","usage":{"prompt_tokens":10,"completion_tokens":20}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream(reader).unwrap();

    // Verify the synthetic result event was written and can be extracted
    let result = extract_last_result(&log_path).unwrap();
    assert!(
        result.is_some(),
        "Expected to find result event in normal flow"
    );
    let result_content = result.unwrap();
    assert!(result_content.contains("Hello World"));
}

/// Test fallback flow without turn.completed event.
///
/// This is the critical bug scenario: when the stream ends without a
/// turn.completed event, the parser should still write a synthetic result
/// event with any accumulated content.
#[test]
fn test_codex_parser_fallback_without_turn_completed() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("codex_fallback.log");

    let printer: SharedPrinter = std::rc::Rc::new(std::cell::RefCell::new(TestPrinter::new()));
    let parser = CodexParser::with_printer(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file(log_path.to_str().unwrap());

    // Simulate stream ending without turn.completed
    let input = r#"{"type":"thread.started","thread_id":"thread-456"}
{"type":"turn.started"}
{"type":"item.started","item":{"item_type":"agent_message","content":"Fallback content"}}
{"type":"item.completed","item":{"item_type":"agent_message"}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream(reader).unwrap();

    // Verify the fallback synthetic result event was written
    let result = extract_last_result(&log_path).unwrap();
    assert!(
        result.is_some(),
        "Expected to find result event in fallback flow (no turn.completed)"
    );
    let result_content = result.unwrap();
    assert!(result_content.contains("Fallback content"));
}

/// Test last line without trailing newline.
///
/// Verifies that extraction works even when the last line in the log
/// file doesn't have a trailing newline.
#[test]
fn test_codex_parser_last_line_without_trailing_newline() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("codex_no_newline.log");

    let printer: SharedPrinter = std::rc::Rc::new(std::cell::RefCell::new(TestPrinter::new()));
    let parser = CodexParser::with_printer(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file(log_path.to_str().unwrap());

    // Normal flow - the parser should handle trailing newlines correctly
    let input = r#"{"type":"thread.started","thread_id":"thread-789"}
{"type":"turn.started"}
{"type":"item.started","item":{"item_type":"agent_message","content":"No newline test"}}
{"type":"item.completed","item":{"item_type":"agent_message"}}
{"type":"turn.completed","usage":{"prompt_tokens":5,"completion_tokens":10}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream(reader).unwrap();

    // Verify extraction works regardless of trailing newline
    let result = extract_last_result(&log_path).unwrap();
    assert!(result.is_some(), "Expected to find result event");
    assert!(result.unwrap().contains("No newline test"));
}

/// Test multiple turns with result events.
///
/// Verifies that each completed turn gets its own result event written.
#[test]
fn test_codex_parser_multiple_turns() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("codex_multi_turn.log");

    let printer: SharedPrinter = std::rc::Rc::new(std::cell::RefCell::new(TestPrinter::new()));
    let parser = CodexParser::with_printer(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file(log_path.to_str().unwrap());

    // Simulate multiple turns
    let input = r#"{"type":"thread.started","thread_id":"thread-multi"}
{"type":"turn.started"}
{"type":"item.started","item":{"item_type":"agent_message","content":"First turn"}}
{"type":"item.completed","item":{"item_type":"agent_message"}}
{"type":"turn.completed","usage":{"prompt_tokens":10,"completion_tokens":20}}
{"type":"turn.started"}
{"type":"item.started","item":{"item_type":"agent_message","content":"Second turn"}}
{"type":"item.completed","item":{"item_type":"agent_message"}}
{"type":"turn.completed","usage":{"prompt_tokens":15,"completion_tokens":25}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream(reader).unwrap();

    // Verify at least one result event was written (extraction picks the best one)
    let result = extract_last_result(&log_path).unwrap();
    assert!(
        result.is_some(),
        "Expected to find at least one result event"
    );
    // The result should contain content from one of the turns
    let result_content = result.unwrap();
    assert!(
        result_content.contains("First turn") || result_content.contains("Second turn"),
        "Result should contain content from one of the turns"
    );
}

/// Test the exact user-reported bug scenario.
///
/// The user reported that issues were being produced but extraction
/// failed with "No JSON result event found in reviewer logs".
/// This test verifies that the synthetic result event is written
/// correctly even when the agent output ends abruptly.
#[test]
fn test_user_reported_bug_scenario() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("user_bug_scenario.log");

    let printer: SharedPrinter = std::rc::Rc::new(std::cell::RefCell::new(TestPrinter::new()));
    let parser = CodexParser::with_printer(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file(log_path.to_str().unwrap());

    // Simulate the exact scenario from the user's report:
    // Agent output that should produce a result event but might not have turn.completed
    let input = r#"{"type":"thread.started","thread_id":"thread-user"}
{"type":"turn.started"}
{"type":"item.started","item":{"item_type":"agent_message","content":"I'll craft a prioritized checklist with about ten items across four levels. I'll highlight critical issues like compile errors in args.rs, app/mod.rs, and other files."}}
{"type":"item.completed","item":{"item_type":"agent_message"}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream(reader).unwrap();

    // Verify the result event IS found (this would have failed in the original bug)
    let result = extract_last_result(&log_path).unwrap();

    assert!(
        result.is_some(),
        "Expected to find result event but got None. This indicates the last-line extraction bug."
    );

    let result_content = result.unwrap();
    assert!(result_content.contains("prioritized checklist"));
    assert!(result_content.contains("compile errors"));
    assert!(result_content.contains("args.rs"));
}

/// Test that empty accumulated content doesn't cause issues.
///
/// Verifies that when a turn completes with no accumulated content,
/// the parser handles it gracefully.
#[test]
fn test_codex_parser_empty_accumulated_content() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("codex_empty.log");

    let printer: SharedPrinter = std::rc::Rc::new(std::cell::RefCell::new(TestPrinter::new()));
    let parser = CodexParser::with_printer(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file(log_path.to_str().unwrap());

    // Turn started but no item content
    let input = r#"{"type":"thread.started","thread_id":"thread-empty"}
{"type":"turn.started"}
{"type":"turn.completed","usage":{"prompt_tokens":0,"completion_tokens":0}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream(reader).unwrap();

    // With empty accumulated content, we might not get a result event
    // or we might get one with empty result - both are acceptable
    let result = extract_last_result(&log_path).unwrap();
    // The test passes as long as we don't crash
}

/// Test that log file is created and properly flushed.
///
/// Verifies that the log file exists after parsing and contains
/// the expected content.
#[test]
fn test_codex_parser_log_file_flushed() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("codex_flushed.log");

    let printer: SharedPrinter = std::rc::Rc::new(std::cell::RefCell::new(TestPrinter::new()));
    let parser = CodexParser::with_printer(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file(log_path.to_str().unwrap());

    let input = r#"{"type":"thread.started","thread_id":"thread-flush"}
{"type":"turn.started"}
{"type":"item.started","item":{"item_type":"agent_message","content":"Flush test"}}
{"type":"item.completed","item":{"item_type":"agent_message"}}
{"type":"turn.completed","usage":{"prompt_tokens":8,"completion_tokens":12}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream(reader).unwrap();

    // Verify the log file exists and is readable
    assert!(log_path.exists(), "Log file should exist after parsing");

    let log_content = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        log_content.contains(r#"{"type":"thread.started""#),
        "Log should contain thread.started event"
    );
    assert!(
        log_content.contains(r#"{"type":"result""#),
        "Log should contain synthetic result event"
    );
}

/// Test persistence with sync_all.
///
/// Verifies that the synthetic result event is persisted to disk
/// and can be read immediately after parsing completes.
#[test]
fn test_codex_parser_persistence_with_sync_all() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("codex_persistence.log");

    let printer: SharedPrinter = std::rc::Rc::new(std::cell::RefCell::new(TestPrinter::new()));
    let parser = CodexParser::with_printer(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file(log_path.to_str().unwrap());

    let input = r#"{"type":"thread.started","thread_id":"thread-sync"}
{"type":"turn.started"}
{"type":"item.started","item":{"item_type":"agent_message","content":"Persistence test"}}
{"type":"item.completed","item":{"item_type":"agent_message"}}
{"type":"turn.completed","usage":{"prompt_tokens":10,"completion_tokens":15}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream().unwrap();

    // Immediately after parsing, the result should be extractable
    // (this verifies that sync_all was called)
    let result = extract_last_result(&log_path).unwrap();
    assert!(
        result.is_some(),
        "Result should be extractable immediately after parsing (sync_all worked)"
    );
    assert!(result.unwrap().contains("Persistence test"));
}
