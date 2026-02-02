use super::types::{PipelineRuntime, PromptCommand};
use crate::agents::JsonParserType;
use crate::common::split_command;
use crate::logger::argv_requests_json;
use crate::rendering::json_pretty::format_generic_json_for_display;

use std::io::{self, BufRead, Write};
use std::path::Path;

use crate::pipeline::idle_timeout::{ActivityTrackingReader, SharedActivityTimestamp};

use super::streaming_line_reader::StreamingLineReader;

/// Extract session_id from a log file.
pub(super) fn extract_session_id_from_logfile(
    logfile: &str,
    workspace: &dyn crate::workspace::Workspace,
) -> Option<String> {
    let logfile_path = Path::new(logfile);
    let content = workspace.read(logfile_path).ok()?;

    // Look for session_id in the first few lines (init events come first)
    for line in content.lines().take(10) {
        if let Some(session_id) = extract_session_id_from_json_line(line) {
            return Some(session_id);
        }
    }
    None
}

/// Extract session_id from a single JSON line.
///
/// Supports multiple agent formats:
/// - Claude: `{"type":"system","subtype":"init","session_id":"abc123"}`
/// - Gemini: `{"type":"init","session_id":"abc123","model":"gemini-pro"}`
/// - OpenCode: `{"event_type":"...", "session_id":"abc123"}`
fn extract_session_id_from_json_line(line: &str) -> Option<String> {
    // Try to parse as JSON
    let value: serde_json::Value = serde_json::from_str(line).ok()?;

    // Check for session_id field (common across formats)
    if let Some(session_id) = value.get("session_id").and_then(|v| v.as_str()) {
        return Some(session_id.to_string());
    }

    // Check for sessionID field (some agents use camelCase)
    if let Some(session_id) = value.get("sessionID").and_then(|v| v.as_str()) {
        return Some(session_id.to_string());
    }

    None
}

/// Stream agent output from an AgentChildHandle.
///
/// This function streams the agent's stdout in real-time, parsing JSON
/// output based on the parser type, and tracking activity for idle timeout detection.
pub(super) fn stream_agent_output_from_handle(
    stdout: Box<dyn io::Read + Send>,
    cmd: &PromptCommand<'_>,
    runtime: &PipelineRuntime<'_>,
    activity_timestamp: SharedActivityTimestamp,
) -> io::Result<()> {
    // Wrap stdout with activity tracking for idle timeout detection
    let tracked_stdout = ActivityTrackingReader::new(stdout, activity_timestamp);
    // Use StreamingLineReader for real-time streaming instead of BufReader::lines().
    // StreamingLineReader yields lines immediately when newlines are found,
    // enabling character-by-character streaming for agents that output NDJSON gradually.
    let reader = StreamingLineReader::new(tracked_stdout);

    if cmd.parser_type != JsonParserType::Generic
        || argv_requests_json(&split_command(cmd.cmd_str)?)
    {
        let stdout_io = io::stdout();
        let mut out = stdout_io.lock();

        match cmd.parser_type {
            JsonParserType::Claude => {
                let p = crate::json_parser::ClaudeParser::new(
                    *runtime.colors,
                    runtime.config.verbosity,
                )
                .with_display_name(cmd.display_name)
                .with_log_file(cmd.logfile)
                .with_show_streaming_metrics(runtime.config.show_streaming_metrics);
                p.parse_stream(reader, runtime.workspace)?;
            }
            JsonParserType::Codex => {
                let p =
                    crate::json_parser::CodexParser::new(*runtime.colors, runtime.config.verbosity)
                        .with_display_name(cmd.display_name)
                        .with_log_file(cmd.logfile)
                        .with_show_streaming_metrics(runtime.config.show_streaming_metrics);
                p.parse_stream(reader, runtime.workspace)?;
            }
            JsonParserType::Gemini => {
                let p = crate::json_parser::GeminiParser::new(
                    *runtime.colors,
                    runtime.config.verbosity,
                )
                .with_display_name(cmd.display_name)
                .with_log_file(cmd.logfile)
                .with_show_streaming_metrics(runtime.config.show_streaming_metrics);
                p.parse_stream(reader, runtime.workspace)?;
            }
            JsonParserType::OpenCode => {
                let p = crate::json_parser::OpenCodeParser::new(
                    *runtime.colors,
                    runtime.config.verbosity,
                )
                .with_display_name(cmd.display_name)
                .with_log_file(cmd.logfile)
                .with_show_streaming_metrics(runtime.config.show_streaming_metrics);
                p.parse_stream(reader, runtime.workspace)?;
            }
            JsonParserType::Generic => {
                let logfile_path = Path::new(cmd.logfile);
                let mut buf = String::new();
                for line in reader.lines() {
                    let line = line?;
                    // Write raw line to log file for extraction using workspace
                    runtime
                        .workspace
                        .append_bytes(logfile_path, format!("{line}\n").as_bytes())?;
                    buf.push_str(&line);
                    buf.push('\n');
                }

                let formatted = format_generic_json_for_display(&buf, runtime.config.verbosity);
                out.write_all(formatted.as_bytes())?;
            }
        }
    } else {
        let logfile_path = Path::new(cmd.logfile);
        let stdout_io = io::stdout();
        let mut out = stdout_io.lock();

        for line in reader.lines() {
            let line = line?;
            writeln!(out, "{line}")?;
            runtime
                .workspace
                .append_bytes(logfile_path, format!("{line}\n").as_bytes())?;
        }
    }
    Ok(())
}
