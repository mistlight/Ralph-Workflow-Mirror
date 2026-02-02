impl StreamingSession {
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
}
