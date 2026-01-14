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
//! # Message Lifecycle
//!
//! The streaming message lifecycle follows this sequence:
//!
//! 1. **`MessageStart`** (or equivalent init event):
//!    - Resets all streaming state to `Idle`
//!    - Clears accumulated content
//!    - Resets content block state
//!
//! 2. **`ContentBlockStart`** (optional, for each content block):
//!    - If already in a block with `started_output=true`, finalizes the previous block
//!    - Initializes tracking for the new block index
//!    - Clears any accumulated content for this block index
//!
//! 3. **Text/Thinking deltas** (zero or more per block):
//!    - First delta for each `(content_type, index)` shows prefix
//!    - Subsequent deltas update in-place (no prefix)
//!    - Sets `started_output=true` for the current block
//!
//! 4. **`MessageStop`**:
//!    - Finalizes the current content block
//!    - Marks message as displayed to prevent duplicate final output
//!    - Returns whether content was streamed (for emitting completion newline)
//!
//! # Content Block Transitions
//!
//! When transitioning between content blocks (e.g., block 0 → block 1):
//!
//! ```ignore
//! // Streaming "Hello" in block 0
//! session.on_text_delta(0, "Hello");  // started_output = true
//!
//! // Transition to block 1
//! session.on_content_block_start(1);  // Finalizes block 0, started_output was true
//!
//! // Stream "World" in block 1
//! session.on_text_delta(1, "World");  // New block, shows prefix again
//! ```
//!
//! The `ContentBlockState::InBlock { index, started_output }` tracks:
//! - `index`: Which block is currently active
//! - `started_output`: Whether any content was output for this block
//!
//! This state enables proper finalization of previous blocks when new ones start.
//!
//! # Delta Contract
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
use std::collections::{HashMap, HashSet};

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

/// State tracking for content blocks during streaming.
///
/// Replaces the boolean `in_content_block` with richer state that tracks
/// which block is active and whether output has started for that block.
/// This prevents "glued text" bugs where block boundaries are crossed
/// without proper finalization.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum ContentBlockState {
    /// Not currently inside a content block
    #[default]
    NotInBlock,
    /// Inside a content block with tracking for output state
    InBlock {
        /// The block index/identifier
        index: String,
        /// Whether any content has been output for this block
        started_output: bool,
    },
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
    /// Track the current content block state
    current_block: ContentBlockState,
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
    /// Track the current message ID for duplicate detection
    current_message_id: Option<String>,
    /// Track the last finalized message ID to detect duplicates
    last_finalized_message_id: Option<String>,
    /// Track which messages have been displayed to prevent duplicate final output
    displayed_final_messages: HashSet<String>,
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
    ///
    /// # Arguments
    /// * `message_id` - Optional unique identifier for this message (for deduplication)
    pub fn on_message_start(&mut self) {
        self.state = StreamingState::Idle;
        self.streamed_types.clear();
        self.current_block = ContentBlockState::NotInBlock;
        self.accumulated.clear();
        self.key_order.clear();
        self.delta_sizes.clear();
        // Note: We don't reset current_message_id here - it's set by a separate method
        // This allows for more flexible message ID handling
    }

    /// Set the current message ID for tracking.
    ///
    /// This should be called when processing a `MessageStart` event that contains
    /// a message identifier. Used to prevent duplicate display of final messages.
    ///
    /// # Arguments
    /// * `message_id` - The unique identifier for this message (or None to clear)
    pub fn set_current_message_id(&mut self, message_id: Option<String>) {
        self.current_message_id = message_id;
    }

    /// Get the current message ID.
    ///
    /// # Returns
    /// * `Some(id)` - The current message ID
    /// * `None` - No message ID is set
    pub fn get_current_message_id(&self) -> Option<&str> {
        self.current_message_id.as_deref()
    }

    /// Check if a message ID represents a duplicate final message.
    ///
    /// This prevents displaying the same message twice - once after streaming
    /// completes and again when the final "Assistant" event arrives.
    ///
    /// # Arguments
    /// * `message_id` - The message ID to check
    ///
    /// # Returns
    /// * `true` - This message has already been displayed (is a duplicate)
    /// * `false` - This is a new message
    pub fn is_duplicate_final_message(&self, message_id: &str) -> bool {
        self.displayed_final_messages.contains(message_id)
    }

    /// Mark a message as displayed to prevent duplicate display.
    ///
    /// This should be called after displaying a message's final content.
    ///
    /// # Arguments
    /// * `message_id` - The message ID to mark as displayed
    pub fn mark_message_displayed(&mut self, message_id: &str) {
        self.displayed_final_messages.insert(message_id.to_string());
        self.last_finalized_message_id = Some(message_id.to_string());
    }

    /// Mark the start of a content block.
    ///
    /// This should be called when:
    /// - Claude: `ContentBlockStart` event
    /// - Codex: `ItemStarted` with relevant type
    /// - Gemini: Content section begins
    /// - `OpenCode`: Part with content starts
    ///
    /// If we're already in a block, this method finalizes the previous block
    /// by emitting a newline if output had started.
    ///
    /// # Arguments
    /// * `index` - The content block index (for multi-block messages)
    pub fn on_content_block_start(&mut self, index: u64) {
        // Finalize previous block if we're in one
        self.ensure_content_block_finalized();

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

    /// Ensure the current content block is finalized.
    ///
    /// If we're in a block and output has started, this returns true to indicate
    /// that a newline should be emitted. This prevents "glued text" bugs where
    /// content from different blocks is concatenated without separation.
    ///
    /// # Returns
    /// * `true` - A newline should be emitted (output had started)
    /// * `false` - No newline needed (no output or not in a block)
    fn ensure_content_block_finalized(&mut self) -> bool {
        if let ContentBlockState::InBlock { started_output, .. } = &self.current_block {
            let had_output = *started_output;
            self.current_block = ContentBlockState::NotInBlock;
            had_output
        } else {
            false
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
        #[cfg(not(debug_assertions))]
        let _ = expected;
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

        // Auto-repair: Check if this is a snapshot being sent as a delta
        // Do this BEFORE any mutable borrows so we can use immutable methods.
        // Use content-based detection which is more reliable than size-based alone.
        let is_snapshot = self.is_likely_snapshot(delta, key);
        let actual_delta = if is_snapshot {
            // Extract only the new portion to prevent exponential duplication
            match self.get_delta_from_snapshot(delta, key) {
                Ok(extracted) => extracted.to_string(),
                Err(e) => {
                    // Snapshot detection had a false positive - use the original delta
                    eprintln!("Warning: Snapshot extraction failed: {e}. Using original delta.");
                    delta.to_string()
                }
            }
        } else {
            // Genuine delta - use as-is
            delta.to_string()
        };

        // Warn on large deltas BEFORE modification to detect snapshot-as-delta issues
        // Use the original delta size since that's what we actually received
        if delta_size > SNAPSHOT_THRESHOLD {
            eprintln!(
                "Warning: Large delta ({delta_size} chars) for key '{key}'. \
                This may indicate unusual streaming behavior or a snapshot being sent as a delta."
            );
        }

        // Track delta size for pattern detection (use original delta size for detection)
        let content_key = (ContentType::Text, key.to_string());
        let sizes = self.delta_sizes.entry(content_key.clone()).or_default();
        sizes.push(delta_size);

        // Keep only the most recent delta sizes
        if sizes.len() > self.max_delta_history {
            sizes.remove(0);
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

        // Update block state to track this block and mark output as started
        self.current_block = ContentBlockState::InBlock {
            index: key.to_string(),
            started_output: true,
        };

        // Check if this is the first delta for this key
        let is_first = !self.accumulated.contains_key(&content_key);

        // Accumulate the delta (using auto-repaired delta if snapshot was detected)
        self.accumulated
            .entry(content_key.clone())
            .and_modify(|buf| buf.push_str(&actual_delta))
            .or_insert_with(|| actual_delta);

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
    ///
    /// # Returns
    /// * `true` - A completion newline should be emitted (was in a content block)
    /// * `false` - No completion needed (no content block active)
    pub fn on_message_stop(&mut self) -> bool {
        let was_in_block = self.ensure_content_block_finalized();
        self.state = StreamingState::Finalized;

        // Mark the current message as displayed to prevent duplicate display
        // when the final "Assistant" event arrives
        if let Some(message_id) = self.current_message_id.clone() {
            self.mark_message_displayed(&message_id);
        }

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
            // Exact snapshot detection: text starts with previous and is longer
            if text.len() > previous.len() && text.starts_with(previous) {
                return true;
            }
            // Explicitly handle duplicate content (identical to previous)
            // This is not a snapshot, just a duplicate delta that should be ignored
            if text == *previous {
                return false;
            }

            // Fuzzy snapshot detection: check for high overlap ratio
            // Some agents may add prefixes or have minor whitespace differences
            // If the text contains most of the previous content (>80% overlap), it's likely a snapshot
            if Self::is_fuzzy_snapshot_match(text, previous) {
                return true;
            }
        }

        false
    }

    /// Check if text is a fuzzy snapshot match using overlap ratio.
    ///
    /// This handles cases where agents send snapshot-style content with minor differences
    /// like prefixes, extra whitespace, or formatting changes.
    ///
    /// Returns true if text contains >85% of previous content as a subsequence.
    #[expect(clippy::cast_precision_loss)]
    fn is_fuzzy_snapshot_match(text: &str, previous: &str) -> bool {
        // For very short previous content, skip fuzzy matching to avoid false positives
        if previous.len() < 20 {
            return false;
        }

        // Check if previous is contained within text
        if text.contains(previous) {
            // Calculate overlap ratio
            let overlap_ratio = previous.len() as f64 / text.len() as f64;
            // If >85% of the incoming text is the previous content, it's likely a snapshot
            // (High threshold to avoid false positives while catching true snapshots)
            overlap_ratio > 0.85
        } else {
            false
        }
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
    /// * `Ok(usize)` - The length of the delta portion (new content only)
    /// * `Err` - If the text is not actually a snapshot (doesn't start with accumulated content)
    ///
    /// # Note
    /// Returns the length of the delta portion as `usize` since we can't return
    /// a reference to `text` with the correct lifetime. Callers can slice `text`
    /// themselves using `&text[delta_len..]`.
    pub fn extract_delta_from_snapshot(&self, text: &str, key: &str) -> Result<usize, String> {
        let content_key = (ContentType::Text, key.to_string());

        if let Some(previous) = self.accumulated.get(&content_key) {
            // Try exact match first (previous is at the start)
            if text.starts_with(previous) {
                return Ok(previous.len());
            }

            // Try fuzzy match (previous is contained somewhere in text)
            // This handles cases where agents add prefixes before the accumulated content
            if let Some(pos) = text.find(previous) {
                let delta_start = pos + previous.len();
                return Ok(delta_start);
            }
        }

        // If we get here, the text wasn't actually a snapshot
        // This could indicate a false positive from is_likely_snapshot
        Err(format!(
            "extract_delta_from_snapshot called on non-snapshot text. \
            key={key:?}, text={text:?}. Snapshot detection may have had a false positive."
        ))
    }

    /// Get the delta portion as a string slice from a snapshot.
    ///
    /// This is a convenience wrapper that returns the actual substring
    /// instead of just the length.
    ///
    /// # Returns
    /// * `Ok(&str)` - The delta portion (new content only)
    /// * `Err` - If the text is not actually a snapshot
    pub fn get_delta_from_snapshot<'a>(&self, text: &'a str, key: &str) -> Result<&'a str, String> {
        let delta_len = self.extract_delta_from_snapshot(text, key)?;
        Ok(&text[delta_len..])
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
        let delta = session.get_delta_from_snapshot("Hello World", "0").unwrap();
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
        let delta = session.get_delta_from_snapshot("Hello", "0").unwrap();
        assert_eq!(delta, "");
    }

    #[test]
    fn test_extract_delta_from_snapshot_returns_error_on_non_snapshot() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_content_block_start(0);

        // First delta is "Hello"
        session.on_text_delta(0, "Hello");

        // Calling on non-snapshot should return error (not panic)
        let result = session.get_delta_from_snapshot("World", "0");
        assert!(result.is_err());
        assert!(result
            .unwrap_err()
            .contains("extract_delta_from_snapshot called on non-snapshot text"));
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
        let delta = session
            .get_delta_from_snapshot("Hello World", "main")
            .unwrap();
        assert_eq!(delta, " World");
    }

    // Tests for fuzzy snapshot detection

    #[test]
    fn test_fuzzy_snapshot_detection_with_prefix() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // First delta is "Hello World! This is a test message that is long enough to trigger fuzzy matching"
        let long_text =
            "Hello World! This is a test message that is long enough to trigger fuzzy matching";
        session.on_text_delta(0, long_text);

        // GLM might send: "Hello World! This is a test message that is long enough to trigger fuzzy matching and more"
        // The previous content is embedded within the new text (with minimal prefix)
        let with_prefix = format!("{long_text} and more");

        // Should detect as snapshot using fuzzy matching (overlap > 85%)
        let is_snapshot = session.is_likely_snapshot(&with_prefix, "0");
        assert!(is_snapshot, "Should detect fuzzy snapshot with prefix");

        // Should extract the delta portion (content after the previous text)
        let delta = session.get_delta_from_snapshot(&with_prefix, "0").unwrap();
        assert!(
            delta.contains("and more"),
            "Delta should contain new content"
        );
        assert!(
            !delta.contains("Hello World"),
            "Delta should not contain the previous content"
        );
    }

    #[test]
    fn test_fuzzy_snapshot_detection_no_false_positive() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // First delta is "Hello"
        session.on_text_delta(0, "Hello");

        // Genuine delta "World!" should NOT be detected as snapshot
        // even though it's short and doesn't contain much overlap
        let is_snapshot = session.is_likely_snapshot("World!", "0");
        assert!(
            !is_snapshot,
            "Genuine delta should not be flagged as fuzzy snapshot"
        );
    }

    #[test]
    fn test_fuzzy_snapshot_detection_requires_minimum_length() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Short content (below 20 char threshold)
        session.on_text_delta(0, "Hi there");

        // Even with high overlap, short content shouldn't trigger fuzzy matching
        let with_prefix = "Response: Hi there, how are you?";
        let is_snapshot = session.is_likely_snapshot(with_prefix, "0");
        assert!(
            !is_snapshot,
            "Short content should not trigger fuzzy snapshot detection"
        );
    }

    #[test]
    fn test_fuzzy_snapshot_detection_requires_high_overlap() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // First delta is "This is a moderately long message for testing fuzzy snapshot detection thresholds"
        let long_text =
            "This is a moderately long message for testing fuzzy snapshot detection thresholds";
        session.on_text_delta(0, long_text);

        // For fuzzy detection, the text must NOT start with previous (otherwise exact match triggers)
        // Create text where previous is embedded but NOT at the start
        let embedded_not_at_start = format!("Some prefix text {long_text} plus a lot more content to reduce the overlap ratio below threshold and ensure fuzzy detection does not trigger");
        let is_snapshot = session.is_likely_snapshot(&embedded_not_at_start, "0");
        assert!(
            !is_snapshot,
            "Low overlap with embedded content should not trigger fuzzy snapshot detection"
        );
    }

    #[test]
    fn test_snapshot_extraction_with_fuzzy_match() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // First delta is a long message
        let long_text =
            "This is a moderately long message for testing snapshot extraction with fuzzy matching";
        session.on_text_delta(0, long_text);

        // New text with minimal additional content (overlap > 85%)
        let with_prefix = format!("{long_text} plus extra");

        // Should detect as snapshot (overlap > 85%)
        assert!(session.is_likely_snapshot(&with_prefix, "0"));

        // Should extract delta correctly (content after the embedded previous text)
        let delta = session.get_delta_from_snapshot(&with_prefix, "0").unwrap();
        assert!(
            delta.contains("plus extra"),
            "Delta should have new content"
        );
    }

    #[test]
    fn test_snapshot_extraction_exact_match_takes_priority() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // First delta is "Hello"
        session.on_text_delta(0, "Hello");

        // Exact match: "Hello World" (starts with previous)
        let exact_match = "Hello World";
        let delta1 = session.get_delta_from_snapshot(exact_match, "0").unwrap();
        assert_eq!(delta1, " World");

        // Reset for second test
        session.on_message_start();
        session.on_text_delta(0, "Hello");

        // Fuzzy match: "Prefix: Hello World" (contains previous somewhere)
        let fuzzy_match = "Prefix: Hello World";
        let delta2 = session.get_delta_from_snapshot(fuzzy_match, "0").unwrap();
        assert!(delta2.contains(" World") || delta2.starts_with(" World"));
    }

    #[test]
    fn test_token_by_token_streaming_scenario() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Simulate token-by-token streaming
        let tokens = ["H", "e", "l", "l", "o", " ", "W", "o", "r", "l", "d", "!"];

        for token in tokens {
            let show_prefix = session.on_text_delta(0, token);

            // Only first token should show prefix
            if token == "H" {
                assert!(show_prefix, "First token should show prefix");
            } else {
                assert!(!show_prefix, "Subsequent tokens should not show prefix");
            }
        }

        // Verify accumulated content
        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some("Hello World!")
        );
    }

    #[test]
    fn test_snapshot_in_token_stream() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // First few tokens as genuine deltas
        session.on_text_delta(0, "Hello");
        session.on_text_delta(0, " World");

        // Now GLM sends a snapshot instead of delta
        let snapshot = "Hello World! This is additional content";
        assert!(
            session.is_likely_snapshot(snapshot, "0"),
            "Should detect snapshot in token stream"
        );

        // Extract delta and continue
        let delta = session.get_delta_from_snapshot(snapshot, "0").unwrap();
        assert!(delta.contains("! This is additional content"));

        // Apply the delta
        session.on_text_delta(0, delta);

        // Verify final accumulated content
        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some("Hello World! This is additional content")
        );
    }

    // Tests for message identity tracking

    #[test]
    fn test_set_and_get_current_message_id() {
        let mut session = StreamingSession::new();

        // Initially no message ID
        assert!(session.get_current_message_id().is_none());

        // Set a message ID
        session.set_current_message_id(Some("msg-123".to_string()));
        assert_eq!(session.get_current_message_id(), Some("msg-123"));

        // Clear the message ID
        session.set_current_message_id(None);
        assert!(session.get_current_message_id().is_none());
    }

    #[test]
    fn test_mark_message_displayed() {
        let mut session = StreamingSession::new();

        // Initially not marked as displayed
        assert!(!session.is_duplicate_final_message("msg-123"));

        // Mark as displayed
        session.mark_message_displayed("msg-123");
        assert!(session.is_duplicate_final_message("msg-123"));

        // Different message ID is not a duplicate
        assert!(!session.is_duplicate_final_message("msg-456"));
    }

    #[test]
    fn test_message_stop_marks_displayed() {
        let mut session = StreamingSession::new();

        // Set a message ID
        session.set_current_message_id(Some("msg-123".to_string()));

        // Start a message with content
        session.on_message_start();
        session.on_text_delta(0, "Hello");

        // Stop should mark as displayed
        session.on_message_stop();
        assert!(session.is_duplicate_final_message("msg-123"));
    }

    #[test]
    fn test_multiple_messages_tracking() {
        let mut session = StreamingSession::new();

        // First message
        session.set_current_message_id(Some("msg-1".to_string()));
        session.on_message_start();
        session.on_text_delta(0, "First");
        session.on_message_stop();
        assert!(session.is_duplicate_final_message("msg-1"));

        // Second message
        session.set_current_message_id(Some("msg-2".to_string()));
        session.on_message_start();
        session.on_text_delta(0, "Second");
        session.on_message_stop();
        assert!(session.is_duplicate_final_message("msg-1"));
        assert!(session.is_duplicate_final_message("msg-2"));
    }
}
