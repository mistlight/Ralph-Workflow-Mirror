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
}

impl StreamingSession {
    /// Create a new streaming session.
    pub fn new() -> Self {
        Self::default()
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
    /// # Arguments
    /// * `key` - The content key (e.g., `main`, `agent_msg`, `reasoning`)
    /// * `delta` - The text delta to accumulate
    ///
    /// # Returns
    /// * `true` - Show prefix with this delta (first chunk)
    /// * `false` - Don't show prefix (subsequent chunks)
    pub fn on_text_delta_key(&mut self, key: &str, delta: &str) -> bool {
        // Mark that we're streaming text content
        self.streamed_types.insert(ContentType::Text, true);
        self.state = StreamingState::Streaming;
        self.in_content_block = true;

        // Get the key for this content
        let content_key = (ContentType::Text, key.to_string());

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
}
