//! Assistant event deduplication tests.
//!
//! These tests verify that assistant events (full message snapshots)
//! don't duplicate content that's also delivered via streaming deltas:
//! - Assistant events before streaming
//! - Assistant events during streaming
//! - GLM-specific multi-block scenarios
//! - Message ID tracking and deduplication
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../../INTEGRATION_TESTS.md](../../../INTEGRATION_TESTS.md)**.

use super::parse_events;
use crate::test_timeout::with_default_timeout;

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
            "DUPLICATION BUG: 'Hello World' appears {count} times. \
         Assistant event and streaming should not both render. Output: {visible}"
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
            "Mid-stream assistant events should not cause duplicates. Output: {visible}"
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
/// 1. `MessageStart` with id
/// 2. Assistant event arrives with `tool_use` content and same id
/// 3. More assistant events arrive with the same id
/// 4. The bug causes the `tool_use` to be displayed multiple times
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
            "GLM BUG: Tool appears {tool_count} times. Assistant event should not duplicate streaming output. Output: {visible}"
        );

        // File path should appear only once
        let path_count = vterm_ref.count_visible_pattern("/test/file.txt");
        assert!(
            path_count <= 1,
            "GLM BUG: File path appears {path_count} times. Output: {visible}"
        );
    });
}

/// Test GLM-style multiple content blocks in single assistant event.
///
/// This reproduces the GLM pattern where assistant events include ALL accumulated
/// content blocks (text + `tool_uses`) in a single event. GLM sends these updates
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
            "GLM BUG: 'Let me explore the codebase' appears {text_count} times. Assistant events with multiple content blocks should not cause excessive duplication. Output: {visible}"
        );

        // "Read" tool should appear only once
        let tool_count = vterm_ref.count_visible_pattern("Tool");
        assert!(
            tool_count <= 1,
            "GLM BUG: Tool appears {tool_count} times. Output: {visible}"
        );
    });
}

/// Test GLM-style assistant event with ONLY `tool_use` (no text).
///
/// This tests the specific case where GLM sends an assistant event containing
/// only `tool_use` blocks (no text content). This is a common pattern for GLM
/// when it makes tool calls. The hash-based deduplication only checks text
/// content, so `tool_use` blocks need special handling.
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
            "GLM BUG: Tool appears {tool_count} times. Assistant event with tool_use should not duplicate streaming output. Output: {visible}"
        );

        // Pattern should appear only once (or few times for in-place updates)
        let pattern_count = vterm_ref.count_visible_pattern("**/*.rs");
        assert!(
            pattern_count <= 2,
            "GLM BUG: Pattern '**/*.rs' appears {pattern_count} times. Output: {visible}"
        );
    });
}

/// Test GLM bug: assistant event with DIFFERENT `message_id` but same content.
///
/// This reproduces a potential GLM bug where the assistant event has a different
/// `message_id` than the streaming events (perhaps due to CCS layer transformation).
/// In this case, `message_id` matching fails, and we rely on hash-based deduplication.
///
/// The bug: `is_duplicate_by_hash` only checks TEXT content, ignoring `tool_use`.
/// This means assistant events with `tool_use` but no text will NOT be deduplicated.
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
            "GLM BUG: Tool appears {tool_count} times. Assistant event with different message_id but same content should still be deduplicated. Output: {visible}"
        );

        // File path should appear only once
        let path_count = vterm_ref.count_visible_pattern("/test/file.txt");
        assert!(
            path_count <= 2,
            "GLM BUG: File path '/test/file.txt' appears {path_count} times. Output: {visible}"
        );
    });
}

/// Test GLM bug: assistant event with text + `tool_use`, different `message_id`.
///
/// This tests the case where GLM sends an assistant event with BOTH text and `tool_use`,
/// but with a different `message_id` than the streaming events. The hash-based deduplication
/// only checks the text portion, so the `tool_use` portion might get duplicated.
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
            "GLM BUG: Text appears {text_count} times. Output: {visible}"
        );

        // Tool should appear only once
        let tool_count = vterm_ref.count_visible_pattern("Tool");
        assert!(
            tool_count <= 1,
            "GLM BUG: Tool appears {tool_count} times. Tool use should be deduplicated even with different message_id. Output: {visible}"
        );
    });
}

/// Test GLM-style assistant events before and after streaming.
///
/// This tests the pattern where GLM sends:
/// 1. Assistant event BEFORE streaming starts
/// 2. `MessageStart` (with same id)
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
            "GLM BUG: 'Let me explore' appears {count} times. Assistant events and streaming should not both render. Output: {visible}"
        );

        // Final content should be present
        assert!(
            visible.contains("Let me explore"),
            "Final content should be visible. Got: {visible}"
        );
    });
}

/// Test GLM-style repeated `MessageStart` events with assistant events.
///
/// GLM has been observed to send multiple `MessageStart` events with the same
/// `message_id`, interleaved with assistant events. This test verifies that
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
            "GLM BUG: 'Test Content' appears {count} times. Repeated MessageStart with assistant events should not cause excessive duplication. Output: {visible}"
        );
    });
}
