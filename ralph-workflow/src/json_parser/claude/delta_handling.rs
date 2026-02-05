// Claude delta handling methods.
//
// Contains methods for handling streaming delta events.

impl ClaudeParser {
    fn finalize_in_place_full_mode(
        &self,
        session: &mut std::cell::RefMut<'_, StreamingSession>,
    ) -> String {
        let terminal_mode = *self.terminal_mode.borrow();
        if terminal_mode != TerminalMode::Full {
            return String::new();
        }

        // Prefer thinking finalization when active (it owns the cursor-up state).
        if self.thinking_active_index.borrow().is_some() {
            return self.finalize_thinking_full_mode(session);
        }

        // Otherwise, finalize an active text streaming line.
        if *self.text_line_active.borrow() {
            *self.text_line_active.borrow_mut() = false;
            *self.cursor_up_active.borrow_mut() = false;
            return TextDeltaRenderer::render_completion(terminal_mode);
        }

        // Defensive fallback: if the last output used the cursor-up in-place pattern,
        // finalize even if higher-level flags were reset by protocol violations.
        if *self.cursor_up_active.borrow() {
            *self.cursor_up_active.borrow_mut() = false;
            return TextDeltaRenderer::render_completion(terminal_mode);
        }

        String::new()
    }

    fn finalize_thinking_full_mode(
        &self,
        session: &mut std::cell::RefMut<'_, StreamingSession>,
    ) -> String {
        let terminal_mode = *self.terminal_mode.borrow();
        match terminal_mode {
            TerminalMode::Full => {
                let Some(_index) = self.thinking_active_index.borrow_mut().take() else {
                    return String::new();
                };
                *self.cursor_up_active.borrow_mut() = false;
                // Keep `session` in the signature for symmetry with other finalizers.
                // Thinking finalization is parser-owned state in Full mode.
                let _ = session;
                // Finalize the multi-line in-place update pattern for thinking.
                // This leaves the final thinking line visible and moves the cursor to the next line.
                <crate::json_parser::delta_display::ThinkingDeltaRenderer as DeltaRenderer>::render_completion(
                    terminal_mode,
                )
            }
            TerminalMode::Basic | TerminalMode::None => {
                let _ = session;
                String::new()
            }
        }
    }

    /// Handle content block delta events
    fn handle_content_block_delta(
        &self,
        session: &mut std::cell::RefMut<'_, StreamingSession>,
        index: u64,
        delta: ContentBlockDelta,
    ) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        match delta {
            ContentBlockDelta::TextDelta { text: Some(text) } => {
                let thinking_finalize = self.finalize_thinking_full_mode(session);
                *self.suppress_thinking_for_message.borrow_mut() = true;
                let index_str = index.to_string();

                // Track this delta with StreamingSession for state management.
                //
                // StreamingSession handles protocol/streaming quality concerns (including
                // snapshot-as-delta repairs and consecutive duplicate filtering) and returns
                // whether a prefix should be displayed for this stream.
                //
                // The parser layer still applies additional deduplication:
                // - Skip whitespace-only accumulated output
                // - Hash-based deduplication after sanitization (whitespace-insensitive)
                let show_prefix = session.on_text_delta(index, &text);

                // Get accumulated text for streaming display
                let accumulated_text = session
                    .get_accumulated(ContentType::Text, &index_str)
                    .unwrap_or("");

                // Check if this message was pre-rendered from an assistant event.
                // When an assistant event arrives BEFORE streaming deltas, we render it
                // and mark the message_id as pre-rendered. ALL subsequent streaming deltas
                // for this message should be suppressed to prevent duplication.
                if let Some(message_id) = session.get_current_message_id() {
                    if session.is_message_pre_rendered(message_id) {
                        return String::new();
                    }
                }

                // Sanitize the accumulated text to check if it's empty
                // This is needed to skip rendering when the accumulated content is just whitespace
                let sanitized_text = super::delta_display::sanitize_for_display(accumulated_text);

                // Skip rendering if the sanitized text is empty (e.g., only whitespace)
                // This prevents rendering empty lines when the accumulated content is just whitespace
                if sanitized_text.is_empty() {
                    return String::new();
                }

                // Check if this sanitized content has already been rendered
                // This prevents duplicates when accumulated content differs only by whitespace
                if session.is_content_hash_rendered(ContentType::Text, &index_str, &sanitized_text)
                {
                    return String::new();
                }

                // Use TextDeltaRenderer for consistent rendering
                let terminal_mode = *self.terminal_mode.borrow();

                if terminal_mode == TerminalMode::Full {
                    *self.text_line_active.borrow_mut() = true;
                    *self.cursor_up_active.borrow_mut() = true;
                }

                // Use prefix trie to detect if new content extends previously rendered content
                // If yes, we do an in-place update (carriage return + new content)
                let has_prefix = session.has_rendered_prefix(ContentType::Text, &index_str);

                let output = if show_prefix && !has_prefix {
                    // First delta with no prefix match - use the renderer with prefix
                    TextDeltaRenderer::render_first_delta(
                        accumulated_text,
                        prefix,
                        *c,
                        terminal_mode,
                    )
                } else {
                    // Either continuation OR prefix match - use renderer for in-place update
                    // This handles the case where "Hello" becomes "Hello World" - we REPLACE
                    TextDeltaRenderer::render_subsequent_delta(
                        accumulated_text,
                        prefix,
                        *c,
                        terminal_mode,
                    )
                };

                // Mark this sanitized content as rendered for future duplicate detection
                // We use the sanitized text (not the rendered output) to avoid false positives
                // when the same accumulated text is rendered with different terminal modes
                session.mark_rendered(ContentType::Text, &index_str);
                session.mark_content_hash_rendered(ContentType::Text, &index_str, &sanitized_text);

                format!("{thinking_finalize}{output}")
            }
            ContentBlockDelta::ThinkingDelta {
                thinking: Some(text),
            } => {
                let show_prefix = session.on_thinking_delta(index, &text);

                if *self.suppress_thinking_for_message.borrow() {
                    // Accumulate for state/deduplication, but don't render late thinking.
                    return String::new();
                }

                *self.thinking_active_index.borrow_mut() = Some(index);

                // In non-TTY modes, we suppress per-delta thinking output and flush once
                // at the next output boundary (or at message_stop).
                let terminal_mode = *self.terminal_mode.borrow();
                match terminal_mode {
                    TerminalMode::Full => {
                        let index_str = index.to_string();
                        let accumulated = session
                            .get_accumulated(ContentType::Thinking, &index_str)
                            .unwrap_or("");
                        let out = if show_prefix {
                            crate::json_parser::delta_display::ThinkingDeltaRenderer::render_first_delta(
                                accumulated,
                                prefix,
                                *c,
                                terminal_mode,
                            )
                        } else {
                            crate::json_parser::delta_display::ThinkingDeltaRenderer::render_subsequent_delta(
                                accumulated,
                                prefix,
                                *c,
                                terminal_mode,
                            )
                        };

                        *self.cursor_up_active.borrow_mut() = true;
                        out
                    }
                    TerminalMode::Basic | TerminalMode::None => String::new(),
                }
            }
            ContentBlockDelta::ToolUseDelta {
                tool_use: Some(tool_delta),
            } => {
                let thinking_finalize = self.finalize_in_place_full_mode(session);
                *self.suppress_thinking_for_message.borrow_mut() = true;
                // Track tool name for GLM/CCS deduplication (if available in delta)
                if let Some(serde_json::Value::String(name)) = tool_delta.get("name") {
                    session.set_tool_name(index, Some(name.to_string()));
                }

                // Handle tool input streaming
                // Extract the tool input from the delta
                let input_str =
                    tool_delta
                        .get("input")
                        .map_or_else(String::new, |input| match input {
                            serde_json::Value::String(s) => s.clone(),
                            other => format_tool_input(other),
                        });

                if input_str.is_empty() {
                    thinking_finalize
                } else {
                    // Accumulate tool input
                    session.on_tool_input_delta(index, &input_str);

                    // Show partial tool input in real-time
                    let formatter = DeltaDisplayFormatter::new();
                    let terminal_mode = *self.terminal_mode.borrow();
                    let tool_out =
                        formatter.format_tool_input(&input_str, prefix, *c, terminal_mode);
                    format!("{thinking_finalize}{tool_out}")
                }
            }
            _ => String::new(),
        }
    }

    /// Handle text delta events
    fn handle_text_delta(
        &self,
        session: &mut std::cell::RefMut<'_, StreamingSession>,
        text: &str,
    ) -> String {
        let thinking_finalize = self.finalize_thinking_full_mode(session);
        *self.suppress_thinking_for_message.borrow_mut() = true;
        let c = &self.colors;
        let prefix = &self.display_name;

        // Standalone text delta (not part of content block)
        // Use default index "0" for standalone text
        let default_index = 0u64;
        let default_index_str = "0";

        // Track this delta with StreamingSession for state management.
        //
        // StreamingSession handles protocol/streaming quality concerns (including
        // snapshot-as-delta repairs and consecutive duplicate filtering) and returns
        // whether a prefix should be displayed for this stream.
        //
        // The parser layer still applies additional deduplication:
        // - Skip whitespace-only accumulated output
        // - Hash-based deduplication after sanitization (whitespace-insensitive)
        let show_prefix = session.on_text_delta(default_index, text);

        // Get accumulated text for streaming display
        let accumulated_text = session
            .get_accumulated(ContentType::Text, default_index_str)
            .unwrap_or("");

        // Sanitize the accumulated text to check if it's empty
        // This is needed to skip rendering when the accumulated content is just whitespace
        let sanitized_text = super::delta_display::sanitize_for_display(accumulated_text);

        // Skip rendering if the sanitized text is empty (e.g., only whitespace)
        // This prevents rendering empty lines when the accumulated content is just whitespace
        if sanitized_text.is_empty() {
            return String::new();
        }

        // Check if this sanitized content has already been rendered
        // This prevents duplicates when accumulated content differs only by whitespace
        if session.is_content_hash_rendered(ContentType::Text, default_index_str, &sanitized_text) {
            return String::new();
        }

        // Use TextDeltaRenderer for consistent rendering across all parsers
        let terminal_mode = *self.terminal_mode.borrow();

        if terminal_mode == TerminalMode::Full {
            *self.text_line_active.borrow_mut() = true;
            *self.cursor_up_active.borrow_mut() = true;
        }

        // Use prefix trie to detect if new content extends previously rendered content
        // If yes, we do an in-place update (carriage return + new content)
        let has_prefix = session.has_rendered_prefix(ContentType::Text, default_index_str);

        let output = if show_prefix && !has_prefix {
            // First delta with no prefix match - use the renderer with prefix
            TextDeltaRenderer::render_first_delta(accumulated_text, prefix, *c, terminal_mode)
        } else {
            // Either continuation OR prefix match - use renderer for in-place update
            // This handles the case where "Hello" becomes "Hello World" - we REPLACE
            TextDeltaRenderer::render_subsequent_delta(accumulated_text, prefix, *c, terminal_mode)
        };

        // Mark this sanitized content as rendered for future duplicate detection
        // We use the sanitized text (not the rendered output) to avoid false positives
        // when the same accumulated text is rendered with different terminal modes
        session.mark_rendered(ContentType::Text, default_index_str);
        session.mark_content_hash_rendered(ContentType::Text, default_index_str, &sanitized_text);

        format!("{thinking_finalize}{output}")
    }

    /// Handle message stop events
    fn handle_message_stop(&self, session: &mut std::cell::RefMut<'_, StreamingSession>) -> String {
        let c = &self.colors;

        let terminal_mode = *self.terminal_mode.borrow();

        // In Full mode, finalize any active thinking line.
        let thinking_finalize = self.finalize_thinking_full_mode(session);

        // In non-TTY modes, flush thinking and text once at message_stop.
        let (thinking_flush_non_tty, text_flush_non_tty) = match terminal_mode {
            TerminalMode::Full => (String::new(), String::new()),
            TerminalMode::Basic | TerminalMode::None => {
                // Flush accumulated thinking
                // We format the output directly here because the renderers now suppress
                // output in non-TTY modes (to prevent per-delta spam).
                let thinking_output =
                    if let Some(index) = self.thinking_active_index.borrow_mut().take() {
                        let index_str = index.to_string();
                        let accumulated = session
                            .get_accumulated(ContentType::Thinking, &index_str)
                            .unwrap_or("");
                        let sanitized =
                            crate::json_parser::delta_display::sanitize_for_display(accumulated);
                        if sanitized.is_empty() {
                            String::new()
                        } else {
                            // Format the line directly (bypass renderer which suppresses in non-TTY)
                            format!(
                                "{}[{}]{} {}Thinking: {}{}{}\n",
                                c.dim(),
                                &self.display_name,
                                c.reset(),
                                c.dim(),
                                c.cyan(),
                                sanitized,
                                c.reset()
                            )
                        }
                    } else {
                        String::new()
                    };

                // Flush accumulated text content for all content blocks
                // We format the output directly here because the renderers now suppress
                // output in non-TTY modes (to prevent per-delta spam).
                let mut text_output = String::new();
                for index in 0..10 {
                    // Reasonable upper bound for content blocks
                    let index_str = index.to_string();
                    let accumulated = session
                        .get_accumulated(ContentType::Text, &index_str)
                        .unwrap_or("");
                    let sanitized =
                        crate::json_parser::delta_display::sanitize_for_display(accumulated);
                    if !sanitized.is_empty() {
                        // Format the line directly (bypass renderer which suppresses in non-TTY)
                        let line = format!(
                            "{}[{}]{} {}{}{}\n",
                            c.dim(),
                            &self.display_name,
                            c.reset(),
                            c.white(),
                            sanitized,
                            c.reset()
                        );
                        text_output.push_str(&line);
                    }
                }

                (thinking_output, text_output)
            }
        };

        // Message complete - add final newline if we were in a content block
        // OR if any content was streamed (handles edge cases where block state
        // may not have been set but content was still streamed)
        let metrics = session.get_streaming_quality_metrics();
        let was_in_block = session.on_message_stop();

        // In Full mode, a streamed text line can leave the cursor positioned on the line
        // (via a trailing "\n\x1b[1A"). Normally `was_in_block` implies we should emit a
        // completion sequence, but some real-world logs can violate block lifecycle ordering.
        // If we have an active text streaming line, still emit a completion sequence.
        let needs_text_completion = terminal_mode == TerminalMode::Full
            && (*self.text_line_active.borrow() || *self.cursor_up_active.borrow());
        let should_emit_completion = was_in_block || needs_text_completion;

        if should_emit_completion {
            if terminal_mode == TerminalMode::Full {
                *self.text_line_active.borrow_mut() = false;
                *self.cursor_up_active.borrow_mut() = false;
            }

            let completion = format!(
                "{}{}",
                c.reset(),
                TextDeltaRenderer::render_completion(terminal_mode)
            );
            // Show streaming quality metrics in debug mode or when flag is set
            let show_metrics = (self.verbosity.is_debug() || self.show_streaming_metrics)
                && metrics.total_deltas > 0;
            let completion_with_metrics = if show_metrics {
                format!("{}\n{}", completion, metrics.format(*c))
            } else {
                completion
            };
            format!("{thinking_finalize}{thinking_flush_non_tty}{text_flush_non_tty}{completion_with_metrics}")
        } else {
            format!("{thinking_finalize}{thinking_flush_non_tty}{text_flush_non_tty}")
        }
    }

    /// Handle error events
    fn handle_error_event(&self, err: crate::json_parser::types::StreamError) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        let msg = err
            .message
            .unwrap_or_else(|| "Unknown streaming error".to_string());
        format!(
            "{}[{}]{} {}Error: {}{}\n",
            c.dim(),
            prefix,
            c.reset(),
            c.red(),
            msg,
            c.reset()
        )
    }

    /// Handle unknown events
    fn handle_unknown_event(&self) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        // Unknown stream event - in debug mode, log it
        if self.verbosity.is_debug() {
            format!(
                "{}[{}]{} {}Unknown streaming event{}\n",
                c.dim(),
                prefix,
                c.reset(),
                c.dim(),
                c.reset()
            )
        } else {
            String::new()
        }
    }
}
