// OpenCode event formatting methods.
//
// Contains all the format_*_event methods for the OpenCodeParser.

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

            // Add final newline if we were streaming text
            let terminal_mode = *self.terminal_mode.borrow();
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

            let mut out = format!(
                "{}{}[{}]{} {}{} Step finished{} {}({}",
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

    /// Format a `tool_use` event
    ///
    /// Based on OpenCode source (`run.ts` lines 163-174, `message-v2.ts` lines 221-287):
    /// - Shows tool name with status-specific icon and color
    /// - Status handling: pending (…), running (►), completed (✓), error (✗)
    /// - Title/description when available (from `state.title`)
    /// - Tool-specific input formatting based on tool type
    /// - Tool output/results shown at Normal+ verbosity
    /// - Error messages shown in red when status is "error"
    pub(super) fn format_tool_use_event(&self, event: &OpenCodeEvent) -> String {
        let c = &self.colors;
        let prefix = &self.display_name;

        event.part.as_ref().map_or_else(String::new, |part| {
            let tool_name = part.tool.as_deref().unwrap_or("unknown");
            let status = part
                .state
                .as_ref()
                .and_then(|s| s.status.as_deref())
                .unwrap_or("pending");
            let title = part.state.as_ref().and_then(|s| s.title.as_deref());

            // Status-specific icon and color based on ToolState variants from message-v2.ts
            // Statuses: "pending", "running", "completed", "error"
            let (icon, color) = match status {
                "completed" => (CHECK, c.green()),
                "error" => (CROSS, c.red()),
                "running" => ('►', c.cyan()),
                _ => ('…', c.yellow()), // "pending" or unknown
            };

            let mut out = format!(
                "{}[{}]{} {}Tool{}: {}{}{} {}{}{}\n",
                c.dim(),
                prefix,
                c.reset(),
                c.magenta(),
                c.reset(),
                c.bold(),
                tool_name,
                c.reset(),
                color,
                icon,
                c.reset()
            );

            // Show title if available (from state.title)
            if let Some(t) = title {
                let limit = self.verbosity.truncate_limit("text");
                let preview = truncate_text(t, limit);
                let _ = writeln!(
                    out,
                    "{}[{}]{} {}  └─ {}{}",
                    c.dim(),
                    prefix,
                    c.reset(),
                    c.dim(),
                    preview,
                    c.reset()
                );
            }

            // Show tool input at Normal+ verbosity with tool-specific formatting
            if self.verbosity.show_tool_input() {
                if let Some(ref state) = part.state {
                    if let Some(ref input_val) = state.input {
                        let input_str = Self::format_tool_specific_input(tool_name, input_val);
                        let limit = self.verbosity.truncate_limit("tool_input");
                        let preview = truncate_text(&input_str, limit);
                        if !preview.is_empty() {
                            let _ = writeln!(
                                out,
                                "{}[{}]{} {}  └─ {}{}",
                                c.dim(),
                                prefix,
                                c.reset(),
                                c.dim(),
                                preview,
                                c.reset()
                            );
                        }
                    }
                }
            }

            // Show error message when status is "error"
            if status == "error" {
                if let Some(ref state) = part.state {
                    if let Some(ref error_msg) = state.error {
                        let limit = self.verbosity.truncate_limit("tool_result");
                        let preview = truncate_text(error_msg, limit);
                        let _ = writeln!(
                            out,
                            "{}[{}]{} {}  └─ {}Error:{} {}{}{}",
                            c.dim(),
                            prefix,
                            c.reset(),
                            c.red(),
                            c.bold(),
                            c.reset(),
                            c.red(),
                            preview,
                            c.reset()
                        );
                    }
                }
            }

            // Show tool output at Normal+ verbosity when completed
            // (Changed from verbose-only to match OpenCode's interactive mode behavior)
            if self.verbosity.show_tool_input() && status == "completed" {
                if let Some(ref state) = part.state {
                    if let Some(ref output_val) = state.output {
                        let output_str = match output_val {
                            serde_json::Value::String(s) => s.clone(),
                            other => other.to_string(),
                        };
                        if !output_str.is_empty() {
                            let limit = self.verbosity.truncate_limit("tool_result");
                            // Format multi-line output with proper indentation
                            self.format_tool_output(&mut out, &output_str, limit, prefix, *c);
                        }
                    }
                }
            }
            out
        })
    }

    /// Format tool output with proper multi-line handling
    ///
    /// For single-line outputs, shows inline. For multi-line outputs (like file contents),
    /// shows only the first few lines as a preview.
    pub(super) fn format_tool_output(
        &self,
        out: &mut String,
        output: &str,
        limit: usize,
        prefix: &str,
        c: crate::logger::Colors,
    ) {
        use crate::config::truncation::MAX_OUTPUT_LINES;

        let lines: Vec<&str> = output.lines().collect();
        let is_multiline = lines.len() > 1;

        if is_multiline {
            // Multi-line output: show header then first few lines
            let _ = writeln!(
                out,
                "{}[{}]{} {}  └─ Output:{}",
                c.dim(),
                prefix,
                c.reset(),
                c.cyan(),
                c.reset()
            );

            let mut chars_used = 0;
            let indent = format!("{}[{}]{}     ", c.dim(), prefix, c.reset());

            for (lines_shown, line) in lines.iter().enumerate() {
                // Stop if we've shown enough lines OR exceeded char limit
                if lines_shown >= MAX_OUTPUT_LINES || chars_used + line.len() > limit {
                    let remaining = lines.len() - lines_shown;
                    if remaining > 0 {
                        let _ = writeln!(out, "{}{}...({} more lines)", indent, c.dim(), remaining);
                    }
                    break;
                }
                let _ = writeln!(out, "{}{}{}{}", indent, c.dim(), line, c.reset());
                chars_used += line.len() + 1;
            }
        } else {
            // Single-line output: show inline
            let preview = truncate_text(output, limit);
            if !preview.is_empty() {
                let _ = writeln!(
                    out,
                    "{}[{}]{} {}  └─ Output:{} {}",
                    c.dim(),
                    prefix,
                    c.reset(),
                    c.cyan(),
                    c.reset(),
                    preview
                );
            }
        }
    }

    /// Format tool input based on tool type
    ///
    /// From OpenCode source, each tool has specific input fields:
    /// - `read`: `filePath`, `offset?`, `limit?`
    /// - `bash`: `command`, `timeout?`
    /// - `write`: `filePath`, `content`
    /// - `edit`: `filePath`, ...
    /// - `glob`: `pattern`, `path?`
    /// - `grep`: `pattern`, `path?`, `include?`
    /// - `fetch`: `url`, `format?`, `timeout?`
    pub(super) fn format_tool_specific_input(
        tool_name: &str,
        input: &serde_json::Value,
    ) -> String {
        let obj = match input.as_object() {
            Some(o) => o,
            None => return format_tool_input(input),
        };

        match tool_name {
            "read" | "view" => {
                // Primary: filePath, optional: offset, limit
                let file_path = obj.get("filePath").and_then(|v| v.as_str()).unwrap_or("");
                let mut result = file_path.to_string();
                if let Some(offset) = obj.get("offset").and_then(|v| v.as_u64()) {
                    result.push_str(&format!(" (offset: {offset})"));
                }
                if let Some(limit) = obj.get("limit").and_then(|v| v.as_u64()) {
                    result.push_str(&format!(" (limit: {limit})"));
                }
                result
            }
            "bash" => {
                // Primary: command
                obj.get("command")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            }
            "write" => {
                // Primary: filePath (don't show content in summary)
                let file_path = obj.get("filePath").and_then(|v| v.as_str()).unwrap_or("");
                let content_len = obj
                    .get("content")
                    .and_then(|v| v.as_str())
                    .map(|s| s.len())
                    .unwrap_or(0);
                if content_len > 0 {
                    format!("{file_path} ({content_len} bytes)")
                } else {
                    file_path.to_string()
                }
            }
            "edit" => {
                // Primary: filePath
                obj.get("filePath")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string()
            }
            "glob" => {
                // Primary: pattern, optional: path
                let pattern = obj.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                let path = obj.get("path").and_then(|v| v.as_str());
                if let Some(p) = path {
                    format!("{pattern} in {p}")
                } else {
                    pattern.to_string()
                }
            }
            "grep" => {
                // Primary: pattern, optional: path, include
                let pattern = obj.get("pattern").and_then(|v| v.as_str()).unwrap_or("");
                let mut result = format!("/{pattern}/");
                if let Some(path) = obj.get("path").and_then(|v| v.as_str()) {
                    result.push_str(&format!(" in {path}"));
                }
                if let Some(include) = obj.get("include").and_then(|v| v.as_str()) {
                    result.push_str(&format!(" ({include})"));
                }
                result
            }
            "fetch" | "webfetch" => {
                // Primary: url, optional: format
                let url = obj.get("url").and_then(|v| v.as_str()).unwrap_or("");
                let format = obj.get("format").and_then(|v| v.as_str());
                if let Some(f) = format {
                    format!("{url} ({f})")
                } else {
                    url.to_string()
                }
            }
            "todowrite" | "todoread" => {
                // Show count of todos if available
                if let Some(todos) = obj.get("todos").and_then(|v| v.as_array()) {
                    format!("{} items", todos.len())
                } else {
                    format_tool_input(input)
                }
            }
            _ => {
                // Fallback to generic formatting
                format_tool_input(input)
            }
        }
    }

    /// Format a `text` event
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
