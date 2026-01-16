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

use std::cell::RefCell;
use std::fs::File;
use std::io::{BufRead, BufReader, Write};
use std::path::Path;
use std::rc::Rc;

use ralph_workflow::config::Verbosity;
use ralph_workflow::json_parser::{ClaudeParser, SharedPrinter, TestPrinter};
use ralph_workflow::logger::Colors;

/// Load and parse a log file in NDJSON format
fn load_log_file(path: &Path) -> Vec<String> {
    let file = File::open(path).unwrap_or_else(|_| panic!("Failed to open log file: {:?}", path));
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for line_result in reader.lines() {
        if let Ok(line) = line_result {
            if !line.trim().is_empty() {
                events.push(line);
            }
        }
    }

    events
}

/// Test ClaudeParser with real log data to detect duplicate output
#[test]
fn test_claude_parser_no_duplicates_with_real_log() {
    // Load the real log file
    // Try multiple possible paths
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

    let events = load_log_file(Path::new(log_path));
    assert!(!events.is_empty(), "Log file should contain events");

    // Create a TestPrinter and wrap it as SharedPrinter
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Process each event through the parser
    for event_line in &events {
        // Use parse_event which returns Some(output) or None
        // This tests the full parsing pipeline including deduplication
        if let Some(output) = parser.parse_event(event_line) {
            // Write the output to the printer (simulating what parse_stream does)
            let mut printer_ref = test_printer.borrow_mut();
            let _ = write!(printer_ref, "{output}");
            let _ = printer_ref.flush();
        }
    }

    // Now we can verify the actual output using the original test_printer reference
    let printer_ref = test_printer.borrow();

    // Check for duplicate consecutive lines in the rendered output
    let duplicates = printer_ref.find_duplicate_consecutive_lines();
    if !duplicates.is_empty() {
        panic!(
            "Found duplicate consecutive lines in output:\n{:?}\n\nFull output:\n{}",
            duplicates,
            printer_ref.get_output()
        );
    }

    // Additional sanity checks
    let output = printer_ref.get_output();

    // Verify we got some output (not empty)
    assert!(
        !output.trim().is_empty(),
        "Parser should produce some output"
    );

    // Verify output contains expected patterns
    assert!(
        output.contains("[Claude]"),
        "Output should contain Claude prefix"
    );

    // Log summary for debugging
    println!(
        "Processed {} events, output length: {} chars, {} lines",
        events.len(),
        output.len(),
        printer_ref.get_lines().len()
    );
}

/// Test ClaudeParser with synthetic snapshot glitch data
#[test]
fn test_claude_parser_snapshot_glitch() {
    // Use Rc pattern to retain test printer access
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Simulate snapshot glitch: agent sends accumulated content as delta
    let accumulated = "The quick brown fox jumps over the lazy dog.";
    let snapshot_event = format!(
        r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}}}"#,
        accumulated
    );

    let events: Vec<&str> = vec![
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

    for event in events {
        if let Some(output) = parser.parse_event(event) {
            let mut printer_ref = test_printer.borrow_mut();
            let _ = write!(printer_ref, "{output}");
            let _ = printer_ref.flush();
        }
    }

    // Verify no duplicates occurred
    let printer_ref = test_printer.borrow();
    let duplicates = printer_ref.find_duplicate_consecutive_lines();
    assert!(
        duplicates.is_empty(),
        "Snapshot glitch should not cause duplicates: {:?}\nOutput:\n{}",
        duplicates,
        printer_ref.get_output()
    );

    // Verify the final content includes both the glitched portion and new content
    let output = printer_ref.get_output();
    assert!(
        output.contains("New content"),
        "Output should contain content after snapshot glitch"
    );
}

/// Test that intentional repetition is preserved
#[test]
fn test_claude_parser_intentional_repetition_preserved() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Test "echo echo echo" pattern - should be preserved
    let events = vec![
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"echo"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" "}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"echo"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" "}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"echo"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    for event in events {
        if let Some(output) = parser.parse_event(event) {
            let mut printer_ref = test_printer.borrow_mut();
            let _ = write!(printer_ref, "{output}");
            let _ = printer_ref.flush();
        }
    }

    // Verify the intentional repetition appears in output
    let printer_ref = test_printer.borrow();
    let output = printer_ref.get_output();

    // The output should contain "echo echo echo" (with spaces)
    assert!(
        output.contains("echo") && output.contains("echo echo"),
        "Intentional repetition should be preserved in output"
    );

    // But we should NOT have duplicate consecutive lines
    let duplicates = printer_ref.find_duplicate_consecutive_lines();
    assert!(
        duplicates.is_empty(),
        "Should not have duplicate consecutive lines: {:?}",
        duplicates
    );
}

/// Test that consecutive identical deltas are filtered
#[test]
fn test_claude_parser_consecutive_duplicates_filtered() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    // Test consecutive identical deltas (3 strikes heuristic)
    let repeated_text = "This is a repeated message";
    let repeated_event = format!(
        r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}}}"#,
        repeated_text
    );

    let events: Vec<&str> = vec![
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        &repeated_event,
        &repeated_event,
        &repeated_event,
        &repeated_event,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    for event in events {
        if let Some(output) = parser.parse_event(event) {
            let mut printer_ref = test_printer.borrow_mut();
            let _ = write!(printer_ref, "{output}");
            let _ = printer_ref.flush();
        }
    }

    // Verify no duplicate consecutive lines
    let printer_ref = test_printer.borrow();
    let duplicates = printer_ref.find_duplicate_consecutive_lines();
    assert!(
        duplicates.is_empty(),
        "Consecutive identical deltas should be filtered: {:?}\nOutput:\n{}",
        duplicates,
        printer_ref.get_output()
    );

    // Verify the content appears at least once
    let output = printer_ref.get_output();
    assert!(
        output.contains("This is a repeated message"),
        "Content should appear in output"
    );
}

/// Test that the parser correctly handles content blocks with no deltas
#[test]
fn test_claude_parser_empty_content_block() {
    let test_printer = Rc::new(RefCell::new(TestPrinter::new()));
    let printer: SharedPrinter = test_printer.clone();
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, printer);

    let events = vec![
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    for event in events {
        if let Some(output) = parser.parse_event(event) {
            let mut printer_ref = test_printer.borrow_mut();
            let _ = write!(printer_ref, "{output}");
            let _ = printer_ref.flush();
        }
    }

    // Empty content block should produce no duplicate lines
    let printer_ref = test_printer.borrow();
    assert!(
        !printer_ref.has_duplicate_consecutive_lines(),
        "Empty content block should not produce duplicates"
    );
}
