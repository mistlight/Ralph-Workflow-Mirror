//! Unified streaming state tracking module.
//!
//! This module provides a single source of truth for streaming state across
//! all parsers (`Claude`, `Codex`, `Gemini`, `OpenCode`). It implements the streaming
//! contract:
//!
//! # Streaming Contract
//!
//! 1. **Delta contract**: Each streaming event contains only newly generated text
//! 2. **Message lifecycle**: `MessageStart` → (`ContentBlockStart` + deltas)* → `MessageStop`
//! 3. **Deduplication rule**: Content displayed during streaming is never re-displayed
//! 4. **State reset**: Streaming state resets on `MessageStart`/Init events
//!
//! # Stream Contract
//!
//! This module enforces a strict **delta contract** - all streaming events must
//! contain only the newly generated text (deltas), not the full accumulated content.
//!
//! Treating snapshots as deltas causes exponential duplication bugs. The session
//! validates that incoming content is genuinely delta-sized and rejects likely
//! snapshot-as-delta violations.
//!
//! # Example
//!
//! ```ignore
//! use crate::json_parser::streaming_state::{StreamingSession, StreamingState};
//!
//! let mut session = StreamingSession::new();
//!
//! // Message starts - reset state
//! session.on_message_start();
//!
//! // Content block starts
//! session.on_content_block_start(0);
//!
//! // Text deltas arrive - accumulate and display
//! let should_show_prefix = session.on_text_delta(0, "Hello");
//! assert!(should_show_prefix); // First chunk shows prefix
//!
//! let should_show_prefix = session.on_text_delta(0, " World");
//! assert!(!should_show_prefix); // Subsequent chunks don't show prefix
//!
//! // Check if content was already streamed (for deduplication)
//! assert!(session.has_any_streamed_content());
//!
//! // Message stops - finalize
//! session.on_message_stop();
//! ```

use crate::json_parser::types::ContentType;
use std::collections::HashMap;

/// Ralph enforces a **delta contract** for all streaming content.
///
/// Every streaming event must contain only the newly generated text (delta),
/// never the full accumulated content (snapshot).
///
/// # Contract Violations
///
/// If a parser emits snapshot-style content when deltas are expected, it will
/// cause exponential duplication bugs. The `StreamingSession` validates that
/// incoming content is delta-sized and logs warnings when violations are detected.
///
/// # Validation Threshold
///
/// Deltas are expected to be small chunks (typically < 100 chars). If a single
/// "delta" exceeds `SNAPSHOT_THRESHOLD` characters, it may indicate a snapshot
/// being treated as a delta.
///
/// # Pattern Detection
///
/// In addition to size threshold, we track patterns of repeated large content
/// which may indicate a snapshot-as-delta bug where the same content is being
/// sent repeatedly as if it were incremental.
const SNAPSHOT_THRESHOLD: usize = 200;

/// Streaming state for the current message lifecycle.
///
/// Tracks whether we're in the middle of streaming content and whether
/// that content has been displayed to the user.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum StreamingState {
    /// No active streaming - idle state
    #[default]
    Idle,
    /// Currently streaming content deltas
    Streaming,
    /// Content has been finalized (after `MessageStop` or equivalent)
    Finalized,
}

/// Unified streaming session tracker.
///
/// Provides a single source of truth for streaming state across all parsers.
/// Tracks:
/// - Current streaming state (`Idle`/`Streaming`/`Finalized`)
/// - Which content types have been streamed
/// - Accumulated content by content type and index
/// - Whether prefix should be shown on next delta
/// - Delta size patterns for detecting snapshot-as-delta violations
///
/// # Lifecycle
///
/// 1. **Start**: `on_message_start()` - resets all state
/// 2. **Stream**: `on_text_delta()` / `on_thinking_delta()` - accumulate content
/// 3. **Stop**: `on_message_stop()` - finalize the message
/// 4. **Repeat**: Back to step 1 for next message
#[derive(Debug, Default, Clone)]
pub struct StreamingSession {
    /// Current streaming state
    state: StreamingState,
    /// Track which content types have been streamed (for deduplication)
    /// Maps `ContentType` → whether it has been streamed
    streamed_types: HashMap<ContentType, bool>,
    /// Track whether we're currently inside a content block
    in_content_block: bool,
    /// Accumulated content by (`content_type`, index) for display
    /// This mirrors `DeltaAccumulator` but adds deduplication tracking
    accumulated: HashMap<(ContentType, String), String>,
    /// Track the order of keys for `most_recent` operations
    key_order: Vec<(ContentType, String)>,
    /// Track recent delta sizes for pattern detection
    /// Maps `(content_type, key)` → vec of recent delta sizes
    delta_sizes: HashMap<(ContentType, String), Vec<usize>>,
    /// Maximum number of delta sizes to track per key
    max_delta_history: usize,
}

impl StreamingSession {
    /// Create a new streaming session.
    pub fn new() -> Self {
        Self {
            max_delta_history: 10,
            ..Default::default()
        }
    }

    /// Reset the session on new message start.
    ///
    /// This should be called when:
    /// - Claude: `MessageStart` event
    /// - Codex: `TurnStarted` event
    /// - Gemini: `init` event or new message
    /// - `OpenCode`: New part starts
    pub fn on_message_start(&mut self) {
        self.state = StreamingState::Idle;
        self.streamed_types.clear();
        self.in_content_block = false;
        self.accumulated.clear();
        self.key_order.clear();
        self.delta_sizes.clear();
    }

    /// Mark the start of a content block.
    ///
    /// This should be called when:
    /// - Claude: `ContentBlockStart` event
    /// - Codex: `ItemStarted` with relevant type
    /// - Gemini: Content section begins
    /// - `OpenCode`: Part with content starts
    ///
    /// # Arguments
    /// * `index` - The content block index (for multi-block messages)
    pub fn on_content_block_start(&mut self, index: u64) {
        // Clear accumulated content for this specific index
        let index_str = index.to_string();
        for content_type in [
            ContentType::Text,
            ContentType::Thinking,
            ContentType::ToolInput,
        ] {
            let key = (content_type, index_str.clone());
            self.accumulated.remove(&key);
            self.key_order.retain(|k| k != &key);
        }
    }

    /// Assert that the session is in a valid lifecycle state.
    ///
    /// In debug builds, this will panic if the current state doesn't match
    /// any of the expected states. In release builds, this does nothing.
    ///
    /// # Arguments
    /// * `expected` - Slice of acceptable states
    fn assert_lifecycle_state(&self, expected: &[StreamingState]) {
        #[cfg(debug_assertions)]
        assert!(
            expected.contains(&self.state),
            "Invalid lifecycle state: expected {:?}, got {:?}. \
            This indicates a bug in the parser's event handling.",
            expected,
            self.state
        );
    }

    /// Process a text delta and return whether prefix should be shown.
    ///
    /// # Arguments
    /// * `index` - The content block index
    /// * `delta` - The text delta to accumulate
    ///
    /// # Returns
    /// * `true` - Show prefix with this delta (first chunk)
    /// * `false` - Don't show prefix (subsequent chunks)
    pub fn on_text_delta(&mut self, index: u64, delta: &str) -> bool {
        self.on_text_delta_key(&index.to_string(), delta)
    }

    /// Process a text delta with a string key and return whether prefix should be shown.
    ///
    /// This variant is for parsers that use string keys instead of numeric indices
    /// (e.g., Codex uses `agent_msg`, `reasoning`; Gemini uses `main`; `OpenCode` uses `main`).
    ///
    /// # Delta Validation
    ///
    /// This method validates that incoming content appears to be a genuine delta
    /// (small chunk) rather than a snapshot (full accumulated content). Large "deltas"
    /// that exceed `SNAPSHOT_THRESHOLD` trigger a warning as they may indicate a
    /// contract violation.
    ///
    /// Additionally, we track patterns of delta sizes to detect repeated large
    /// content being sent as if it were incremental (a common snapshot-as-delta bug).
    ///
    /// # Arguments
    /// * `key` - The content key (e.g., `main`, `agent_msg`, `reasoning`)
    /// * `delta` - The text delta to accumulate
    ///
    /// # Returns
    /// * `true` - Show prefix with this delta (first chunk)
    /// * `false` - Don't show prefix (subsequent chunks)
    pub fn on_text_delta_key(&mut self, key: &str, delta: &str) -> bool {
        // Lifecycle enforcement: deltas should only arrive during streaming
        // or idle (first delta starts streaming), never after finalization
        self.assert_lifecycle_state(&[StreamingState::Idle, StreamingState::Streaming]);

        let delta_size = delta.len();

        // Track delta size for pattern detection
        let content_key = (ContentType::Text, key.to_string());
        let sizes = self.delta_sizes.entry(content_key.clone()).or_default();
        sizes.push(delta_size);

        // Keep only the most recent delta sizes
        if sizes.len() > self.max_delta_history {
            sizes.remove(0);
        }

        // Validate delta size - warn if it looks like a snapshot
        if delta_size > SNAPSHOT_THRESHOLD {
            // This is a potential snapshot-as-delta bug
            // Log via eprintln since we don't have a logger here
            eprintln!(
                "Warning: Large delta ({delta_size} chars) for key '{key}'. \
                This may indicate a snapshot being treated as a delta, \
                which can cause exponential duplication bugs."
            );
        }

        // Pattern detection: Check if we're seeing repeated large deltas
        // This indicates the same content is being sent repeatedly (snapshot-as-delta)
        if sizes.len() >= 3 {
            // Check if at least 3 of the last N deltas were large
            let large_count = sizes.iter().filter(|&&s| s > SNAPSHOT_THRESHOLD).count();
            if large_count >= 3 {
                eprintln!(
                    "Warning: Detected pattern of {large_count} large deltas for key '{key}'. \
                    This strongly suggests a snapshot-as-delta bug where the same \
                    large content is being sent repeatedly. File: streaming_state.rs, Line: {}",
                    line!()
                );
            }
        }

        // Mark that we're streaming text content
        self.streamed_types.insert(ContentType::Text, true);
        self.state = StreamingState::Streaming;
        self.in_content_block = true;

        // Check if this is the first delta for this key
        let is_first = !self.accumulated.contains_key(&content_key);

        // Accumulate the delta
        self.accumulated
            .entry(content_key.clone())
            .and_modify(|buf| buf.push_str(delta))
            .or_insert_with(|| delta.to_string());

        // Track order
        if is_first {
            self.key_order.push(content_key);
        }

        // Show prefix only on the very first delta
        is_first
    }

    /// Process a thinking delta and return whether prefix should be shown.
    ///
    /// # Arguments
    /// * `index` - The content block index
    /// * `delta` - The thinking delta to accumulate
    ///
    /// # Returns
    /// * `true` - Show prefix with this delta (first chunk)
    /// * `false` - Don't show prefix (subsequent chunks)
    pub fn on_thinking_delta(&mut self, index: u64, delta: &str) -> bool {
        self.on_thinking_delta_key(&index.to_string(), delta)
    }

    /// Process a thinking delta with a string key and return whether prefix should be shown.
    ///
    /// This variant is for parsers that use string keys instead of numeric indices.
    ///
    /// # Arguments
    /// * `key` - The content key (e.g., "reasoning")
    /// * `delta` - The thinking delta to accumulate
    ///
    /// # Returns
    /// * `true` - Show prefix with this delta (first chunk)
    /// * `false` - Don't show prefix (subsequent chunks)
    pub fn on_thinking_delta_key(&mut self, key: &str, delta: &str) -> bool {
        // Mark that we're streaming thinking content
        self.streamed_types.insert(ContentType::Thinking, true);
        self.state = StreamingState::Streaming;

        // Get the key for this content
        let content_key = (ContentType::Thinking, key.to_string());

        // Check if this is the first delta for this key
        let is_first = !self.accumulated.contains_key(&content_key);

        // Accumulate the delta
        self.accumulated
            .entry(content_key.clone())
            .and_modify(|buf| buf.push_str(delta))
            .or_insert_with(|| delta.to_string());

        // Track order
        if is_first {
            self.key_order.push(content_key);
        }

        is_first
    }

    /// Process a tool input delta.
    ///
    /// # Arguments
    /// * `index` - The content block index
    /// * `delta` - The tool input delta to accumulate
    pub fn on_tool_input_delta(&mut self, index: u64, delta: &str) {
        // Mark that we're streaming tool input
        self.streamed_types.insert(ContentType::ToolInput, true);
        self.state = StreamingState::Streaming;

        // Get the key for this content
        let key = (ContentType::ToolInput, index.to_string());

        // Accumulate the delta
        self.accumulated
            .entry(key.clone())
            .and_modify(|buf| buf.push_str(delta))
            .or_insert_with(|| delta.to_string());

        // Track order
        if !self.key_order.contains(&key) {
            self.key_order.push(key);
        }
    }

    /// Finalize the message on stop event.
    ///
    /// This should be called when:
    /// - Claude: `MessageStop` event
    /// - Codex: `TurnCompleted` or `ItemCompleted` with text
    /// - Gemini: Message completion
    /// - `OpenCode`: Part completion
    #[expect(clippy::missing_const_for_fn)]
    pub fn on_message_stop(&mut self) -> bool {
        let was_in_block = self.in_content_block;
        self.state = StreamingState::Finalized;
        self.in_content_block = false;

        was_in_block
    }

    /// Check if ANY content has been streamed for this message.
    ///
    /// This is a broader check that returns true if ANY content type
    /// has been streamed. Used to skip entire message display when
    /// all content was already streamed.
    pub fn has_any_streamed_content(&self) -> bool {
        !self.streamed_types.is_empty()
    }

    /// Get accumulated content for a specific type and index.
    ///
    /// # Arguments
    /// * `content_type` - The type of content
    /// * `index` - The content index (as string for flexibility)
    ///
    /// # Returns
    /// * `Some(text)` - Accumulated content
    /// * `None` - No content accumulated for this key
    pub fn get_accumulated(&self, content_type: ContentType, index: &str) -> Option<&str> {
        self.accumulated
            .get(&(content_type, index.to_string()))
            .map(std::string::String::as_str)
    }

    /// Clear content for a specific index (for content block reuse).
    #[cfg(test)]
    pub fn clear_index(&mut self, index: u64) {
        let index_str = index.to_string();
        for content_type in [
            ContentType::Text,
            ContentType::Thinking,
            ContentType::ToolInput,
        ] {
            let key = (content_type, index_str.clone());
            self.accumulated.remove(&key);
            self.key_order.retain(|k| k != &key);
        }
    }

    /// Check if incoming text is likely a snapshot (full accumulated content) rather than a delta.
    ///
    /// This performs content-based detection by checking if the incoming text starts with
    /// the previously accumulated content. This catches snapshot-as-delta violations that
    /// are within size limits but still represent full accumulated content.
    ///
    /// # Arguments
    /// * `text` - The incoming text to check
    /// * `key` - The content key to compare against
    ///
    /// # Returns
    /// * `true` - The text appears to be a snapshot (starts with previous accumulated content)
    /// * `false` - The text appears to be a genuine delta
    pub fn is_likely_snapshot(&self, text: &str, key: &str) -> bool {
        let content_key = (ContentType::Text, key.to_string());

        // Check if we have accumulated content for this key
        if let Some(previous) = self.accumulated.get(&content_key) {
            // A snapshot would start with the previous accumulated content
            // Only consider it a snapshot if:
            // 1. The incoming text is at least as long as previous content
            // 2. The incoming text starts with the exact previous content
            // 3. The incoming text is not identical to previous content (that's just a duplicate, not a snapshot)
            if text.len() > previous.len() && text.starts_with(previous) {
                return true;
            }
        }

        false
    }

    /// Extract the delta portion from a snapshot.
    ///
    /// When a snapshot is detected (full accumulated content sent as a "delta"),
    /// this method extracts only the new portion that hasn't been accumulated yet.
    ///
    /// # Arguments
    /// * `text` - The snapshot text (full accumulated content + new content)
    /// * `key` - The content key to compare against
    ///
    /// # Returns
    /// * The delta portion (new content only)
    ///
    /// # Panics
    /// Panics if the text is not actually a snapshot (doesn't start with accumulated content).
    /// Callers should check with `is_likely_snapshot()` first.
    ///
    /// # Note
    /// Returns the length of the delta portion as `usize` since we can't return
    /// a reference to `text` with the correct lifetime. Callers can slice `text`
    /// themselves using `&text[delta_len..]`.
    pub fn extract_delta_from_snapshot(&self, text: &str, key: &str) -> usize {
        let content_key = (ContentType::Text, key.to_string());

        if let Some(previous) = self.accumulated.get(&content_key) {
            if text.starts_with(previous) {
                return previous.len();
            }
        }

        // If we get here, the text wasn't actually a snapshot
        // This indicates a bug in the caller's logic
        panic!(
            "extract_delta_from_snapshot called on non-snapshot text. \
            key={key:?}, text={text:?}. This is a bug - callers must check is_likely_snapshot first."
        );
    }

    /// Get the delta portion as a string slice from a snapshot.
    ///
    /// This is a convenience wrapper that returns the actual substring
    /// instead of just the length.
    ///
    /// # Panics
    /// Panics if the text is not actually a snapshot.
    pub fn get_delta_from_snapshot<'a>(&self, text: &'a str, key: &str) -> &'a str {
        let delta_len = self.extract_delta_from_snapshot(text, key);
        &text[delta_len..]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_session_lifecycle() {
        let mut session = StreamingSession::new();

        // Initially no content streamed
        assert!(!session.has_any_streamed_content());

        // Message start
        session.on_message_start();
        assert!(!session.has_any_streamed_content());

        // Text delta
        let show_prefix = session.on_text_delta(0, "Hello");
        assert!(show_prefix);
        assert!(session.has_any_streamed_content());

        // Another delta
        let show_prefix = session.on_text_delta(0, " World");
        assert!(!show_prefix);

        // Message stop
        let was_in_block = session.on_message_stop();
        assert!(was_in_block);
    }

    #[test]
    fn test_accumulated_content() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        session.on_text_delta(0, "Hello");
        session.on_text_delta(0, " World");

        let accumulated = session.get_accumulated(ContentType::Text, "0");
        assert_eq!(accumulated, Some("Hello World"));
    }

    #[test]
    fn test_reset_between_messages() {
        let mut session = StreamingSession::new();

        // First message
        session.on_message_start();
        session.on_text_delta(0, "First");
        assert!(session.has_any_streamed_content());
        session.on_message_stop();

        // Second message - state should be reset
        session.on_message_start();
        assert!(!session.has_any_streamed_content());
    }

    #[test]
    fn test_multiple_indices() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        session.on_text_delta(0, "First block");
        session.on_text_delta(1, "Second block");

        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some("First block")
        );
        assert_eq!(
            session.get_accumulated(ContentType::Text, "1"),
            Some("Second block")
        );
    }

    #[test]
    fn test_clear_index() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        session.on_text_delta(0, "Before");
        session.clear_index(0);
        session.on_text_delta(0, "After");

        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some("After")
        );
    }

    #[test]
    fn test_delta_validation_warns_on_large_delta() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Create a delta larger than SNAPSHOT_THRESHOLD
        let large_delta = "x".repeat(SNAPSHOT_THRESHOLD + 1);

        // This should trigger a warning but still work
        let show_prefix = session.on_text_delta(0, &large_delta);
        assert!(show_prefix);

        // Content should still be accumulated correctly
        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some(large_delta.as_str())
        );
    }

    #[test]
    fn test_delta_validation_no_warning_for_small_delta() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Small delta should not trigger warning
        let small_delta = "Hello, world!";
        let show_prefix = session.on_text_delta(0, small_delta);
        assert!(show_prefix);

        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some(small_delta)
        );
    }

    // Tests for snapshot-as-delta detection methods

    #[test]
    fn test_is_likely_snapshot_detects_snapshot() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_content_block_start(0);

        // First delta is "Hello"
        session.on_text_delta(0, "Hello");

        // Simulate GLM sending full accumulated content as next "delta"
        // "Hello World" contains "Hello" at the start
        let is_snapshot = session.is_likely_snapshot("Hello World", "0");
        assert!(is_snapshot, "Should detect snapshot-as-delta");
    }

    #[test]
    fn test_is_likely_snapshot_returns_false_for_genuine_delta() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_content_block_start(0);

        // First delta is "Hello"
        session.on_text_delta(0, "Hello");

        // Genuine delta " World" doesn't start with previous content
        let is_snapshot = session.is_likely_snapshot(" World", "0");
        assert!(
            !is_snapshot,
            "Genuine delta should not be flagged as snapshot"
        );
    }

    #[test]
    fn test_is_likely_snapshot_returns_false_when_no_previous_content() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_content_block_start(0);

        // No previous content, so anything is a genuine first delta
        let is_snapshot = session.is_likely_snapshot("Hello", "0");
        assert!(
            !is_snapshot,
            "First delta should not be flagged as snapshot"
        );
    }

    #[test]
    fn test_extract_delta_from_snapshot() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_content_block_start(0);

        // First delta is "Hello"
        session.on_text_delta(0, "Hello");

        // Snapshot "Hello World" should extract " World" as delta
        let delta = session.get_delta_from_snapshot("Hello World", "0");
        assert_eq!(delta, " World");
    }

    #[test]
    fn test_extract_delta_from_snapshot_empty_delta() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_content_block_start(0);

        // First delta is "Hello"
        session.on_text_delta(0, "Hello");

        // Snapshot "Hello" (identical to previous) should extract "" as delta
        let delta = session.get_delta_from_snapshot("Hello", "0");
        assert_eq!(delta, "");
    }

    #[test]
    #[should_panic(expected = "extract_delta_from_snapshot called on non-snapshot text")]
    fn test_extract_delta_from_snapshot_panics_on_non_snapshot() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_content_block_start(0);

        // First delta is "Hello"
        session.on_text_delta(0, "Hello");

        // Calling on non-snapshot should panic
        session.get_delta_from_snapshot("World", "0");
    }

    #[test]
    fn test_snapshot_detection_with_string_keys() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Test with string keys (like Codex/Gemini use)
        session.on_text_delta_key("main", "Hello");

        // Should detect snapshot for string key
        let is_snapshot = session.is_likely_snapshot("Hello World", "main");
        assert!(is_snapshot);

        // Should extract delta correctly
        let delta = session.get_delta_from_snapshot("Hello World", "main");
        assert_eq!(delta, " World");
    }
}
