// Codex `item.started` event handlers.
//
// This file implements delta handlers for Codex streaming items.
//
// # CCS Spam Prevention Architecture (Layers 1 & 2: Suppress & Accumulate)
//
// These handlers work with the delta renderers to implement the first two layers
// of the spam prevention architecture for Codex agents (ccs/codex):
//
// ## Architecture Overview
//
// 1. **Layer 1 (Suppression):** Delta renderers return empty strings in non-TTY modes
//    - Called by these handlers via `TextDeltaRenderer::render_*` and `ThinkingDeltaRenderer::render_*`
//    - Implemented in `ralph-workflow/src/json_parser/delta_display/renderer.rs`
//
// 2. **Layer 2 (Accumulation):** StreamingSession accumulates content across deltas
//    - These handlers call `session.on_text_delta_key()` and `session.on_thinking_delta_key()`
//    - Content is preserved for flush at completion
//
// 3. **Layer 3 (Flush):** Completion handlers flush accumulated content ONCE
//    - Implemented in `ralph-workflow/src/json_parser/codex/event_handlers/items_completed.rs`
//
// ## Delta Rendering Strategy
//
// ### Full Mode (TTY)
// - Each delta calls renderer which returns a line with cursor positioning
// - Visual effect: one line updating in-place
// - `render_first_delta`: prefix + content + `\n\x1b[1A` (cursor up)
// - `render_subsequent_delta`: `\x1b[2K\r` (clear line) + prefix + content + `\n\x1b[1A`
//
// ### Basic/None Modes (non-TTY)
// - Each delta calls renderer which returns empty string (suppression at Layer 1)
// - Content is accumulated in StreamingSession (Layer 2)
// - No output until completion boundary (Layer 3)
//
// ## Validation
//
// See comprehensive regression tests:
// - `tests/integration_tests/ccs_comprehensive_spam_verification.rs` - Architecture verification
// - `tests/integration_tests/ccs_nuclear_spam_test.rs` - 500+ deltas with hard assertions
// - `tests/integration_tests/ccs_all_delta_types_spam_reproduction.rs` - 1000+ deltas
// - `tests/integration_tests/codex_reasoning_spam_regression.rs` - Original Codex fix
//
// ## Cross-References
//
// - Renderer suppression: `ralph-workflow/src/json_parser/delta_display/renderer.rs`
// - Completion flush: `ralph-workflow/src/json_parser/codex/event_handlers/items_completed.rs`
// - Claude equivalent: `ralph-workflow/src/json_parser/claude/delta_handling.rs`

/// Handle `ItemStarted` event for `agent_message` type.
///
/// This handler implements Layers 1 & 2 (Suppress & Accumulate) of the spam
/// prevention architecture for Codex `agent_message` items. In non-TTY modes,
/// the renderer returns empty strings while StreamingSession accumulates content.
///
/// # CCS Spam Prevention Architecture
///
/// 1. **Layer 1 (Suppression):** Renderer returns empty strings in non-TTY modes
/// 2. **Layer 2 (Accumulation):** StreamingSession preserves content across deltas
/// 3. **Layer 3 (Flush):** Completion handler emits accumulated content once
///
/// See file-level documentation for details.
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
///
/// # Reasoning Output Strategy (Bug Fix: Codex Thinking Spam)
///
/// This handler prevents reasoning spam in logs by aligning with Claude's approach:
///
/// ## Non-TTY Modes (Basic/None)
/// - Per-delta reasoning output is **suppressed** (returns `String::new()`)
/// - Accumulated reasoning is flushed **once** at `item.completed` boundary
/// - This prevents dozens of repeated "[ccs/codex] Thinking:" lines in logs
///
/// ## Full TTY Mode
/// - Uses `ThinkingDeltaRenderer` for in-place updates with cursor positioning
/// - First delta: shows prefix + content + cursor up (`\n\x1b[1A`)
/// - Subsequent deltas: clear line + rewrite + cursor up (`\x1b[2K\r...\n\x1b[1A`)
/// - Completion: cursor down + newline (`\x1b[1B\n`)
///
/// ## State Tracking
/// - Uses `StreamingSession::on_thinking_delta_key("reasoning", ...)` to detect first vs subsequent chunks
/// - Accumulates in `reasoning_accumulator` for backward compatibility with completion handler
///
/// ## Regression Test
/// See `tests/integration_tests/codex_reasoning_spam_regression.rs` for verification.
pub fn handle_reasoning_started(ctx: &EventHandlerContext, text: Option<&String>) -> String {
    if let Some(text) = text {
        // Use StreamingSession to track first vs subsequent chunks
        let show_prefix = {
            let mut session = ctx.streaming_session.borrow_mut();
            session.on_thinking_delta_key("reasoning", text)
        };

        // Accumulate for backward compatibility with reasoning_completed
        let mut acc = ctx.reasoning_accumulator.borrow_mut();
        acc.add_delta(ContentType::Thinking, "reasoning", text);
        drop(acc);

        // In non-TTY modes, suppress per-delta reasoning output and flush once
        // at the next output boundary (item.completed or message boundary).
        match ctx.terminal_mode {
            TerminalMode::Full => {
                let session = ctx.streaming_session.borrow();
                let accumulated = session
                    .get_accumulated(ContentType::Thinking, "reasoning")
                    .unwrap_or("");

                if show_prefix {
                    ThinkingDeltaRenderer::render_first_delta(
                        accumulated,
                        ctx.display_name,
                        *ctx.colors,
                        ctx.terminal_mode,
                    )
                } else {
                    ThinkingDeltaRenderer::render_subsequent_delta(
                        accumulated,
                        ctx.display_name,
                        *ctx.colors,
                        ctx.terminal_mode,
                    )
                }
            }
            TerminalMode::Basic | TerminalMode::None => {
                // Suppress per-delta output in non-TTY modes
                // Will be flushed at reasoning completion boundary
                String::new()
            }
        }
    } else if ctx.verbosity.is_verbose() {
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

    let mut out = match ctx.terminal_mode {
        TerminalMode::Full | TerminalMode::Basic => format!(
            "{}[{}]{} {}MCP Tool{}: {}{}{}\n",
            ctx.colors.dim(),
            ctx.display_name,
            ctx.colors.reset(),
            ctx.colors.magenta(),
            ctx.colors.reset(),
            ctx.colors.bold(),
            tool_name,
            ctx.colors.reset()
        ),
        TerminalMode::None => format!("[{}] MCP Tool: {}\n", ctx.display_name, tool_name),
    };

    if ctx.verbosity.show_tool_input() {
        if let Some(args) = arguments {
            let args_str = format_tool_input(args);
            let limit = ctx.verbosity.truncate_limit("tool_input");
            let preview = truncate_text(&args_str, limit);
            if !preview.is_empty() {
                // This is a one-shot preview at item start, not streaming per-delta output.
                // Always render it, including in Basic/None modes, so non-TTY logs remain
                // observable.
                let tool_input_line = match ctx.terminal_mode {
                    TerminalMode::Full | TerminalMode::Basic => format!(
                        "{}[{}]{} {}  └─ {}{}{}\n",
                        ctx.colors.dim(),
                        ctx.display_name,
                        ctx.colors.reset(),
                        ctx.colors.dim(),
                        ctx.colors.reset(),
                        preview,
                        ctx.colors.reset()
                    ),
                    TerminalMode::None => format!("[{}]   └─ {}\n", ctx.display_name, preview),
                };
                out.push_str(&tool_input_line);
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
