/// Handle `ItemCompleted` event for `agent_message` type.
pub fn handle_agent_message_completed(ctx: &EventHandlerContext, text: Option<&String>) -> String {
    let (is_duplicate, was_streaming, metrics, streamed_agent_msg) = {
        let session = ctx.streaming_session.borrow();
        let is_duplicate = session
            .get_current_message_id()
            .is_some_and(|message_id| session.is_duplicate_final_message(message_id));
        let was_streaming = session.has_any_streamed_content();
        let metrics = session.get_streaming_quality_metrics();
        let streamed_agent_msg = session
            .get_accumulated(ContentType::Text, "agent_msg")
            .map(std::string::ToString::to_string);
        (is_duplicate, was_streaming, metrics, streamed_agent_msg)
    };

    let _was_in_block = ctx.streaming_session.borrow_mut().on_message_stop();

    // Duplicate completion events must be suppressed even if we streamed content.
    // Codex can emit duplicate `item.completed` events for the same message; if we
    // flush before checking duplication, we can print the final message twice.
    if is_duplicate {
        // Still finalize any cursor state (Full) and optionally emit metrics.
        // In Basic/None, do not emit an extra newline for suppressed duplicates.
        let completion = match ctx.terminal_mode {
            TerminalMode::Full => TextDeltaRenderer::render_completion(ctx.terminal_mode),
            TerminalMode::Basic | TerminalMode::None => String::new(),
        };
        let show_metrics =
            (ctx.verbosity.is_debug() || ctx.show_streaming_metrics) && metrics.total_deltas > 0;
        if show_metrics {
            return format!("{}\n{}", completion, metrics.format(*ctx.colors));
        }
        return completion;
    }

    // If we streamed any content, the per-delta renderer may have suppressed output in non-TTY
    // modes. Flush the final accumulated agent message ONCE at completion so logs remain
    // observable, while still preventing per-delta prefix spam.
    if was_streaming {
        // In Basic/None we already flush newline-terminated output below, so avoid appending an
        // additional completion newline (which would create a blank line in non-TTY logs).
        let completion = match ctx.terminal_mode {
            TerminalMode::Full => TextDeltaRenderer::render_completion(ctx.terminal_mode),
            TerminalMode::Basic | TerminalMode::None => String::new(),
        };
        let show_metrics =
            (ctx.verbosity.is_debug() || ctx.show_streaming_metrics) && metrics.total_deltas > 0;

        let flush = match ctx.terminal_mode {
            TerminalMode::Full => String::new(),
            TerminalMode::Basic | TerminalMode::None => streamed_agent_msg.map_or_else(
                String::new,
                |msg| {
                    let limit = ctx.verbosity.truncate_limit("agent_msg");
                    let preview = truncate_text(&msg, limit);
                    if preview.is_empty() {
                        String::new()
                    } else {
                        // TerminalMode::None must be plain text even when colors are enabled.
                        match ctx.terminal_mode {
                            TerminalMode::Basic => format!(
                                "{}[{}]{} {}{}{}\n",
                                ctx.colors.dim(),
                                ctx.display_name,
                                ctx.colors.reset(),
                                ctx.colors.white(),
                                preview,
                                ctx.colors.reset()
                            ),
                            TerminalMode::None => {
                                format!("[{}] {}\n", ctx.display_name, preview)
                            }
                            TerminalMode::Full => unreachable!(),
                        }
                    }
                },
            ),
        };

        // Clear the streaming key after first completion so duplicates have nothing to flush.
        ctx.streaming_session
            .borrow_mut()
            .clear_key(ContentType::Text, "agent_msg");

        let mut out = String::new();
        out.push_str(&flush);
        out.push_str(&completion);
        if show_metrics {
            out.push('\n');
            out.push_str(&metrics.format(*ctx.colors));
        }
        return out;
    }

    if let Some(text) = text {
        let limit = ctx.verbosity.truncate_limit("agent_msg");
        let preview = truncate_text(text, limit);
        return match ctx.terminal_mode {
            TerminalMode::Full | TerminalMode::Basic => format!(
                "{}[{}]{} {}{}{}\n",
                ctx.colors.dim(),
                ctx.display_name,
                ctx.colors.reset(),
                ctx.colors.white(),
                preview,
                ctx.colors.reset()
            ),
            TerminalMode::None => format!("[{}] {}\n", ctx.display_name, preview),
        };
    }

    String::new()
}

/// Handle `ItemCompleted` event for `reasoning` type.
///
/// # Reasoning Completion Strategy (Bug Fix: Codex Thinking Spam)
///
/// This handler completes the reasoning spam fix by flushing accumulated content:
///
/// ## Full TTY Mode
/// - Reasoning was already rendered in-place during deltas
/// - Emit cursor finalization sequence (`\x1b[1B\n`) to move cursor down
///
/// ## Non-TTY Modes (Basic/None)
/// - Per-delta output was suppressed during streaming
/// - Now flush the final accumulated thinking content **once** with "Thinking:" label
/// - If no streamed thinking exists, render the completion text once (non-spam),
///   and always clear the streaming key to avoid cross-item contamination.
///
/// ## Regression Test
/// See `tests/integration_tests/codex_reasoning_spam_regression.rs` for verification.
pub fn handle_reasoning_completed(ctx: &EventHandlerContext, text: Option<&String>) -> String {
    let full_reasoning = ctx
        .reasoning_accumulator
        .borrow()
        .get(ContentType::Thinking, "reasoning")
        .map(std::string::ToString::to_string);
    ctx.reasoning_accumulator
        .borrow_mut()
        .clear_key(ContentType::Thinking, "reasoning");

    let completion_text = full_reasoning
        .as_deref()
        .or(text.map(std::string::String::as_str));

    match ctx.terminal_mode {
        TerminalMode::Full => {
            // In Full mode, most reasoning arrives via deltas rendered in-place.
            // If Codex provides reasoning only at completion, render it once here.
            let streamed_thinking = {
                let session = ctx.streaming_session.borrow();
                session
                    .get_accumulated(ContentType::Thinking, "reasoning")
                    .map(std::string::ToString::to_string)
            };

            let result = if let Some(thinking) = streamed_thinking {
                if !thinking.is_empty() {
                    ThinkingDeltaRenderer::render_completion(ctx.terminal_mode)
                } else {
                    String::new()
                }
            } else if let Some(text) = completion_text {
                let sanitized = sanitize_for_display(text);
                if sanitized.is_empty() {
                    String::new()
                } else {
                    let rendered = ThinkingDeltaRenderer::render_first_delta(
                        &sanitized,
                        ctx.display_name,
                        *ctx.colors,
                        ctx.terminal_mode,
                    );
                    let completion = ThinkingDeltaRenderer::render_completion(ctx.terminal_mode);
                    format!("{rendered}{completion}")
                }
            } else {
                String::new()
            };

            ctx.streaming_session
                .borrow_mut()
                .clear_key(ContentType::Thinking, "reasoning");
            result
        }
        TerminalMode::Basic | TerminalMode::None => {
            // In non-TTY modes, suppress per-delta output and flush once at completion.
            //
            // If we received streamed reasoning deltas, flush the accumulated thinking once.
            // If reasoning arrives only at completion (no deltas), preserve the existing
            // verbose-mode "Thought:" summary behavior.
            let streamed_thinking = {
                let session = ctx.streaming_session.borrow();
                session
                    .get_accumulated(ContentType::Thinking, "reasoning")
                    .map(std::string::ToString::to_string)
            };

            // Format the output directly because the renderers now suppress
            // output in non-TTY modes (to prevent per-delta spam).
            let rendered = if let Some(thinking) = streamed_thinking {
                let sanitized = sanitize_for_display(&thinking);
                if sanitized.is_empty() {
                    String::new()
                } else {
                    // TerminalMode::None must be plain text even when colors are enabled.
                    match ctx.terminal_mode {
                        TerminalMode::Basic => format!(
                            "{}[{}]{} {}Thinking: {}{}{}\n",
                            ctx.colors.dim(),
                            ctx.display_name,
                            ctx.colors.reset(),
                            ctx.colors.dim(),
                            ctx.colors.cyan(),
                            sanitized,
                            ctx.colors.reset()
                        ),
                        TerminalMode::None => {
                            format!("[{}] Thinking: {}\n", ctx.display_name, sanitized)
                        }
                        TerminalMode::Full => unreachable!(),
                    }
                }
            } else if let Some(text) = completion_text {
                if ctx.verbosity.is_verbose() {
                    let limit = ctx.verbosity.truncate_limit("text");
                    let preview = truncate_text(text, limit);
                    match ctx.terminal_mode {
                        TerminalMode::Basic => format!(
                            "{}[{}]{} {}Thought:{} {}{}{}\n",
                            ctx.colors.dim(),
                            ctx.display_name,
                            ctx.colors.reset(),
                            ctx.colors.cyan(),
                            ctx.colors.reset(),
                            ctx.colors.dim(),
                            preview,
                            ctx.colors.reset()
                        ),
                        TerminalMode::None => {
                            format!("[{}] Thought: {}\n", ctx.display_name, preview)
                        }
                        TerminalMode::Full => unreachable!(),
                    }
                } else {
                    let sanitized = sanitize_for_display(text);
                    if sanitized.is_empty() {
                        String::new()
                    } else {
                        // TerminalMode::None must be plain text even when colors are enabled.
                        match ctx.terminal_mode {
                            TerminalMode::Basic => format!(
                                "{}[{}]{} {}Thinking: {}{}{}\n",
                                ctx.colors.dim(),
                                ctx.display_name,
                                ctx.colors.reset(),
                                ctx.colors.dim(),
                                ctx.colors.cyan(),
                                sanitized,
                                ctx.colors.reset()
                            ),
                            TerminalMode::None => {
                                format!("[{}] Thinking: {}\n", ctx.display_name, sanitized)
                            }
                            TerminalMode::Full => unreachable!(),
                        }
                    }
                }
            } else {
                String::new()
            };

            // Always clear key-scoped streaming state, even if the rendered output is empty.
            ctx.streaming_session
                .borrow_mut()
                .clear_key(ContentType::Thinking, "reasoning");

            rendered
        }
    }
}

/// Handle `ItemCompleted` event for `command_execution` type.
pub fn handle_command_execution_completed(ctx: &EventHandlerContext) -> String {
    format!(
        "{}[{}]{} {}{} Command done{}\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.green(),
        CHECK,
        ctx.colors.reset()
    )
}

/// Handle `ItemCompleted` event for `file_change`/`file_write` types.
pub fn handle_file_write_completed(ctx: &EventHandlerContext, path: Option<String>) -> String {
    let path = path.unwrap_or_else(|| "unknown".to_string());
    format!(
        "{}[{}]{} {}File{}: {}\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.yellow(),
        ctx.colors.reset(),
        path
    )
}

/// Handle `ItemCompleted` event for `file_read` type.
pub fn handle_file_read_completed(ctx: &EventHandlerContext, path: Option<String>) -> String {
    if ctx.verbosity.is_verbose() {
        let path = path.unwrap_or_else(|| "unknown".to_string());
        format!(
            "{}[{}]{} {}{} Read:{} {}\n",
            ctx.colors.dim(),
            ctx.display_name,
            ctx.colors.reset(),
            ctx.colors.green(),
            CHECK,
            ctx.colors.reset(),
            path
        )
    } else {
        String::new()
    }
}

/// Handle `ItemCompleted` event for `mcp_tool_call`/`mcp` types.
pub fn handle_mcp_tool_completed(ctx: &EventHandlerContext, tool_name: Option<String>) -> String {
    let tool_name = tool_name.unwrap_or_else(|| "tool".to_string());
    format!(
        "{}[{}]{} {}{} MCP:{} {} done\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.green(),
        CHECK,
        ctx.colors.reset(),
        tool_name
    )
}

/// Handle `ItemCompleted` event for `web_search` type.
pub fn handle_web_search_completed(ctx: &EventHandlerContext) -> String {
    format!(
        "{}[{}]{} {}{} Search completed{}\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.green(),
        CHECK,
        ctx.colors.reset()
    )
}

/// Handle `ItemCompleted` event for `plan_update` type.
pub fn handle_plan_update_completed(ctx: &EventHandlerContext, plan: Option<&String>) -> String {
    if ctx.verbosity.is_verbose() {
        let limit = ctx.verbosity.truncate_limit("text");
        plan.map_or_else(
            || {
                format!(
                    "{}[{}]{} {}{} Plan updated{}\n",
                    ctx.colors.dim(),
                    ctx.display_name,
                    ctx.colors.reset(),
                    ctx.colors.green(),
                    CHECK,
                    ctx.colors.reset()
                )
            },
            |plan| {
                let preview = truncate_text(plan, limit);
                format!(
                    "{}[{}]{} {}Plan:{} {}\n",
                    ctx.colors.dim(),
                    ctx.display_name,
                    ctx.colors.reset(),
                    ctx.colors.blue(),
                    ctx.colors.reset(),
                    preview
                )
            },
        )
    } else {
        String::new()
    }
}
