//! Integration tests for OpenCode parser with log file verification.
//!
//! These tests verify that the OpenCode parser correctly handles streaming events,
//! produces proper output, and writes events to log files for extraction.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (log file contents), not internal state
//! - Uses `tempfile::TempDir` to mock at architectural boundary (filesystem)
//! - Tests are deterministic and isolated

use crate::test_timeout::with_default_timeout;
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
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("opencode_test.log");

        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Quiet, printer)
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
    });
}

/// Test streaming text events accumulate correctly.
#[test]
fn test_opencode_parser_text_streaming() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

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
    });
}

/// Test tool use events with completed status.
#[test]
fn test_opencode_parser_tool_use_completed() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Verbose, printer);

        let input = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"ses_44f9562d4ffe","part":{"id":"prt_bb06ac80c001","sessionID":"ses_44f9562d4ffe","messageID":"msg_bb06a9dc1001","type":"tool","callID":"call_8a2985d92e63","tool":"read","state":{"status":"completed","input":{"filePath":"/test/PLAN.md"},"output":"<file>\n00001| # Implementation Plan\n</file>","title":"PLAN.md"}}}"#;

        let reader = BufReader::new(input.as_bytes());
        parser.parse_stream_for_test(reader).unwrap();

        let output = test_printer.borrow().get_output();
        assert!(output.contains("read"), "Output should contain tool name");
    });
}

/// Test tool use events with started status.
#[test]
fn test_opencode_parser_tool_use_started() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

        let input = r#"{"type":"tool_use","timestamp":1768191346000,"sessionID":"test","part":{"id":"prt_001","type":"tool","callID":"call_123","tool":"Bash","state":{"status":"started","input":{"command":"ls -la"}}}}"#;

        let reader = BufReader::new(input.as_bytes());
        parser.parse_stream_for_test(reader).unwrap();

        let output = test_printer.borrow().get_output();
        assert!(
            output.contains("Bash"),
            "Output should contain Bash tool name"
        );
    });
}

/// Test step finish with token statistics.
#[test]
fn test_opencode_parser_step_finish_with_stats() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

        let input = r#"{"type":"step_finish","timestamp":1768191347296,"sessionID":"test","part":{"id":"prt_001","type":"step-finish","reason":"tool-calls","cost":0.05,"tokens":{"input":108,"output":151,"reasoning":10,"cache":{"read":11236,"write":500}}}}"#;

        let reader = BufReader::new(input.as_bytes());
        parser.parse_stream_for_test(reader).unwrap();

        let output = test_printer.borrow().get_output();
        assert!(
            output.contains("Step finished"),
            "Output should contain Step finished"
        );
    });
}

/// Test step finish with different reasons.
#[test]
fn test_opencode_parser_step_finish_reasons() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

        // Test end-turn reason
        let input_end_turn = r#"{"type":"step_finish","timestamp":1000,"sessionID":"test","part":{"id":"prt_001","type":"step-finish","reason":"end-turn"}}"#;

        let reader = BufReader::new(input_end_turn.as_bytes());
        parser.parse_stream_for_test(reader).unwrap();

        let output = test_printer.borrow().get_output();
        assert!(
            output.contains("Step finished"),
            "Output should contain Step finished for end-turn"
        );
    });
}

/// Test tool output with object payload.
#[test]
fn test_opencode_parser_tool_output_object() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Verbose, printer);

        let input = r#"{"type":"tool_use","timestamp":1768191346712,"sessionID":"test","part":{"id":"prt_001","type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"/test.rs"},"output":{"ok":true,"bytes":123}}}}"#;

        let reader = BufReader::new(input.as_bytes());
        parser.parse_stream_for_test(reader).unwrap();

        let output = test_printer.borrow().get_output();
        assert!(output.contains("Output"), "Output should contain Output");
    });
}

/// Test log file is properly flushed.
#[test]
fn test_opencode_parser_log_file_flushed() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("opencode_flush.log");

        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Quiet, printer)
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
    });
}

/// Test multiple tool operations in sequence.
#[test]
fn test_opencode_parser_tool_sequence() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

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
    });
}

// ============================================================================
// Log Content Tests
// ============================================================================

/// Test log file contains all events for later analysis.
#[test]
fn test_opencode_parser_log_contains_events() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("opencode_events.log");

        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Quiet, printer)
                .with_log_file_for_test(log_path.to_str().unwrap());

        let input = r#"{"type":"step_start","timestamp":1000,"sessionID":"extract-test","part":{"type":"step-start"}}
{"type":"text","timestamp":1001,"sessionID":"extract-test","part":{"type":"text","text":"Extractable content from OpenCode"}}
{"type":"step_finish","timestamp":1002,"sessionID":"extract-test","part":{"type":"step-finish","reason":"end-turn"}}"#;

        let reader = BufReader::new(input.as_bytes());
        parser.parse_stream_for_test(reader).unwrap();

        // Verify log file contains all events for analysis
        assert!(log_path.exists(), "Log file should exist");
        let log_content = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            log_content.contains("step_start"),
            "Log should contain step_start event"
        );
        assert!(
            log_content.contains("Extractable content"),
            "Log should contain text content"
        );
        assert!(
            log_content.contains("step_finish"),
            "Log should contain step_finish event"
        );
    });
}

/// Test multiple steps in conversation.
#[test]
fn test_opencode_parser_multiple_steps() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("opencode_multi.log");

        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Quiet, printer)
                .with_log_file_for_test(log_path.to_str().unwrap());

        let input = r#"{"type":"step_start","timestamp":1000,"sessionID":"multi-step","part":{"type":"step-start"}}
{"type":"text","timestamp":1001,"sessionID":"multi-step","part":{"type":"text","text":"First step content"}}
{"type":"step_finish","timestamp":1002,"sessionID":"multi-step","part":{"type":"step-finish","reason":"tool-calls"}}
{"type":"step_start","timestamp":1003,"sessionID":"multi-step","part":{"type":"step-start"}}
{"type":"text","timestamp":1004,"sessionID":"multi-step","part":{"type":"text","text":"Second step content"}}
{"type":"step_finish","timestamp":1005,"sessionID":"multi-step","part":{"type":"step-finish","reason":"end-turn"}}"#;

        let reader = BufReader::new(input.as_bytes());
        parser.parse_stream_for_test(reader).unwrap();

        // Verify log contains both steps
        let log_content = std::fs::read_to_string(&log_path).unwrap();
        assert!(
            log_content.contains("First step") && log_content.contains("Second step"),
            "Log should contain both steps"
        );
    });
}

// ============================================================================
// Error Handling Tests
// ============================================================================

/// Test handling of malformed JSON.
#[test]
fn test_opencode_parser_malformed_json() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

        // Mix of valid and invalid JSON
        let input = r#"{"type":"step_start","timestamp":1000,"sessionID":"malformed","part":{"type":"step-start"}}
{not valid json at all}
{"type":"text","timestamp":1002,"sessionID":"malformed","part":{"type":"text","text":"Valid after invalid"}}
{"type":"step_finish","timestamp":1003,"sessionID":"malformed","part":{"type":"step-finish","reason":"end-turn"}}"#;

        let reader = BufReader::new(input.as_bytes());
        // Should not panic on malformed JSON
        let result = parser.parse_stream_for_test(reader);
        assert!(
            result.is_ok(),
            "Parser should handle malformed JSON gracefully"
        );

        // Valid events should still be processed
        let output = test_printer.borrow().get_output();
        assert!(
            output.contains("Valid after invalid") || output.contains("Step"),
            "Valid events should still be processed"
        );
    });
}

/// Test handling of truncated stream (network disconnect simulation).
#[test]
fn test_opencode_parser_truncated_stream() {
    with_default_timeout(|| {
        let temp_dir = TempDir::new().unwrap();
        let log_path = temp_dir.path().join("opencode_truncated.log");

        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Quiet, printer)
                .with_log_file_for_test(log_path.to_str().unwrap());

        // Stream that ends without step_finish event
        let input = r#"{"type":"step_start","timestamp":1000,"sessionID":"truncated","part":{"type":"step-start"}}
{"type":"text","timestamp":1001,"sessionID":"truncated","part":{"type":"text","text":"Partial content"}}
{"type":"text","timestamp":1002,"sessionID":"truncated","part":{"type":"text","text":" more content"}}"#;

        let reader = BufReader::new(input.as_bytes());
        let result = parser.parse_stream_for_test(reader);

        // Parser should handle truncated stream gracefully
        assert!(
            result.is_ok(),
            "Parser should handle truncated stream without panic"
        );

        // Log should still contain the partial events
        assert!(
            log_path.exists(),
            "Log file should exist even for truncated stream"
        );
    });
}

/// Test error reason handling in step_finish.
#[test]
fn test_opencode_parser_error_reason() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

        let input = r#"{"type":"step_start","timestamp":1000,"sessionID":"error-test","part":{"type":"step-start"}}
{"type":"text","timestamp":1001,"sessionID":"error-test","part":{"type":"text","text":"Some content"}}
{"type":"step_finish","timestamp":1002,"sessionID":"error-test","part":{"type":"step-finish","reason":"error","error":"Rate limit exceeded"}}"#;

        let reader = BufReader::new(input.as_bytes());
        parser.parse_stream_for_test(reader).unwrap();

        let output = test_printer.borrow().get_output();
        // Should still produce output even with error reason
        assert!(
            output.contains("Step") || output.contains("content"),
            "Parser should process content before error"
        );
    });
}

// ============================================================================
// Deduplication Tests
// ============================================================================

/// Test that consecutive identical text deltas are handled.
#[test]
fn test_opencode_parser_consecutive_text_handled() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

        // Same text sent multiple times (potential bug scenario)
        let input = r#"{"type":"step_start","timestamp":1000,"sessionID":"dedup","part":{"type":"step-start"}}
{"type":"text","timestamp":1001,"sessionID":"dedup","part":{"type":"text","text":"Hello"}}
{"type":"text","timestamp":1002,"sessionID":"dedup","part":{"type":"text","text":"Hello"}}
{"type":"text","timestamp":1003,"sessionID":"dedup","part":{"type":"text","text":"Hello"}}
{"type":"step_finish","timestamp":1004,"sessionID":"dedup","part":{"type":"step-finish","reason":"end-turn"}}"#;

        let reader = BufReader::new(input.as_bytes());
        parser.parse_stream_for_test(reader).unwrap();

        // Verify output doesn't have excessive repetition in render
        let output = test_printer.borrow().get_output();
        // Count occurrences - should be reasonable
        let hello_count = output.matches("Hello").count();
        assert!(
            hello_count <= 4,
            "Consecutive text events should not cause excessive duplication. Got {} occurrences",
            hello_count
        );
    });
}

/// Test that interleaved tool and text events work correctly.
#[test]
fn test_opencode_parser_interleaved_tool_text() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Normal, printer);

        let input = r#"{"type":"step_start","timestamp":1000,"sessionID":"interleaved","part":{"type":"step-start"}}
{"type":"text","timestamp":1001,"sessionID":"interleaved","part":{"type":"text","text":"Let me check that file"}}
{"type":"tool_use","timestamp":1002,"sessionID":"interleaved","part":{"type":"tool","tool":"read","state":{"status":"started","input":{"filePath":"test.rs"}}}}
{"type":"tool_use","timestamp":1003,"sessionID":"interleaved","part":{"type":"tool","tool":"read","state":{"status":"completed","input":{"filePath":"test.rs"},"output":"file content"}}}
{"type":"text","timestamp":1004,"sessionID":"interleaved","part":{"type":"text","text":"Now I can see the content"}}
{"type":"step_finish","timestamp":1005,"sessionID":"interleaved","part":{"type":"step-finish","reason":"end-turn"}}"#;

        let reader = BufReader::new(input.as_bytes());
        parser.parse_stream_for_test(reader).unwrap();

        let output = test_printer.borrow().get_output();
        // Both text blocks should be in output
        assert!(
            output.contains("Let me check"),
            "First text should be in output"
        );
        assert!(
            output.contains("Now I can see"),
            "Second text should be in output"
        );
    });
}

// ============================================================================
// Token/Cost Tracking Tests
// ============================================================================

/// Test cost and token reporting in step_finish.
#[test]
fn test_opencode_parser_cost_and_tokens() {
    with_default_timeout(|| {
        let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
        let printer: SharedPrinter = test_printer.clone();
        let parser =
            OpenCodeParser::with_printer_for_test(Colors::new(), Verbosity::Verbose, printer);

        let input = r#"{"type":"step_start","timestamp":1000,"sessionID":"cost-test","part":{"type":"step-start"}}
{"type":"text","timestamp":1001,"sessionID":"cost-test","part":{"type":"text","text":"Response content"}}
{"type":"step_finish","timestamp":1002,"sessionID":"cost-test","part":{"type":"step-finish","reason":"end-turn","cost":0.0125,"tokens":{"input":100,"output":250,"reasoning":50,"cache":{"read":1000,"write":100}}}}"#;

        let reader = BufReader::new(input.as_bytes());
        parser.parse_stream_for_test(reader).unwrap();

        let output = test_printer.borrow().get_output();
        // At verbose level, should show some stats
        assert!(
            output.contains("Step finished") || output.contains("250") || output.contains("100"),
            "Verbose output should contain step finish info"
        );
    });
}
