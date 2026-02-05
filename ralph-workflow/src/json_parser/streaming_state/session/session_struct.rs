// StreamingSession struct definition and basic methods.
//
// This file contains the struct definition for StreamingSession and its
// basic construction methods.

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
    pub(super) state: StreamingState,
    /// Track which content types have been streamed (for deduplication)
    /// Maps `ContentType` → whether it has been streamed
    pub(super) streamed_types: HashMap<ContentType, bool>,
    /// Track the current content block state
    pub(super) current_block: ContentBlockState,
    /// Accumulated content by (`content_type`, index) for display
    /// This mirrors `DeltaAccumulator` but adds deduplication tracking
    pub(super) accumulated: HashMap<(ContentType, String), String>,
    /// Track the order of keys for `most_recent` operations
    pub(super) key_order: Vec<(ContentType, String)>,
    /// Track recent delta sizes for pattern detection
    /// Maps `(content_type, key)` → vec of recent delta sizes
    pub(super) delta_sizes: HashMap<(ContentType, String), Vec<usize>>,
    /// Maximum number of delta sizes to track per key
    pub(super) max_delta_history: usize,
    /// Track the current message ID for duplicate detection
    pub(super) current_message_id: Option<String>,
    /// Track which messages have been displayed to prevent duplicate final output
    pub(super) displayed_final_messages: HashSet<String>,
    /// Track which (`content_type`, key) pairs have had output started.
    /// This is independent of `accumulated` to handle cases where accumulated
    /// content may be cleared (e.g., repeated `ContentBlockStart` for same index).
    /// Cleared on `on_message_start` to ensure fresh state for each message.
    pub(super) output_started_for_key: HashSet<(ContentType, String)>,
    /// Whether to emit verbose warnings about streaming anomalies.
    /// When false, suppresses diagnostic warnings that are useful for debugging
    /// but noisy in production (e.g., GLM protocol violations, snapshot detection).
    pub(super) verbose_warnings: bool,
    /// Count of snapshot repairs performed during this session
    pub(super) snapshot_repairs_count: usize,
    /// Count of deltas that exceeded the size threshold
    pub(super) large_delta_count: usize,
    /// Count of protocol violations detected (e.g., `MessageStart` during streaming)
    pub(super) protocol_violations: usize,
    /// Hash of the final streamed content (for deduplication)
    /// Computed at `message_stop` using all accumulated content
    pub(super) final_content_hash: Option<u64>,
    /// Track the last rendered content for each key to detect when rendering
    /// would produce identical output (prevents visual repetition).
    /// Maps `(content_type, key)` → the last accumulated content that was rendered.
    pub(super) last_rendered: HashMap<(ContentType, String), String>,
    /// Track rendered content hashes for duplicate detection.
    ///
    /// This stores a hash of the *sanitized content* together with the `(content_type, key)`
    /// it was rendered for. This is preserved across repeated `MessageStart` boundaries.
    ///
    /// Keying by `(content_type, key)` is important because some parsers reuse keys within the
    /// same turn (e.g., Codex can reuse `reasoning` for multiple items). When that happens,
    /// we need `clear_key()` to fully reset per-key deduplication state.
    pub(super) rendered_content_hashes: HashSet<(ContentType, String, u64)>,
    /// Track the last delta for each key to detect exact duplicate deltas.
    /// This is preserved across `MessageStart` boundaries to prevent duplicate processing.
    /// Maps `(content_type, key)` → the last delta that was processed.
    pub(super) last_delta: HashMap<(ContentType, String), String>,
    /// Track consecutive duplicates for resend glitch detection ("3 strikes" heuristic).
    /// Maps `(content_type, key)` → (count, `delta_hash`) where count tracks how many
    /// times the exact same delta has arrived consecutively. When count exceeds
    /// the threshold, the delta is dropped as a resend glitch.
    pub(super) consecutive_duplicates: HashMap<(ContentType, String), (usize, u64)>,
    /// Delta deduplicator using KMP and rolling hash for snapshot detection.
    /// Provides O(n+m) guaranteed complexity for detecting snapshot-as-delta violations.
    /// Cleared on message boundaries to prevent false positives.
    pub(super) deduplicator: DeltaDeduplicator,
    /// Track message IDs that have been fully rendered from an assistant event BEFORE streaming.
    /// When an assistant event arrives before streaming deltas, we render it and record
    /// the message_id. ALL subsequent streaming deltas for that message_id should be
    /// suppressed to prevent duplication.
    pub(super) pre_rendered_message_ids: HashSet<String>,
    /// Track content hashes of assistant events that have been rendered during streaming.
    /// This prevents duplicate assistant events with the same content from being rendered
    /// multiple times. GLM/CCS may send multiple assistant events during streaming with
    /// the same content but different message_ids.
    /// This is preserved across `MessageStart` boundaries to handle mid-stream assistant events.
    pub(super) rendered_assistant_content_hashes: HashSet<u64>,
    /// Track tool names by index for GLM/CCS deduplication.
    /// GLM sends assistant events with tool_use blocks (name + input) during streaming,
    /// but only the input is accumulated via deltas. We track the tool name to properly
    /// reconstruct the normalized representation for deduplication.
    /// Maps the content block index to the tool name.
    pub(super) tool_names: HashMap<u64, Option<String>>,
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
}
