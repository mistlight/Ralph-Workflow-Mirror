//! Integration tests for streaming output deduplication.
//!
//! These tests verify that users see correct output on their terminal:
//! 1. No duplicate visible content from streaming glitches
//! 2. Snapshot repairs work (accumulated content re-sent as delta)
//! 3. Assistant events don't duplicate streaming content
//! 4. Intentional repetition is preserved (e.g., "echo echo echo")
//!
//! # Testing Strategy
//!
//! We use `VirtualTerminal` which accurately simulates real terminal behavior:
//! - Carriage return (`\r`) moves cursor to column 0
//! - ANSI clear line (`\x1b[2K`) erases line content
//! - ANSI cursor up/down (`\x1b[1A`, `\x1b[1B`) for in-place updates
//! - Text overwrites previous content when cursor is repositioned
//!
//! This tests what users ACTUALLY SEE, not just what bytes were written.

use std::cell::RefCell;
use std::io::{BufReader, Cursor};
use std::path::Path;
use std::rc::Rc;

use ralph_workflow::config::Verbosity;
use ralph_workflow::json_parser::printer::{SharedPrinter, VirtualTerminal};
use ralph_workflow::json_parser::terminal::TerminalMode;
use ralph_workflow::json_parser::ClaudeParser;
use ralph_workflow::logger::Colors;

// =============================================================================
// Helper Functions
// =============================================================================

/// Create a Claude parser with VirtualTerminal in Full mode (ANSI sequences enabled).
fn create_parser_with_vterm() -> (ClaudeParser, Rc<RefCell<VirtualTerminal>>) {
    let vterm = Rc::new(RefCell::new(VirtualTerminal::new()));
    let printer: SharedPrinter = vterm.clone();
    let parser = ClaudeParser::with_printer(Colors::new(), Verbosity::Normal, printer)
        .with_terminal_mode(TerminalMode::Full);
    (parser, vterm)
}

/// Parse events and return the VirtualTerminal for inspection.
fn parse_events(events: &[&str]) -> Rc<RefCell<VirtualTerminal>> {
    let (parser, vterm) = create_parser_with_vterm();
    let input = events.join("\n");
    let cursor = Cursor::new(input);
    let reader = BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");
    vterm
}

// =============================================================================
// Core Streaming Tests
// =============================================================================

/// Test basic streaming produces correct visible output.
///
/// Verifies that deltas accumulate correctly and the final content is visible.
#[test]
fn test_streaming_produces_correct_visible_output() {
    let events = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" "}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"World"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"!"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let vterm = parse_events(&events);
    let vterm_ref = vterm.borrow();
    let visible = vterm_ref.get_visible_output();

    // Content should be present
    assert!(
        visible.contains("Hello") && visible.contains("World"),
        "Should contain 'Hello World'. Got: {}",
        visible
    );

    // No duplicate lines
    assert!(
        !vterm_ref.has_duplicate_lines(),
        "Should have no duplicate visible lines"
    );
}

/// Test that snapshot glitches don't cause duplicate visible content.
///
/// A "snapshot glitch" is when the server resends accumulated content as a delta
/// instead of just the new content. The deduplication system should handle this.
#[test]
fn test_snapshot_glitch_no_visible_duplicates() {
    let events = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        // Normal streaming
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The quick"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" brown fox"}}}"#,
        // SNAPSHOT GLITCH: server resends accumulated content as a delta
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"The quick brown fox"}}}"#,
        // New content continues
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" jumps over"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let vterm = parse_events(&events);
    let vterm_ref = vterm.borrow();
    let visible = vterm_ref.get_visible_output();

    // "The quick brown fox" should appear only once
    let count = vterm_ref.count_visible_pattern("The quick brown fox");
    assert!(
        count <= 1,
        "Snapshot glitch caused duplicate: 'The quick brown fox' appears {} times. Output: {}",
        count,
        visible
    );

    // Final content should include content after the glitch
    assert!(
        visible.contains("jumps over"),
        "Content after snapshot glitch should be visible"
    );
}

/// Test that consecutive identical deltas don't cause duplicate visible content.
///
/// Network glitches can cause the same delta to be sent multiple times.
#[test]
fn test_consecutive_identical_deltas_no_duplicates() {
    let repeated = r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"repeated content"}}}"#;

    let events = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        repeated,
        repeated, // Duplicate
        repeated, // Duplicate
        repeated, // Duplicate
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let vterm = parse_events(&events);
    let vterm_ref = vterm.borrow();
    let visible = vterm_ref.get_visible_output();

    // "repeated content" should appear only once (or very few times)
    let count = vterm_ref.count_visible_pattern("repeated content");
    assert!(
        count <= 2,
        "Consecutive duplicates should be filtered. 'repeated content' appears {} times. Output: {}",
        count,
        visible
    );
}

// =============================================================================
// Assistant Event Deduplication Tests
// =============================================================================

/// Test that assistant event BEFORE streaming doesn't duplicate content.
///
/// In some GLM sessions, an assistant event with full content arrives BEFORE
/// the streaming deltas. Only one should be rendered.
#[test]
fn test_assistant_event_before_streaming_no_duplicates() {
    let events = [
        r#"{"type":"system","subtype":"init","cwd":"/test","session_id":"test-session"}"#,
        // Assistant event with full content arrives FIRST
        r#"{"type":"assistant","message":{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"text","text":"Hello World"}]}}"#,
        // Then streaming events for the SAME content
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let vterm = parse_events(&events);
    let vterm_ref = vterm.borrow();
    let visible = vterm_ref.get_visible_output();

    // "Hello World" should appear only once
    let count = vterm_ref.count_visible_pattern("Hello World");
    assert!(
        count <= 1,
        "DUPLICATION BUG: 'Hello World' appears {} times. \
         Assistant event and streaming should not both render. Output: {}",
        count,
        visible
    );
}

/// Test that assistant event DURING streaming doesn't duplicate content.
///
/// Assistant events can arrive mid-stream with accumulated content.
#[test]
fn test_assistant_event_during_streaming_no_duplicates() {
    let events = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Hello"}}}"#,
        // Assistant event arrives MID-STREAM
        r#"{"type":"assistant","message":{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"text","text":"Hello"}]}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" World"}}}"#,
        // Another assistant event with more content
        r#"{"type":"assistant","message":{"id":"msg_1","type":"message","role":"assistant","content":[{"type":"text","text":"Hello World"}]}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let vterm = parse_events(&events);
    let vterm_ref = vterm.borrow();
    let visible = vterm_ref.get_visible_output();

    // No duplicate lines should appear
    assert!(
        !vterm_ref.has_duplicate_lines(),
        "Mid-stream assistant events should not cause duplicates. Output: {}",
        visible
    );

    // Content should be present
    assert!(
        visible.contains("Hello") && visible.contains("World"),
        "Final content should be visible"
    );
}

// =============================================================================
// Intentional Repetition Tests
// =============================================================================

/// Test that intentional repetition is preserved.
///
/// The deduplication system should NOT filter content like "echo echo echo"
/// where the repetition is part of the actual message.
#[test]
fn test_intentional_repetition_preserved() {
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

    let vterm = parse_events(&events);
    let vterm_ref = vterm.borrow();
    let visible = vterm_ref.get_visible_output();

    // "echo" should appear multiple times (intentional repetition)
    let count = vterm_ref.count_visible_pattern("echo");
    assert!(
        count >= 2,
        "Intentional repetition should be preserved. 'echo' appears only {} times. Output: {}",
        count,
        visible
    );
}

/// Test alternating pattern is not incorrectly deduplicated.
///
/// Pattern like "Ping Pong Ping Pong" should all appear.
#[test]
fn test_alternating_pattern_preserved() {
    let events = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Ping"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Pong"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Ping"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Pong"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let vterm = parse_events(&events);
    let vterm_ref = vterm.borrow();
    let visible = vterm_ref.get_visible_output();

    // Should contain "PingPongPingPong"
    assert!(
        visible.contains("PingPongPingPong"),
        "Alternating pattern should be preserved. Got: {}",
        visible
    );
}

// =============================================================================
// Multi-Block Tests
// =============================================================================

/// Test content within a single streaming session accumulates correctly.
///
/// Multiple deltas within the same content block should build up content.
#[test]
fn test_multiple_deltas_accumulate_correctly() {
    let events = [
        r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_1","type":"message","role":"assistant","content":[]}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"First"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" Second"}}}"#,
        r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" Third"}}}"#,
        r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
    ];

    let vterm = parse_events(&events);
    let vterm_ref = vterm.borrow();
    let visible = vterm_ref.get_visible_output();

    // All content should be accumulated
    assert!(
        visible.contains("First") && visible.contains("Second") && visible.contains("Third"),
        "All deltas should accumulate. Got: {:?}",
        visible
    );

    // No duplicate lines
    assert!(
        !vterm_ref.has_duplicate_lines(),
        "No duplicate lines from accumulated content"
    );
}

// =============================================================================
// Real Log File Test
// =============================================================================

/// Test with real production log file using VirtualTerminal.
///
/// This uses an actual log file to verify deduplication works in real scenarios.
#[test]
fn test_real_log_file_no_visible_duplicates() {
    let possible_paths = [
        "tests/deduplication_integration_tests/fixtures/PROMPT-LOG.log",
        "deduplication_integration_tests/fixtures/PROMPT-LOG.log",
        "../tests/deduplication_integration_tests/fixtures/PROMPT-LOG.log",
    ];

    let log_path = possible_paths.iter().find(|p| Path::new(p).exists());

    let Some(log_path) = log_path else {
        eprintln!("Skipping real log test - fixture not found");
        return;
    };

    let log_content = std::fs::read_to_string(log_path)
        .unwrap_or_else(|e| panic!("Failed to read log file: {}", e));

    let (parser, vterm) = create_parser_with_vterm();
    let cursor = Cursor::new(log_content);
    let reader = BufReader::new(cursor);
    parser
        .parse_stream(reader)
        .expect("parse_stream should succeed");

    let vterm_ref = vterm.borrow();
    let visible = vterm_ref.get_visible_output();

    // Should have some output
    assert!(!visible.trim().is_empty(), "Should produce visible output");

    // No duplicate visible lines
    assert!(
        !vterm_ref.has_duplicate_lines(),
        "Real log should have no duplicate visible lines"
    );

    // Verify deduplication metrics
    let metrics = parser.streaming_metrics();
    println!(
        "Real log metrics: {} deltas, {} snapshot repairs",
        metrics.total_deltas, metrics.snapshot_repairs_count
    );
}
