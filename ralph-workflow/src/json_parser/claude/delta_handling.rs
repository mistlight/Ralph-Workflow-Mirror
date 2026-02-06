// Claude delta handling methods.
//
// Contains methods for handling streaming delta events.
//
// # CCS Spam Prevention (Critical Fix)
//
// This module implements non-TTY flush logic to prevent repeated prefixed lines
// for CCS agents (ccs/codex, ccs/glm) in logs and CI output. The spam bug occurred
// because delta renderers emitted one line per delta in non-TTY modes, resulting
// in hundreds of repeated "[ccs/glm]" lines for a single streamed message.
//
// ## Fix Architecture
//
// 1. **Suppression:** Delta renderers (TextDeltaRenderer, ThinkingDeltaRenderer)
//    return empty strings in non-TTY modes (Basic/None) to suppress per-delta output.
//
// 2. **Accumulation:** StreamingSession accumulates content by (ContentType, index)
//    across all deltas for text, thinking, and tool input.
//
// 3. **Flush:** ClaudeParser::handle_message_stop flushes accumulated content ONCE
//    at completion boundaries, emitting a single prefixed line per content block.
//
// ## Validation
//
// The fix is validated with comprehensive regression tests covering ultra-extreme
// scenarios (1000+ deltas per block, multi-turn sessions, all delta types).
//
// See comprehensive regression tests:
// - `tests/integration_tests/ccs_delta_spam_systematic_reproduction.rs` (NEW: systematic reproduction & verification)
// - `tests/integration_tests/ccs_all_delta_types_spam_reproduction.rs` (1000+ deltas per block)
// - `tests/integration_tests/ccs_nuclear_full_log_regression.rs` (NUCLEAR: real production logs, 12,000+ deltas)
// - `tests/integration_tests/ccs_streaming_edge_cases.rs` (edge cases: empty deltas, rapid transitions)
// - `tests/integration_tests/ccs_extreme_streaming_regression.rs` (500+ deltas per block)
// - `tests/integration_tests/ccs_streaming_spam_all_deltas.rs` (all delta types)
// - `tests/integration_tests/ccs_real_world_log_regression.rs` (production log with 12,596 deltas)
// - `tests/integration_tests/codex_reasoning_spam_regression.rs` (original reasoning fix)

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

        // Defensive fallback: if the last output left us in an unexpected cursor state
        // (e.g., raw passthrough escape sequences), finalize even if higher-level flags
        // were reset by protocol violations.
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

        // If an assistant event fully rendered this message before streaming started,
        // suppress ALL subsequent streaming deltas and avoid accumulating them.
        //
        // Rationale: if we keep accumulating deltas, the non-TTY flush at `message_stop`
        // would re-emit already-rendered content.
        if session
            .get_current_message_id()
            .is_some_and(|message_id| session.is_message_pre_rendered(message_id))
        {
            return String::new();
        }

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

                // `on_text_delta` returns whether the prefix should be shown, not whether output
                // should be emitted. If the accumulated content is non-empty and not a duplicate,
                // we still need to render it even when `show_prefix` is false.

                // Get accumulated text for streaming display
                let accumulated_text = session
                    .get_accumulated(ContentType::Text, &index_str)
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
                if session.is_content_hash_rendered(ContentType::Text, &index_str, &sanitized_text)
                {
                    return String::new();
                }

                // Use TextDeltaRenderer for consistent rendering
                let terminal_mode = *self.terminal_mode.borrow();

                if terminal_mode == TerminalMode::Full {
                    *self.text_line_active.borrow_mut() = true;
                }

                // Use prefix trie to detect if new content extends previously rendered content
                // If yes, we do an in-place update (append-only: emit only new suffix)
                let has_prefix = session.has_rendered_prefix(ContentType::Text, &index_str);

                let output = if terminal_mode == TerminalMode::Full {
                    // Append-only pattern in Full mode: track last rendered and emit only new content
                    let key = format!("text:{}", index);
                    let last_rendered = self
                        .last_rendered_content
                        .borrow()
                        .get(&key)
                        .cloned()
                        .unwrap_or_default();

                    if last_rendered.is_empty() {
                        // First delta for this index: emit prefix + content
                        let rendered = TextDeltaRenderer::render_first_delta(
                            accumulated_text,
                            prefix,
                            *c,
                            terminal_mode,
                        );
                        // Track what we rendered (the sanitized content, not the ANSI codes)
                        self.last_rendered_content
                            .borrow_mut()
                            .insert(key, sanitized_text.clone());
                        rendered
                    } else {
                        // Subsequent delta: emit only NEW suffix
                        // Compute longest common prefix between last rendered and current
                        let new_suffix = crate::json_parser::delta_display::compute_append_only_suffix(
                            &last_rendered,
                            &sanitized_text,
                        );

                        // Track new rendered content
                        self.last_rendered_content
                            .borrow_mut()
                            .insert(key, sanitized_text.clone());

                        // Emit only the new suffix (no prefix, no control codes)
                        format!("{}{}{}", c.white(), new_suffix, c.reset())
                    }
                } else {
                    // Basic/None mode: suppress per-delta output (existing behavior)
                    if show_prefix && !has_prefix {
                        TextDeltaRenderer::render_first_delta(
                            accumulated_text,
                            prefix,
                            *c,
                            terminal_mode,
                        )
                    } else {
                        TextDeltaRenderer::render_subsequent_delta(
                            accumulated_text,
                            prefix,
                            *c,
                            terminal_mode,
                        )
                    }
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
                let _show_prefix = session.on_thinking_delta(index, &text);

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
                        let sanitized =
                            crate::json_parser::delta_display::sanitize_for_display(accumulated);

                        // Append-only pattern: track last rendered and emit only new content
                        let key = format!("thinking:{}", index);
                        let last_rendered = self
                            .last_rendered_content
                            .borrow()
                            .get(&key)
                            .cloned()
                            .unwrap_or_default();

                        let out = if last_rendered.is_empty() {
                            // First delta for this thinking block: emit prefix + content
                            let rendered = crate::json_parser::delta_display::ThinkingDeltaRenderer::render_first_delta(
                                accumulated,
                                prefix,
                                *c,
                                terminal_mode,
                            );
                            // Track what we rendered (the sanitized content)
                            self.last_rendered_content
                                .borrow_mut()
                                .insert(key, sanitized.clone());
                            rendered
                        } else {
                            // Subsequent delta: emit only NEW suffix
                            let new_suffix = crate::json_parser::delta_display::compute_append_only_suffix(
                                &last_rendered,
                                &sanitized,
                            );

                            // Track new rendered content
                            self.last_rendered_content
                                .borrow_mut()
                                .insert(key, sanitized.clone());

                            // Emit only the new suffix (no prefix, no \r)
                            // Use the same color scheme as ThinkingDeltaRenderer for consistency
                            format!("{}{}{}", c.cyan(), new_suffix, c.reset())
                        };

                        out
                    }
                    TerminalMode::Basic | TerminalMode::None => {
                        // Track all thinking indices that accumulated content so we can flush them
                        // at message_stop. Providers can emit multiple thinking content blocks in a
                        // single message, so tracking only the "active" index would drop earlier
                        // thinking blocks from non-TTY output.
                        self.thinking_non_tty_indices.borrow_mut().insert(index);
                        String::new()
                    }
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

                    // Tool input is rendered once at tool completion/message_stop in non-TTY modes
                    // to avoid repeated prefixed lines for partial JSON chunks.
                    let terminal_mode = *self.terminal_mode.borrow();
                    if matches!(terminal_mode, TerminalMode::Basic | TerminalMode::None) {
                        thinking_finalize
                    } else {
                        // Show partial tool input in real-time in Full TTY mode
                        let formatter = DeltaDisplayFormatter::new();
                        let tool_out =
                            formatter.format_tool_input(&input_str, prefix, *c, terminal_mode);
                        format!("{thinking_finalize}{tool_out}")
                    }
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
            // Append-only streaming keeps the cursor on the current line; we still track
            // that a streaming text line is active so newline-based output can ensure the
            // final completion newline is emitted at message boundaries.
            *self.text_line_active.borrow_mut() = true;
        }

        // Use prefix trie to detect if new content extends previously rendered content
        let has_prefix = session.has_rendered_prefix(ContentType::Text, default_index_str);

        let output = if terminal_mode == TerminalMode::Full {
            // Append-only pattern in Full mode: track last rendered and emit only new content
            let key = format!("text:{}", default_index);
            let last_rendered = self
                .last_rendered_content
                .borrow()
                .get(&key)
                .cloned()
                .unwrap_or_default();

            if last_rendered.is_empty() {
                // First delta for this index: emit prefix + content
                let rendered = TextDeltaRenderer::render_first_delta(
                    accumulated_text,
                    prefix,
                    *c,
                    terminal_mode,
                );
                // Track what we rendered (the sanitized content, not the ANSI codes)
                self.last_rendered_content
                    .borrow_mut()
                    .insert(key, sanitized_text.clone());
                rendered
            } else {
                // Subsequent delta: emit only NEW suffix
                // Compute longest common prefix between last rendered and current
                let new_suffix = crate::json_parser::delta_display::compute_append_only_suffix(
                    &last_rendered,
                    &sanitized_text,
                );

                // Track new rendered content
                self.last_rendered_content
                    .borrow_mut()
                    .insert(key, sanitized_text.clone());

                // Emit only the new suffix (no prefix, no control codes)
                format!("{}{}{}", c.white(), new_suffix, c.reset())
            }
        } else {
            // Basic/None mode: use original logic
            if show_prefix && !has_prefix {
                TextDeltaRenderer::render_first_delta(accumulated_text, prefix, *c, terminal_mode)
            } else {
                // In Basic/None modes, render_subsequent_delta returns empty string anyway
                TextDeltaRenderer::render_subsequent_delta(
                    accumulated_text,
                    prefix,
                    *c,
                    terminal_mode,
                )
            }
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

        // In non-TTY modes, flush thinking, tool input, and text once at message_stop.
        let (thinking_flush_non_tty, tool_input_flush_non_tty, text_flush_non_tty) =
            match terminal_mode {
                TerminalMode::Full => (String::new(), String::new(), String::new()),
                TerminalMode::Basic | TerminalMode::None => {
                    // If the final assistant message was already rendered (pre-rendered), do not
                    // flush accumulated streaming state in non-TTY modes.
                    //
                    // Some providers emit a complete assistant event before streaming deltas;
                    // those deltas can still arrive and would otherwise be accumulated and flushed
                    // here, duplicating already-rendered content.
                    if session
                        .get_current_message_id()
                        .is_some_and(|message_id| session.is_message_pre_rendered(message_id))
                    {
                        // Clear any pending thinking indices to avoid cross-message contamination.
                        self.thinking_active_index.borrow_mut().take();
                        self.thinking_non_tty_indices.borrow_mut().clear();
                        (String::new(), String::new(), String::new())
                    } else {
                        // Flush accumulated thinking.
                        // We format the output directly here because the renderers now suppress
                        // output in non-TTY modes (to prevent per-delta spam).
                        let mut thinking_output = String::new();
                        {
                            let indices: Vec<u64> =
                                if !self.thinking_non_tty_indices.borrow().is_empty() {
                                    self.thinking_non_tty_indices
                                        .borrow()
                                        .iter()
                                        .copied()
                                        .collect()
                                } else {
                                    // Backward-compatible fallback: if we never recorded indices (older
                                    // behavior), flush the single active index.
                                    self.thinking_active_index
                                        .borrow()
                                        .iter()
                                        .copied()
                                        .collect()
                                };

                            // Reset parser-owned tracking so subsequent messages don't inherit indices.
                            self.thinking_non_tty_indices.borrow_mut().clear();
                            self.thinking_active_index.borrow_mut().take();

                            for index in indices {
                                let index_str = index.to_string();
                                let accumulated = session
                                    .get_accumulated(ContentType::Thinking, &index_str)
                                    .unwrap_or("");
                                let sanitized =
                                    crate::json_parser::delta_display::sanitize_for_display(
                                        accumulated,
                                    );
                                if sanitized.is_empty() {
                                    continue;
                                }

                                let prefix_fmt = match terminal_mode {
                                    TerminalMode::Basic => format!(
                                        "{}[{}]{} {}",
                                        c.dim(),
                                        &self.display_name,
                                        c.reset(),
                                        c.dim()
                                    ),
                                    TerminalMode::None => format!("[{}] ", &self.display_name),
                                    TerminalMode::Full => unreachable!(),
                                };

                                let label_fmt = match terminal_mode {
                                    TerminalMode::Basic => format!("Thinking: {}", c.cyan()),
                                    TerminalMode::None => "Thinking: ".to_string(),
                                    TerminalMode::Full => unreachable!(),
                                };

                                let suffix_fmt = match terminal_mode {
                                    TerminalMode::Basic => c.reset().to_string(),
                                    TerminalMode::None => String::new(),
                                    TerminalMode::Full => unreachable!(),
                                };

                                thinking_output.push_str(&format!(
                                    "{prefix_fmt}{label_fmt}{sanitized}{suffix_fmt}\n"
                                ));
                            }
                        }

                        // Flush accumulated tool input.
                        // Tool input deltas can arrive as partial JSON chunks; in non-TTY modes we
                        // render the final accumulated value once at message_stop.
                        //
                        // IMPORTANT: Tool inputs can contain secrets. Respect the global verbosity
                        // policy (same as assistant tool blocks) rather than unconditionally printing.
                        let mut tool_output = String::new();
                        if self.verbosity.show_tool_input() {
                            for index_str in session.accumulated_keys(ContentType::ToolInput) {
                                let accumulated = session
                                    .get_accumulated(ContentType::ToolInput, &index_str)
                                    .unwrap_or("");
                                let sanitized =
                                    crate::json_parser::delta_display::sanitize_for_display(
                                        accumulated,
                                    );
                                if !sanitized.is_empty() {
                                    let prefix_fmt = match terminal_mode {
                                        TerminalMode::Basic => format!(
                                            "{}[{}]{} {}",
                                            c.dim(),
                                            &self.display_name,
                                            c.reset(),
                                            c.dim()
                                        ),
                                        TerminalMode::None => {
                                            format!("[{}] ", &self.display_name)
                                        }
                                        TerminalMode::Full => unreachable!(),
                                    };

                                    let label_fmt = match terminal_mode {
                                        TerminalMode::Basic => {
                                            format!("Tool input: {}", c.cyan())
                                        }
                                        TerminalMode::None => "Tool input: ".to_string(),
                                        TerminalMode::Full => unreachable!(),
                                    };

                                    let suffix_fmt = match terminal_mode {
                                        TerminalMode::Basic => c.reset().to_string(),
                                        TerminalMode::None => String::new(),
                                        TerminalMode::Full => unreachable!(),
                                    };

                                    tool_output.push_str(&format!(
                                        "{prefix_fmt}{label_fmt}{sanitized}{suffix_fmt}\n"
                                    ));
                                }
                            }
                        }

                        // Flush accumulated text content for all content blocks.
                        // We format the output directly here because the renderers now suppress
                        // output in non-TTY modes (to prevent per-delta spam).
                        let mut text_output = String::new();
                        for index_str in session.accumulated_keys(ContentType::Text) {
                            let accumulated = session
                                .get_accumulated(ContentType::Text, &index_str)
                                .unwrap_or("");
                            let sanitized = crate::json_parser::delta_display::sanitize_for_display(
                                accumulated,
                            );
                            if !sanitized.is_empty() {
                                let prefix_fmt = match terminal_mode {
                                    TerminalMode::Basic => format!(
                                        "{}[{}]{} {}",
                                        c.dim(),
                                        &self.display_name,
                                        c.reset(),
                                        c.white()
                                    ),
                                    TerminalMode::None => {
                                        format!("[{}] ", &self.display_name)
                                    }
                                    TerminalMode::Full => unreachable!(),
                                };

                                let suffix_fmt = match terminal_mode {
                                    TerminalMode::Basic => c.reset().to_string(),
                                    TerminalMode::None => String::new(),
                                    TerminalMode::Full => unreachable!(),
                                };

                                text_output
                                    .push_str(&format!("{prefix_fmt}{sanitized}{suffix_fmt}\n"));
                            }
                        }

                        (thinking_output, tool_output, text_output)
                    }
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

            let completion = if terminal_mode == TerminalMode::Full {
                format!(
                    "{}{}",
                    c.reset(),
                    TextDeltaRenderer::render_completion(terminal_mode)
                )
            } else {
                // In non-TTY modes, flush output paths already include newline terminators and
                // individual lines already end with a reset. Emitting an additional standalone
                // reset here can leave a trailing ANSI sequence after the final newline.
                String::new()
            };

            // Show streaming quality metrics in debug mode or when flag is set
            let show_metrics = (self.verbosity.is_debug() || self.show_streaming_metrics)
                && metrics.total_deltas > 0;
            let completion_with_metrics = if show_metrics {
                if terminal_mode == TerminalMode::Full {
                    format!("{}\n{}", completion, metrics.format(*c))
                } else {
                    // In non-TTY, the flush output already ended with a newline, so metrics can be
                    // appended directly without inserting an extra blank line.
                    metrics.format(*c)
                }
            } else {
                completion
            };

            format!("{thinking_finalize}{thinking_flush_non_tty}{tool_input_flush_non_tty}{text_flush_non_tty}{completion_with_metrics}")
        } else {
            format!("{thinking_finalize}{thinking_flush_non_tty}{tool_input_flush_non_tty}{text_flush_non_tty}")
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
