//! Integration tests for streaming output deduplication.
//!
//! These tests verify that users see correct output on their terminal:
//! 1. No duplicate visible content from streaming glitches
//! 2. Snapshot repairs work (accumulated content re-sent as delta)
//! 3. Assistant events don't duplicate streaming content
//! 4. Intentional repetition is preserved (e.g., "echo echo echo")
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (visible terminal output), not internal state
//! - Uses `VirtualTerminal` to mock at architectural boundary (terminal I/O)
//! - Tests are deterministic and isolated
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

use crate::test_timeout::with_default_timeout;
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
/// This verifies that when streaming deltas are received, they accumulate
/// correctly and the final content is visible without duplicates.
#[test]
fn test_streaming_produces_correct_visible_output() {
    with_default_timeout(|| {
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
    });
}

/// Test that snapshot glitches don't cause duplicate visible content.
///
/// This verifies that when the server resends accumulated content as a delta,
/// the deduplication system prevents duplicate visible content.
#[test]
fn test_snapshot_glitch_no_visible_duplicates() {
    with_default_timeout(|| {
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
    });
}

/// Test that consecutive identical deltas don't cause duplicate visible content.
///
/// This verifies that when network glitches cause duplicate deltas,
/// the deduplication system filters them to prevent visible duplicates.
#[test]
fn test_consecutive_identical_deltas_no_duplicates() {
    with_default_timeout(|| {
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
    });
}

// =============================================================================
// Assistant Event Deduplication Tests
// =============================================================================

/// Test that assistant event BEFORE streaming doesn't duplicate content.
///
/// This verifies that when an assistant event arrives before streaming deltas,
/// only one is rendered to prevent duplicate visible content.
#[test]
fn test_assistant_event_before_streaming_no_duplicates() {
    with_default_timeout(|| {
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
    });
}

/// Test that assistant event DURING streaming doesn't duplicate content.
///
/// This verifies that when assistant events arrive mid-stream with accumulated content,
/// they don't cause duplicate visible content in the terminal.
#[test]
fn test_assistant_event_during_streaming_no_duplicates() {
    with_default_timeout(|| {
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
    });
}

/// Test GLM-style multiple assistant events with tool use content.
///
/// This reproduces the specific GLM/CCS bug pattern where:
/// 1. MessageStart with id
/// 2. Assistant event arrives with tool_use content and same id
/// 3. More assistant events arrive with the same id
/// 4. The bug causes the tool_use to be displayed multiple times
#[test]
fn test_glm_multiple_assistant_events_same_id_no_duplicates() {
    with_default_timeout(|| {
        let events = [
            // GLM-style: MessageStart with tool_use
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_glm_001","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_123","name":"Read","input":{}}]}}}"#,
            // ContentBlockStart for the tool
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_123","name":"Read","input":{}}}}"#,
            // Delta with partial input
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"file_path\":\"/test/file.txt\"}"}}}"#,
            // Assistant event with full content (GLM sends this during streaming)
            r#"{"type":"assistant","message":{"id":"msg_glm_001","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_123","name":"Read","input":{"file_path":"/test/file.txt"}}]}}"#,
            // ContentBlockStop
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            // MessageStop
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ];

        let vterm = parse_events(&events);
        let vterm_ref = vterm.borrow();
        let visible = vterm_ref.get_visible_output();

        // "Read" (the tool name) should appear only once
        let tool_count = vterm_ref.count_visible_pattern("Tool");
        assert!(
            tool_count <= 1,
            "GLM BUG: Tool appears {} times. Assistant event should not duplicate streaming output. Output: {}",
            tool_count,
            visible
        );

        // File path should appear only once
        let path_count = vterm_ref.count_visible_pattern("/test/file.txt");
        assert!(
            path_count <= 1,
            "GLM BUG: File path appears {} times. Output: {}",
            path_count,
            visible
        );
    });
}

/// Test GLM-style multiple content blocks in single assistant event.
///
/// This reproduces the GLM pattern where assistant events include ALL accumulated
/// content blocks (text + tool_uses) in a single event. GLM sends these updates
/// as it accumulates more content blocks, and each assistant event should not
/// re-render content that was already displayed.
#[test]
fn test_glm_assistant_event_with_multiple_content_blocks() {
    with_default_timeout(|| {
        let events = [
            // MessageStart with initial text content
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_glm_004","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"text","text":"Let me explore"}]}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
            // First streaming delta
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Let me explore the codebase"}}}"#,
            // Assistant event arrives with text content
            r#"{"type":"assistant","message":{"id":"msg_glm_004","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"text","text":"Let me explore the codebase"}]}}"#,
            // Second content block starts (tool_use)
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call_abc","name":"Read","input":{}}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"file_path\":\"/test.txt\"}"}}}"#,
            // Assistant event arrives with BOTH text and tool_use (GLM pattern)
            r#"{"type":"assistant","message":{"id":"msg_glm_004","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"text","text":"Let me explore the codebase"},{"type":"tool_use","id":"call_abc","name":"Read","input":{"file_path":"/test.txt"}}]}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ];

        let vterm = parse_events(&events);
        let vterm_ref = vterm.borrow();
        let visible = vterm_ref.get_visible_output();

        // "Let me explore the codebase" should appear only once or very few times
        let text_count = vterm_ref.count_visible_pattern("Let me explore the codebase");
        assert!(
            text_count <= 2,
            "GLM BUG: 'Let me explore the codebase' appears {} times. Assistant events with multiple content blocks should not cause excessive duplication. Output: {}",
            text_count,
            visible
        );

        // "Read" tool should appear only once
        let tool_count = vterm_ref.count_visible_pattern("Tool");
        assert!(
            tool_count <= 1,
            "GLM BUG: Tool appears {} times. Output: {}",
            tool_count,
            visible
        );
    });
}

/// Test GLM-style assistant event with ONLY tool_use (no text).
///
/// This tests the specific case where GLM sends an assistant event containing
/// only tool_use blocks (no text content). This is a common pattern for GLM
/// when it makes tool calls. The hash-based deduplication only checks text
/// content, so tool_use blocks need special handling.
#[test]
fn test_glm_assistant_event_only_tool_use_blocks() {
    with_default_timeout(|| {
        let events = [
            // MessageStart with tool_use in content
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_glm_005","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_xyz","name":"Glob","input":{}}]}}}"#,
            // ContentBlockStart for tool_use
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_xyz","name":"Glob","input":{}}}}"#,
            // Delta with tool input
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"pattern\":\"**/*.rs\"}"}}}"#,
            // Assistant event arrives with tool_use content (GLM pattern)
            r#"{"type":"assistant","message":{"id":"msg_glm_005","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_xyz","name":"Glob","input":{"pattern":"**/*.rs"}}]}}"#,
            // ContentBlockStop
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            // MessageStop
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ];

        let vterm = parse_events(&events);
        let vterm_ref = vterm.borrow();
        let visible = vterm_ref.get_visible_output();

        // "Glob" (the tool name) should appear only once
        let tool_count = vterm_ref.count_visible_pattern("Tool");
        assert!(
            tool_count <= 1,
            "GLM BUG: Tool appears {} times. Assistant event with tool_use should not duplicate streaming output. Output: {}",
            tool_count,
            visible
        );

        // Pattern should appear only once (or few times for in-place updates)
        let pattern_count = vterm_ref.count_visible_pattern("**/*.rs");
        assert!(
            pattern_count <= 2,
            "GLM BUG: Pattern '**/*.rs' appears {} times. Output: {}",
            pattern_count,
            visible
        );
    });
}

/// Test GLM bug: assistant event with DIFFERENT message_id but same content.
///
/// This reproduces a potential GLM bug where the assistant event has a different
/// message_id than the streaming events (perhaps due to CCS layer transformation).
/// In this case, message_id matching fails, and we rely on hash-based deduplication.
///
/// The bug: `is_duplicate_by_hash` only checks TEXT content, ignoring tool_use.
/// This means assistant events with tool_use but no text will NOT be deduplicated.
#[test]
fn test_glm_assistant_event_different_message_id_tool_use_only() {
    with_default_timeout(|| {
        let events = [
            // MessageStart with one message_id
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_streaming_123","type":"message","role":"assistant","model":"glm-4.7","content":[]}}}"#,
            // ContentBlockStart for tool_use
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_test","name":"Read","input":{}}}}"#,
            // Delta with tool input
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"file_path\":\"/test/file.txt\"}"}}}"#,
            // Assistant event arrives with DIFFERENT message_id (CCS layer may have changed it)
            // but same tool_use content. This should still be deduplicated!
            r#"{"type":"assistant","message":{"id":"msg_assistant_456","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_test","name":"Read","input":{"file_path":"/test/file.txt"}}]}}"#,
            // ContentBlockStop
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            // MessageStop
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ];

        let vterm = parse_events(&events);
        let vterm_ref = vterm.borrow();
        let visible = vterm_ref.get_visible_output();

        // "Read" tool should appear only once
        let tool_count = vterm_ref.count_visible_pattern("Tool");
        assert!(
            tool_count <= 1,
            "GLM BUG: Tool appears {} times. Assistant event with different message_id but same content should still be deduplicated. Output: {}",
            tool_count,
            visible
        );

        // File path should appear only once
        let path_count = vterm_ref.count_visible_pattern("/test/file.txt");
        assert!(
            path_count <= 2,
            "GLM BUG: File path '/test/file.txt' appears {} times. Output: {}",
            path_count,
            visible
        );
    });
}

/// Test GLM bug: assistant event with text + tool_use, different message_id.
///
/// This tests the case where GLM sends an assistant event with BOTH text and tool_use,
/// but with a different message_id than the streaming events. The hash-based deduplication
/// only checks the text portion, so the tool_use portion might get duplicated.
#[test]
fn test_glm_assistant_event_text_and_tool_different_message_id() {
    with_default_timeout(|| {
        let events = [
            // MessageStart with one message_id, includes text in content
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_streaming_789","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"text","text":"I'll read the file"}]}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
            // Text streaming
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"I'll read the file now"}}}"#,
            // Tool use starts
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":1,"content_block":{"type":"tool_use","id":"call_abc","name":"Read","input":{}}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":1,"delta":{"type":"input_json_delta","partial_json":"{\"file_path\":\"/data.txt\"}"}}}"#,
            // Assistant event arrives with DIFFERENT message_id, same text AND tool_use
            // The hash check will match the text, but tool_use might be duplicated
            r#"{"type":"assistant","message":{"id":"msg_assistant_999","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"text","text":"I'll read the file now"},{"type":"tool_use","id":"call_abc","name":"Read","input":{"file_path":"/data.txt"}}]}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":1}}"#,
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ];

        let vterm = parse_events(&events);
        let vterm_ref = vterm.borrow();
        let visible = vterm_ref.get_visible_output();

        // Text should not be duplicated
        let text_count = vterm_ref.count_visible_pattern("I'll read the file now");
        assert!(
            text_count <= 2,
            "GLM BUG: Text appears {} times. Output: {}",
            text_count,
            visible
        );

        // Tool should appear only once
        let tool_count = vterm_ref.count_visible_pattern("Tool");
        assert!(
            tool_count <= 1,
            "GLM BUG: Tool appears {} times. Tool use should be deduplicated even with different message_id. Output: {}",
            tool_count,
            visible
        );
    });
}

/// Test GLM-style assistant events before and after streaming.
///
/// This tests the pattern where GLM sends:
/// 1. Assistant event BEFORE streaming starts
/// 2. MessageStart (with same id)
/// 3. Streaming deltas
/// 4. Additional assistant events DURING streaming
#[test]
fn test_glm_assistant_event_before_and_during_streaming() {
    with_default_timeout(|| {
        let events = [
            // Assistant event arrives FIRST (pre-streaming)
            r#"{"type":"assistant","message":{"id":"msg_glm_002","type":"message","role":"assistant","content":[{"type":"text","text":"Let me explore"}]}}"#,
            // Then MessageStart with same id
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_glm_002","type":"message","role":"assistant","content":[]}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
            // Streaming deltas that extend the content
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Let me explore"}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":" the codebase"}}}"#,
            // Another assistant event mid-stream with accumulated content
            r#"{"type":"assistant","message":{"id":"msg_glm_002","type":"message","role":"assistant","content":[{"type":"text","text":"Let me explore the codebase"}]}}"#,
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ];

        let vterm = parse_events(&events);
        let vterm_ref = vterm.borrow();
        let visible = vterm_ref.get_visible_output();

        // "Let me explore" should appear only once (or very few times due to in-place updates)
        let count = vterm_ref.count_visible_pattern("Let me explore");
        assert!(
            count <= 2, // Allow some margin for in-place updates
            "GLM BUG: 'Let me explore' appears {} times. Assistant events and streaming should not both render. Output: {}",
            count,
            visible
        );

        // Final content should be present
        assert!(
            visible.contains("Let me explore"),
            "Final content should be visible. Got: {}",
            visible
        );
    });
}

/// Test GLM-style repeated MessageStart events with assistant events.
///
/// GLM has been observed to send multiple MessageStart events with the same
/// message_id, interleaved with assistant events. This test verifies that
/// deduplication still works correctly.
#[test]
fn test_glm_repeated_message_start_with_assistant_events() {
    with_default_timeout(|| {
        let events = [
            // First MessageStart
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_glm_003","type":"message","role":"assistant","content":[{"type":"text","text":"Test"}]}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
            // Assistant event
            r#"{"type":"assistant","message":{"id":"msg_glm_003","type":"message","role":"assistant","content":[{"type":"text","text":"Test Content"}]}}"#,
            // Second MessageStart (GLM behavior - repeated)
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_glm_003","type":"message","role":"assistant","content":[]}}}"#,
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"text","text":""}}}"#,
            // Streaming deltas
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"text_delta","text":"Test Content"}}}"#,
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ];

        let vterm = parse_events(&events);
        let vterm_ref = vterm.borrow();
        let visible = vterm_ref.get_visible_output();

        // "Test Content" should not be duplicated excessively
        let count = vterm_ref.count_visible_pattern("Test Content");
        assert!(
            count <= 2,
            "GLM BUG: 'Test Content' appears {} times. Repeated MessageStart with assistant events should not cause excessive duplication. Output: {}",
            count,
            visible
        );
    });
}

// =============================================================================
// Intentional Repetition Tests
// =============================================================================

/// Test that intentional repetition is preserved.
///
/// This verifies that when repetition is part of the actual message content,
/// the deduplication system preserves it rather than filtering it out.
#[test]
fn test_intentional_repetition_preserved() {
    with_default_timeout(|| {
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
    });
}

/// Test alternating pattern is not incorrectly deduplicated.
///
/// This verifies that when alternating patterns like "Ping Pong Ping Pong"
/// appear in content, they are preserved and not filtered out.
#[test]
fn test_alternating_pattern_preserved() {
    with_default_timeout(|| {
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
    });
}

// =============================================================================
// Multi-Block Tests
// =============================================================================

/// Test content within a single streaming session accumulates correctly.
///
/// This verifies that when multiple deltas arrive in the same content block,
/// they build up content correctly without duplication.
#[test]
fn test_multiple_deltas_accumulate_correctly() {
    with_default_timeout(|| {
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
    });
}

// =============================================================================
// Real Log File Test
// =============================================================================

/// Test GLM bug: assistant event arrives AFTER MessageStart but BEFORE streaming deltas.
///
/// This reproduces a specific GLM bug where:
/// 1. MessageStart arrives (setting the message_id)
/// 2. ContentBlockStart arrives with tool_use but no input
/// 3. Assistant event arrives with full content (BEFORE any ToolUseDelta)
/// 4. ToolUseDelta arrives after the assistant event
///
/// The bug: When the assistant event arrives BEFORE any streaming deltas,
/// `has_any_streamed_content()` returns false, so the assistant event is rendered.
/// Then when the ToolUseDelta arrives, it's ALSO rendered, causing duplication.
#[test]
fn test_glm_assistant_event_before_streaming_deltas() {
    with_default_timeout(|| {
        let events = [
            // MessageStart with same message_id as assistant event
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_glm_bug_early","type":"message","role":"assistant","model":"glm-4.7","content":[]}}}"#,
            // ContentBlockStart with tool_use
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_123","name":"Glob"}}}"#,
            // Assistant event arrives BEFORE any ToolUseDelta (message_id matches)
            // At this point, has_any_streamed_content() is false, so this gets rendered
            r#"{"type":"assistant","message":{"id":"msg_glm_bug_early","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_123","name":"Glob","input":{"pattern":"**/*.rs"}}]}}"#,
            // ToolUseDelta arrives AFTER the assistant event
            // This should be suppressed since assistant event was already rendered
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"pattern\":\"**/*.rs\"}"}}}"#,
            // ContentBlockStop
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            // MessageStop
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ];

        let vterm = parse_events(&events);
        let vterm_ref = vterm.borrow();
        let visible = vterm_ref.get_visible_output();

        // "Glob" (the tool name) should appear only once
        let tool_count = vterm_ref.count_visible_pattern("Tool");
        assert!(
            tool_count <= 1,
            "GLM BUG: Tool appears {} times. Assistant event before streaming deltas should not cause duplication. Output: {}",
            tool_count,
            visible
        );

        // Pattern should appear only once
        let pattern_count = vterm_ref.count_visible_pattern("**/*.rs");
        assert!(
            pattern_count <= 2,
            "GLM BUG: Pattern '**/*.rs' appears {} times. Output: {}",
            pattern_count,
            visible
        );
    });
}

/// Test GLM bug: assistant event arrives BEFORE tool name is tracked, with DIFFERENT message_id.
///
/// This reproduces a specific GLM bug where:
/// 1. MessageStart arrives with empty content
/// 2. ContentBlockStart arrives with tool_use that has no input (input is None)
///    - The pattern `ContentBlock::ToolUse { name, input: Some(i) }` doesn't match
///    - So set_tool_name is NOT called
/// 3. ToolUseDelta arrives WITHOUT name field (so name is still not tracked)
/// 4. Assistant event arrives with DIFFERENT message_id (CCS layer transformation)
///    and full tool_use content
///
/// The bug: When the assistant event arrives, `tool_names` doesn't have the tool name,
/// so `is_duplicate_tool_use` produces "TOOL_USE::" instead of "TOOL_USE:Glob:...",
/// causing the hash comparison to fail and the assistant event to be rendered again.
/// Since the message_id is different, the message_id check also fails to deduplicate.
#[test]
fn test_glm_assistant_event_before_tool_name_tracked_different_id() {
    with_default_timeout(|| {
        let events = [
            // MessageStart with empty content (no tool_use block, so no name to track)
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_glm_bug_streaming","type":"message","role":"assistant","model":"glm-4.7","content":[]}}}"#,
            // ContentBlockStart with tool_use but NO input field (input is null/None)
            // The pattern `ContentBlock::ToolUse { name, input: Some(i) }` does NOT match
            // so set_tool_name is NOT called here
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_123","name":"Glob"}}}"#,
            // ToolUseDelta WITHOUT name field (GLM may not send name in delta)
            // This means set_tool_name is still NOT called
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"pattern\":\"**/*.rs\"}"}}}"#,
            // Assistant event arrives with DIFFERENT message_id (CCS layer may have changed it)
            // and full tool_use content (including name)
            // This should be deduplicated via hash, but the bug causes it to be rendered again
            r#"{"type":"assistant","message":{"id":"msg_glm_bug_assistant","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_123","name":"Glob","input":{"pattern":"**/*.rs"}}]}}"#,
            // ContentBlockStop
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            // MessageStop
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ];

        let vterm = parse_events(&events);
        let vterm_ref = vterm.borrow();
        let visible = vterm_ref.get_visible_output();

        // "Glob" (the tool name) should appear only once
        let tool_count = vterm_ref.count_visible_pattern("Tool");
        assert!(
            tool_count <= 1,
            "GLM BUG: Tool appears {} times. Assistant event should not duplicate streaming output even when tool name wasn't tracked before and message_id differs. Output: {}",
            tool_count,
            visible
        );

        // Pattern should appear only once (or few times for in-place updates)
        let pattern_count = vterm_ref.count_visible_pattern("**/*.rs");
        assert!(
            pattern_count <= 2,
            "GLM BUG: Pattern '**/*.rs' appears {} times. Output: {}",
            pattern_count,
            visible
        );
    });
}

/// Test GLM bug: assistant event arrives BEFORE tool name is tracked.
///
/// This reproduces a specific GLM bug where:
/// 1. MessageStart arrives with empty content
/// 2. ContentBlockStart arrives with tool_use that has no input (input is None)
///    - The pattern `ContentBlock::ToolUse { name, input: Some(i) }` doesn't match
///    - So set_tool_name is NOT called
/// 3. ToolUseDelta arrives WITHOUT name field (so name is still not tracked)
/// 4. Assistant event arrives with full tool_use content
///
/// The bug: When the assistant event arrives, `tool_names` doesn't have the tool name,
/// so `is_duplicate_tool_use` produces "TOOL_USE::" instead of "TOOL_USE:Glob:...",
/// causing the hash comparison to fail and the assistant event to be rendered again.
#[test]
fn test_glm_assistant_event_before_tool_name_tracked() {
    with_default_timeout(|| {
        let events = [
            // MessageStart with empty content (no tool_use block, so no name to track)
            r#"{"type":"stream_event","event":{"type":"message_start","message":{"id":"msg_glm_bug_001","type":"message","role":"assistant","model":"glm-4.7","content":[]}}}"#,
            // ContentBlockStart with tool_use but NO input field (input is null/None)
            // The pattern `ContentBlock::ToolUse { name, input: Some(i) }` does NOT match
            // so set_tool_name is NOT called here
            r#"{"type":"stream_event","event":{"type":"content_block_start","index":0,"content_block":{"type":"tool_use","id":"call_123","name":"Glob"}}}"#,
            // ToolUseDelta WITHOUT name field (GLM may not send name in delta)
            // This means set_tool_name is still NOT called
            r#"{"type":"stream_event","event":{"type":"content_block_delta","index":0,"delta":{"type":"input_json_delta","partial_json":"{\"pattern\":\"**/*.rs\"}"}}}"#,
            // Assistant event arrives with full tool_use content (including name)
            // This should be deduplicated, but the bug causes it to be rendered again
            r#"{"type":"assistant","message":{"id":"msg_glm_bug_001","type":"message","role":"assistant","model":"glm-4.7","content":[{"type":"tool_use","id":"call_123","name":"Glob","input":{"pattern":"**/*.rs"}}]}}"#,
            // ContentBlockStop
            r#"{"type":"stream_event","event":{"type":"content_block_stop","index":0}}"#,
            // MessageStop
            r#"{"type":"stream_event","event":{"type":"message_stop"}}"#,
        ];

        let vterm = parse_events(&events);
        let vterm_ref = vterm.borrow();
        let visible = vterm_ref.get_visible_output();

        // "Glob" (the tool name) should appear only once
        let tool_count = vterm_ref.count_visible_pattern("Tool");
        assert!(
            tool_count <= 1,
            "GLM BUG: Tool appears {} times. Assistant event should not duplicate streaming output even when tool name wasn't tracked before. Output: {}",
            tool_count,
            visible
        );

        // Pattern should appear only once (or few times for in-place updates)
        let pattern_count = vterm_ref.count_visible_pattern("**/*.rs");
        assert!(
            pattern_count <= 2,
            "GLM BUG: Pattern '**/*.rs' appears {} times. Output: {}",
            pattern_count,
            visible
        );
    });
}

/// Test with real production log file using VirtualTerminal.
///
/// This verifies that when an actual production log file is parsed,
/// the deduplication system prevents duplicate visible content.
#[test]
fn test_real_log_file_no_visible_duplicates() {
    with_default_timeout(|| {
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
    });
}
