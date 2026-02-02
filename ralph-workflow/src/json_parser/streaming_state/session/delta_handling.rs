// Delta processing and accumulation methods for StreamingSession.
//
// This file contains methods for processing deltas, handling deduplication,
// snapshot detection, and content accumulation.

impl StreamingSession {
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

    /// Record the tool name for a specific content block index.
    ///
    /// This is used for GLM/CCS deduplication where assistant events contain
    /// tool_use blocks (name + input) but streaming only accumulates the input.
    /// By tracking the name separately, we can reconstruct the normalized
    /// representation for proper hash-based deduplication.
    ///
    /// # Arguments
    /// * `index` - The content block index
    /// * `name` - The tool name (if available)
    pub fn set_tool_name(&mut self, index: u64, name: Option<String>) {
        self.tool_names.insert(index, name);
    }

    /// Compute hash of all accumulated content for deduplication.
    ///
    /// This computes a hash of ALL accumulated content across all content types
    /// and indices. This is used to detect if a final message contains the same
    /// content that was already streamed.
    ///
    /// # Returns
    /// * `Some(hash)` - Hash of all accumulated content, or None if no content
    pub(super) fn compute_content_hash(&self) -> Option<u64> {
        if self.accumulated.is_empty() {
            return None;
        }

        let mut hasher = DefaultHasher::new();

        // Collect and sort keys for consistent hashing
        // Sort by numeric index (not lexicographic) to preserve actual message order.
        // This is critical for correctness when indices don't sort lexicographically
        // (e.g., 0, 1, 10, 2 should sort as 0, 1, 2, 10).
        let mut keys: Vec<_> = self.accumulated.keys().collect();
        keys.sort_by_key(|k| {
            let type_order = match k.0 {
                ContentType::Text => 0,
                ContentType::ToolInput => 1,
                ContentType::Thinking => 2,
            };
            let index = k.1.parse::<u64>().unwrap_or(u64::MAX);
            (index, type_order)
        });

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
    /// * `content` - The content to check (typically normalized content from assistant events)
    /// * `tool_name_hints` - Optional tool names from assistant event (by content block index)
    ///
    /// # Returns
    /// * `true` - The content hash matches the previously streamed content
    /// * `false` - The content is different or no content was streamed
    pub fn is_duplicate_by_hash(
        &self,
        content: &str,
        tool_name_hints: Option<&std::collections::HashMap<usize, String>>,
    ) -> bool {
        // Check if content contains both text and tool_use markers (mixed content)
        let has_tool_use = content.contains("TOOL_USE:");
        let has_text = self
            .accumulated
            .iter()
            .any(|((ct, _), v)| *ct == ContentType::Text && !v.is_empty());

        if has_tool_use && has_text {
            // Mixed content: both text and tool_use need to be checked
            // Reconstruct the full normalized content from both text and tool_use
            return self.is_duplicate_mixed_content(content, tool_name_hints);
        } else if has_tool_use {
            // Only tool_use content
            return self.is_duplicate_tool_use(content, tool_name_hints);
        }

        // Only text content - check if the input content matches ALL accumulated text content combined
        // This handles the case where assistant events arrive during streaming (before message_stop)
        // We collect and combine all text content in index order
        let mut text_keys: Vec<_> = self
            .accumulated
            .keys()
            .filter(|(ct, _)| *ct == ContentType::Text)
            .collect();
        // Sort by numeric index (not lexicographic) to preserve actual message order
        // This is critical for correctness when indices don't sort lexicographically (e.g., 0, 1, 10, 2)
        // unwrap_or(u64::MAX) handles non-numeric indices by sorting them last; this should
        // never happen in practice since GLM always sends numeric string indices, but if
        // it does occur, it indicates a protocol violation and will be visible in output
        text_keys.sort_by_key(|k| k.1.parse::<u64>().unwrap_or(u64::MAX));

        // Combine all accumulated text content in sorted order
        let combined_content: String = text_keys
            .iter()
            .filter_map(|key| self.accumulated.get(key))
            .cloned()
            .collect();

        // Direct string comparison is more reliable than hashing
        // because hashing can have collisions and we want exact match
        combined_content == content
    }

    /// Check if mixed content (text + tool_use) from an assistant event matches accumulated content.
    ///
    /// This handles the case where assistant events contain both text and tool_use blocks.
    /// We reconstruct the full normalized content from both text and tool_use accumulated content
    /// and compare it against the assistant event content.
    ///
    /// # Arguments
    /// * `normalized_content` - Content potentially containing both text and "TOOL_USE:{name}:{input}" markers
    /// * `tool_name_hints` - Optional tool names from assistant event (by content block index)
    ///
    /// # Returns
    /// * `true` - All content (text + tool_use) matches accumulated content
    /// * `false` - Content differs or not accumulated yet
    fn is_duplicate_mixed_content(
        &self,
        normalized_content: &str,
        tool_name_hints: Option<&std::collections::HashMap<usize, String>>,
    ) -> bool {
        // Collect all content blocks in order (text and tool_use interleaved)
        // We need to reconstruct the exact order as it appears in the assistant event
        let mut reconstructed = String::new();

        // Get all accumulated content keys and sort by index
        let mut all_keys: Vec<_> = self.accumulated.keys().collect();
        all_keys.sort_by_key(|k| {
            // Sort by index first (to preserve actual order), then by content type as tiebreaker
            let index = k.1.parse::<u64>().unwrap_or(u64::MAX);
            let type_order = match k.0 {
                ContentType::Text => 0,
                ContentType::ToolInput => 1,
                ContentType::Thinking => 2,
            };
            (index, type_order)
        });

        for (ct, index_str) in all_keys {
            if let Some(accumulated_content) = self.accumulated.get(&(*ct, index_str.clone())) {
                match ct {
                    ContentType::Text => {
                        // Text content is added as-is
                        reconstructed.push_str(accumulated_content);
                    }
                    ContentType::ToolInput => {
                        // Tool_use content needs normalization with tool name
                        let index_num = index_str.parse::<u64>().unwrap_or(0);
                        let tool_name = tool_name_hints
                            .and_then(|hints| hints.get(&(index_num as usize)).map(|s| s.as_str()))
                            .or_else(|| self.tool_names.get(&index_num).and_then(|n| n.as_deref()))
                            .unwrap_or("");

                        // Normalize: "TOOL_USE:{name}:{input}"
                        reconstructed
                            .push_str(&format!("TOOL_USE:{}:{}", tool_name, accumulated_content));
                    }
                    ContentType::Thinking => {
                        // Thinking content - not currently used in assistant events
                        // but included for completeness
                    }
                }
            }
        }

        // Check if the reconstructed content matches the input
        normalized_content == reconstructed
    }

    /// Check if tool_use content from an assistant event matches accumulated ToolInput.
    ///
    /// Assistant events may contain normalized tool_use blocks (with "TOOL_USE:" prefix).
    /// This method reconstructs the normalized representation from accumulated content
    /// and checks if it matches the assistant event content.
    ///
    /// # Arguments
    /// * `normalized_content` - Content potentially containing "TOOL_USE:{name}:{input}" markers
    /// * `tool_name_hints` - Optional tool names from assistant event (by content block index)
    ///
    /// # Returns
    /// * `true` - All tool_use blocks match accumulated content
    /// * `false` - Tool_use content differs or not accumulated yet
    fn is_duplicate_tool_use(
        &self,
        normalized_content: &str,
        tool_name_hints: Option<&std::collections::HashMap<usize, String>>,
    ) -> bool {
        // Reconstruct the normalized representation from accumulated content
        // For each tool_use index, create "TOOL_USE:{name}:{input}" and check if it's in the content
        let mut reconstructed = String::new();

        // Get all ToolInput keys and sort by index
        let mut tool_keys: Vec<_> = self
            .accumulated
            .keys()
            .filter(|(ct, _)| *ct == ContentType::ToolInput)
            .collect();
        tool_keys.sort_by_key(|k| k.1.parse::<u64>().unwrap_or(u64::MAX));

        for (ct, index_str) in tool_keys {
            if let Some(accumulated_input) = self.accumulated.get(&(*ct, index_str.clone())) {
                // Get the tool name from hints first (from assistant event), then from tracking
                let index_num = index_str.parse::<u64>().unwrap_or(0);
                let tool_name = tool_name_hints
                    .and_then(|hints| hints.get(&(index_num as usize)).map(|s| s.as_str()))
                    .or_else(|| self.tool_names.get(&index_num).and_then(|n| n.as_deref()))
                    .unwrap_or("");

                // Normalize: "TOOL_USE:{name}:{input}"
                reconstructed.push_str(&format!("TOOL_USE:{}:{}", tool_name, accumulated_input));
            }
        }

        // Check if the reconstructed content matches the input
        if reconstructed.is_empty() {
            return false;
        }

        normalized_content == reconstructed
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
    #[cfg(test)]
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
    #[cfg(test)]
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

    /// Mark content as rendered using pre-sanitized content.
    ///
    /// This method uses the sanitized content (with whitespace normalized)
    /// for hash-based deduplication, which prevents duplicates when the
    /// accumulated content differs only by whitespace.
    ///
    /// # Arguments
    /// * `content_type` - The type of content
    /// * `index` - The content index (as string for flexibility)
    /// * `content` - The content to hash
    pub fn mark_content_hash_rendered(
        &mut self,
        content_type: ContentType,
        index: &str,
        content: &str,
    ) {
        // Also update last_rendered for compatibility
        self.mark_rendered(content_type, index);

        // Add the hash of the content to the rendered set
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let hash = hasher.finish();
        self.rendered_content_hashes.insert(hash);
    }

    /// Check if sanitized content has already been rendered.
    ///
    /// This method checks the hash of the sanitized content against the
    /// rendered set to prevent duplicate rendering.
    ///
    /// # Arguments
    /// * `_content_type` - The type of content (kept for API consistency)
    /// * `_index` - The content index (kept for API consistency)
    /// * `sanitized_content` - The sanitized content to check
    ///
    /// # Returns
    /// * `true` - This exact content has been rendered before
    /// * `false` - This exact content has not been rendered
    pub fn is_content_hash_rendered(
        &self,
        _content_type: ContentType,
        _index: &str,
        content: &str,
    ) -> bool {
        // Compute hash of exact content
        let mut hasher = DefaultHasher::new();
        content.hash(&mut hasher);
        let hash = hasher.finish();

        // Check if this hash has been rendered before
        self.rendered_content_hashes.contains(&hash)
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
