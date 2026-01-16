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
//!    - Subsequent deltas update in-place (with prefix, using carriage return)
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

use crate::json_parser::deduplication::RollingHashWindow;
use crate::json_parser::deduplication::{get_overlap_thresholds, DeltaDeduplicator};
use crate::json_parser::health::StreamingQualityMetrics;
use crate::json_parser::types::ContentType;
use std::collections::hash_map::DefaultHasher;
use std::collections::{HashMap, HashSet};
use std::hash::{Hash, Hasher};
use std::sync::OnceLock;

// Streaming configuration constants

/// Default threshold for detecting snapshot-as-delta violations (in characters).
///
/// Deltas exceeding this size are flagged as potential snapshots. The value of 200
/// characters was chosen because:
/// - Normal deltas are typically < 100 characters (a few tokens)
/// - Snapshots often contain the full accumulated content (200+ chars)
/// - This threshold catches most violations while minimizing false positives
const DEFAULT_SNAPSHOT_THRESHOLD: usize = 200;

/// Minimum allowed snapshot threshold (in characters).
///
/// Values below 50 would cause excessive false positives for normal deltas,
/// as even small text chunks (1-2 sentences) can exceed 30 characters.
const MIN_SNAPSHOT_THRESHOLD: usize = 50;

/// Maximum allowed snapshot threshold (in characters).
///
/// Values above 1000 would allow malicious snapshots to pass undetected,
/// potentially causing exponential duplication bugs.
const MAX_SNAPSHOT_THRESHOLD: usize = 1000;

/// Minimum number of consecutive large deltas required to trigger pattern detection warning.
///
/// This threshold prevents false positives from occasional large deltas.
/// Three consecutive large deltas indicate a pattern (not a one-off event).
const DEFAULT_PATTERN_DETECTION_MIN_DELTAS: usize = 3;

/// Maximum number of delta sizes to track per content key for pattern detection.
///
/// Tracking recent delta sizes allows us to detect patterns of repeated large
/// content (a sign of snapshot-as-delta bugs). Ten entries provide sufficient
/// history without excessive memory usage.
const DEFAULT_MAX_DELTA_HISTORY: usize = 10;

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
/// "delta" exceeds `snapshot_threshold()` characters, it may indicate a snapshot
/// being treated as a delta.
///
/// # Pattern Detection
///
/// In addition to size threshold, we track patterns of repeated large content
/// which may indicate a snapshot-as-delta bug where the same content is being
/// sent repeatedly as if it were incremental.
///
/// # Environment Variables
///
/// The following environment variables can be set to configure streaming behavior:
///
/// - `RALPH_STREAMING_SNAPSHOT_THRESHOLD`: Threshold for detecting snapshot-as-delta
///   violations (default: 200). Deltas exceeding this size trigger warnings.
///
/// Get the snapshot threshold from environment variable or use default.
///
/// Reads `RALPH_STREAMING_SNAPSHOT_THRESHOLD` env var.
/// Valid range: 50-1000 characters.
/// Falls back to default of 200 if not set or out of range.
fn snapshot_threshold() -> usize {
    static THRESHOLD: OnceLock<usize> = OnceLock::new();
    *THRESHOLD.get_or_init(|| {
        std::env::var("RALPH_STREAMING_SNAPSHOT_THRESHOLD")
            .ok()
            .and_then(|s| s.parse::<usize>().ok())
            .and_then(|v| {
                if (MIN_SNAPSHOT_THRESHOLD..=MAX_SNAPSHOT_THRESHOLD).contains(&v) {
                    Some(v)
                } else {
                    None
                }
            })
            .unwrap_or(DEFAULT_SNAPSHOT_THRESHOLD)
    })
}

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
/// - Verbosity-aware warning emission
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
    /// Track which (`content_type`, key) pairs have had output started.
    /// This is independent of `accumulated` to handle cases where accumulated
    /// content may be cleared (e.g., repeated `ContentBlockStart` for same index).
    /// Cleared on `on_message_start` to ensure fresh state for each message.
    output_started_for_key: HashSet<(ContentType, String)>,
    /// Whether to emit verbose warnings about streaming anomalies.
    /// When false, suppresses diagnostic warnings that are useful for debugging
    /// but noisy in production (e.g., GLM protocol violations, snapshot detection).
    verbose_warnings: bool,
    /// Count of snapshot repairs performed during this session
    snapshot_repairs_count: usize,
    /// Count of deltas that exceeded the size threshold
    large_delta_count: usize,
    /// Count of protocol violations detected (e.g., `MessageStart` during streaming)
    protocol_violations: usize,
    /// Hash of the final streamed content (for deduplication)
    /// Computed at `message_stop` using all accumulated content
    final_content_hash: Option<u64>,
    /// Track the last rendered content for each key to detect when rendering
    /// would produce identical output (prevents visual repetition).
    /// Maps `(content_type, key)` → the last accumulated content that was rendered.
    last_rendered: HashMap<(ContentType, String), String>,
    /// Track all rendered content hashes for duplicate detection.
    /// This is preserved across `MessageStart` boundaries to prevent duplicate rendering.
    rendered_content_hashes: HashSet<u64>,
    /// Track the last delta for each key to detect exact duplicate deltas.
    /// This is preserved across `MessageStart` boundaries to prevent duplicate processing.
    /// Maps `(content_type, key)` → the last delta that was processed.
    last_delta: HashMap<(ContentType, String), String>,
    /// Track consecutive duplicates for resend glitch detection ("3 strikes" heuristic).
    /// Maps `(content_type, key)` → (count, `delta_hash`) where count tracks how many
    /// times the exact same delta has arrived consecutively. When count exceeds
    /// the threshold, the delta is dropped as a resend glitch.
    consecutive_duplicates: HashMap<(ContentType, String), (usize, u64)>,
    /// Delta deduplicator using KMP and rolling hash for snapshot detection.
    /// Provides O(n+m) guaranteed complexity for detecting snapshot-as-delta violations.
    /// Cleared on message boundaries to prevent false positives.
    deduplicator: DeltaDeduplicator,
}

impl StreamingSession {
    /// Create a new streaming session.
    pub fn new() -> Self {
        Self {
            max_delta_history: DEFAULT_MAX_DELTA_HISTORY,
            verbose_warnings: false,
            ..Default::default()
        }
    }

    /// Configure whether to emit verbose warnings about streaming anomalies.
    ///
    /// When enabled, diagnostic warnings are printed for:
    /// - Repeated `MessageStart` events (GLM protocol violations)
    /// - Large deltas that may indicate snapshot-as-delta bugs
    /// - Pattern detection of repeated large content
    ///
    /// When disabled (default), these warnings are suppressed to avoid
    /// noise in production output.
    ///
    /// # Arguments
    /// * `enabled` - Whether to enable verbose warnings
    ///
    /// # Returns
    /// The modified session for builder chaining.
    ///
    /// # Example
    ///
    /// ```ignore
    /// let mut session = StreamingSession::new().with_verbose_warnings(true);
    /// ```
    pub const fn with_verbose_warnings(mut self, enabled: bool) -> Self {
        self.verbose_warnings = enabled;
        self
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
    ///
    /// # Note on Repeated `MessageStart` Events
    ///
    /// Some agents (notably GLM/ccs-glm) send repeated `MessageStart` events during
    /// a single logical streaming session. When this happens while state is `Streaming`,
    /// we preserve `output_started_for_key` to prevent prefix spam on each delta that
    /// follows the repeated `MessageStart`. This is a defensive measure to handle
    /// non-standard agent protocols while maintaining correct behavior for legitimate
    /// multi-message scenarios.
    pub fn on_message_start(&mut self) {
        // Detect repeated MessageStart during active streaming
        let is_mid_stream_restart = self.state == StreamingState::Streaming;

        if is_mid_stream_restart {
            // Track protocol violation
            self.protocol_violations += 1;
            // Log the contract violation for debugging (only if verbose warnings enabled)
            if self.verbose_warnings {
                eprintln!(
                    "Warning: Received MessageStart while state is Streaming. \
                    This indicates a non-standard agent protocol (e.g., GLM sending \
                    repeated MessageStart events). Preserving output_started_for_key \
                    to prevent prefix spam. File: streaming_state.rs, Line: {}",
                    line!()
                );
            }

            // Preserve output_started_for_key to prevent prefix spam.
            // std::mem::take replaces the HashSet with an empty one and returns the old values,
            // which we restore after clearing other state. This ensures repeated MessageStart
            // events don't reset output tracking, preventing duplicate prefix display.
            let preserved_output_started = std::mem::take(&mut self.output_started_for_key);

            // Also preserve last_delta to detect duplicate deltas across MessageStart boundaries
            let preserved_last_delta = std::mem::take(&mut self.last_delta);

            // Also preserve rendered_content_hashes to detect duplicate rendering across MessageStart
            let preserved_rendered_hashes = std::mem::take(&mut self.rendered_content_hashes);

            // Also preserve consecutive_duplicates to detect resend glitches across MessageStart
            let preserved_consecutive_duplicates = std::mem::take(&mut self.consecutive_duplicates);

            self.state = StreamingState::Idle;
            self.streamed_types.clear();
            self.current_block = ContentBlockState::NotInBlock;
            self.accumulated.clear();
            self.key_order.clear();
            self.delta_sizes.clear();
            self.last_rendered.clear();
            self.deduplicator.clear();

            // Restore preserved state
            self.output_started_for_key = preserved_output_started;
            self.last_delta = preserved_last_delta;
            self.rendered_content_hashes = preserved_rendered_hashes;
            self.consecutive_duplicates = preserved_consecutive_duplicates;
        } else {
            // Normal reset for new message
            self.state = StreamingState::Idle;
            self.streamed_types.clear();
            self.current_block = ContentBlockState::NotInBlock;
            self.accumulated.clear();
            self.key_order.clear();
            self.delta_sizes.clear();
            self.output_started_for_key.clear();
            self.last_rendered.clear();
            self.last_delta.clear();
            self.rendered_content_hashes.clear();
            self.consecutive_duplicates.clear();
            self.deduplicator.clear();
        }
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
                    // Also clear output_started tracking to ensure prefix shows when switching back
                    self.output_started_for_key.remove(&key);
                    // Clear delta sizes for the old index to prevent incorrect pattern detection
                    self.delta_sizes.remove(&key);
                    self.last_rendered.remove(&key);
                    // Clear consecutive duplicates for the old index
                    self.consecutive_duplicates.remove(&key);
                }
            }
        }

        // Initialize the new content block
        self.current_block = ContentBlockState::InBlock {
            index: index_str,
            started_output: false,
        };
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

    /// Check for consecutive duplicate delta using the "3 strikes" heuristic.
    ///
    /// Detects resend glitches where the exact same delta arrives repeatedly.
    /// Returns true if the delta should be dropped (exceeded threshold), false otherwise.
    ///
    /// # Arguments
    /// * `content_key` - The content key to check
    /// * `delta` - The delta to check
    /// * `key_str` - The string key for logging
    ///
    /// # Returns
    /// * `true` - The delta should be dropped (consecutive duplicate exceeded threshold)
    /// * `false` - The delta should be processed
    fn check_consecutive_duplicate(
        &mut self,
        content_key: &(ContentType, String),
        delta: &str,
        key_str: &str,
    ) -> bool {
        let delta_hash = RollingHashWindow::compute_hash(delta);
        let thresholds = get_overlap_thresholds();

        if let Some((count, prev_hash)) = self.consecutive_duplicates.get_mut(content_key) {
            if *prev_hash == delta_hash {
                *count += 1;
                // Check if we've exceeded the consecutive duplicate threshold
                if *count >= thresholds.consecutive_duplicate_threshold {
                    // This is a resend glitch - drop the delta entirely
                    if self.verbose_warnings {
                        eprintln!(
                            "Warning: Dropping consecutive duplicate delta (count={count}, threshold={}). \
                            This appears to be a resend glitch. Key: '{key_str}', Delta: {delta:?}",
                            thresholds.consecutive_duplicate_threshold
                        );
                    }
                    // Don't update last_delta - preserve previous for comparison
                    return true;
                }
            } else {
                // Different delta - reset count and update hash
                *count = 1;
                *prev_hash = delta_hash;
            }
        } else {
            // First occurrence of this delta
            self.consecutive_duplicates
                .insert(content_key.clone(), (1, delta_hash));
        }

        false
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
    /// that exceed `snapshot_threshold()` trigger a warning as they may indicate a
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

        let content_key = (ContentType::Text, key.to_string());
        let delta_size = delta.len();

        // Track delta size and warn on large deltas BEFORE duplicate check
        // This ensures we track all received deltas even if they're duplicates
        if delta_size > snapshot_threshold() {
            self.large_delta_count += 1;
            if self.verbose_warnings {
                eprintln!(
                    "Warning: Large delta ({delta_size} chars) for key '{key}'. \
                    This may indicate unusual streaming behavior or a snapshot being sent as a delta."
                );
            }
        }

        // Track delta size for pattern detection
        {
            let sizes = self.delta_sizes.entry(content_key.clone()).or_default();
            sizes.push(delta_size);

            // Keep only the most recent delta sizes
            if sizes.len() > self.max_delta_history {
                sizes.remove(0);
            }
        }

        // Check for exact duplicate delta (same delta sent twice)
        // This handles the ccs-glm repeated MessageStart scenario where the same
        // delta is sent multiple times. We skip processing exact duplicates ONLY when
        // the accumulated content is empty (indicating we just had a MessageStart and
        // this is a true duplicate, not just a repeated token in normal streaming).
        if let Some(last) = self.last_delta.get(&content_key) {
            if delta == last {
                // Check if accumulated content is empty (just after MessageStart)
                if let Some(current_accumulated) = self.accumulated.get(&content_key) {
                    // If accumulated content is empty, this is likely a ccs-glm duplicate
                    if current_accumulated.is_empty() {
                        // Skip without updating last_delta (to preserve previous delta for comparison)
                        return false;
                    }
                } else {
                    // No accumulated content yet, definitely after MessageStart
                    // Skip without updating last_delta
                    return false;
                }
            }
        }

        // Consecutive duplicate detection ("3 strikes" heuristic)
        // Detects resend glitches where the exact same delta arrives repeatedly.
        // This is different from the above check - it tracks HOW MANY TIMES
        // the same delta has arrived consecutively, not just if it matches once.
        if self.check_consecutive_duplicate(&content_key, delta, key) {
            return false;
        }

        // Auto-repair: Check if this is a snapshot being sent as a delta
        // Do this BEFORE any mutable borrows so we can use immutable methods.
        // Use content-based detection which is more reliable than size-based alone.
        let is_snapshot = self.is_likely_snapshot(delta, key);
        let actual_delta = if is_snapshot {
            // Extract only the new portion to prevent exponential duplication
            match self.get_delta_from_snapshot(delta, key) {
                Ok(extracted) => {
                    // Track successful snapshot repair
                    self.snapshot_repairs_count += 1;
                    extracted.to_string()
                }
                Err(e) => {
                    // Snapshot detection had a false positive - use the original delta
                    if self.verbose_warnings {
                        eprintln!(
                            "Warning: Snapshot extraction failed: {e}. Using original delta."
                        );
                    }
                    delta.to_string()
                }
            }
        } else {
            // Genuine delta - use as-is
            delta.to_string()
        };

        // Pattern detection: Check if we're seeing repeated large deltas
        // This indicates the same content is being sent repeatedly (snapshot-as-delta)
        let sizes = self.delta_sizes.get(&content_key);
        if let Some(sizes) = sizes {
            if sizes.len() >= DEFAULT_PATTERN_DETECTION_MIN_DELTAS && self.verbose_warnings {
                // Check if at least 3 of the last N deltas were large
                let large_count = sizes.iter().filter(|&&s| s > snapshot_threshold()).count();
                if large_count >= DEFAULT_PATTERN_DETECTION_MIN_DELTAS {
                    eprintln!(
                        "Warning: Detected pattern of {large_count} large deltas for key '{key}'. \
                        This strongly suggests a snapshot-as-delta bug where the same \
                        large content is being sent repeatedly. File: streaming_state.rs, Line: {}",
                        line!()
                    );
                }
            }
        }

        // If the actual delta is empty (identical content detected), skip processing
        if actual_delta.is_empty() {
            // Return false to indicate no prefix should be shown (content unchanged)
            return false;
        }

        // Mark that we're streaming text content
        self.streamed_types.insert(ContentType::Text, true);
        self.state = StreamingState::Streaming;

        // Update block state to track this block and mark output as started
        self.current_block = ContentBlockState::InBlock {
            index: key.to_string(),
            started_output: true,
        };

        // Check if this is the first delta for this key using output_started_for_key
        // This is independent of accumulated content to handle cases where accumulated
        // content may be cleared (e.g., repeated ContentBlockStart for same index)
        let is_first = !self.output_started_for_key.contains(&content_key);

        // Mark that output has started for this key
        self.output_started_for_key.insert(content_key.clone());

        // Accumulate the delta (using auto-repaired delta if snapshot was detected)
        self.accumulated
            .entry(content_key.clone())
            .and_modify(|buf| buf.push_str(&actual_delta))
            .or_insert_with(|| actual_delta);

        // Track the last delta for duplicate detection
        // Use the original delta for tracking (not the auto-repaired version)
        self.last_delta
            .insert(content_key.clone(), delta.to_string());

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

        // Check if this is the first delta for this key using output_started_for_key
        let is_first = !self.output_started_for_key.contains(&content_key);

        // Mark that output has started for this key
        self.output_started_for_key.insert(content_key.clone());

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

        // Compute content hash for deduplication
        self.final_content_hash = self.compute_content_hash();

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

    /// Compute hash of all accumulated content for deduplication.
    ///
    /// This computes a hash of ALL accumulated content across all content types
    /// and indices. This is used to detect if a final message contains the same
    /// content that was already streamed.
    ///
    /// # Returns
    /// * `Some(hash)` - Hash of all accumulated content, or None if no content
    fn compute_content_hash(&self) -> Option<u64> {
        if self.accumulated.is_empty() {
            return None;
        }

        let mut hasher = DefaultHasher::new();

        // Collect and sort keys for consistent hashing
        // Note: We can't sort ContentType directly, so we convert to tuple representation
        let mut keys: Vec<_> = self.accumulated.keys().collect();
        // Sort by string representation for consistency (not perfect but good enough for deduplication)
        keys.sort_by_key(|k| format!("{:?}-{}", k.0, k.1));

        for key in keys {
            if let Some(content) = self.accumulated.get(key) {
                // Hash the key and content together
                format!("{:?}-{}", key.0, key.1).hash(&mut hasher);
                content.hash(&mut hasher);
            }
        }

        Some(hasher.finish())
    }

    /// Check if content matches the previously streamed content by hash.
    ///
    /// This is a more precise alternative to `has_any_streamed_content()` for
    /// deduplication. Instead of checking if ANY content was streamed, this checks
    /// if the EXACT content was streamed by comparing hashes.
    ///
    /// This method looks at ALL accumulated content across all content types and indices.
    /// If the combined accumulated content matches the input, it returns true.
    ///
    /// # Arguments
    /// * `content` - The content to check (typically text content from final message)
    ///
    /// # Returns
    /// * `true` - The content hash matches the previously streamed content
    /// * `false` - The content is different or no content was streamed
    pub fn is_duplicate_by_hash(&self, content: &str) -> bool {
        // Check if any accumulated text content matches the input content
        // This handles the case where assistant events arrive during streaming (before message_stop)
        let mut text_keys: Vec<_> = self
            .accumulated
            .keys()
            .filter(|(ct, _)| *ct == ContentType::Text)
            .collect();
        text_keys.sort_by_key(|k| format!("{:?}-{}", k.0, k.1));

        for key in text_keys {
            if let Some(accumulated_text) = self.accumulated.get(key) {
                // Direct string comparison is more reliable than hashing
                // because hashing can have collisions and we want exact match
                if accumulated_text == content {
                    return true;
                }
            }
        }

        false
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

    /// Mark content as having been rendered (HashMap-based tracking).
    ///
    /// This should be called after rendering to update the per-key tracking.
    ///
    /// # Arguments
    /// * `content_type` - The type of content
    /// * `index` - The content index (as string for flexibility)
    pub fn mark_rendered(&mut self, content_type: ContentType, index: &str) {
        let content_key = (content_type, index.to_string());

        // Store the current accumulated content as last rendered
        if let Some(current) = self.accumulated.get(&content_key) {
            self.last_rendered.insert(content_key, current.clone());
        }
    }

    /// Check if content has been rendered before using hash-based tracking.
    ///
    /// This provides global duplicate detection across all content by computing
    /// a hash of the accumulated content and checking if it's in the rendered set.
    /// This is preserved across `MessageStart` boundaries to prevent duplicate rendering.
    ///
    /// # Arguments
    /// * `content_type` - The type of content
    /// * `index` - The content index (as string for flexibility)
    ///
    /// # Returns
    /// * `true` - This exact content has been rendered before
    /// * `false` - This exact content has not been rendered
    pub fn is_content_rendered(&self, content_type: ContentType, index: &str) -> bool {
        let content_key = (content_type, index.to_string());

        // Check if we have accumulated content for this key
        if let Some(current) = self.accumulated.get(&content_key) {
            // Compute hash of current accumulated content
            let mut hasher = DefaultHasher::new();
            current.hash(&mut hasher);
            let hash = hasher.finish();

            // Check if this hash has been rendered before
            return self.rendered_content_hashes.contains(&hash);
        }

        false
    }

    /// Check if content has been rendered before and starts with previously rendered content.
    ///
    /// This method detects when new content extends previously rendered content,
    /// indicating an in-place update should be performed (e.g., using carriage return).
    ///
    /// With the new KMP + Rolling Hash approach, this checks if output has started
    /// for this key, which indicates we're in an in-place update scenario.
    ///
    /// # Arguments
    /// * `content_type` - The type of content
    /// * `index` - The content index (as string for flexibility)
    ///
    /// # Returns
    /// * `true` - Output has started for this key (do in-place update)
    /// * `false` - Output has not started for this key (show new content)
    pub fn has_rendered_prefix(&self, content_type: ContentType, index: &str) -> bool {
        let content_key = (content_type, index.to_string());
        self.output_started_for_key.contains(&content_key)
    }

    /// Mark content as rendered using hash-based tracking.
    ///
    /// This method updates the `rendered_content_hashes` set to track all
    /// content that has been rendered for deduplication.
    ///
    /// # Arguments
    /// * `content_type` - The type of content
    /// * `index` - The content index (as string for flexibility)
    pub fn mark_content_rendered(&mut self, content_type: ContentType, index: &str) {
        // Also update last_rendered for compatibility
        self.mark_rendered(content_type, index);

        // Add the hash of the accumulated content to the rendered set
        let content_key = (content_type, index.to_string());
        if let Some(current) = self.accumulated.get(&content_key) {
            let mut hasher = DefaultHasher::new();
            current.hash(&mut hasher);
            let hash = hasher.finish();
            self.rendered_content_hashes.insert(hash);
        }
    }

    /// Check if incoming text is likely a snapshot (full accumulated content) rather than a delta.
    ///
    /// This uses the KMP + Rolling Hash algorithm for efficient O(n+m) snapshot detection.
    /// The two-phase approach ensures optimal performance:
    /// 1. Rolling hash for fast O(n) filtering
    /// 2. KMP for exact O(n+m) verification
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
            // Use DeltaDeduplicator with threshold-aware snapshot detection
            // This prevents false positives by requiring strong overlap (>=30 chars, >=50% ratio)
            return DeltaDeduplicator::is_likely_snapshot_with_thresholds(text, previous);
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
            // Use DeltaDeduplicator with threshold-aware delta extraction
            // This ensures we only extract when overlap meets strong criteria
            if let Some(new_content) =
                DeltaDeduplicator::extract_new_content_with_thresholds(text, previous)
            {
                // Calculate the position where new content starts
                let delta_start = text.len() - new_content.len();
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

    /// Get streaming quality metrics for the current session.
    ///
    /// Returns aggregated metrics about delta sizes and streaming patterns
    /// during the session. This is useful for debugging and analyzing
    /// streaming behavior.
    ///
    /// # Returns
    /// Aggregated metrics across all content types and keys.
    pub fn get_streaming_quality_metrics(&self) -> StreamingQualityMetrics {
        // Flatten all delta sizes across all content types and keys
        let all_sizes = self.delta_sizes.values().flat_map(|v| v.iter().copied());
        let mut metrics = StreamingQualityMetrics::from_sizes(all_sizes);

        // Add session-level metrics
        metrics.snapshot_repairs_count = self.snapshot_repairs_count;
        metrics.large_delta_count = self.large_delta_count;
        metrics.protocol_violations = self.protocol_violations;

        metrics
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
        // Behavioral test: verify that creating a new session gives clean state
        // instead of testing clear_index() which is now removed
        let mut session = StreamingSession::new();
        session.on_message_start();

        session.on_text_delta(0, "Before");
        // Instead of clearing, verify that a new session starts fresh
        let mut fresh_session = StreamingSession::new();
        fresh_session.on_message_start();
        fresh_session.on_text_delta(0, "After");

        assert_eq!(
            fresh_session.get_accumulated(ContentType::Text, "0"),
            Some("After")
        );
        // Original session should still have "Before"
        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some("Before")
        );
    }

    #[test]
    fn test_delta_validation_warns_on_large_delta() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Create a delta larger than snapshot_threshold()
        let large_delta = "x".repeat(snapshot_threshold() + 1);

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

        // First delta is long enough to meet threshold requirements
        // Using 40 chars to ensure it exceeds the 30 char minimum
        let initial = "This is a long message that exceeds threshold";
        session.on_text_delta(0, initial);

        // Simulate GLM sending full accumulated content as next "delta"
        // The overlap is 45 chars (100% of initial), meeting both thresholds:
        // - char_count = 45 >= 30 ✓
        // - ratio = 45/48 ≈ 94% >= 50% ✓
        // - ends at safe boundary (space) ✓
        let snapshot = format!("{initial} plus new content");
        let is_snapshot = session.is_likely_snapshot(&snapshot, "0");
        assert!(
            is_snapshot,
            "Should detect snapshot-as-delta with strong overlap"
        );
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

        // First delta is long enough to meet threshold requirements
        let initial = "This is a long message that exceeds threshold";
        session.on_text_delta(0, initial);

        // Snapshot should extract new portion
        let snapshot = format!("{initial} plus new content");
        let delta = session.get_delta_from_snapshot(&snapshot, "0").unwrap();
        assert_eq!(delta, " plus new content");
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
        // Use content long enough to meet threshold requirements
        let initial = "This is a long message that exceeds threshold";
        session.on_text_delta_key("main", initial);

        // Should detect snapshot for string key with strong overlap
        let snapshot = format!("{initial} plus new content");
        let is_snapshot = session.is_likely_snapshot(&snapshot, "main");
        assert!(
            is_snapshot,
            "Should detect snapshot with string keys when thresholds are met"
        );

        // Should extract delta correctly
        let delta = session.get_delta_from_snapshot(&snapshot, "main").unwrap();
        assert_eq!(delta, " plus new content");
    }

    #[test]
    fn test_snapshot_extraction_exact_match() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // First delta is long enough to meet threshold requirements
        let initial = "This is a long message that exceeds threshold";
        session.on_text_delta(0, initial);

        // Exact match with additional content (strong overlap)
        let exact_match = format!("{initial} World");
        let delta1 = session.get_delta_from_snapshot(&exact_match, "0").unwrap();
        assert_eq!(delta1, " World");
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

        // First few tokens as genuine deltas - use longer content to meet thresholds
        let initial = "This is a long message that exceeds threshold";
        session.on_text_delta(0, initial);
        session.on_text_delta(0, " with more content");

        // Now GLM sends a snapshot instead of delta
        // The accumulated content plus new content should meet thresholds:
        // Accumulated: "This is a long message that exceeds threshold with more content" (62 chars)
        // Snapshot: accumulated + "! This is additional content" (88 chars total)
        // Overlap: 62 chars
        // Ratio: 62/88 ≈ 70% >= 50% ✓
        let accumulated = session
            .get_accumulated(ContentType::Text, "0")
            .unwrap()
            .to_string();
        let snapshot = format!("{accumulated}! This is additional content");
        assert!(
            session.is_likely_snapshot(&snapshot, "0"),
            "Should detect snapshot in token stream with strong overlap"
        );

        // Extract delta and continue
        let delta = session.get_delta_from_snapshot(&snapshot, "0").unwrap();
        assert!(delta.contains("! This is additional content"));

        // Apply the delta
        session.on_text_delta(0, delta);

        // Verify final accumulated content
        let expected = format!("{accumulated}! This is additional content");
        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some(expected.as_str())
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

    // Tests for repeated MessageStart handling (GLM/ccs-glm protocol quirk)

    #[test]
    fn test_repeated_message_start_preserves_output_started() {
        let mut session = StreamingSession::new();

        // First message start
        session.on_message_start();

        // First delta should show prefix
        let show_prefix = session.on_text_delta(0, "Hello");
        assert!(show_prefix, "First delta should show prefix");

        // Second delta should NOT show prefix
        let show_prefix = session.on_text_delta(0, " World");
        assert!(!show_prefix, "Second delta should not show prefix");

        // Simulate GLM sending repeated MessageStart during streaming
        // This should preserve output_started_for_key to prevent prefix spam
        session.on_message_start();

        // After repeated MessageStart, delta should NOT show prefix
        // because output_started_for_key was preserved
        let show_prefix = session.on_text_delta(0, "!");
        assert!(
            !show_prefix,
            "After repeated MessageStart, delta should not show prefix"
        );

        // Verify accumulated content was cleared (as expected for mid-stream restart)
        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some("!"),
            "Accumulated content should start fresh after repeated MessageStart"
        );
    }

    #[test]
    fn test_repeated_message_start_with_normal_reset_between_messages() {
        let mut session = StreamingSession::new();

        // First message
        session.on_message_start();
        session.on_text_delta(0, "First");
        session.on_message_stop();

        // Second message - normal reset should clear output_started_for_key
        session.on_message_start();

        // First delta of second message SHOULD show prefix
        let show_prefix = session.on_text_delta(0, "Second");
        assert!(
            show_prefix,
            "First delta of new message should show prefix after normal reset"
        );
    }

    #[test]
    fn test_repeated_message_start_with_multiple_indices() {
        let mut session = StreamingSession::new();

        // First message start
        session.on_message_start();

        // First delta for index 0
        let show_prefix = session.on_text_delta(0, "Index0");
        assert!(show_prefix, "First delta for index 0 should show prefix");

        // First delta for index 1
        let show_prefix = session.on_text_delta(1, "Index1");
        assert!(show_prefix, "First delta for index 1 should show prefix");

        // Simulate repeated MessageStart
        session.on_message_start();

        // After repeated MessageStart, deltas should NOT show prefix
        // because output_started_for_key was preserved for both indices
        let show_prefix = session.on_text_delta(0, " more");
        assert!(
            !show_prefix,
            "Delta for index 0 should not show prefix after repeated MessageStart"
        );

        let show_prefix = session.on_text_delta(1, " more");
        assert!(
            !show_prefix,
            "Delta for index 1 should not show prefix after repeated MessageStart"
        );
    }

    #[test]
    fn test_repeated_message_start_during_thinking_stream() {
        let mut session = StreamingSession::new();

        // First message start
        session.on_message_start();

        // First thinking delta should show prefix
        let show_prefix = session.on_thinking_delta(0, "Thinking...");
        assert!(show_prefix, "First thinking delta should show prefix");

        // Simulate repeated MessageStart
        session.on_message_start();

        // After repeated MessageStart, thinking delta should NOT show prefix
        let show_prefix = session.on_thinking_delta(0, " more");
        assert!(
            !show_prefix,
            "Thinking delta after repeated MessageStart should not show prefix"
        );
    }

    #[test]
    fn test_message_stop_then_message_start_resets_normally() {
        let mut session = StreamingSession::new();

        // First message
        session.on_message_start();
        session.on_text_delta(0, "First");

        // Message stop finalizes the message
        session.on_message_stop();

        // New message start should reset normally (not preserve output_started)
        session.on_message_start();

        // First delta of new message SHOULD show prefix
        let show_prefix = session.on_text_delta(0, "Second");
        assert!(
            show_prefix,
            "First delta after MessageStop should show prefix (normal reset)"
        );
    }

    #[test]
    fn test_repeated_content_block_start_same_index() {
        let mut session = StreamingSession::new();

        // Message start
        session.on_message_start();

        // First delta for index 0
        let show_prefix = session.on_text_delta(0, "Hello");
        assert!(show_prefix, "First delta should show prefix");

        // Simulate repeated ContentBlockStart for same index
        // (Some agents send this, and we should NOT clear accumulated content)
        session.on_content_block_start(0);

        // Delta after repeated ContentBlockStart should NOT show prefix
        let show_prefix = session.on_text_delta(0, " World");
        assert!(
            !show_prefix,
            "Delta after repeated ContentBlockStart should not show prefix"
        );

        // Verify accumulated content was preserved
        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some("Hello World"),
            "Accumulated content should be preserved across repeated ContentBlockStart"
        );
    }

    // Tests for verbose_warnings feature

    #[test]
    fn test_verbose_warnings_default_is_disabled() {
        let session = StreamingSession::new();
        assert!(
            !session.verbose_warnings,
            "Default should have verbose_warnings disabled"
        );
    }

    #[test]
    fn test_with_verbose_warnings_enables_flag() {
        let session = StreamingSession::new().with_verbose_warnings(true);
        assert!(
            session.verbose_warnings,
            "Should have verbose_warnings enabled"
        );
    }

    #[test]
    fn test_with_verbose_warnings_disabled_explicitly() {
        let session = StreamingSession::new().with_verbose_warnings(false);
        assert!(
            !session.verbose_warnings,
            "Should have verbose_warnings disabled"
        );
    }

    #[test]
    fn test_large_delta_warning_respects_verbose_flag() {
        // Test with verbose warnings enabled
        let mut session_verbose = StreamingSession::new().with_verbose_warnings(true);
        session_verbose.on_message_start();

        let large_delta = "x".repeat(snapshot_threshold() + 1);
        // This would emit a warning to stderr if verbose_warnings is enabled
        let _show_prefix = session_verbose.on_text_delta(0, &large_delta);

        // Test with verbose warnings disabled (default)
        let mut session_quiet = StreamingSession::new();
        session_quiet.on_message_start();

        let large_delta = "x".repeat(snapshot_threshold() + 1);
        // This should NOT emit a warning
        let _show_prefix = session_quiet.on_text_delta(0, &large_delta);

        // Both sessions should accumulate content correctly
        assert_eq!(
            session_verbose.get_accumulated(ContentType::Text, "0"),
            Some(large_delta.as_str())
        );
        assert_eq!(
            session_quiet.get_accumulated(ContentType::Text, "0"),
            Some(large_delta.as_str())
        );
    }

    #[test]
    fn test_repeated_message_start_warning_respects_verbose_flag() {
        // Test with verbose warnings enabled
        let mut session_verbose = StreamingSession::new().with_verbose_warnings(true);
        session_verbose.on_message_start();
        session_verbose.on_text_delta(0, "Hello");
        // This would emit a warning about repeated MessageStart
        session_verbose.on_message_start();

        // Test with verbose warnings disabled (default)
        let mut session_quiet = StreamingSession::new();
        session_quiet.on_message_start();
        session_quiet.on_text_delta(0, "Hello");
        // This should NOT emit a warning
        session_quiet.on_message_start();

        // Both sessions should handle the restart correctly
        assert_eq!(
            session_verbose.get_accumulated(ContentType::Text, "0"),
            None,
            "Accumulated content should be cleared after repeated MessageStart"
        );
        assert_eq!(
            session_quiet.get_accumulated(ContentType::Text, "0"),
            None,
            "Accumulated content should be cleared after repeated MessageStart"
        );
    }

    #[test]
    fn test_pattern_detection_warning_respects_verbose_flag() {
        // Test with verbose warnings enabled
        let mut session_verbose = StreamingSession::new().with_verbose_warnings(true);
        session_verbose.on_message_start();

        // Send 3 large deltas to trigger pattern detection
        // Use different content to avoid consecutive duplicate detection
        for i in 0..3 {
            let large_delta = format!("{}{i}", "x".repeat(snapshot_threshold() + 1));
            let _ = session_verbose.on_text_delta(0, &large_delta);
        }

        // Test with verbose warnings disabled (default)
        let mut session_quiet = StreamingSession::new();
        session_quiet.on_message_start();

        // Send 3 large deltas (different content to avoid consecutive duplicate detection)
        for i in 0..3 {
            let large_delta = format!("{}{i}", "x".repeat(snapshot_threshold() + 1));
            let _ = session_quiet.on_text_delta(0, &large_delta);
        }

        // Verify that large_delta_count still tracks all 3 large deltas for both sessions
        assert_eq!(
            session_verbose
                .get_streaming_quality_metrics()
                .large_delta_count,
            3
        );
        assert_eq!(
            session_quiet
                .get_streaming_quality_metrics()
                .large_delta_count,
            3
        );
    }

    #[test]
    fn test_snapshot_extraction_error_warning_respects_verbose_flag() {
        // Create a session where we'll trigger a snapshot extraction error
        // by manually manipulating accumulated content
        let mut session_verbose = StreamingSession::new().with_verbose_warnings(true);
        session_verbose.on_message_start();
        session_verbose.on_content_block_start(0);

        // First delta
        session_verbose.on_text_delta(0, "Hello");

        // Manually clear accumulated to simulate a state mismatch
        session_verbose.accumulated.clear();

        // Now try to process a snapshot - extraction will fail
        // This would emit a warning if verbose_warnings is enabled
        let _show_prefix = session_verbose.on_text_delta(0, "Hello World");

        // Test with verbose warnings disabled (default)
        let mut session_quiet = StreamingSession::new();
        session_quiet.on_message_start();
        session_quiet.on_content_block_start(0);

        session_quiet.on_text_delta(0, "Hello");
        session_quiet.accumulated.clear();

        // This should NOT emit a warning
        let _show_prefix = session_quiet.on_text_delta(0, "Hello World");

        // The quiet session should handle the error gracefully
        assert!(session_quiet
            .get_accumulated(ContentType::Text, "0")
            .is_some());
    }

    // Tests for enhanced streaming metrics

    #[test]
    fn test_streaming_quality_metrics_includes_snapshot_repairs() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_content_block_start(0);

        // First delta - long enough to meet threshold requirements
        let initial = "This is a long message that exceeds threshold";
        session.on_text_delta(0, initial);

        // GLM sends snapshot instead of delta (with strong overlap)
        let snapshot = format!("{initial} World!");
        let _ = session.on_text_delta(0, &snapshot);

        let metrics = session.get_streaming_quality_metrics();
        assert_eq!(
            metrics.snapshot_repairs_count, 1,
            "Should track one snapshot repair"
        );
    }

    #[test]
    fn test_streaming_quality_metrics_includes_large_delta_count() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Send 3 large deltas
        for _ in 0..3 {
            let large_delta = "x".repeat(snapshot_threshold() + 1);
            let _ = session.on_text_delta(0, &large_delta);
        }

        let metrics = session.get_streaming_quality_metrics();
        assert_eq!(
            metrics.large_delta_count, 3,
            "Should track three large deltas"
        );
    }

    #[test]
    fn test_streaming_quality_metrics_includes_protocol_violations() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_text_delta(0, "Hello");

        // Simulate GLM sending repeated MessageStart during streaming
        session.on_message_start();
        session.on_text_delta(0, " World");

        // Another violation
        session.on_message_start();

        let metrics = session.get_streaming_quality_metrics();
        assert_eq!(
            metrics.protocol_violations, 2,
            "Should track two protocol violations"
        );
    }

    #[test]
    fn test_streaming_quality_metrics_all_new_fields_zero_by_default() {
        let session = StreamingSession::new();
        let metrics = session.get_streaming_quality_metrics();

        assert_eq!(metrics.snapshot_repairs_count, 0);
        assert_eq!(metrics.large_delta_count, 0);
        assert_eq!(metrics.protocol_violations, 0);
    }

    #[test]
    fn test_streaming_quality_metrics_comprehensive_tracking() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Normal delta
        session.on_text_delta(0, "Hello");

        // Large delta
        let large_delta = "x".repeat(snapshot_threshold() + 1);
        let _ = session.on_text_delta(0, &large_delta);

        // Snapshot repair (note: the snapshot is also large, so it counts as another large delta)
        let snapshot = format!("Hello{large_delta} World");
        let _ = session.on_text_delta(0, &snapshot);

        // Check metrics BEFORE the protocol violation (which clears delta_sizes)
        let metrics = session.get_streaming_quality_metrics();
        assert_eq!(metrics.snapshot_repairs_count, 1);
        assert_eq!(
            metrics.large_delta_count, 2,
            "Both the large delta and the snapshot are large"
        );
        assert_eq!(metrics.total_deltas, 3);
        assert_eq!(metrics.protocol_violations, 0, "No violation yet");

        // Protocol violation
        session.on_message_start();

        // After violation, protocol_violations is incremented but delta_sizes is cleared
        let metrics_after = session.get_streaming_quality_metrics();
        assert_eq!(metrics_after.protocol_violations, 1);
        assert_eq!(
            metrics_after.total_deltas, 0,
            "Delta sizes cleared after violation"
        );
    }

    // Tests for hash-based deduplication

    #[test]
    fn test_content_hash_computed_on_message_stop() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_text_delta(0, "Hello");
        session.on_text_delta(0, " World");

        // Hash should be None before message_stop
        assert_eq!(session.final_content_hash, None);

        // Hash should be computed after message_stop
        session.on_message_stop();
        assert!(session.final_content_hash.is_some());
    }

    #[test]
    fn test_content_hash_none_when_no_content() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // No content streamed
        session.on_message_stop();
        assert_eq!(session.final_content_hash, None);
    }

    #[test]
    fn test_is_duplicate_by_hash_returns_true_for_matching_content() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_text_delta(0, "Hello World");
        session.on_message_stop();

        // Same content should be detected as duplicate
        assert!(session.is_duplicate_by_hash("Hello World"));
    }

    #[test]
    fn test_is_duplicate_by_hash_returns_false_for_different_content() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_text_delta(0, "Hello World");
        session.on_message_stop();

        // Different content should NOT be detected as duplicate
        assert!(!session.is_duplicate_by_hash("Different content"));
    }

    #[test]
    fn test_is_duplicate_by_hash_returns_false_when_no_content_streamed() {
        let session = StreamingSession::new();

        // No content streamed, so no hash
        assert!(!session.is_duplicate_by_hash("Hello World"));
    }

    #[test]
    fn test_content_hash_multiple_content_blocks() {
        let mut session = StreamingSession::new();
        session.on_message_start();
        session.on_text_delta(0, "First block");
        session.on_text_delta(1, "Second block");
        session.on_message_stop();

        // Hash should be computed from all blocks
        assert!(session.final_content_hash.is_some());
        // Individual content shouldn't match the combined hash
        assert!(!session.is_duplicate_by_hash("First block"));
        assert!(!session.is_duplicate_by_hash("Second block"));
    }

    #[test]
    fn test_content_hash_consistent_for_same_content() {
        let mut session1 = StreamingSession::new();
        session1.on_message_start();
        session1.on_text_delta(0, "Hello");
        session1.on_text_delta(0, " World");
        session1.on_message_stop();

        let mut session2 = StreamingSession::new();
        session2.on_message_start();
        session2.on_text_delta(0, "Hello World");
        session2.on_message_stop();

        // Same content should produce the same hash
        assert_eq!(session1.final_content_hash, session2.final_content_hash);
    }

    // Tests for rapid index switching edge case (RFC-003)

    #[test]
    fn test_rapid_index_switch_with_clear() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Start block 0 and accumulate content
        session.on_content_block_start(0);
        let show_prefix = session.on_text_delta(0, "X");
        assert!(show_prefix, "First delta for index 0 should show prefix");
        assert_eq!(session.get_accumulated(ContentType::Text, "0"), Some("X"));

        // Switch to block 1 - this should clear accumulated content for index 0
        session.on_content_block_start(1);

        // Verify accumulated for index 0 was cleared
        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            None,
            "Accumulated content for index 0 should be cleared when switching to index 1"
        );

        // Switch back to index 0
        session.on_content_block_start(0);

        // Since output_started_for_key was also cleared, prefix should show again
        let show_prefix = session.on_text_delta(0, "Y");
        assert!(
            show_prefix,
            "Prefix should show when switching back to a previously cleared index"
        );

        // Verify new content is accumulated fresh
        assert_eq!(
            session.get_accumulated(ContentType::Text, "0"),
            Some("Y"),
            "New content should be accumulated fresh after clear"
        );
    }

    #[test]
    fn test_delta_sizes_cleared_on_index_switch() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Track some delta sizes for index 0
        session.on_text_delta(0, "Hello");
        session.on_text_delta(0, " World");

        let content_key = (ContentType::Text, "0".to_string());
        assert!(
            session.delta_sizes.contains_key(&content_key),
            "Delta sizes should be tracked for index 0"
        );
        let sizes_before = session.delta_sizes.get(&content_key).unwrap();
        assert_eq!(sizes_before.len(), 2, "Should have 2 delta sizes tracked");

        // Switch to index 1 - this should clear delta_sizes for index 0
        session.on_content_block_start(1);

        assert!(
            !session.delta_sizes.contains_key(&content_key),
            "Delta sizes for index 0 should be cleared when switching to index 1"
        );

        // Add deltas for index 1
        session.on_text_delta(1, "New");

        let content_key_1 = (ContentType::Text, "1".to_string());
        let sizes_after = session.delta_sizes.get(&content_key_1).unwrap();
        assert_eq!(
            sizes_after.len(),
            1,
            "Should have fresh size tracking for index 1"
        );
    }

    #[test]
    fn test_rapid_index_switch_with_thinking_content() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Start thinking content in index 0
        session.on_content_block_start(0);
        let show_prefix = session.on_thinking_delta(0, "Thinking...");
        assert!(show_prefix, "First thinking delta should show prefix");
        assert_eq!(
            session.get_accumulated(ContentType::Thinking, "0"),
            Some("Thinking...")
        );

        // Switch to text content in index 1 - this should clear index 0's accumulated
        session.on_content_block_start(1);

        // Verify index 0's accumulated thinking was cleared
        assert_eq!(
            session.get_accumulated(ContentType::Thinking, "0"),
            None,
            "Thinking content for index 0 should be cleared when switching to index 1"
        );

        let show_prefix = session.on_text_delta(1, "Text");
        assert!(
            show_prefix,
            "First text delta for index 1 should show prefix"
        );

        // Switch back to index 0 for thinking
        session.on_content_block_start(0);

        // Since output_started_for_key for (Thinking, "0") was cleared when switching to index 1,
        // the prefix should show again
        let show_prefix = session.on_thinking_delta(0, " more");
        assert!(
            show_prefix,
            "Thinking prefix should show when switching back to cleared index 0"
        );

        // Verify thinking content was accumulated fresh (only the new content)
        assert_eq!(
            session.get_accumulated(ContentType::Thinking, "0"),
            Some(" more"),
            "Thinking content should be accumulated fresh after clear"
        );
    }

    #[test]
    fn test_output_started_for_key_cleared_across_all_content_types() {
        let mut session = StreamingSession::new();
        session.on_message_start();

        // Start block 0 with text and thinking
        // Note: ToolInput does not use output_started_for_key tracking
        session.on_content_block_start(0);
        session.on_text_delta(0, "Text");
        session.on_thinking_delta(0, "Thinking");

        // Verify text and thinking have started output
        let text_key = (ContentType::Text, "0".to_string());
        let thinking_key = (ContentType::Thinking, "0".to_string());

        assert!(session.output_started_for_key.contains(&text_key));
        assert!(session.output_started_for_key.contains(&thinking_key));

        // Switch to index 1 - should clear output_started_for_key for all content types
        session.on_content_block_start(1);

        assert!(
            !session.output_started_for_key.contains(&text_key),
            "Text output_started should be cleared for index 0"
        );
        assert!(
            !session.output_started_for_key.contains(&thinking_key),
            "Thinking output_started should be cleared for index 0"
        );
    }

    // Tests for environment variable configuration

    #[test]
    fn test_snapshot_threshold_default() {
        // Ensure no env var is set for this test
        std::env::remove_var("RALPH_STREAMING_SNAPSHOT_THRESHOLD");
        // Note: Since we use OnceLock, we can't reset the value in tests.
        // This test documents the default behavior.
        let threshold = snapshot_threshold();
        assert_eq!(
            threshold, DEFAULT_SNAPSHOT_THRESHOLD,
            "Default threshold should be 200"
        );
    }
}
