//! Integration tests for OpenCode parser with log file verification.
//!
//! These tests verify that the OpenCode parser correctly handles streaming events,
//! produces proper output, and writes events to log files for extraction.

use ralph_workflow::config::Verbosity;
use ralph_workflow::json_parser::opencode::OpenCodeParser;
use ralph_workflow::json_parser::printer::{SharedPrinter, TestPrinter};
use ralph_workflow::logger::Colors;
use std::cell::RefCell;
use std::io::BufReader;
use std::rc::Rc;
use tempfile::TempDir;

/// Test normal flow with step_start, text, and step_finish events.
#[test]
fn test_opencode_parser_normal_flow() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("opencode_test.log");

    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file_for_test(log_path.to_str().unwrap());

    let input = r#"{"type":"step_start","timestamp":1768191337567,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06aa45c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"step-start","snapshot":"5d36aa035d4df6edb73a68058733063258114ed5"}}
{"type":"text","timestamp":1768191347231,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac633001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"text","text":"Hello World"}}
{"type":"step_finish","timestamp":1768191347296,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06aca1d001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"step-finish","reason":"end-turn","cost":0.05,"tokens":{"input":108,"output":151,"reasoning":0,"cache":{"read":11236,"write":0}}}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    // Verify the log file exists and contains events
    assert!(log_path.exists(), "Log file should exist after parsing");

    let log_content = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        log_content.contains(r#""type":"step_start""#),
        "Log should contain step_start event"
    );
    assert!(
        log_content.contains(r#""type":"step_finish""#),
        "Log should contain step_finish event"
    );
}

/// Test streaming text events accumulate correctly.
#[test]
fn test_opencode_parser_text_streaming() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

    let input = r#"{"type":"step_start","timestamp":1000,"sessionID":"test","part":{"id":"part1","type":"step-start"}}
{"type":"text","timestamp":1001,"sessionID":"test","part":{"id":"text1","type":"text","text":"Hello"}}
{"type":"text","timestamp":1002,"sessionID":"test","part":{"id":"text2","type":"text","text":" World"}}
{"type":"step_finish","timestamp":1003,"sessionID":"test","part":{"id":"finish1","type":"step-finish","reason":"end-turn"}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    // Verify text streaming was processed
    let output = test_printer.borrow().get_output();
    assert!(
        output.contains("Hello"),
        "Output should contain streamed Hello"
    );
}

/// Test tool use events with completed status.
#[test]
fn test_opencode_parser_tool_use_completed() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Verbose, printer);

    let input = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"tool","callID":"call_8a2985d92e63","tool":"read","state":{"status":"completed","input":{"filePath":"/test/PLAN.md"},"output":"<file>\n00001| # Implementation Plan\n</file>","title":"PLAN.md"}}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    let output = test_printer.borrow().get_output();
    assert!(output.contains("read"), "Output should contain tool name");
}

/// Test tool use events with started status.
#[test]
fn test_opencode_parser_tool_use_started() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

    let input = r#"{"type":"tool_use","timestamp":1768191346000,"sessionID":"test","part":{"id":"prt_001","type":"tool","callID":"call_123","tool":"Bash","state":{"status":"started","input":{"command":"ls -la"}}}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    let output = test_printer.borrow().get_output();
    assert!(
        output.contains("Bash"),
        "Output should contain Bash tool name"
    );
}

/// Test step finish with token statistics.
#[test]
fn test_opencode_parser_step_finish_with_stats() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

    let input = r#"{"type":"step_finish","timestamp":1768191347296,"sessionID":"test","part":{"id":"prt_001","type":"step-finish","reason":"tool-calls","cost":0.05,"tokens":{"input":108,"output":151,"reasoning":10,"cache":{"read":11236,"write":500}}}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    let output = test_printer.borrow().get_output();
    assert!(
        output.contains("Step finished"),
        "Output should contain Step finished"
    );
}

/// Test step finish with different reasons.
#[test]
fn test_opencode_parser_step_finish_reasons() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

    // Test end-turn reason
    let input_end_turn = r#"{"type":"step_finish","timestamp":1000,"sessionID":"test","part":{"id":"prt_001","type":"step-finish","reason":"end-turn"}}"#;

    let reader = BufReader::new(input_end_turn.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    let output = test_printer.borrow().get_output();
    assert!(
        output.contains("Step finished"),
        "Output should contain Step finished for end-turn"
    );
}

/// Test tool output with object payload.
#[test]
fn test_opencode_parser_tool_output_object() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Verbose, printer);

    let input = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"test","part":{"id":"prt_001","type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"/test.rs"},"output":{"ok":true,"bytes":123}}}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    let output = test_printer.borrow().get_output();
    assert!(output.contains("Output"), "Output should contain Output");
}

/// Test log file is properly flushed.
#[test]
fn test_opencode_parser_log_file_flushed() {
    let temp_dir = TempDir::new().unwrap();
    let log_path = temp_dir.path().join("opencode_flush.log");

    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Quiet, printer)
        .with_log_file_for_test(log_path.to_str().unwrap());

    let input = r#"{"type":"step_start","timestamp":1000,"sessionID":"test","part":{"id":"prt_001","type":"step-start"}}
{"type":"text","timestamp":1001,"sessionID":"test","part":{"id":"text1","type":"text","text":"Test message"}}
{"type":"step_finish","timestamp":1002,"sessionID":"test","part":{"id":"finish1","type":"step-finish","reason":"end-turn"}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    // Verify log file exists and is readable immediately
    assert!(log_path.exists(), "Log file should exist after parsing");

    let log_content = std::fs::read_to_string(&log_path).unwrap();
    assert!(
        log_content.contains(r#"sessionID"#),
        "Log should be readable immediately after parsing (sync_all worked)"
    );
}

/// Test multiple tool operations in sequence.
#[test]
fn test_opencode_parser_tool_sequence() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let parser = OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

    let input = r#"{"type":"step_start","timestamp":1000,"sessionID":"test","part":{"type":"step-start"}}
{"type":"tool_use","timestamp":1001,"sessionID":"test","part":{"type":"tool","tool":"read","state":{"status":"started","input":{"filePath":"file1.rs"}}}}
{"type":"tool_use","timestamp":1002,"sessionID":"test","part":{"type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"file1.rs"},"output":"content1"}}}
{"type":"tool_use","timestamp":1003,"sessionID":"test","part":{"type":"tool","tool":"write","state":{"status":"started","input":{"filePath":"file2.rs"}}}}
{"type":"tool_use","timestamp":1004,"sessionID":"test","part":{"type":"tool","tool":"write","state":{"status":"completed","input":{"filePath":"file2.rs"}}}}
{"type":"step_finish","timestamp":1005,"sessionID":"test","part":{"type":"step-finish","reason":"tool-calls"}}"#;

    let reader = BufReader::new(input.as_bytes());
    parser.parse_stream_for_test(reader).unwrap();

    let output = test_printer.borrow().get_output();
    assert!(output.contains("read"), "Output should contain read tool");
    assert!(output.contains("write"), "Output should contain write tool");
}
