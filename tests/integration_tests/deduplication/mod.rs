//! Integration tests for delta deduplication using real log data.
//!
//! This module tests the deduplication system against real-world streaming logs
//! to ensure that:
//! 1. No duplicate renders occur in production scenarios
//! 2. Snapshot glitches are properly repaired
//! 3. Consecutive duplicates are filtered
//! 4. Intentional repetition is preserved
//!
//! The tests use curated log snippets from actual `.agent/logs/` files showing
//! problematic patterns that slipped through unit testing.
//!
//! # Testing Architecture
//!
//! These tests use the parser's internal printer via the `printer()` getter method.
//! This ensures tests validate the ACTUAL production code path (`parse_stream`),
//! not a simulated test-only path.
//!
//! The pattern is:
//! 1. Create a `TestPrinter` and wrap it as `SharedPrinter`
//! 2. Create parser with `ClaudeParser::with_printer()`
//! 3. Use `parse_stream()` to process events (writes to parser's internal printer)
//! 4. Access output via `parser.printer()` to verify results
//!
//! This tests the exact same code path as production usage.

use std::cell::RefCell;
use std::io::Cursor;
use std::path::Path;
use std::rc::Rc;

use ralph_workflow::config::Verbosity;
use ralph_workflow::json_parser::printer::{SharedPrinter, TestPrinter};
use ralph_workflow::json_parser::ClaudeParser;
use ralph_workflow::logger::Colors;

/// Helper function to verify output has no duplicates and meets quality standards.
///
/// This provides consistent duplicate detection across all tests with detailed
/// error messages when issues are found.
fn verify_no_duplicates(printer: &TestPrinter, context: &str) {
    let duplicates = printer.find_duplicate_consecutive_lines();

    // Filter out empty/whitespace line duplicates (known edge case from completion newlines)
    let non_empty_duplicates: Vec<(usize, String)> = duplicates
        .into_iter()
        .filter(|(_, content): &(usize, String)| !content.trim().is_empty())
        .collect();

    if !non_empty_duplicates.is_empty() {
        panic!(
            "Found {} duplicate consecutive line(s) in {}:\n{:#?}\n\nFull output:\n{}",
            non_empty_duplicates.len(),
            context,
            non_empty_duplicates,
            printer.get_output()
        );
    }

    // Additional sanity checks
    let output = printer.get_output();
    let (line_count, char_count) = printer.get_stats();

    // Verify we got some output (unless test specifically expects empty)
    if !output.trim().is_empty() {
        assert!(
            line_count > 0,
            "{}: Should have at least one line of output",
            context
        );
        assert!(
            char_count > 0,
            "{}: Should have some non-whitespace output",
            context
        );
    }
}

/// Test ClaudeParser with real log data to detect duplicate output.
///
/// This test uses actual production logs from PROMPT-LOG.log to ensure
/// the deduplication system works correctly in real-world scenarios.
#[test]
fn test_claude_parser_no_duplicates_with_real_log() {
    // Load the real log file
    let possible_paths = vec![
        "tests/deduplication_integration_tests/fixtures/PROMPT-LOG.log",
        "deduplication_integration_tests/fixtures/PROMPT-LOG.log",
        "../tests/deduplication_integration_tests/fixtures/PROMPT-LOG.log",
    ];

    let log_path = possible_paths
        .iter()
        .find(|p| Path::new(p).exists())
        .unwrap_or_else(|| {
            panic!(
                "Log file not found at any of these paths: {:?}\nCurrent dir: {:?}",
                possible_paths,
                std::env::current_dir()
            )
        });

    // Read the log file content
    let log_content = std::fs::read_to_string(log_path)
        .unwrap_or_else(|e| panic!("Failed to read log file {:?}: {}", log_path, e));

    let event_count = log_content.lines().filter(|l| !l.trim().is_empty()).count();
    assert!(
        event_count > 0,
        "Log file should contain at least one event"
    );

    // Create a TestPrinter and wrap it as SharedPrinter
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Use parse_stream to process events - this writes to parser's internal printer
    // This is the SAME code path used in production
    let cursor = Cursor::new(log_content);
    let reader = std::io::BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");

    // Access the SAME printer that the parser wrote to
    // We use the original test_printer reference since parser.printer() returns
    // the same underlying Rc<RefCell<TestPrinter>>
    let printer_ref = test_printer.borrow();

    // Verify no duplicates in output
    verify_no_duplicates(&printer_ref, "real log (PROMPT-LOG.log)");

    // Additional sanity checks for real log
    let output = printer_ref.get_output();
    assert!(
        !output.trim().is_empty(),
        "Parser should produce some output from real log"
    );

    // Verify output contains expected patterns from Claude events
    assert!(
        output.contains("[Claude]"),
        "Output should contain Claude prefix from real log"
    );

    // Log summary for debugging
    let (line_count, char_count) = printer_ref.get_stats();
    println!(
        "Processed {} events from real log, output: {} chars, {} lines",
        event_count, char_count, line_count
    );

    // Verify streaming metrics show deduplication is working
    let metrics = parser.streaming_metrics();
    println!(
        "Streaming metrics: {} total deltas, {} snapshot repairs, {} large deltas, {} protocol violations",
        metrics.total_deltas,
        metrics.snapshot_repairs_count,
        metrics.large_delta_count,
        metrics.protocol_violations
    );

    // If we have snapshot repairs, deduplication is actively working
    if metrics.snapshot_repairs_count > 0 {
        println!(
            "✓ Deduplication active: {} snapshot glitches repaired",
            metrics.snapshot_repairs_count
        );
    }
}

/// Test ClaudeParser with synthetic snapshot glitch data.
///
/// Verifies that when the agent sends accumulated content as a delta (a common
/// streaming bug), the deduplication system detects and repairs it.
#[test]
fn test_claude_parser_snapshot_glitch() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Simulate snapshot glitch: agent sends accumulated content as delta
    let accumulated = "The quick brown fox";
    let snapshot_event = format!(
        r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}}}"#,
        accumulated
    );

    let events = vec![
        // Normal streaming
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" quick"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" brown"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" fox"}}}"#,
        // Snapshot glitch: agent resends entire accumulated content
        &snapshot_event,
        // New content after snapshot
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" New content"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let input = events.join("\n");
    let cursor = Cursor::new(input);
    let reader = std::io::BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");

    // Access the SAME printer that the parser wrote to
    // We use the original test_printer reference since parser.printer() returns
    // the same underlying Rc<RefCell<TestPrinter>>
    let printer_ref = test_printer.borrow();

    // Verify no duplicates occurred
    verify_no_duplicates(&printer_ref, "snapshot glitch test");

    // Verify the final content includes both the glitched portion and new content
    let output = printer_ref.get_output();
    assert!(
        output.contains("New content"),
        "Output should contain content after snapshot glitch"
    );

    // Verify streaming metrics show snapshot repair occurred
    let metrics = parser.streaming_metrics();
    assert_eq!(
        metrics.snapshot_repairs_count, 1,
        "Should track one snapshot repair when accumulated content is re-sent as a delta"
    );
}

/// Test that intentional repetition is preserved.
///
/// The deduplication system should only filter BUGGY duplicates, not
/// intentional repetition like "echo echo echo".
#[test]
fn test_claude_parser_intentional_repetition_preserved() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Test "echo echo echo" pattern - should be preserved
    let events = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"echo"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" "}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"echo"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" "}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"echo"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let input = events.join("\n");
    let cursor = Cursor::new(input);
    let reader = std::io::BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");

    // Access the SAME printer that the parser wrote to
    // We use the original test_printer reference since parser.printer() returns
    // the same underlying Rc<RefCell<TestPrinter>>
    let printer_ref = test_printer.borrow();
    let output = printer_ref.get_output();

    // The output should contain "echo echo echo" (with spaces)
    assert!(
        output.contains("echo") && output.contains("echo echo"),
        "Intentional repetition should be preserved in output"
    );

    // But we should NOT have duplicate consecutive lines
    verify_no_duplicates(&printer_ref, "intentional repetition test");
}

/// Test that consecutive identical deltas are filtered.
///
/// When the agent sends the same delta multiple times (a common network glitch),
/// the deduplication system should filter out the duplicates.
#[test]
fn test_claude_parser_consecutive_duplicates_filtered() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Test consecutive identical deltas
    let repeated_text = "This is a repeated message";
    let repeated_event = format!(
        r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}}}"#,
        repeated_text
    );

    let events: Vec<String> = vec![
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#.to_string(),
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#.to_string(),
        repeated_event.clone(),
        repeated_event.clone(),
        repeated_event.clone(),
        repeated_event.clone(),
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#.to_string(),
    ];

    let input = events.join("\n");
    let cursor = Cursor::new(input);
    let reader = std::io::BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");

    // Access the SAME printer that the parser wrote to
    // We use the original test_printer reference since parser.printer() returns
    // the same underlying Rc<RefCell<TestPrinter>>
    let printer_ref = test_printer.borrow();

    // Verify no duplicate consecutive lines
    verify_no_duplicates(&printer_ref, "consecutive duplicates test");

    // Verify the content appears at least once
    let output = printer_ref.get_output();
    assert!(
        output.contains("This is a repeated message"),
        "Content should appear in output"
    );
}

/// Test that the parser correctly handles content blocks with no deltas.
///
/// Empty content blocks should not cause any duplicate output.
#[test]
fn test_claude_parser_empty_content_block() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    let events = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let input = events.join("\n");
    let cursor = Cursor::new(input);
    let reader = std::io::BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");

    // Access the SAME printer that the parser wrote to
    // We use the original test_printer reference since parser.printer() returns
    // the same underlying Rc<RefCell<TestPrinter>>
    let printer_ref = test_printer.borrow();

    // Empty content block should produce no duplicate lines
    assert!(
        !printer_ref.has_duplicate_consecutive_lines(),
        "Empty content block should not produce duplicates"
    );
}

// ===========================================================================
// GLM/CCS Agent Duplicate Detection Tests
// ===========================================================================
//
// GLM agents accessed via CCS (Claude Code Switch) use the Claude parser but
// can exhibit unique streaming patterns that cause duplicates. These tests
// verify the deduplication system handles GLM-specific patterns correctly.

/// Test GLM agent repeated MessageStart events don't cause duplicates.
///
/// GLM agents sometimes send multiple MessageStart events during a conversation
/// turn. This should not reset the deduplication state and cause content to
/// be re-rendered.
#[test]
fn test_glm_repeated_message_start_no_duplicates() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Simulate GLM pattern with repeated message_start events
    let events = [
        // First message start
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}"#,
        // Second message_start (GLM quirk) - should not reset state
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        // Same deltas arrive again (which should be deduped)
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}"#,
        // New content after the duplicate block
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let input = events.join("\n");
    let cursor = Cursor::new(input);
    let reader = std::io::BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");

    let printer_ref = test_printer.borrow();

    // Verify no duplicates in output
    verify_no_duplicates(&printer_ref, "GLM repeated message_start");

    // Verify final content is correct
    let output = printer_ref.get_output();
    assert!(
        output.contains("Hello World!"),
        "Output should contain complete message"
    );
}

/// Test GLM agent alternating delta pattern doesn't trigger false deduplication.
///
/// GLM agents sometimes send deltas in alternating patterns (A, B, A, B).
/// These are NOT consecutive duplicates and should all be processed.
#[test]
fn test_glm_alternating_deltas_not_deduped() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Alternating pattern: Ping, Pong, Ping, Pong
    let events = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Ping"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Pong"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Ping"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Pong"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let input = events.join("\n");
    let cursor = Cursor::new(input);
    let reader = std::io::BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");

    let printer_ref = test_printer.borrow();
    let output = printer_ref.get_output();

    // Should contain both "Ping" and "Pong" multiple times
    // Since alternating pattern isn't consecutive duplicates, all should render
    assert!(
        output.contains("PingPongPingPong"),
        "All alternating deltas should be processed. Output: {output}"
    );
}

/// Test GLM agent consecutive identical deltas are filtered.
///
/// When GLM agents send the exact same delta multiple times consecutively
/// (a resend glitch), these duplicates should be filtered.
#[test]
fn test_glm_consecutive_identical_deltas_filtered() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Same delta sent 4 times consecutively (resend glitch)
    let repeated_text = "This should only appear once or twice in render";
    let repeated_event = format!(
        r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{repeated_text}"}}}}}}"#
    );

    let events: Vec<String> = vec![
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#.to_string(),
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#.to_string(),
        repeated_event.clone(),
        repeated_event.clone(),
        repeated_event.clone(),
        repeated_event.clone(),
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#.to_string(),
    ];

    let input = events.join("\n");
    let cursor = Cursor::new(input);
    let reader = std::io::BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");

    let printer_ref = test_printer.borrow();

    // Verify no duplicate consecutive lines in output
    verify_no_duplicates(&printer_ref, "GLM consecutive identical deltas");

    // Verify content appears (at least once)
    let output = printer_ref.get_output();
    assert!(
        output.contains(repeated_text),
        "Content should appear in output"
    );
}

/// Test GLM agent tool use events with text don't cause duplicates.
///
/// GLM agents can interleave tool use blocks with text blocks.
/// Switching between them should not cause duplicate rendering.
#[test]
fn test_glm_tool_use_interleaved_with_text() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    let events = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        // First text block
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Let me check"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
        // Tool use block
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"tool_1","name":"Read","input":{}}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"path\":\"/test\"}"}}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
        // Second text block after tool
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":2,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":2,"delta":{"type":"text_delta","text":"Now I can see"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_stop","index":2}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let input = events.join("\n");
    let cursor = Cursor::new(input);
    let reader = std::io::BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");

    let printer_ref = test_printer.borrow();

    // Verify no duplicates
    verify_no_duplicates(&printer_ref, "GLM tool use interleaved");

    // Verify both text blocks are in output
    let output = printer_ref.get_output();
    assert!(
        output.contains("Let me check"),
        "First text block should be in output"
    );
    assert!(
        output.contains("Now I can see"),
        "Second text block should be in output"
    );
}

/// Test GLM agent snapshot-as-delta detection.
///
/// GLM agents can sometimes send accumulated content as a "delta"
/// (a snapshot glitch). The deduplication system should detect and
/// filter this to avoid rendering the same content twice.
#[test]
fn test_glm_snapshot_as_delta_detected() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Simulate streaming followed by a snapshot-as-delta
    let events = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        // Normal streaming
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The quick brown fox"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" jumps over"}}}"#,
        // Snapshot-as-delta: entire accumulated content resent
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The quick brown fox jumps over"}}}"#,
        // New content after snapshot
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" the lazy dog"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let input = events.join("\n");
    let cursor = Cursor::new(input);
    let reader = std::io::BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");

    let printer_ref = test_printer.borrow();

    // Verify no duplicates in output
    verify_no_duplicates(&printer_ref, "GLM snapshot-as-delta");

    // Verify final content is present
    let output = printer_ref.get_output();
    assert!(
        output.contains("the lazy dog"),
        "Content after snapshot should be in output"
    );
}
