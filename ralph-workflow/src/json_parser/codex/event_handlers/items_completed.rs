/// Handle `ItemCompleted` event for `agent_message` type.
pub fn handle_agent_message_completed(ctx: &EventHandlerContext, text: Option<&String>) -> String {
    let session = ctx.streaming_session.borrow();
    let is_duplicate = session.get_current_message_id().map_or_else(
        || session.has_any_streamed_content(),
        |message_id| session.is_duplicate_final_message(message_id),
    );
    let was_streaming = session.has_any_streamed_content();
    let metrics = session.get_streaming_quality_metrics();
    drop(session);

    let _was_in_block = ctx.streaming_session.borrow_mut().on_message_stop();

    if is_duplicate || was_streaming {
        let completion = TextDeltaRenderer::render_completion(ctx.terminal_mode);
        let show_metrics =
            (ctx.verbosity.is_debug() || ctx.show_streaming_metrics) && metrics.total_deltas > 0;
        if show_metrics {
            return format!("{}\n{}", completion, metrics.format(*ctx.colors));
        }
        return completion;
    }

    if let Some(text) = text {
        let limit = ctx.verbosity.truncate_limit("agent_msg");
        let preview = truncate_text(text, limit);
        return format!(
            "{}[{}]{} {}{}{}\n",
            ctx.colors.dim(),
            ctx.display_name,
            ctx.colors.reset(),
            ctx.colors.white(),
            preview,
            ctx.colors.reset()
        );
    }
    String::new()
}

/// Handle `ItemCompleted` event for `reasoning` type.
pub fn handle_reasoning_completed(ctx: &EventHandlerContext, text: Option<&String>) -> String {
    let full_reasoning = ctx
        .reasoning_accumulator
        .borrow()
        .get(ContentType::Thinking, "reasoning")
        .map(std::string::ToString::to_string);
    ctx.reasoning_accumulator
        .borrow_mut()
        .clear_key(ContentType::Thinking, "reasoning");

    // In Full mode, we already rendered thinking in-place during deltas.
    // Just emit a completion sequence if needed.
    match ctx.terminal_mode {
        TerminalMode::Full => {
            // In Full mode, thinking was already rendered in-place.
            // Emit cursor finalization (move cursor down from in-place update position).
            let session = ctx.streaming_session.borrow();
            let has_thinking = session
                .get_accumulated(ContentType::Thinking, "reasoning")
                .is_some_and(|s| !s.is_empty());
            drop(session);

            if has_thinking {
                ThinkingDeltaRenderer::render_completion(ctx.terminal_mode)
            } else {
                String::new()
            }
        }
        TerminalMode::Basic | TerminalMode::None => {
            // In non-TTY modes, we suppressed per-delta output during streaming.
            // Now flush the final accumulated thinking content once.
            //
            // Check if we have accumulated thinking from streaming deltas
            let session = ctx.streaming_session.borrow();
            let streamed_thinking = session
                .get_accumulated(ContentType::Thinking, "reasoning")
                .map(std::string::ToString::to_string);
            drop(session);

            // If we have streamed thinking, flush it with "Thinking:" label
            if let Some(thinking) = streamed_thinking {
                let sanitized = sanitize_for_display(&thinking);
                if !sanitized.is_empty() {
                    return ThinkingDeltaRenderer::render_first_delta(
                        &sanitized,
                        ctx.display_name,
                        *ctx.colors,
                        ctx.terminal_mode,
                    );
                }
            }

            // Otherwise, in Verbose mode, show summarized "Thought:" line
            // This handles the case where reasoning comes only in the completion event (no deltas)
            if ctx.verbosity.is_verbose() {
                if let Some(text) = full_reasoning.as_ref().or(text) {
                    let limit = ctx.verbosity.truncate_limit("text");
                    let preview = truncate_text(text, limit);
                    return format!(
                        "{}[{}]{} {}Thought:{} {}{}{}\n",
                        ctx.colors.dim(),
                        ctx.display_name,
                        ctx.colors.reset(),
                        ctx.colors.cyan(),
                        ctx.colors.reset(),
                        ctx.colors.dim(),
                        preview,
                        ctx.colors.reset()
                    );
                }
            }
            String::new()
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
