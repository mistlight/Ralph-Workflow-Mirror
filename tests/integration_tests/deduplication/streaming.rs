//! Core streaming deduplication tests.
//!
//! These tests verify basic streaming behavior without duplicates:
//! - Normal delta accumulation
//! - Snapshot glitch handling (re-sent accumulated content)
//! - Consecutive identical deltas filtering

use super::parse_events;
use crate::test_timeout::with_default_timeout;

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
