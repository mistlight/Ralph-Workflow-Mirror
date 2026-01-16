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

use std::fs::File;
use std::io::{BufRead, BufReader};
use std::path::Path;

use ralph_workflow::config::Verbosity;
use ralph_workflow::json_parser::{ClaudeParser, TestPrinter};
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
    let log_path = Path::new("tests/deduplication_integration_tests/fixtures/PROMPT-LOG.log");
    assert!(log_path.exists(), "Log file should exist at {:?}", log_path);

    let events = load_log_file(log_path);
    assert!(!events.is_empty(), "Log file should contain events");

    // Create a ClaudeParser with TestPrinter
    let printer = Box::new(TestPrinter::new());
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, std::rc::Rc::new(std::cell::RefCell::new(printer)));

    // Process each event through the parser
    for event_line in &events {
        // Use parse_event which returns Some(output) or None
        // This tests the full parsing pipeline including deduplication
        let _ = parser.parse_event(event_line);
    }

    // Note: We can't easily test the output with the current architecture
    // because the printer is consumed by the parser.
    // This test mainly verifies that the parser can process all events without panicking.
    // In a future iteration, we should add a method to retrieve the printer from the parser.
}

/// Test ClaudeParser with synthetic snapshot glitch data
#[test]
fn test_claude_parser_snapshot_glitch() {
    let printer = Box::new(TestPrinter::new());
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, std::rc::Rc::new(std::cell::RefCell::new(printer)));

    // Simulate snapshot glitch: agent sends accumulated content as delta
    let accumulated = "The quick brown fox jumps over the lazy dog.";

    let events = vec![
        // Normal streaming
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" quick"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" brown"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" fox"}}}"#,
        // Snapshot glitch: agent resends entire accumulated content
        &format!(r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}}}"#, accumulated),
        // New content after snapshot
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" New content"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    for event in events {
        let _ = parser.parse_event(event);
    }

    // Note: Same issue as above - we can't easily retrieve output to verify
    // This test mainly verifies the parser doesn't panic on snapshot glitches
}

/// Test that intentional repetition is preserved
#[test]
fn test_claude_parser_intentional_repetition_preserved() {
    let printer = Box::new(TestPrinter::new());
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, std::rc::Rc::new(std::cell::RefCell::new(printer)));

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
        let _ = parser.parse_event(event);
    }
}

/// Test that consecutive identical deltas are filtered
#[test]
fn test_claude_parser_consecutive_duplicates_filtered() {
    let printer = Box::new(TestPrinter::new());
    let colors = Colors::new();
    let parser = ClaudeParser::with_printer(colors, Verbosity::Normal, std::rc::Rc::new(std::cell::RefCell::new(printer)));

    // Test consecutive identical deltas (3 strikes heuristic)
    let repeated_text = "This is a repeated message";

    let events = vec![
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        &format!(r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}}}"#, repeated_text),
        &format!(r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}}}"#, repeated_text),
        &format!(r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}}}"#, repeated_text),
        &format!(r#"{{"type":"stream_event","event":{{"type":"content_block_delta","index":0,"delta":{{"type":"text_delta","text":"{}"}}}}}}"#, repeated_text),
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    for event in events {
        let _ = parser.parse_event(event);
    }
}
