// Text + error formatting.

impl OpenCodeParser {
    pub(super) fn format_text_event(&self, event: &OpenCodeEvent) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        if let Some(ref part) = event.part {
            if let Some(ref text) = part.text {
                // Accumulate streaming text using StreamingSession
                let (show_prefix, accumulated_text) = {
                    let mut session = self.streaming_session.borrow_mut();
                    let show_prefix = session.on_text_delta_key("main", text);
                    // Get accumulated text for streaming display
                    let accumulated_text = session
                        .get_accumulated(ContentType::Text, "main")
                        .unwrap_or("")
                        .to_string();
                    (show_prefix, accumulated_text)
                };

                // Show delta in real-time (both verbose and normal mode)
                let limit = self.verbosity.truncate_limit("text");
                let preview = truncate_text(&accumulated_text, limit);

                // Use TextDeltaRenderer for consistent rendering across all parsers
                let terminal_mode = *self.terminal_mode.borrow();
                if show_prefix {
                    // First delta: use renderer with prefix
                    return TextDeltaRenderer::render_first_delta(
                        &preview,
                        prefix,
                        *c,
                        terminal_mode,
                    );
                }
                // Subsequent deltas: use renderer for in-place update
                return TextDeltaRenderer::render_subsequent_delta(
                    &preview,
                    prefix,
                    *c,
                    terminal_mode,
                );
            }
        }
        String::new()
    }

    /// Format an `error` event
    ///
    /// From OpenCode source (`run.ts` lines 192-202), error events are emitted for session errors:
    /// ```typescript
    /// if (event.type === "session.error") {
    ///   let err = String(props.error.name)
    ///   if ("data" in props.error && props.error.data && "message" in props.error.data) {
    ///     err = String(props.error.data.message)
    ///   }
    ///   outputJsonEvent("error", { error: props.error })
    /// }
    /// ```
    pub(super) fn format_error_event(&self, event: &OpenCodeEvent, raw_line: &str) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        // Try to extract error message from the event
        let error_msg = event.error.as_ref().map_or_else(
            || {
                // Fallback: try to extract from raw JSON
                if let Ok(json) = serde_json::from_str::<serde_json::Value>(raw_line) {
                    json.get("error")
                        .and_then(|e| {
                            // Try data.message first (as in run.ts)
                            e.get("data")
                                .and_then(|d| d.get("message"))
                                .and_then(|m| m.as_str())
                                .map(String::from)
                                // Then try direct message
                                .or_else(|| {
                                    e.get("message").and_then(|m| m.as_str()).map(String::from)
                                })
                                // Then try name
                                .or_else(|| {
                                    e.get("name").and_then(|n| n.as_str()).map(String::from)
                                })
                        })
                        .unwrap_or_else(|| "Unknown error".to_string())
                } else {
                    "Unknown error".to_string()
                }
            },
            |err| {
                // Try data.message first (as in run.ts)
                err.data
                    .as_ref()
                    .and_then(|d| d.get("message"))
                    .and_then(|m| m.as_str())
                    .map(String::from)
                    // Then try direct message
                    .or_else(|| err.message.clone())
                    // Then try name
                    .or_else(|| err.name.clone())
                    .unwrap_or_else(|| "Unknown error".to_string())
            },
        );

        let limit = self.verbosity.truncate_limit("text");
        let preview = truncate_text(&error_msg, limit);

        format!(
            "{}[{}]{} {}{} Error:{} {}{}{}\n",
            c.dim(),
            prefix,
            c.reset(),
            c.red(),
            CROSS,
            c.reset(),
            c.red(),
            preview,
            c.reset()
        )
    }
}
