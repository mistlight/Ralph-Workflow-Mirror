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
}
