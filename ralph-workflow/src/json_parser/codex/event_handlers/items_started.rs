/// Handle `ItemStarted` event for `agent_message` type.
pub fn handle_agent_message_started(ctx: &EventHandlerContext, text: Option<&String>) -> String {
    if let Some(text) = text {
        let (show_prefix, accumulated_text) = {
            let mut session = ctx.streaming_session.borrow_mut();
            let show_prefix = session.on_text_delta_key("agent_msg", text);
            let accumulated_text = session
                .get_accumulated(ContentType::Text, "agent_msg")
                .unwrap_or("")
                .to_string();
            (show_prefix, accumulated_text)
        };
        if show_prefix {
            return TextDeltaRenderer::render_first_delta(
                &accumulated_text,
                ctx.display_name,
                *ctx.colors,
                ctx.terminal_mode,
            );
        }
        return TextDeltaRenderer::render_subsequent_delta(
            &accumulated_text,
            ctx.display_name,
            *ctx.colors,
            ctx.terminal_mode,
        );
    }
    if ctx.verbosity.is_verbose() {
        String::new()
    } else {
        format!(
            "{}[{}]{} {}Thinking...{}\n",
            ctx.colors.dim(),
            ctx.display_name,
            ctx.colors.reset(),
            ctx.colors.blue(),
            ctx.colors.reset()
        )
    }
}

/// Handle `ItemStarted` event for `reasoning` type.
pub fn handle_reasoning_started(ctx: &EventHandlerContext, text: Option<&String>) -> String {
    if let Some(text) = text {
        let mut acc = ctx.reasoning_accumulator.borrow_mut();
        acc.add_delta(ContentType::Thinking, "reasoning", text);
        let formatter = DeltaDisplayFormatter::new();
        return formatter.format_thinking(text, ctx.display_name, *ctx.colors);
    }
    if ctx.verbosity.is_verbose() {
        format!(
            "{}[{}]{} {}Reasoning...{}\n",
            ctx.colors.dim(),
            ctx.display_name,
            ctx.colors.reset(),
            ctx.colors.cyan(),
            ctx.colors.reset()
        )
    } else {
        String::new()
    }
}

/// Handle `ItemStarted` event for `command_execution` type.
pub fn handle_command_execution_started(
    ctx: &EventHandlerContext,
    command: Option<String>,
) -> String {
    let cmd = command.unwrap_or_default();
    let limit = ctx.verbosity.truncate_limit("command");
    let preview = truncate_text(&cmd, limit);
    format!(
        "{}[{}]{} {}Exec{}: {}{}{}\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.magenta(),
        ctx.colors.reset(),
        ctx.colors.white(),
        preview,
        ctx.colors.reset()
    )
}

/// Handle `ItemStarted` event for `file_read`/`file_write` types.
pub fn handle_file_io_started(
    ctx: &EventHandlerContext,
    path: Option<String>,
    action: &str,
) -> String {
    let path = path.unwrap_or_default();
    format!(
        "{}[{}]{} {}{}:{} {}\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.yellow(),
        action,
        ctx.colors.reset(),
        path
    )
}

/// Handle `ItemStarted` event for `mcp_tool_call`/`mcp` types.
pub fn handle_mcp_tool_started(
    ctx: &EventHandlerContext,
    tool_name: Option<&String>,
    arguments: Option<&serde_json::Value>,
) -> String {
    let default = String::from("unknown");
    let tool_name = tool_name.unwrap_or(&default);
    let mut out = format!(
        "{}[{}]{} {}MCP Tool{}: {}{}{}\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.magenta(),
        ctx.colors.reset(),
        ctx.colors.bold(),
        tool_name,
        ctx.colors.reset()
    );
    if ctx.verbosity.show_tool_input() {
        if let Some(args) = arguments {
            let args_str = format_tool_input(args);
            let limit = ctx.verbosity.truncate_limit("tool_input");
            let preview = truncate_text(&args_str, limit);
            if !preview.is_empty() {
                let _ = std::writeln!(
                    out,
                    "{}[{}]{} {}  └─ {}{}",
                    ctx.colors.dim(),
                    ctx.display_name,
                    ctx.colors.reset(),
                    ctx.colors.dim(),
                    preview,
                    ctx.colors.reset()
                );
            }
        }
    }
    out
}

/// Handle `ItemStarted` event for `web_search` type.
pub fn handle_web_search_started(ctx: &EventHandlerContext, query: Option<&String>) -> String {
    let default = String::new();
    let query = query.unwrap_or(&default);
    let limit = ctx.verbosity.truncate_limit("command");
    let preview = truncate_text(query, limit);
    format!(
        "{}[{}]{} {}Search{}: {}{}{}\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.cyan(),
        ctx.colors.reset(),
        ctx.colors.white(),
        preview,
        ctx.colors.reset()
    )
}

/// Handle `ItemStarted` event for `plan_update` type.
pub fn handle_plan_update_started(ctx: &EventHandlerContext) -> String {
    format!(
        "{}[{}]{} {}Updating plan...{}\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.blue(),
        ctx.colors.reset()
    )
}

/// Handle `ItemStarted` event for unknown/other types.
pub fn handle_unknown_item_started(
    ctx: &EventHandlerContext,
    item_type: Option<String>,
    path: Option<String>,
) -> String {
    if ctx.verbosity.is_verbose() {
        if let Some(t) = item_type {
            return format!(
                "{}[{}]{} {}{}:{} {}\n",
                ctx.colors.dim(),
                ctx.display_name,
                ctx.colors.reset(),
                ctx.colors.dim(),
                t,
                ctx.colors.reset(),
                path.unwrap_or_default()
            );
        }
    }
    String::new()
}
