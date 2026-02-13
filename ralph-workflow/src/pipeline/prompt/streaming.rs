use super::types::{PipelineRuntime, PromptCommand};
use crate::agents::JsonParserType;
use crate::common::split_command;
use crate::logger::argv_requests_json;
use crate::rendering::json_pretty::format_generic_json_for_display;

use std::io::{self, BufRead, Read, Write};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{mpsc, Arc};
use std::time::Duration;

use crate::pipeline::idle_timeout::{ActivityTrackingReader, SharedActivityTimestamp};

use super::streaming_line_reader::StreamingLineReader;

struct CancelAwareReceiverBufRead {
    rx: mpsc::Receiver<io::Result<Vec<u8>>>,
    cancel: Arc<AtomicBool>,
    poll_interval: Duration,
    buffer: Vec<u8>,
    consumed: usize,
    eof: bool,
}

impl CancelAwareReceiverBufRead {
    fn new(
        rx: mpsc::Receiver<io::Result<Vec<u8>>>,
        cancel: Arc<AtomicBool>,
        poll_interval: Duration,
    ) -> Self {
        Self {
            rx,
            cancel,
            poll_interval,
            buffer: Vec::new(),
            consumed: 0,
            eof: false,
        }
    }

    fn refill_if_needed(&mut self) -> io::Result<()> {
        if self.cancel.load(Ordering::Acquire) {
            self.buffer.clear();
            self.consumed = 0;
            self.eof = true;
            return Ok(());
        }

        if self.eof {
            return Ok(());
        }

        if self.consumed < self.buffer.len() {
            return Ok(());
        }

        self.buffer.clear();
        self.consumed = 0;

        loop {
            if self.cancel.load(Ordering::Acquire) {
                self.eof = true;
                return Ok(());
            }
            match self.rx.recv_timeout(self.poll_interval) {
                Ok(Ok(chunk)) => {
                    if chunk.is_empty() {
                        self.eof = true;
                        return Ok(());
                    }
                    self.buffer = chunk;
                    return Ok(());
                }
                Ok(Err(e)) => return Err(e),
                Err(mpsc::RecvTimeoutError::Timeout) => continue,
                Err(mpsc::RecvTimeoutError::Disconnected) => {
                    self.eof = true;
                    return Ok(());
                }
            }
        }
    }
}

impl Read for CancelAwareReceiverBufRead {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        self.refill_if_needed()?;
        if self.eof {
            return Ok(0);
        }

        let available = self.buffer.len() - self.consumed;
        if available == 0 {
            return Ok(0);
        }
        let to_copy = available.min(buf.len());
        buf[..to_copy].copy_from_slice(&self.buffer[self.consumed..self.consumed + to_copy]);
        self.consumed += to_copy;
        Ok(to_copy)
    }
}

impl BufRead for CancelAwareReceiverBufRead {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        self.refill_if_needed()?;
        if self.eof {
            return Ok(&[]);
        }
        Ok(&self.buffer[self.consumed..])
    }

    fn consume(&mut self, amt: usize) {
        self.consumed = (self.consumed + amt).min(self.buffer.len());
        if self.consumed == self.buffer.len() {
            self.buffer.clear();
            self.consumed = 0;
        }
    }
}

fn spawn_stdout_pump(
    stdout: Box<dyn io::Read + Send>,
    activity_timestamp: SharedActivityTimestamp,
    tx: mpsc::Sender<io::Result<Vec<u8>>>,
    cancel: Arc<AtomicBool>,
) -> std::thread::JoinHandle<()> {
    std::thread::spawn(move || {
        let mut tracked_stdout = ActivityTrackingReader::new(stdout, activity_timestamp);
        let mut buf = [0u8; 4096];

        loop {
            if cancel.load(Ordering::Acquire) {
                return;
            }
            match tracked_stdout.read(&mut buf) {
                Ok(0) => {
                    if tx.send(Ok(Vec::new())).is_err() {
                        return;
                    }
                    return;
                }
                Ok(n) => {
                    if tx.send(Ok(buf[..n].to_vec())).is_err() {
                        return;
                    }
                }
                Err(e) if e.kind() == io::ErrorKind::WouldBlock => {
                    if cancel.load(Ordering::Acquire) {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => {
                    let _ = tx.send(Err(e));
                    return;
                }
            }
        }
    })
}

fn cleanup_stdout_pump(
    pump_handle: std::thread::JoinHandle<()>,
    cancel: &Arc<AtomicBool>,
    runtime: &PipelineRuntime<'_>,
    parse_result: &io::Result<()>,
) {
    let should_detach = cancel.load(Ordering::Acquire) || parse_result.is_err();
    if should_detach {
        // Best-effort: avoid leaking a live pump thread after cancellation.
        let deadline = std::time::Instant::now() + Duration::from_secs(2);
        while !pump_handle.is_finished() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(10));
        }
        if pump_handle.is_finished() {
            let _ = pump_handle.join();
        } else {
            runtime
                .logger
                .warn("Stdout pump thread did not exit; detaching thread");
            drop(pump_handle);
        }
    } else {
        let _ = pump_handle.join();
    }
}

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

/// Extract error message from a logfile containing agent JSON output.
///
/// This function searches for error events in the logfile (typically from stdout)
/// and extracts the error message. This is critical for agents like OpenCode that
/// emit errors as JSON to stdout rather than stderr.
///
/// Supported error formats:
/// - OpenCode: `{"type":"error","error":{"message":"usage limit reached"}}`
/// - OpenCode: `{"type":"error","error":{"data":{"message":"Invalid API key"}}}`
/// - Claude: `{"type":"error","message":"Rate limit exceeded"}`
/// - Generic: Any JSON with "error" or "message" fields
///
/// # Arguments
///
/// * `logfile` - Path to the agent's log file
/// * `workspace` - Workspace for file access
///
/// # Returns
///
/// The extracted error message, or `None` if no error found
pub fn extract_error_message_from_logfile(
    logfile: &str,
    workspace: &dyn crate::workspace::Workspace,
) -> Option<String> {
    let logfile_path = Path::new(logfile);
    let content = workspace.read(logfile_path).ok()?;

    // Search through all lines for error events
    // Error events are typically emitted near the end, but we search all lines
    // to handle cases where multiple attempts are logged to the same file
    for line in content.lines().rev().take(50) {
        if let Some(error_msg) = extract_error_from_json_line(line) {
            return Some(error_msg);
        }
    }
    None
}

/// Extract error message from a single JSON line.
///
/// Supports multiple agent error formats:
/// - OpenCode: `{"type":"error","error":{"message":"..."}}`
/// - OpenCode: `{"type":"error","error":{"data":{"message":"..."}}}`
/// - OpenCode: `{"type":"error","error":{"name":"APIError"}}`
/// - OpenCode: `{"type":"error","error":{"code":"usage_limit_exceeded"}}`
/// - OpenCode: `{"type":"error","error":{"provider":"anthropic","message":"..."}}`
/// - Claude: `{"type":"error","message":"..."}`
/// - Generic: `{"error":{"message":"..."}}`
///
/// # OpenCode Error Code Detection
///
/// OpenCode (and some providers) emit structured JSON errors with stable error codes.
/// Error codes are more reliable than message text for detection because they don't
/// change across OpenCode versions or provider updates.
///
/// Supported error codes (verified 2026-02-12 against OpenCode source):
/// - `usage_limit_exceeded`: Usage/quota limit reached
/// - `rate_limit_exceeded`: Rate limit reached
/// - `quota_exceeded`: Quota exhausted
/// - `insufficient_quota`: OpenAI quota exhaustion (source: /packages/opencode/src/provider/error.ts)
/// - `usage_limit_reached`: Alternative usage limit code
///
/// Source: https://github.com/anomalyco/opencode
/// - /packages/opencode/src/cli/cmd/run.ts (error emission)
/// - /packages/opencode/src/session/message-v2.ts (error format definitions)
/// - /packages/opencode/src/provider/error.ts (error code parsing)
///
/// # Provider-Specific Error Format
///
/// OpenCode multi-provider gateway forwards errors from underlying providers
/// (OpenAI, Anthropic, Google, etc.) with a `provider` field:
///
/// ```json
/// {
///   "type": "error",
///   "error": {
///     "provider": "anthropic",
///     "message": "usage limit reached"
///   }
/// }
/// ```
///
/// This format captures provider-specific usage limit errors that should trigger
/// agent fallback.
fn extract_error_from_json_line(line: &str) -> Option<String> {
    // Try to parse as JSON
    let value: serde_json::Value = serde_json::from_str(line).ok()?;

    // Check if this is an error event (type == "error")
    if let Some(event_type) = value.get("type").and_then(|v| v.as_str()) {
        if event_type != "error" {
            return None;
        }
    }

    // PRIORITY 1: OpenCode format: {"error": {"code": "usage_limit_exceeded"}}
    // Error codes are stable and more reliable than message text for detection.
    // Check error codes FIRST before falling back to message extraction.
    //
    // Supported codes: insufficient_quota, usage_limit_exceeded, quota_exceeded,
    // rate_limit_exceeded, usage_limit_reached
    if let Some(error_code) = value.pointer("/error/code").and_then(|v| v.as_str()) {
        return Some(error_code.to_string());
    }

    // PRIORITY 2: OpenCode multi-provider gateway format: {"error": {"provider": "...", "message": "..."}}
    // This captures provider-specific usage limit errors (e.g., "anthropic: usage limit reached")
    // Check provider-specific format BEFORE generic message extraction to preserve provider context.
    //
    // Examples: {"error": {"provider": "openai", "message": "usage limit exceeded"}}
    //           {"error": {"provider": "anthropic", "message": "usage limit reached"}}
    if let Some(provider) = value.pointer("/error/provider").and_then(|v| v.as_str()) {
        if let Some(msg) = value.pointer("/error/message").and_then(|v| v.as_str()) {
            return Some(format!("{}: {}", provider, msg));
        }
    }

    // PRIORITY 3: OpenCode format: {"error": {"data": {"message": "..."}}}
    if let Some(data_message) = value
        .pointer("/error/data/message")
        .and_then(|v| v.as_str())
    {
        return Some(data_message.to_string());
    }

    // PRIORITY 4: OpenCode format: {"error": {"message": "..."}}
    if let Some(error_message) = value.pointer("/error/message").and_then(|v| v.as_str()) {
        return Some(error_message.to_string());
    }

    // PRIORITY 5: OpenCode format: {"error": {"name": "APIError"}}
    if let Some(error_name) = value.pointer("/error/name").and_then(|v| v.as_str()) {
        return Some(error_name.to_string());
    }

    // PRIORITY 6: Claude format: {"message": "..."}
    if let Some(message) = value.get("message").and_then(|v| v.as_str()) {
        return Some(message.to_string());
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
    cancel: Arc<AtomicBool>,
) -> io::Result<()> {
    // UNBOUNDED CHANNEL JUSTIFICATION (streaming.rs:383):
    //
    // This channel transfers stdout chunks from the pump thread to the parser.
    // Unbounded is safe here because:
    //
    // 1. Lifetime bound: Channel exists only during agent execution (minutes to hours max)
    // 2. Natural backpressure: Pump thread blocks on stdout read; agent controls rate
    // 3. Memory bound: Limited by total agent output size (typically <100MB)
    // 4. Timeout protection: Idle timeout monitor terminates stuck agents
    // 5. Cancel-aware: Parser can signal cancellation to drain/stop pump
    //
    // Alternative considered: sync_channel(capacity)
    // Rejected because: Would require tuning capacity per agent type; unbounded
    // provides simpler code with equivalent safety due to bounded agent lifetime.
    //
    // See: tests/integration_tests/memory_safety/channel_bounds.rs for verification
    let (tx, rx) = mpsc::channel();
    let pump_handle = spawn_stdout_pump(stdout, activity_timestamp, tx, Arc::clone(&cancel));

    // Cancel-aware buffering: lets the main thread stop parsing promptly when the
    // idle-timeout monitor fires, even if the underlying stdout read is blocked.
    let receiver_reader =
        CancelAwareReceiverBufRead::new(rx, Arc::clone(&cancel), Duration::from_millis(50));
    let reader = StreamingLineReader::new(receiver_reader);

    let parse_result = (|| {
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
                    let p = crate::json_parser::CodexParser::new(
                        *runtime.colors,
                        runtime.config.verbosity,
                    )
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
    })();

    cleanup_stdout_pump(pump_handle, &cancel, runtime, &parse_result);
    parse_result
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::Workspace;

    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::Duration;

    #[derive(Debug)]
    struct ControlledReader {
        stop: Arc<AtomicBool>,
    }

    impl io::Read for ControlledReader {
        fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
            if self.stop.load(Ordering::Acquire) {
                return Ok(0);
            }
            if buf.is_empty() {
                return Ok(0);
            }
            buf[0] = b'x';
            Ok(1)
        }
    }

    #[test]
    fn stdout_pump_exits_when_receiver_dropped() {
        let stop = Arc::new(AtomicBool::new(false));
        let reader: Box<dyn io::Read + Send> = Box::new(ControlledReader {
            stop: Arc::clone(&stop),
        });

        let timestamp = crate::pipeline::idle_timeout::new_activity_timestamp();
        let (tx, rx) = mpsc::channel();
        let cancel = Arc::new(AtomicBool::new(false));

        let handle = spawn_stdout_pump(reader, timestamp, tx, cancel);
        drop(rx);

        let test_result = {
            let handle_ref = &handle;
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(move || {
                let deadline = std::time::Instant::now() + Duration::from_millis(200);
                while std::time::Instant::now() < deadline {
                    if handle_ref.is_finished() {
                        return;
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                panic!("stdout pump thread did not exit after receiver drop");
            }))
        };

        // Always stop and join so the test doesn't leak threads.
        stop.store(true, Ordering::Release);
        let _ = handle.join();

        if let Err(payload) = test_result {
            std::panic::resume_unwind(payload);
        }
    }

    #[test]
    fn test_extract_error_from_json_line_opencode_usage_limit() {
        let line = r#"{"type":"error","timestamp":1768191346712,"sessionID":"ses_123","error":{"message":"usage limit reached"}}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, Some("usage limit reached".to_string()));
    }

    #[test]
    fn test_extract_error_from_json_line_opencode_data_message() {
        let line = r#"{"type":"error","error":{"data":{"message":"Invalid API key"}}}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, Some("Invalid API key".to_string()));
    }

    #[test]
    fn test_extract_error_from_json_line_opencode_error_name() {
        let line = r#"{"type":"error","error":{"name":"APIError"}}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, Some("APIError".to_string()));
    }

    #[test]
    fn test_extract_error_from_json_line_claude_format() {
        let line = r#"{"type":"error","message":"Rate limit exceeded"}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, Some("Rate limit exceeded".to_string()));
    }

    #[test]
    fn test_extract_error_from_json_line_not_error_event() {
        let line = r#"{"type":"init","session_id":"abc123"}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_error_from_json_line_invalid_json() {
        let line = "This is not JSON";
        let result = extract_error_from_json_line(line);
        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_error_message_from_logfile_opencode_usage_limit() {
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        // Create a test workspace with a logfile containing OpenCode JSON output
        let workspace = MemoryWorkspace::new(PathBuf::from("/tmp/test"));
        let logfile_path = std::path::Path::new(".agent/tmp/opencode.log");

        // Simulate OpenCode JSON output with multiple events including error
        let log_content = r#"{"type":"init","timestamp":1768191346000,"sessionID":"ses_123","model":"claude-3.5-sonnet"}
{"type":"message","timestamp":1768191346100,"content":"Processing request..."}
{"type":"message","timestamp":1768191346200,"content":"Analyzing code..."}
{"type":"error","timestamp":1768191346712,"sessionID":"ses_123","error":{"message":"usage limit reached"}}
"#;

        workspace.write(logfile_path, log_content).unwrap();

        // Extract error message from logfile
        let result = extract_error_message_from_logfile(logfile_path.to_str().unwrap(), &workspace);

        assert_eq!(result, Some("usage limit reached".to_string()));
    }

    #[test]
    fn test_extract_error_message_from_logfile_no_error() {
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let workspace = MemoryWorkspace::new(PathBuf::from("/tmp/test"));
        let logfile_path = std::path::Path::new(".agent/tmp/opencode.log");

        // Logfile with no error events
        let log_content = r#"{"type":"init","timestamp":1768191346000,"sessionID":"ses_123"}
{"type":"message","timestamp":1768191346100,"content":"All good"}
{"type":"completion","timestamp":1768191346200,"status":"success"}
"#;

        workspace.write(logfile_path, log_content).unwrap();

        let result = extract_error_message_from_logfile(logfile_path.to_str().unwrap(), &workspace);

        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_error_message_from_logfile_file_not_found() {
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let workspace = MemoryWorkspace::new(PathBuf::from("/tmp/test"));

        // Try to extract from non-existent file
        let result = extract_error_message_from_logfile(".agent/tmp/nonexistent.log", &workspace);

        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_error_message_from_logfile_empty_file() {
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let workspace = MemoryWorkspace::new(PathBuf::from("/tmp/test"));
        let logfile_path = std::path::Path::new(".agent/tmp/empty.log");

        workspace.write(logfile_path, "").unwrap();

        let result = extract_error_message_from_logfile(logfile_path.to_str().unwrap(), &workspace);

        assert_eq!(result, None);
    }

    #[test]
    fn test_extract_error_message_from_logfile_error_on_first_line() {
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let workspace = MemoryWorkspace::new(PathBuf::from("/tmp/test"));
        let logfile_path = std::path::Path::new(".agent/tmp/opencode.log");

        // Error on first line
        let log_content = r#"{"type":"error","error":{"message":"Invalid API key"}}
{"type":"init","timestamp":1768191346000,"sessionID":"ses_123"}
"#;

        workspace.write(logfile_path, log_content).unwrap();

        let result = extract_error_message_from_logfile(logfile_path.to_str().unwrap(), &workspace);

        assert_eq!(result, Some("Invalid API key".to_string()));
    }

    #[test]
    fn test_extract_error_message_from_logfile_multiple_errors() {
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let workspace = MemoryWorkspace::new(PathBuf::from("/tmp/test"));
        let logfile_path = std::path::Path::new(".agent/tmp/opencode.log");

        // Multiple error events - should return the LAST one (most recent)
        let log_content = r#"{"type":"error","error":{"message":"First error"}}
{"type":"message","timestamp":1768191346100,"content":"Retrying..."}
{"type":"error","error":{"message":"Second error"}}
{"type":"message","timestamp":1768191346200,"content":"Retrying again..."}
{"type":"error","error":{"message":"Final error"}}
"#;

        workspace.write(logfile_path, log_content).unwrap();

        let result = extract_error_message_from_logfile(logfile_path.to_str().unwrap(), &workspace);

        // Should return the last error (searched in reverse, take last 50 lines)
        assert_eq!(result, Some("Final error".to_string()));
    }

    #[test]
    fn test_extract_error_message_from_logfile_claude_format() {
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let workspace = MemoryWorkspace::new(PathBuf::from("/tmp/test"));
        let logfile_path = std::path::Path::new(".agent/tmp/claude.log");

        // Claude error format
        let log_content = r#"{"type":"init","session_id":"abc123"}
{"type":"message","content":"Working..."}
{"type":"error","message":"Rate limit exceeded"}
"#;

        workspace.write(logfile_path, log_content).unwrap();

        let result = extract_error_message_from_logfile(logfile_path.to_str().unwrap(), &workspace);

        assert_eq!(result, Some("Rate limit exceeded".to_string()));
    }

    #[test]
    fn test_extract_error_message_from_logfile_opencode_data_format() {
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let workspace = MemoryWorkspace::new(PathBuf::from("/tmp/test"));
        let logfile_path = std::path::Path::new(".agent/tmp/opencode.log");

        // OpenCode nested data format
        let log_content = r#"{"type":"init","sessionID":"ses_123"}
{"type":"error","error":{"data":{"message":"Nested error message"}}}
"#;

        workspace.write(logfile_path, log_content).unwrap();

        let result = extract_error_message_from_logfile(logfile_path.to_str().unwrap(), &workspace);

        assert_eq!(result, Some("Nested error message".to_string()));
    }

    #[test]
    fn test_extract_error_from_json_line_opencode_error_code() {
        // Test extraction of OpenCode error codes (usage_limit_exceeded, etc.)
        // Error codes are more stable than message text for detection
        let line = r#"{"type":"error","timestamp":1768191346712,"sessionID":"ses_123","error":{"code":"usage_limit_exceeded","message":"Usage limit reached"}}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, Some("usage_limit_exceeded".to_string()));
    }

    #[test]
    fn test_extract_error_from_json_line_opencode_quota_exceeded_code() {
        // Test extraction of quota_exceeded error code
        let line =
            r#"{"type":"error","error":{"code":"quota_exceeded","message":"Quota limit reached"}}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, Some("quota_exceeded".to_string()));
    }

    #[test]
    fn test_extract_error_from_json_line_opencode_usage_limit_reached_code() {
        // Test extraction of usage_limit_reached error code
        let line = r#"{"type":"error","error":{"code":"usage_limit_reached"}}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, Some("usage_limit_reached".to_string()));
    }

    #[test]
    fn test_extract_error_from_json_line_opencode_provider_error() {
        // Test extraction of provider-specific errors (e.g., "anthropic: usage limit reached")
        // OpenCode multi-provider gateway forwards errors with provider prefix
        let line = r#"{"type":"error","timestamp":1768191346712,"sessionID":"ses_123","error":{"provider":"anthropic","message":"usage limit reached"}}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, Some("anthropic: usage limit reached".to_string()));
    }

    #[test]
    fn test_extract_error_from_json_line_opencode_provider_error_openai() {
        // Test provider-specific error for OpenAI
        let line =
            r#"{"type":"error","error":{"provider":"openai","message":"usage limit exceeded"}}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, Some("openai: usage limit exceeded".to_string()));
    }

    #[test]
    fn test_extract_error_from_json_line_opencode_provider_error_google() {
        // Test provider-specific error for Google
        let line = r#"{"type":"error","error":{"provider":"google","message":"quota exceeded"}}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, Some("google: quota exceeded".to_string()));
    }

    #[test]
    fn test_extract_error_from_json_line_opencode_zen_error() {
        // Test OpenCode Zen usage limit error
        let line = r#"{"type":"error","timestamp":1768191346712,"error":{"message":"OpenCode Zen usage limit has been reached"}}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(
            result,
            Some("OpenCode Zen usage limit has been reached".to_string())
        );
    }

    #[test]
    fn test_extract_error_from_json_line_error_code_priority() {
        // Test that error codes are extracted even when message is present
        // Error codes should have priority as they're more stable
        let line = r#"{"type":"error","error":{"code":"usage_limit_exceeded","message":"The usage limit has been reached [retryin]"}}"#;
        let result = extract_error_from_json_line(line);
        assert_eq!(result, Some("usage_limit_exceeded".to_string()));
    }

    #[test]
    fn test_extract_error_message_from_logfile_multiple_formats() {
        // Test that we correctly extract errors from logs with multiple event types
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let workspace = MemoryWorkspace::new(PathBuf::from("/tmp/test"));
        let logfile_path = std::path::Path::new(".agent/tmp/test.log");

        let log_content = r#"{"type":"text","text":"Processing request..."}
{"type":"tool_use","tool":"read","state":{"status":"completed"}}
{"type":"error","error":{"code":"usage_limit_exceeded","message":"Usage limit reached"}}
{"type":"text","text":"Operation failed"}
"#;

        workspace.write(logfile_path, log_content).unwrap();

        let result = extract_error_message_from_logfile(logfile_path.to_str().unwrap(), &workspace);

        assert_eq!(result, Some("usage_limit_exceeded".to_string()));
    }

    #[test]
    fn test_extract_error_message_from_logfile_provider_specific() {
        // Test extraction of provider-specific error from logfile
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let workspace = MemoryWorkspace::new(PathBuf::from("/tmp/test"));
        let logfile_path = std::path::Path::new(".agent/tmp/opencode.log");

        let log_content = r#"{"type":"init","sessionID":"ses_123"}
{"type":"message","content":"Working..."}
{"type":"error","error":{"provider":"anthropic","message":"usage limit reached"}}
"#;

        workspace.write(logfile_path, log_content).unwrap();

        let result = extract_error_message_from_logfile(logfile_path.to_str().unwrap(), &workspace);

        assert_eq!(result, Some("anthropic: usage limit reached".to_string()));
    }

    #[test]
    fn test_extract_error_message_from_logfile_opencode_zen() {
        // Test extraction of OpenCode Zen usage limit error from logfile
        use crate::workspace::MemoryWorkspace;
        use std::path::PathBuf;

        let workspace = MemoryWorkspace::new(PathBuf::from("/tmp/test"));
        let logfile_path = std::path::Path::new(".agent/tmp/opencode.log");

        let log_content = r#"{"type":"init","sessionID":"ses_123"}
{"type":"message","content":"Processing..."}
{"type":"error","error":{"message":"OpenCode Zen usage limit has been reached"}}
"#;

        workspace.write(logfile_path, log_content).unwrap();

        let result = extract_error_message_from_logfile(logfile_path.to_str().unwrap(), &workspace);

        assert_eq!(
            result,
            Some("OpenCode Zen usage limit has been reached".to_string())
        );
    }
}
