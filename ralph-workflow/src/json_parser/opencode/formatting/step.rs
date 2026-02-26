// Step lifecycle formatting.

impl OpenCodeParser {
    /// Format a `step_start` event
    pub(super) fn format_step_start_event(&self, event: &OpenCodeEvent) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        let session = event.session_id.as_deref().unwrap_or("unknown");

        // Create unique step ID for duplicate detection.
        //
        // OpenCode normally includes a stable `part.id` and/or `part.messageID`. However,
        // in minimal / test fixtures those fields may be absent. In that case, do NOT
        // fall back to a constant like "{session}:step" (it would collapse multiple
        // steps into one and break lifecycle state).
        //
        // Priority:
        // 1) part.message_id (best)
        // 2) session_id + part.id
        // 3) session_id + part.snapshot
        // 4) session_id + timestamp + counter (best-effort uniqueness)
        let step_id = event.part.as_ref().and_then(|part| {
            part.message_id.clone().or_else(|| {
                part.id
                    .as_ref()
                    .map(|id| format!("{session}:{id}"))
                    .or_else(|| {
                        part.snapshot
                            .as_ref()
                            .map(|snapshot| format!("{session}:{snapshot}"))
                    })
            })
        });

        let step_id =
            step_id.unwrap_or_else(|| self.next_fallback_step_id(session, event.timestamp));

        // Defensive: OpenCode can emit duplicate `step_start` events for the same message.
        // Suppress duplicates to avoid spamming and to avoid resetting streaming state mid-step.
        if self
            .streaming_session
            .borrow()
            .get_current_message_id()
            .is_some_and(|current| current == step_id)
        {
            return String::new();
        }

        // Reset streaming state on new step
        self.streaming_session.borrow_mut().on_message_start();
        self.last_rendered_content.borrow_mut().clear();
        self.streaming_session
            .borrow_mut()
            .set_current_message_id(Some(step_id));

        let snapshot = event
            .part
            .as_ref()
            .and_then(|p| p.snapshot.as_ref())
            .map(|s| format!("({s:.8}...)"))
            .unwrap_or_default();
        format!(
            "{}[{}]{} {}Step started{} {}{}{}\n",
            c.dim(),
            prefix,
            c.reset(),
            c.cyan(),
            c.reset(),
            c.dim(),
            snapshot,
            c.reset()
        )
    }

    /// Format a `step_finish` event
    pub(super) fn format_step_finish_event(&self, event: &OpenCodeEvent) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        if self
            .streaming_session
            .borrow()
            .get_current_message_id()
            .is_none()
        {
            let session = event.session_id.as_deref().unwrap_or("unknown");
            let step_id = event.part.as_ref().and_then(|part| {
                part.message_id.clone().or_else(|| {
                    part.id
                        .as_ref()
                        .map(|id| format!("{session}:{id}"))
                        .or_else(|| {
                            part.snapshot
                                .as_ref()
                                .map(|snapshot| format!("{session}:{snapshot}"))
                        })
                })
            });
            let step_id =
                step_id.unwrap_or_else(|| self.next_fallback_step_id(session, event.timestamp));
            self.streaming_session
                .borrow_mut()
                .set_current_message_id(Some(step_id));
        }

        // Check for duplicate final message using message ID or fallback to streaming content check
        let session = self.streaming_session.borrow();
        let is_duplicate = session.get_current_message_id().map_or_else(
            || session.has_any_streamed_content(),
            |message_id| session.is_duplicate_final_message(message_id),
        );
        let was_streaming = session.has_any_streamed_content();
        let metrics = session.get_streaming_quality_metrics();
        drop(session);

        // Finalize the message (this marks it as displayed)
        let _was_in_block = self.streaming_session.borrow_mut().on_message_stop();

        // In non-TTY modes, per-delta output is suppressed. Flush accumulated assistant text
        // once at the completion boundary so piped/log output contains the assistant content.
        let terminal_mode = *self.terminal_mode.borrow();
        let text_flush_non_tty = match terminal_mode {
            TerminalMode::Full => String::new(),
            TerminalMode::Basic | TerminalMode::None => {
                let session = self.streaming_session.borrow();
                let mut out = String::new();
                for key in session.accumulated_keys(ContentType::Text) {
                    let accumulated = session.get_accumulated(ContentType::Text, &key).unwrap_or("");
                    let sanitized = crate::json_parser::delta_display::sanitize_for_display(accumulated);
                    if sanitized.is_empty() {
                        continue;
                    }
                    match terminal_mode {
                        TerminalMode::Basic => {
                            out.push_str(&format!(
                                "{}[{}]{} {}{}{}\n",
                                c.dim(),
                                prefix,
                                c.reset(),
                                c.white(),
                                sanitized,
                                c.reset()
                            ));
                        }
                        TerminalMode::None => {
                            writeln!(out, "[{prefix}] {sanitized}").unwrap();
                        }
                        TerminalMode::Full => unreachable!(),
                    }
                }
                out
            }
        };

        event.part.as_ref().map_or_else(String::new, |part| {
            let reason = part.reason.as_deref().unwrap_or("unknown");
            let cost = part.cost.unwrap_or(0.0);

            let tokens_str = part.tokens.as_ref().map_or_else(String::new, |tokens| {
                let input = tokens.input.unwrap_or(0);
                let output = tokens.output.unwrap_or(0);
                let reasoning = tokens.reasoning.unwrap_or(0);
                let cache_read = tokens.cache.as_ref().and_then(|c| c.read).unwrap_or(0);
                if reasoning > 0 {
                    format!("in:{input} out:{output} reason:{reasoning} cache:{cache_read}")
                } else if cache_read > 0 {
                    format!("in:{input} out:{output} cache:{cache_read}")
                } else {
                    format!("in:{input} out:{output}")
                }
            });

            let is_success = reason == "tool-calls" || reason == "end_turn";
            let icon = if is_success { CHECK } else { CROSS };
            let color = if is_success { c.green() } else { c.yellow() };

            // Add completion marker if we were streaming text.
            //
            // In Full mode we only need the final newline; in Basic/None modes we already
            // flushed the accumulated text above (newline-terminated), and the renderer's
            // completion is a no-op.
            let newline_prefix = if is_duplicate || was_streaming {
                let completion = TextDeltaRenderer::render_completion(terminal_mode);
                let show_metrics = (self.verbosity.is_debug() || self.show_streaming_metrics)
                    && metrics.total_deltas > 0;
                if show_metrics {
                    format!("{}\n{}", completion, metrics.format(*c))
                } else {
                    completion
                }
            } else {
                String::new()
            };

            // Prepend the non-TTY text flush so logs include assistant content.
            let flush_prefix = &text_flush_non_tty;

            let mut out = format!(
                "{}{}{}[{}]{} {}{} Step finished{} {}({}",
                flush_prefix,
                newline_prefix,
                c.dim(),
                prefix,
                c.reset(),
                color,
                icon,
                c.reset(),
                c.dim(),
                reason
            );
            if !tokens_str.is_empty() {
                let _ = write!(out, ", {tokens_str}");
            }
            if cost > 0.0 {
                let _ = write!(out, ", ${cost:.4}");
            }
            let _ = writeln!(out, "){}", c.reset());
            out
        })
    }
}
