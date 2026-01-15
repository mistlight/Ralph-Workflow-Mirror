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

use crate::json_parser::health::StreamingQualityMetrics;
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
/// - Persistent "output started" tracking independent of accumulated content
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

        // Check if we're transitioning to a different index BEFORE finalizing.
        // This is important because some agents (e.g., GLM) may send ContentBlockStart
        // repeatedly for the same index, and we should NOT clear accumulated content
        // in that case (which would cause the next delta to show prefix again).
        let (is_same_index, old_index) = match &self.current_block {
            ContentBlockState::NotInBlock => (false, None),
            ContentBlockState::InBlock {
                index: current_index,
                ..
            } => (current_index == &index_str, Some(current_index.clone())),
        };

        // Finalize previous block if we're in one
        self.ensure_content_block_finalized();

        // Only clear accumulated content if transitioning to a DIFFERENT index.
        // We clear the OLD index's content, not the new one.
        if !is_same_index {
            if let Some(old) = old_index {
                for content_type in [
                    ContentType::Text,
                    ContentType::Thinking,
                    ContentType::ToolInput,
                ] {
                    let key = (content_type, old.clone());
                    self.accumulated.remove(&key);
                    self.key_order.retain(|k| k != &key);
                }
            }
        }
    }

    /// Ensure the current content block is finalized.
    ///
    /// If we're in a block and output has started, this returns true to indicate
    /// that a newline should be emitted. This prevents "glued text" bugs where
    /// content from different blocks is concatenated without separation.
    ///
    /// # Block Finalization
    ///
    /// Called by:
    /// - `on_content_block_start()` - when transitioning to a new block
    /// - `on_message_stop()` - when the message completes
    ///
    /// The return value indicates whether a visual separator (newline) should
    /// be emitted. Currently, this is a simple boolean based on `started_output`.
    /// See [`ContentBlockState`] for future enhancement notes on type-aware separators.
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
            self.get_delta_from_snapshot(delta, key)
                .map_or_else(|_| delta.to_string(), str::to_string)
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

        // Accumulate the delta (using auto-repaired delta if snapshot was detected)
        self.accumulated
            .entry(content_key.clone())
            .and_modify(|buf| buf.push_str(&actual_delta))
            .or_insert_with(|| actual_delta);

        // Track order
        self.key_order.push(content_key);

        // First delta for each key shows prefix
        true
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

        // Accumulate the delta
        self.accumulated
            .entry(content_key.clone())
            .and_modify(|buf| buf.push_str(delta))
            .or_insert_with(|| delta.to_string());

        // Track order
        self.key_order.push(content_key);

        // First delta for each key shows prefix
        true
    }

    /// Process a tool input delta.
    ///
    /// # Arguments
    /// * `index` - The content block index
    /// * `delta` - The tool input delta to accumulate
    ///
    /// # Returns
    /// * `true` - Show prefix with this delta (first chunk)
    /// * `false` - Don't show prefix (subsequent chunks)
    pub fn on_tool_input_delta(&mut self, index: u64, delta: &str) -> bool {
        self.on_tool_input_delta_key(&index.to_string(), delta)
    }

    /// Process a tool input delta with a string key.
    ///
    /// # Arguments
    /// * `key` - The content key
    /// * `delta` - The tool input delta to accumulate
    ///
    /// # Returns
    /// * `true` - Show prefix with this delta (first chunk)
    /// * `false` - Don't show prefix (subsequent chunks)
    pub fn on_tool_input_delta_key(&mut self, key: &str, delta: &str) -> bool {
        // Mark that we're streaming tool input
        self.streamed_types.insert(ContentType::ToolInput, true);
        self.state = StreamingState::Streaming;

        // Get the key for this content
        let content_key = (ContentType::ToolInput, key.to_string());

        // Accumulate the delta
        self.accumulated
            .entry(content_key.clone())
            .and_modify(|buf| buf.push_str(delta))
            .or_insert_with(|| delta.to_string());

        // Track order
        self.key_order.push(content_key);

        // First delta for each key shows prefix
        true
    }

    /// Mark the end of a message.
    ///
    /// This should be called when:
    /// - Claude: `MessageStop` event
    /// - Codex: `TurnCompleted` or `TurnFailed` event
    /// - Gemini: Result event or message completion
    /// - `OpenCode`: Part completion
    ///
    /// # Returns
    /// * `true` - Content was streamed (completion newline needed)
    /// * `false` - No content was streamed (no completion needed)
    pub fn on_message_stop(&mut self) -> bool {
        // Finalize the current content block
        let was_in_block = self.ensure_content_block_finalized();

        // Mark message as displayed if we have a message ID
        if let Some(ref message_id) = self.current_message_id {
            self.displayed_final_messages.insert(message_id.clone());
            self.last_finalized_message_id = Some(message_id.clone());
        }

        // Update state
        self.state = StreamingState::Finalized;

        was_in_block
    }

    /// Get accumulated content for a content type and key.
    ///
    /// # Arguments
    /// * `content_type` - The type of content (`Text`, `Thinking`, `ToolInput`)
    /// * `key` - The content key (index or string identifier)
    ///
    /// # Returns
    /// * `Some(content)` - The accumulated content
    /// * `None` - No content accumulated for this key
    pub fn get_accumulated(&self, content_type: ContentType, key: &str) -> Option<&str> {
        self.accumulated
            .get(&(content_type, key.to_string()))
            .map(std::string::String::as_str)
    }

    /// Check if any content has been streamed.
    ///
    /// This is used for deduplication - if content was already streamed
    /// during delta updates, the final message should not be displayed.
    ///
    /// # Returns
    /// * `true` - At least one content type has been streamed
    /// * `false` - No content has been streamed
    pub fn has_any_streamed_content(&self) -> bool {
        !self.streamed_types.is_empty()
    }

    /// Detect if text is likely a snapshot rather than a genuine delta.
    ///
    /// Snapshot detection uses content-based heuristics:
    /// - Text starts with previously accumulated content
    /// - Text is significantly larger than expected delta size
    ///
    /// # Arguments
    /// * `text` - The incoming text to check
    /// * `key` - The content key to compare against
    ///
    /// # Returns
    /// * `true` - Text appears to be a snapshot (contains accumulated content)
    /// * `false` - Text appears to be a genuine delta
    pub fn is_likely_snapshot(&self, text: &str, key: &str) -> bool {
        // Get previous accumulated content
        let content_key = (ContentType::Text, key.to_string());
        let previous = self.accumulated.get(&content_key);

        if let Some(prev) = previous {
            // Check if text starts with previous content
            if text.starts_with(prev.as_str()) {
                return true;
            }

            // Check for fuzzy match (handles minor formatting differences)
            if Self::is_fuzzy_snapshot_match(text, prev) {
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
    fn is_fuzzy_snapshot_match(text: &str, previous: &str) -> bool {
        // For very short previous content, skip fuzzy matching to avoid false positives
        if previous.len() < 20 {
            return false;
        }

        // Check if previous is contained within text
        if text.contains(previous) {
            // Calculate overlap ratio using integer arithmetic
            // >85% means previous * 100 > text * 85
            let previous_hundredths = previous.len().saturating_mul(100);
            let text_85_hundredths = text.len().saturating_mul(85);
            // If >85% of the incoming text is the previous content, it's likely a snapshot
            // (High threshold to avoid false positives while catching true snapshots)
            previous_hundredths > text_85_hundredths
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
    /// * `Ok(delta)` - The delta portion (new content only)
    /// * `Err` - If the text is not actually a snapshot (doesn't start with accumulated content)
    pub fn get_delta_from_snapshot<'a>(
        &self,
        text: &'a str,
        key: &str,
    ) -> Result<&'a str, &'static str> {
        let content_key = (ContentType::Text, key.to_string());
        let previous = self
            .accumulated
            .get(&content_key)
            .ok_or("No accumulated content for this key")?;

        if !text.starts_with(previous.as_str()) {
            return Err("Text does not start with accumulated content");
        }

        // Extract the portion after the accumulated content
        Ok(&text[previous.len()..])
    }

    /// Get streaming quality metrics for debugging.
    ///
    /// This provides insights into the streaming session quality, including
    /// delta sizes, patterns, and potential issues.
    ///
    /// # Returns
    /// Aggregated metrics across all content types and keys.
    pub fn get_streaming_quality_metrics(&self) -> StreamingQualityMetrics {
        // Flatten all delta sizes across all content types and keys
        let all_sizes = self.delta_sizes.values().flat_map(|v| v.iter().copied());
        StreamingQualityMetrics::from_sizes(all_sizes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_session() -> StreamingSession {
        StreamingSession::new()
    }

    #[test]
    fn test_new_session_is_idle() {
        let session = create_session();
        assert_eq!(session.state, StreamingState::Idle);
    }

    #[test]
    fn test_message_start_resets_state() {
        let mut session = create_session();
        session.state = StreamingState::Streaming;
        session.on_message_start();
        assert_eq!(session.state, StreamingState::Idle);
    }

    #[test]
    fn test_text_delta_starts_streaming() {
        let mut session = create_session();
        session.on_message_start();
        session.on_text_delta(0, "Hello");
        assert_eq!(session.state, StreamingState::Streaming);
    }

    #[test]
    fn test_thinking_delta_starts_streaming() {
        let mut session = create_session();
        session.on_message_start();
        session.on_thinking_delta(0, "Thinking...");
        assert_eq!(session.state, StreamingState::Streaming);
    }

    #[test]
    fn test_message_stop_finalizes() {
        let mut session = create_session();
        session.on_message_start();
        session.on_text_delta(0, "Hello");
        session.on_message_stop();
        assert_eq!(session.state, StreamingState::Finalized);
    }

    #[test]
    fn test_accumulated_content() {
        let mut session = create_session();
        session.on_message_start();
        session.on_text_delta(0, "Hello");
        session.on_text_delta(0, " World");

        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some("Hello World")
        );
    }

    #[test]
    fn test_has_streamed_content() {
        let mut session = create_session();
        assert!(!session.has_any_streamed_content());

        session.on_message_start();
        session.on_text_delta(0, "Hello");
        assert!(session.has_any_streamed_content());
    }

    #[test]
    fn test_message_id_tracking() {
        let mut session = create_session();
        session.set_current_message_id(Some("msg-1".to_string()));
        assert_eq!(session.get_current_message_id(), Some("msg-1"));
    }

    #[test]
    fn test_duplicate_message_detection() {
        let mut session = create_session();

        // First message
        session.set_current_message_id(Some("msg-1".to_string()));
        session.on_message_start();
        session.on_message_stop();

        // Check duplicate detection
        assert!(session.is_duplicate_final_message("msg-1"));
        assert!(!session.is_duplicate_final_message("msg-2"));
    }

    #[test]
    fn test_get_streaming_quality_metrics() {
        let mut session = create_session();
        session.on_message_start();
        session.on_text_delta(0, "Hello");

        let metrics = session.get_streaming_quality_metrics();
        assert_eq!(metrics.total_deltas, 1);
    }

    #[test]
    fn test_get_streaming_quality_metrics_reset_on_message_start() {
        let mut session = create_session();

        // First message
        session.on_message_start();
        session.on_text_delta(0, "First");
        session.on_message_stop();

        // Reset for second message
        session.on_message_start();

        // Metrics should be cleared
        let metrics = session.get_streaming_quality_metrics();
        assert_eq!(
            metrics.total_deltas, 0,
            "Metrics should reset on new message"
        );
    }
}
