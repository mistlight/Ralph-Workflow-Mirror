//! Integration tests for Gemini parser with log file verification.
//!
//! These tests verify that the Gemini parser correctly handles streaming events,
//! produces proper output, and writes events to log files for extraction.

use ralph_workflow::config::Verbosity;
use ralph_workflow::json_parser::gemini::GeminiParser;
use ralph_workflow::json_parser::printer::{SharedPrinter, TestPrinter};
use ralph_workflow::logger::Colors;
use std::cell::RefCell;
use std::io::BufReader;
use std::rc::Rc;
use tempfile::TempDir;

/// Test normal flow with init, message, and result events.
#[test]
fn test_gemini_parser_normal_flow() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("gemini_test.log");

    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = GeminiParser::with_printer_for_test(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file_for_test(log_path.to_str().unwrap());

    let input = r#"{"type":"init","session_id":"gemini-123","model":"gemini-2.0-flash-exp"}
{"type":"message","role":"assistant","content":"Hello World"}
{"type":"result","status":"success","stats":{"total_tokens":100,"input_tokens":50,"output_tokens":50,"duration_ms":2000}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    // Verify the log file exists and contains events
    assert!(log_path.exists(), "Log file should exist after parsing");

    let log_content = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        log_content.contains(r#""type":"init""#),
        "Log should contain init event"
    );
    assert!(
        log_content.contains(r#""type":"result""#),
        "Log should contain result event"
    );

    // Verify printer captured output
    let output = test_printer.borrow().get_output();
    assert!(
        output.contains("Session started"),
        "Output should contain session started message"
    );
}

/// Test streaming delta messages accumulate correctly.
#[test]
fn test_gemini_parser_delta_streaming() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = GeminiParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

    let input = r#"{"type":"init","session_id":"stream-test","model":"gemini-2.0"}
{"type":"message","role":"assistant","content":"Hello","delta":true}
{"type":"message","role":"assistant","content":" World","delta":true}
{"type":"message","role":"assistant","content":"Hello World","delta":false}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    // Verify delta streaming was processed
    let output = test_printer.borrow().get_output();
    assert!(
        output.contains("Hello"),
        "Output should contain streamed Hello"
    );
}

/// Test tool use events are formatted correctly.
#[test]
fn test_gemini_parser_tool_use() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = GeminiParser::with_printer_for_test(Colors::new(), Verbosity::Verbose, printer);

    let input = r#"{"type":"tool_use","tool_name":"Bash","tool_id":"bash-123","parameters":{"command":"ls -la"}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    let output = test_printer.borrow().get_output();
    assert!(output.contains("Tool"), "Output should contain Tool label");
    assert!(output.contains("Bash"), "Output should contain Bash");
}

/// Test tool result events show success/failure status.
#[test]
fn test_gemini_parser_tool_result() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = GeminiParser::with_printer_for_test(Colors::new(), Verbosity::Verbose, printer);

    let input = r#"{"type":"tool_result","tool_id":"bash-123","status":"success","output":"file1.txt\nfile2.txt"}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    let output = test_printer.borrow().get_output();
    assert!(
        output.contains("Tool result"),
        "Output should contain Tool result"
    );
}

/// Test error events are handled properly.
#[test]
fn test_gemini_parser_error_event() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = GeminiParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

    let input = r#"{"type":"error","message":"Rate limit exceeded","code":"429"}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    let output = test_printer.borrow().get_output();
    assert!(output.contains("Error"), "Output should contain Error");
    assert!(
        output.contains("Rate limit exceeded"),
        "Output should contain error message"
    );
    assert!(output.contains("429"), "Output should contain error code");
}

/// Test result event with statistics.
#[test]
fn test_gemini_parser_result_with_stats() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = GeminiParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

    let input = r#"{"type":"result","status":"success","stats":{"total_tokens":250,"input_tokens":50,"output_tokens":200,"duration_ms":180000,"tool_calls":3}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    let output = test_printer.borrow().get_output();
    assert!(
        output.contains("success"),
        "Output should contain success status"
    );
    assert!(
        output.contains("in:50"),
        "Output should contain input tokens"
    );
    assert!(
        output.contains("out:200"),
        "Output should contain output tokens"
    );
    assert!(
        output.contains("3 tools"),
        "Output should contain tool calls"
    );
}

/// Test user role messages.
#[test]
fn test_gemini_parser_user_message() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = GeminiParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

    let input = r#"{"type":"message","role":"user","content":"List files in current directory"}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    let output = test_printer.borrow().get_output();
    assert!(output.contains("user"), "Output should contain user role");
    assert!(
        output.contains("List files"),
        "Output should contain user message"
    );
}

/// Test log file is properly flushed.
#[test]
fn test_gemini_parser_log_file_flushed() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("gemini_flush.log");

    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = GeminiParser::with_printer_for_test(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file_for_test(log_path.to_str().unwrap());

    let input = r#"{"type":"init","session_id":"flush-test","model":"gemini-2.0"}
{"type":"message","role":"assistant","content":"Test message"}
{"type":"result","status":"success"}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    // Verify log file exists and is readable immediately
    assert!(log_path.exists(), "Log file should exist after parsing");

    let log_content = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        log_content.contains(r#"session_id"#),
        "Log should be readable immediately after parsing (sync_all worked)"
    );
}
