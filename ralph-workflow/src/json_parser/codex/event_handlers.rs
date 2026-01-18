//! Event handlers for Codex parser.
//!
//! This module contains individual handler functions for each `CodexEvent` variant.
//! Each handler is responsible for formatting the output for its specific event type.

use crate::common::truncate_text;
use crate::config::Verbosity;
use crate::logger::{Colors, CHECK, CROSS};
use std::cell::RefCell;
use std::fmt::Write as _;
use std::rc::Rc;

use crate::json_parser::delta_display::{DeltaDisplayFormatter, DeltaRenderer, TextDeltaRenderer};
use crate::json_parser::streaming_state::StreamingSession;
use crate::json_parser::terminal::TerminalMode;
use crate::json_parser::types::{
    format_tool_input, CodexItem, CodexUsage, ContentType, DeltaAccumulator,
};

/// Context passed to event handlers containing shared state.
pub struct EventHandlerContext<'a> {
    pub colors: &'a Colors,
    pub verbosity: Verbosity,
    pub display_name: &'a str,
    pub streaming_session: &'a Rc<RefCell<StreamingSession>>,
    pub reasoning_accumulator: &'a Rc<RefCell<DeltaAccumulator>>,
    pub terminal_mode: TerminalMode,
    pub show_streaming_metrics: bool,
}

/// Handle `ItemStarted` event by delegating to type-specific handlers.
///
/// Returns `Some(output)` for events that should bypass the empty check (like streaming),
/// or `None` for events that should go through the normal empty check.
pub fn handle_item_started(ctx: &EventHandlerContext, item: Option<&CodexItem>) -> Option<String> {
    item.and_then(|item| match item.item_type.as_deref() {
        Some("command_execution") => {
            let output = handle_command_execution_started(ctx, item.command.clone());
            (!output.is_empty()).then_some(output)
        }
        Some("agent_message") => Some(handle_agent_message_started(ctx, item.text.as_ref())),
        Some("reasoning") => Some(handle_reasoning_started(ctx, item.text.as_ref())),
        Some("file_read" | "file_write") => {
            let output =
                handle_file_io_started(ctx, item.path.clone(), item.item_type.as_deref().unwrap());
            (!output.is_empty()).then_some(output)
        }
        Some("mcp_tool_call" | "mcp") => {
            let output = handle_mcp_tool_started(ctx, item.tool.as_ref(), item.arguments.as_ref());
            (!output.is_empty()).then_some(output)
        }
        Some("web_search") => {
            let output = handle_web_search_started(ctx, item.query.as_ref());
            (!output.is_empty()).then_some(output)
        }
        Some("plan_update") => {
            let output = handle_plan_update_started(ctx);
            (!output.is_empty()).then_some(output)
        }
        Some(t) => {
            let output = handle_unknown_item_started(ctx, Some(t.to_string()), item.path.clone());
            (!output.is_empty()).then_some(output)
        }
        None => None,
    })
}

/// Handle `ItemCompleted` event by delegating to type-specific handlers.
///
/// Returns `Some(output)` for events that should bypass the empty check (like streaming),
/// or `None` for events that should go through the normal empty check.
pub fn handle_item_completed(
    ctx: &EventHandlerContext,
    item: Option<&CodexItem>,
) -> Option<String> {
    item.and_then(|item| match item.item_type.as_deref() {
        Some("agent_message") => Some(handle_agent_message_completed(ctx, item.text.as_ref())),
        Some("reasoning") => Some(handle_reasoning_completed(ctx, item.text.as_ref())),
        Some("command_execution") => {
            let output = handle_command_execution_completed(ctx);
            (!output.is_empty()).then_some(output)
        }
        Some("file_change" | "file_write") => {
            let output = handle_file_write_completed(ctx, item.path.clone());
            (!output.is_empty()).then_some(output)
        }
        Some("file_read") => {
            let output = handle_file_read_completed(ctx, item.path.clone());
            (!output.is_empty()).then_some(output)
        }
        Some("mcp_tool_call" | "mcp") => {
            let output = handle_mcp_tool_completed(ctx, item.tool.clone());
            (!output.is_empty()).then_some(output)
        }
        Some("web_search") => {
            let output = handle_web_search_completed(ctx);
            (!output.is_empty()).then_some(output)
        }
        Some("plan_update") => {
            let output = handle_plan_update_completed(ctx, item.plan.as_ref());
            (!output.is_empty()).then_some(output)
        }
        _ => None,
    })
}

/// Handle `ThreadStarted` event.
pub fn handle_thread_started(ctx: &EventHandlerContext, thread_id: Option<String>) -> String {
    let tid = thread_id.unwrap_or_else(|| "unknown".to_string());
    ctx.streaming_session
        .borrow_mut()
        .set_current_message_id(Some(tid.clone()));
    format!(
        "{}[{}]{} {}Thread started{} {}({:.8}...){}\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.cyan(),
        ctx.colors.reset(),
        ctx.colors.dim(),
        tid,
        ctx.colors.reset()
    )
}

/// Handle `TurnStarted` event.
pub fn handle_turn_started(ctx: &EventHandlerContext, turn_id: String) -> String {
    ctx.streaming_session.borrow_mut().on_message_start();
    ctx.reasoning_accumulator.borrow_mut().clear();
    ctx.streaming_session
        .borrow_mut()
        .set_current_message_id(Some(turn_id));
    format!(
        "{}[{}]{} {}Turn started{}\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.blue(),
        ctx.colors.reset()
    )
}

/// Handle `TurnCompleted` event.
pub fn handle_turn_completed(ctx: &EventHandlerContext, usage: Option<CodexUsage>) -> String {
    let was_in_block = ctx.streaming_session.borrow_mut().on_message_stop();
    let (input, output) = usage.map_or((0, 0), |u| {
        (u.input_tokens.unwrap_or(0), u.output_tokens.unwrap_or(0))
    });
    let completion = if was_in_block {
        TextDeltaRenderer::render_completion(ctx.terminal_mode)
    } else {
        String::new()
    };
    format!(
        "{}{}[{}]{} {}{} Turn completed{} {}(in:{} out:{}){}\n",
        completion,
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.green(),
        CHECK,
        ctx.colors.reset(),
        ctx.colors.dim(),
        input,
        output,
        ctx.colors.reset()
    )
}

/// Handle `TurnFailed` event.
pub fn handle_turn_failed(ctx: &EventHandlerContext, error: Option<String>) -> String {
    let was_in_block = ctx.streaming_session.borrow_mut().on_message_stop();
    let completion = if was_in_block {
        TextDeltaRenderer::render_completion(ctx.terminal_mode)
    } else {
        String::new()
    };
    let err = error.unwrap_or_else(|| "unknown error".to_string());
    format!(
        "{}{}[{}]{} {}{} Turn failed:{} {}\n",
        completion,
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.red(),
        CROSS,
        ctx.colors.reset(),
        err
    )
}

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

/// Handle `Error` event.
pub fn handle_error(
    ctx: &EventHandlerContext,
    message: Option<String>,
    error: Option<String>,
) -> String {
    let err = message
        .or(error)
        .unwrap_or_else(|| "unknown error".to_string());
    format!(
        "{}[{}]{} {}{} Error:{} {}\n",
        ctx.colors.dim(),
        ctx.display_name,
        ctx.colors.reset(),
        ctx.colors.red(),
        CROSS,
        ctx.colors.reset(),
        err
    )
}

/// Handle `Result` event for console display (debug mode only).
///
/// This function returns formatted output for Result events in debug mode.
/// Result events are always written to the log file via `process_event_line`,
/// but this function provides debug visibility into what's being logged.
pub fn handle_result_for_display(ctx: &EventHandlerContext, result: Option<String>) -> Option<String> {
    if let Some(content) = result {
        let limit = ctx.verbosity.truncate_limit("result");
        let preview = truncate_text(&content, limit);
        Some(format!(
            "{}[{}]{} {}Result:{} {}{}{}\n",
            ctx.colors.dim(),
            ctx.display_name,
            ctx.colors.reset(),
            ctx.colors.green(),
            ctx.colors.reset(),
            ctx.colors.dim(),
            preview,
            ctx.colors.reset()
        ))
    } else {
        None
    }
}

