//! Prompt-based command execution.

use crate::agents::{is_glm_like_agent, JsonParserType};
use crate::common::{format_argv_for_log, split_command, truncate_text};
use crate::config::Config;
use crate::logger::Colors;
use crate::logger::Logger;
use crate::logger::{argv_requests_json, format_generic_json_for_display};
use crate::pipeline::idle_timeout::{
    monitor_idle_timeout, new_activity_timestamp, ActivityTrackingReader, MonitorResult,
    SharedActivityTimestamp, IDLE_TIMEOUT_SECS,
};
use crate::pipeline::Timer;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};
use std::sync::atomic::AtomicBool;
use std::sync::Arc;

/// A line-oriented reader that processes data as it arrives.
///
/// Unlike `BufReader::lines()`, this reader yields lines immediately
/// when newlines are encountered, without waiting for the buffer to fill.
/// This enables real-time streaming for agents that output NDJSON gradually.
///
/// # Buffer Size Limit
///
/// The reader enforces a maximum buffer size to prevent memory exhaustion
/// from malicious or malformed input that never contains newlines.
/// If the buffer exceeds this limit, subsequent reads will fail with an error.
struct StreamingLineReader<R: Read> {
    inner: BufReader<R>,
    buffer: Vec<u8>,
    consumed: usize,
}

/// Maximum buffer size in bytes to prevent unbounded memory growth.
///
/// This limits the impact of agents that output continuous data without newlines.
/// The value of 1 MiB was chosen to:
/// - Handle most legitimate JSON documents (typically < 100KB)
/// - Allow for reasonably long single-line JSON outputs
/// - Prevent memory exhaustion from malicious input
/// - Keep the buffer size manageable for most systems
///
/// If your use case requires larger single-line JSON, consider:
/// - Modifying your agent to output NDJSON (newline-delimited JSON)
/// - Adjusting this constant and rebuilding
const MAX_BUFFER_SIZE: usize = 1024 * 1024; // 1 MiB

/// Maximum safe prompt size in bytes for command-line arguments.
///
/// The OS has a limit on total argument size (ARG_MAX), typically:
/// - Linux: 2MB (but often limited to 128KB per argument)
/// - macOS: ~1MB
/// - Windows: 32KB
///
/// We use a conservative limit of 200KB to:
/// - Leave room for other arguments and environment variables
/// - Work safely across all platforms
/// - Avoid E2BIG (Argument list too long) errors at spawn time
const MAX_PROMPT_SIZE: usize = 200 * 1024; // 200KB

/// Truncate a prompt that exceeds the safe size limit.
///
/// This function intelligently truncates prompts by:
/// 1. Looking for `{{LAST_OUTPUT}}` marker sections (from XSD retry templates)
/// 2. Truncating from the beginning of LAST_OUTPUT content (keeping the end)
/// 3. If no marker found, truncating from the middle to preserve start/end context
///
/// Returns the original prompt if within limits, or a truncated version with a marker.
fn truncate_prompt_if_needed(prompt: &str, logger: &Logger) -> String {
    if prompt.len() <= MAX_PROMPT_SIZE {
        return prompt.to_string();
    }

    let excess = prompt.len() - MAX_PROMPT_SIZE;
    logger.warn(&format!(
        "Prompt exceeds safe limit ({} bytes > {} bytes), truncating {} bytes",
        prompt.len(),
        MAX_PROMPT_SIZE,
        excess
    ));

    // Strategy: Find the largest contiguous block of content that looks like
    // log output or previous agent output, and truncate from its beginning.
    // This preserves the task instructions at the start and the most recent
    // output at the end (which is most relevant for XSD retry errors).

    // Look for common markers that indicate the start of embedded output
    let truncation_markers = [
        "\n---\n",            // Common section separator
        "\n```\n",            // Code block start
        "\n<last-output>",    // Explicit marker
        "\nPrevious output:", // Text marker
    ];

    for marker in truncation_markers {
        if let Some(marker_pos) = prompt.find(marker) {
            // Found a marker - truncate content after it
            let content_start = marker_pos + marker.len();
            if content_start < prompt.len() {
                let before_marker = &prompt[..content_start];
                let after_marker = &prompt[content_start..];

                if after_marker.len() > excess + 100 {
                    // Truncate from the beginning of the content section
                    let keep_from = excess + 100; // Keep extra for clean line boundary
                    let truncated_content = &after_marker[keep_from..];

                    // Find next newline for clean truncation
                    let clean_start = truncated_content.find('\n').map(|i| i + 1).unwrap_or(0);

                    return format!(
                        "{}\n[... {} bytes truncated to fit CLI argument limit ...]\n{}",
                        before_marker,
                        keep_from + clean_start,
                        &truncated_content[clean_start..]
                    );
                }
            }
        }
    }

    // Fallback: truncate from the middle, preserving start and end
    let keep_start = MAX_PROMPT_SIZE / 3;
    let keep_end = MAX_PROMPT_SIZE / 3;
    let start_part = &prompt[..keep_start];
    let end_part = &prompt[prompt.len() - keep_end..];

    // Find clean line boundaries
    let start_end = start_part.rfind('\n').map(|i| i + 1).unwrap_or(keep_start);
    let end_start = end_part.find('\n').map(|i| i + 1).unwrap_or(0);

    format!(
        "{}\n\n[... {} bytes truncated to fit CLI argument limit ...]\n\n{}",
        &prompt[..start_end],
        prompt.len() - start_end - (keep_end - end_start),
        &end_part[end_start..]
    )
}

impl<R: Read> StreamingLineReader<R> {
    /// Create a new streaming line reader with a small buffer for low latency.
    fn new(inner: R) -> Self {
        // Use a smaller buffer (1KB) than default (8KB) for lower latency.
        // This trades slightly more syscalls for faster response to newlines.
        const BUFFER_SIZE: usize = 1024;
        Self {
            inner: BufReader::with_capacity(BUFFER_SIZE, inner),
            buffer: Vec::new(),
            consumed: 0,
        }
    }

    /// Fill the internal buffer from the underlying reader.
    ///
    /// # Errors
    ///
    /// Returns an error if the buffer would exceed `MAX_BUFFER_SIZE`.
    /// This prevents memory exhaustion from malicious input that never contains newlines.
    fn fill_buffer(&mut self) -> io::Result<usize> {
        // Check if we're approaching the limit before reading more
        let current_size = self.buffer.len() - self.consumed;
        if current_size >= MAX_BUFFER_SIZE {
            return Err(io::Error::other(format!(
                "StreamingLineReader buffer exceeded maximum size of {MAX_BUFFER_SIZE} bytes. \
                This may indicate malformed input or an agent that is not sending newlines."
            )));
        }

        let mut read_buf = [0u8; 256];
        let n = self.inner.read(&mut read_buf)?;
        if n > 0 {
            // Check if adding this data would exceed the limit
            let new_size = current_size + n;
            if new_size > MAX_BUFFER_SIZE {
                return Err(io::Error::other(format!(
                    "StreamingLineReader buffer would exceed maximum size of {MAX_BUFFER_SIZE} bytes. \
                    This may indicate malformed input or an agent that is not sending newlines."
                )));
            }
            self.buffer.extend_from_slice(&read_buf[..n]);
        }
        Ok(n)
    }
}

impl<R: Read> Read for StreamingLineReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        // First, consume from the buffer
        let available = self.buffer.len() - self.consumed;
        if available > 0 {
            let to_copy = available.min(buf.len());
            buf[..to_copy].copy_from_slice(&self.buffer[self.consumed..self.consumed + to_copy]);
            self.consumed += to_copy;

            // Compact the buffer if we've consumed everything
            if self.consumed == self.buffer.len() {
                self.buffer.clear();
                self.consumed = 0;
            }
            return Ok(to_copy);
        }

        // Buffer empty - read directly from underlying reader
        self.inner.read(buf)
    }
}

impl<R: Read> BufRead for StreamingLineReader<R> {
    fn fill_buf(&mut self) -> io::Result<&[u8]> {
        const MAX_ATTEMPTS: usize = 8; // Prevent infinite loop

        // If we have unconsumed data, return it
        if self.consumed < self.buffer.len() {
            return Ok(&self.buffer[self.consumed..]);
        }

        // Buffer was fully consumed - clear and try to read more
        self.buffer.clear();
        self.consumed = 0;

        // Try to fill the buffer with at least some data
        let mut total_read = 0;
        for _ in 0..MAX_ATTEMPTS {
            match self.fill_buffer()? {
                0 if total_read == 0 => return Ok(&[]), // EOF
                0 => break,                             // No more data available right now
                n => {
                    total_read += n;
                    // Check if we have a newline
                    if self.buffer.contains(&b'\n') {
                        break;
                    }
                }
            }
        }

        Ok(&self.buffer[self.consumed..])
    }

    fn consume(&mut self, amt: usize) {
        self.consumed = (self.consumed + amt).min(self.buffer.len());

        // Compact the buffer if we've consumed everything
        if self.consumed == self.buffer.len() {
            self.buffer.clear();
            self.consumed = 0;
        }
    }
}

use super::clipboard::get_platform_clipboard_command;
use super::types::CommandResult;

/// A single prompt-based agent invocation.
pub struct PromptCommand<'a> {
    pub label: &'a str,
    pub display_name: &'a str,
    pub cmd_str: &'a str,
    pub prompt: &'a str,
    pub logfile: &'a str,
    pub parser_type: JsonParserType,
    pub env_vars: &'a std::collections::HashMap<String, String>,
}

/// Runtime services required for running agent commands.
pub struct PipelineRuntime<'a> {
    pub timer: &'a mut Timer,
    pub logger: &'a Logger,
    pub colors: &'a Colors,
    pub config: &'a Config,
    /// Optional agent executor for mocking subprocess execution in tests.
    #[cfg(any(test, feature = "test-utils"))]
    pub agent_executor: Option<Arc<dyn super::test_trait::AgentExecutor>>,
    /// Process executor for external process execution.
    pub executor: &'a dyn crate::executor::ProcessExecutor,
}

/// Command configuration for building an agent command.
struct CommandConfig<'a> {
    cmd_str: &'a str,
    prompt: &'a str,
    env_vars: &'a std::collections::HashMap<String, String>,
    logfile: &'a str,
    parser_type: JsonParserType,
}

/// Saves the prompt to a file and optionally copies it to the clipboard.
fn save_prompt_to_file_and_clipboard(
    prompt: &str,
    prompt_path: &std::path::PathBuf,
    interactive: bool,
    logger: &Logger,
    colors: Colors,
    executor: &dyn crate::executor::ProcessExecutor,
) -> io::Result<()> {
    // Save prompt to file
    if let Some(parent) = prompt_path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(prompt_path, prompt)?;
    logger.info(&format!(
        "Prompt saved to {}{}{}",
        colors.cyan(),
        prompt_path.display(),
        colors.reset()
    ));

    // Copy to clipboard if interactive
    if interactive {
        if let Some(clipboard_cmd) = get_platform_clipboard_command() {
            match executor.spawn(clipboard_cmd.binary, clipboard_cmd.args, &[], None) {
                Ok(mut child) => {
                    if let Some(mut stdin) = child.stdin.take() {
                        let _ = stdin.write_all(prompt.as_bytes());
                    }
                    let _ = child.wait();
                    logger.info(&format!(
                        "Prompt copied to clipboard {}({}){}",
                        colors.dim(),
                        clipboard_cmd.paste_hint,
                        colors.reset()
                    ));
                }
                Err(e) => {
                    logger.warn(&format!("Failed to copy to clipboard: {}", e));
                }
            }
        }
    }
    Ok(())
}

/// Builds and configures the agent command with environment variables.
fn build_agent_command(
    config: &CommandConfig<'_>,
    anthropic_env_vars_to_sanitize: &[&str],
    logger: &Logger,
    colors: Colors,
) -> io::Result<(Vec<String>, Command)> {
    let argv = split_command(config.cmd_str)?;
    if argv.is_empty() || config.cmd_str.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Agent command is empty or contains only whitespace",
        ));
    }

    let mut argv_for_log = argv.clone();
    argv_for_log.push("<PROMPT>".to_string());
    let display_cmd = truncate_text(&format_argv_for_log(&argv_for_log), 160);
    logger.info(&format!(
        "Executing: {}{}{}",
        colors.dim(),
        display_cmd,
        colors.reset()
    ));

    // GLM-specific debug logging (only for CCS/Claude-based GLM)
    let is_glm_cmd = is_glm_like_agent(config.cmd_str);
    if is_glm_cmd {
        logger.info(&format!("GLM command details: {display_cmd}"));
        if argv.iter().any(|arg| arg == "-p") {
            logger.info("GLM command includes '-p' flag (correct)");
        } else {
            logger.warn("GLM command may be missing '-p' flag");
        }
    }

    let _uses_json = config.parser_type != JsonParserType::Generic || argv_requests_json(&argv);
    logger.info(&format!("Using {} parser...", config.parser_type));

    if let Some(parent) = Path::new(config.logfile).parent() {
        fs::create_dir_all(parent)?;
    }
    File::create(config.logfile)?;

    let mut command = Command::new(&argv[0]);
    command.args(&argv[1..]);

    // Truncate prompt if it exceeds safe CLI argument limits to prevent E2BIG errors
    let prompt = truncate_prompt_if_needed(config.prompt, logger);
    command.arg(&prompt);

    // Inject environment variables from agent config
    if !config.env_vars.is_empty() {
        logger.info(&format!(
            "Injecting {} environment variable(s) into subprocess",
            config.env_vars.len()
        ));
        for key in config.env_vars.keys() {
            logger.info(&format!("  - {key}"));
        }
        for (key, value) in config.env_vars {
            command.env(key, value);
        }
    }

    // Clear problematic Anthropic env vars that weren't explicitly set by the agent.
    // We build a complete env map, sanitize it, then apply it to the command.
    let mut complete_env: std::collections::HashMap<String, String> =
        std::env::vars().collect();
    for (key, value) in config.env_vars.iter() {
        complete_env.insert(key.clone(), value.clone());
    }
    sanitize_command_env(&mut complete_env, &config.env_vars, anthropic_env_vars_to_sanitize);
    for (key, value) in &complete_env {
        command.env(key, value);
    }

    // Set agent-side buffering disabling environment variables for real-time streaming.
    // These are only set if not already explicitly configured by the user's env_vars.
    // This mitigates the issue where AI agents buffer their stdout instead of streaming.
    //
    // Note: NODE_ENV is set to "production" (not "development") because production mode
    // disables buffering in Node.js applications. This is necessary for real-time streaming
    // but may affect error stack traces and logging levels in Node.js agents.
    let buffering_vars = [("PYTHONUNBUFFERED", "1"), ("NODE_ENV", "production")];
    for (key, value) in buffering_vars {
        if !config.env_vars.contains_key(key) {
            command.env(key, value);
        }
    }

    Ok((argv, command))
}

/// Sanitize environment variables for agent subprocess execution.
///
/// This function removes problematic Anthropic environment variables from the
/// provided environment map, unless they were explicitly set by the agent
/// configuration.
///
/// # Arguments
///
/// * `env_vars` - Mutable reference to environment variables map
/// * `agent_env_vars` - Environment variables explicitly set by agent config
/// * `vars_to_sanitize` - List of environment variable names to remove
///
/// # Behavior
///
/// - Removes all vars in `vars_to_sanitize` from `env_vars`
/// - EXCEPT for vars that are present in `agent_env_vars` (explicitly set)
/// - This prevents GLM CCS credentials from leaking into agent subprocesses
///
/// # Example
///
/// ```ignore
/// let mut env = std::env::vars().collect::<HashMap<_, _>>();
/// let agent_vars = HashMap::from([("ANTHROPIC_API_KEY", "agent-key")]);
/// sanitize_command_env(&mut env, &agent_vars, ANTHROPIC_VARS);
/// // env no longer contains ANTHROPIC_BASE_URL (not in agent_vars)
/// // env still contains ANTHROPIC_API_KEY (explicitly set by agent)
/// ```
pub fn sanitize_command_env(
    env_vars: &mut std::collections::HashMap<String, String>,
    agent_env_vars: &std::collections::HashMap<String, String>,
    vars_to_sanitize: &[&str],
) {
    for &var in vars_to_sanitize {
        if !agent_env_vars.contains_key(var) {
            env_vars.remove(var);
        }
    }
}

/// Spawns the agent process, converting ALL spawn errors into `CommandResult`.
///
/// This ensures that any failure to spawn the agent process is handled by the
/// fallback system instead of crashing the pipeline. Common errors:
/// - `NotFound` (exit code 127): Command not found
/// - `PermissionDenied` (exit code 126): Permission denied  
/// - `ArgumentListTooLong` (exit code 7): Prompt too large for command-line argument
/// - Other errors (exit code 1): Converted to CommandResult for fallback handling
fn spawn_agent_process(mut command: Command, argv: &[String]) -> Result<Child, CommandResult> {
    match command
        .stdin(Stdio::null())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
    {
        Ok(child) => Ok(child),
        Err(e) => {
            // Convert ALL spawn errors to CommandResult so fallback can handle them.
            // This prevents any spawn failure from crashing the entire pipeline.
            let (exit_code, detail) = match e.kind() {
                io::ErrorKind::NotFound => (127, "command not found"),
                io::ErrorKind::PermissionDenied => (126, "permission denied"),
                io::ErrorKind::ArgumentListTooLong => {
                    (7, "argument list too long (prompt exceeds OS limit)")
                }
                io::ErrorKind::InvalidInput => (22, "invalid input"),
                io::ErrorKind::OutOfMemory => (12, "out of memory"),
                _ => (1, "spawn failed"),
            };

            Err(CommandResult {
                exit_code,
                stderr: format!("{}: {} - {}", argv[0], detail, e),
            })
        }
    }
}

/// Streams agent output based on parser type.
///
/// The `activity_timestamp` is updated whenever data is read from stdout,
/// allowing external monitoring for idle timeout detection.
fn stream_agent_output(
    stdout: ChildStdout,
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
                p.parse_stream(reader)?;
            }
            JsonParserType::Codex => {
                let p =
                    crate::json_parser::CodexParser::new(*runtime.colors, runtime.config.verbosity)
                        .with_display_name(cmd.display_name)
                        .with_log_file(cmd.logfile)
                        .with_show_streaming_metrics(runtime.config.show_streaming_metrics);
                p.parse_stream(reader)?;
            }
            JsonParserType::Gemini => {
                let p = crate::json_parser::GeminiParser::new(
                    *runtime.colors,
                    runtime.config.verbosity,
                )
                .with_display_name(cmd.display_name)
                .with_log_file(cmd.logfile)
                .with_show_streaming_metrics(runtime.config.show_streaming_metrics);
                p.parse_stream(reader)?;
            }
            JsonParserType::OpenCode => {
                let p = crate::json_parser::OpenCodeParser::new(
                    *runtime.colors,
                    runtime.config.verbosity,
                )
                .with_display_name(cmd.display_name)
                .with_log_file(cmd.logfile)
                .with_show_streaming_metrics(runtime.config.show_streaming_metrics);
                p.parse_stream(reader)?;
            }
            JsonParserType::Generic => {
                let mut logfile = OpenOptions::new()
                    .create(true)
                    .append(true)
                    .open(cmd.logfile)?;

                let mut buf = String::new();
                for line in reader.lines() {
                    let line = line?;
                    // Write raw line to log file for extraction
                    writeln!(logfile, "{line}")?;
                    buf.push_str(&line);
                    buf.push('\n');
                }
                logfile.flush()?;
                // Ensure data is written to disk before continuing
                // This prevents race conditions where extraction runs before OS commits writes
                let _ = logfile.sync_all();

                let formatted = format_generic_json_for_display(&buf, runtime.config.verbosity);
                out.write_all(formatted.as_bytes())?;
            }
        }
    } else {
        let mut logfile = OpenOptions::new()
            .create(true)
            .append(true)
            .open(cmd.logfile)?;

        let stdout_io = io::stdout();
        let mut out = stdout_io.lock();

        for line in reader.lines() {
            let line = line?;
            writeln!(out, "{line}")?;
            writeln!(logfile, "{line}")?;
        }
        logfile.flush()?;
        // Ensure data is written to disk before continuing
        // This prevents race conditions where extraction runs before OS commits writes
        let _ = logfile.sync_all();
    }
    Ok(())
}

/// Waits for process completion and collects stderr output.
fn wait_for_completion_and_collect_stderr(
    mut child: Child,
    stderr_join_handle: Option<std::thread::JoinHandle<io::Result<String>>>,
    runtime: &PipelineRuntime<'_>,
) -> io::Result<(i32, String)> {
    let status = child.wait()?;
    let exit_code = status.code().unwrap_or(1);

    if status.code().is_none() && runtime.config.verbosity.is_debug() {
        runtime
            .logger
            .warn("Process terminated by signal (no exit code), treating as failure");
    }

    let stderr_output = match stderr_join_handle {
        Some(handle) => match handle.join() {
            Ok(result) => result?,
            Err(panic_payload) => {
                let panic_msg = panic_payload.downcast_ref::<String>().map_or_else(
                    || {
                        panic_payload.downcast_ref::<&str>().map_or_else(
                            || "<unknown panic>".to_string(),
                            std::string::ToString::to_string,
                        )
                    },
                    std::clone::Clone::clone,
                );
                runtime.logger.warn(&format!(
                    "Stderr collection thread panicked: {panic_msg}. This may indicate a bug."
                ));
                String::new()
            }
        },
        None => String::new(),
    };

    if !stderr_output.is_empty() && runtime.config.verbosity.is_debug() {
        runtime.logger.warn(&format!(
            "Agent stderr output detected ({} bytes):",
            stderr_output.len()
        ));
        for (i, line) in stderr_output.lines().take(5).enumerate() {
            runtime.logger.info(&format!("  stderr[{i}]: {line}"));
        }
        if stderr_output.lines().count() > 5 {
            runtime.logger.info(&format!(
                "  ... ({} more lines, see log file for full output)",
                stderr_output.lines().count() - 5
            ));
        }
    }

    Ok((exit_code, stderr_output))
}

/// Run a command with a prompt argument.
///
/// This is an internal helper for `run_with_fallback`.
pub fn run_with_prompt(
    cmd: &PromptCommand<'_>,
    runtime: &mut PipelineRuntime<'_>,
) -> io::Result<CommandResult> {
    const ANTHROPIC_ENV_VARS_TO_SANITIZE: &[&str] = &[
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
    ];

    runtime.timer.start_phase();
    runtime.logger.step(&format!(
        "{}{}{}",
        runtime.colors.bold(),
        cmd.label,
        runtime.colors.reset()
    ));

    save_prompt_to_file_and_clipboard(
        cmd.prompt,
        &runtime.config.prompt_path,
        runtime.config.behavior.interactive,
        runtime.logger,
        *runtime.colors,
        runtime.executor,
    )?;

    // Use agent executor if provided (for testing)
    #[cfg(any(test, feature = "test-utils"))]
    {
        if let Some(executor) = runtime.agent_executor.clone() {
            return run_with_agent_executor(cmd, runtime, &executor);
        }
    }

    run_with_subprocess(cmd, runtime, ANTHROPIC_ENV_VARS_TO_SANITIZE)
}

/// Exit code returned when a process is killed due to SIGTERM.
const SIGTERM_EXIT_CODE: i32 = 143;

/// Run agent using the real subprocess execution.
#[cfg(not(any(test, feature = "test-utils")))]
fn run_with_subprocess(
    cmd: &PromptCommand<'_>,
    runtime: &mut PipelineRuntime<'_>,
    anthropic_env_vars_to_sanitize: &[&str],
) -> io::Result<CommandResult> {
    let (argv, command) = build_agent_command(
        &CommandConfig {
            cmd_str: cmd.cmd_str,
            prompt: cmd.prompt,
            env_vars: cmd.env_vars,
            logfile: cmd.logfile,
            parser_type: cmd.parser_type,
        },
        anthropic_env_vars_to_sanitize,
        runtime.logger,
        *runtime.colors,
    )?;

    let mut child = match spawn_agent_process(command, &argv) {
        Ok(child) => child,
        Err(result) => return Ok(result),
    };

    // Get child PID for idle timeout monitoring
    let child_id = child.id();

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("Failed to capture stdout"))?;

    // Set up idle timeout monitoring
    let activity_timestamp = new_activity_timestamp();
    let monitor_should_stop = Arc::new(AtomicBool::new(false));
    let monitor_should_stop_clone = monitor_should_stop.clone();
    let activity_timestamp_clone = activity_timestamp.clone();

    // Create executor for monitor thread to kill the subprocess if needed
    let monitor_executor: Arc<dyn crate::executor::ProcessExecutor> =
        Arc::new(crate::executor::RealProcessExecutor::new());

    // Spawn idle timeout monitor thread
    let monitor_handle = std::thread::spawn(move || {
        monitor_idle_timeout(
            activity_timestamp_clone,
            child_id,
            IDLE_TIMEOUT_SECS,
            monitor_should_stop_clone,
            monitor_executor,
        )
    });

    let stderr_join_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || -> io::Result<String> {
            const STDERR_MAX_BYTES: usize = 512 * 1024;

            let mut reader = BufReader::new(stderr);
            let mut buf = [0u8; 8192];
            let mut collected = Vec::<u8>::new();
            let mut truncated = false;

            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }

                let remaining = STDERR_MAX_BYTES.saturating_sub(collected.len());
                if remaining == 0 {
                    truncated = true;
                    break;
                }

                let to_take = remaining.min(n);
                collected.extend_from_slice(&buf[..to_take]);
                if to_take < n {
                    truncated = true;
                    break;
                }
            }

            let mut stderr_output = String::from_utf8_lossy(&collected).into_owned();
            if truncated {
                if !stderr_output.ends_with('\n') {
                    stderr_output.push('\n');
                }
                stderr_output.push_str("<stderr truncated>");
            }

            Ok(stderr_output)
        })
    });

    stream_agent_output(stdout, cmd, runtime, activity_timestamp)?;

    // Signal monitor to stop (process completed or streaming ended)
    monitor_should_stop.store(true, std::sync::atomic::Ordering::Release);

    let (exit_code, stderr_output) =
        wait_for_completion_and_collect_stderr(child, stderr_join_handle, runtime)?;

    // Check if monitor killed the process due to idle timeout
    let monitor_result = monitor_handle
        .join()
        .unwrap_or(MonitorResult::ProcessCompleted);

    // If monitor timed out, use SIGTERM exit code regardless of actual exit code
    let final_exit_code = if monitor_result == MonitorResult::TimedOut {
        runtime.logger.warn(&format!(
            "Agent killed due to idle timeout (no output for {} seconds)",
            IDLE_TIMEOUT_SECS
        ));
        SIGTERM_EXIT_CODE
    } else {
        exit_code
    };

    if runtime.config.verbosity.is_verbose() {
        runtime.logger.info(&format!(
            "Phase elapsed: {}",
            runtime.timer.phase_elapsed_formatted()
        ));
    }

    Ok(CommandResult {
        exit_code: final_exit_code,
        stderr: stderr_output,
    })
}

/// Run agent using the real subprocess execution.
#[cfg(any(test, feature = "test-utils"))]
fn run_with_subprocess(
    cmd: &PromptCommand<'_>,
    runtime: &mut PipelineRuntime<'_>,
    anthropic_env_vars_to_sanitize: &[&str],
) -> io::Result<CommandResult> {
    let (argv, command) = build_agent_command(
        &CommandConfig {
            cmd_str: cmd.cmd_str,
            prompt: cmd.prompt,
            env_vars: cmd.env_vars,
            logfile: cmd.logfile,
            parser_type: cmd.parser_type,
        },
        anthropic_env_vars_to_sanitize,
        runtime.logger,
        *runtime.colors,
    )?;

    let mut child = match spawn_agent_process(command, &argv) {
        Ok(child) => child,
        Err(result) => return Ok(result),
    };

    // Get child PID for idle timeout monitoring
    let child_id = child.id();

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("Failed to capture stdout"))?;

    // Set up idle timeout monitoring
    let activity_timestamp = new_activity_timestamp();
    let monitor_should_stop = Arc::new(AtomicBool::new(false));
    let monitor_should_stop_clone = monitor_should_stop.clone();
    let activity_timestamp_clone = activity_timestamp.clone();

    // Create executor for monitor thread to kill the subprocess if needed
    let monitor_executor: Arc<dyn crate::executor::ProcessExecutor> =
        Arc::new(crate::executor::RealProcessExecutor::new());

    // Spawn idle timeout monitor thread
    let monitor_handle = std::thread::spawn(move || {
        monitor_idle_timeout(
            activity_timestamp_clone,
            child_id,
            IDLE_TIMEOUT_SECS,
            monitor_should_stop_clone,
            monitor_executor,
        )
    });

    let stderr_join_handle = child.stderr.take().map(|stderr| {
        std::thread::spawn(move || -> io::Result<String> {
            const STDERR_MAX_BYTES: usize = 512 * 1024;

            let mut reader = BufReader::new(stderr);
            let mut buf = [0u8; 8192];
            let mut collected = Vec::<u8>::new();
            let mut truncated = false;

            loop {
                let n = reader.read(&mut buf)?;
                if n == 0 {
                    break;
                }

                let remaining = STDERR_MAX_BYTES.saturating_sub(collected.len());
                if remaining == 0 {
                    truncated = true;
                    break;
                }

                let to_take = remaining.min(n);
                collected.extend_from_slice(&buf[..to_take]);
                if to_take < n {
                    truncated = true;
                    break;
                }
            }

            let mut stderr_output = String::from_utf8_lossy(&collected).into_owned();
            if truncated {
                if !stderr_output.ends_with('\n') {
                    stderr_output.push('\n');
                }
                stderr_output.push_str("<stderr truncated>");
            }

            Ok(stderr_output)
        })
    });

    stream_agent_output(stdout, cmd, runtime, activity_timestamp)?;

    // Signal monitor to stop (process completed or streaming ended)
    monitor_should_stop.store(true, std::sync::atomic::Ordering::Release);

    let (exit_code, stderr_output) =
        wait_for_completion_and_collect_stderr(child, stderr_join_handle, runtime)?;

    // Check if monitor killed the process due to idle timeout
    let monitor_result = monitor_handle
        .join()
        .unwrap_or(MonitorResult::ProcessCompleted);

    // If monitor timed out, use SIGTERM exit code regardless of actual exit code
    let final_exit_code = if monitor_result == MonitorResult::TimedOut {
        runtime.logger.warn(&format!(
            "Agent killed due to idle timeout (no output for {} seconds)",
            IDLE_TIMEOUT_SECS
        ));
        SIGTERM_EXIT_CODE
    } else {
        exit_code
    };

    if runtime.config.verbosity.is_verbose() {
        runtime.logger.info(&format!(
            "Phase elapsed: {}",
            runtime.timer.phase_elapsed_formatted()
        ));
    }

    Ok(CommandResult {
        exit_code: final_exit_code,
        stderr: stderr_output,
    })
}

/// Run agent using the mocked AgentExecutor (for testing).
#[cfg(any(test, feature = "test-utils"))]
fn run_with_agent_executor(
    cmd: &PromptCommand<'_>,
    runtime: &mut PipelineRuntime<'_>,
    executor: &std::sync::Arc<dyn super::test_trait::AgentExecutor>,
) -> io::Result<CommandResult> {
    use super::test_trait::AgentCommandConfig;

    let (argv, _command) = build_agent_command(
        &CommandConfig {
            cmd_str: cmd.cmd_str,
            prompt: cmd.prompt,
            env_vars: cmd.env_vars,
            logfile: cmd.logfile,
            parser_type: cmd.parser_type,
        },
        &[],
        runtime.logger,
        *runtime.colors,
    )?;

    let display_cmd = truncate_text(&format_argv_for_log(&argv), 160);
    runtime.logger.info(&format!(
        "Executing (mocked): {}{}{}",
        runtime.colors.dim(),
        display_cmd,
        runtime.colors.reset()
    ));

    let result = executor.execute(&AgentCommandConfig {
        cmd: cmd.cmd_str.to_string(),
        prompt: cmd.prompt.to_string(),
        env_vars: cmd.env_vars.clone(),
        parser_type: cmd.parser_type,
        logfile: cmd.logfile.to_string(),
        display_name: cmd.display_name.to_string(),
    })?;

    if runtime.config.verbosity.is_verbose() {
        runtime.logger.info(&format!(
            "Phase elapsed: {}",
            runtime.timer.phase_elapsed_formatted()
        ));
    }

    Ok(CommandResult {
        exit_code: result.exit_code,
        stderr: result.stderr,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn test_logger() -> Logger {
        Logger::new(Colors::new())
    }

    #[test]
    fn test_truncate_prompt_small_content() {
        let logger = test_logger();
        let content = "This is a small prompt that fits within limits.";
        let result = truncate_prompt_if_needed(content, &logger);
        assert_eq!(result, content);
    }

    #[test]
    fn test_truncate_prompt_large_content_with_marker() {
        let logger = test_logger();
        // Create content larger than MAX_PROMPT_SIZE with a section separator
        let prefix = "Task: Do something\n\n---\n";
        let large_content = "x".repeat(MAX_PROMPT_SIZE + 50000);
        let content = format!("{}{}", prefix, large_content);

        let result = truncate_prompt_if_needed(&content, &logger);

        // Should be truncated
        assert!(result.len() < content.len());
        // Should have truncation marker
        assert!(result.contains("truncated"));
        // Should preserve the prefix
        assert!(result.starts_with("Task:"));
    }

    #[test]
    fn test_truncate_prompt_large_content_fallback() {
        let logger = test_logger();
        // Create content larger than MAX_PROMPT_SIZE without any markers
        let content = "a".repeat(MAX_PROMPT_SIZE + 50000);

        let result = truncate_prompt_if_needed(&content, &logger);

        // Should be truncated
        assert!(result.len() < content.len());
        // Should have truncation marker
        assert!(result.contains("truncated"));
    }

    #[test]
    fn test_truncate_prompt_preserves_end() {
        let logger = test_logger();
        // Content with marker and important end content
        let prefix = "Instructions\n\n---\n";
        let middle = "m".repeat(MAX_PROMPT_SIZE);
        let suffix = "\nIMPORTANT_END_MARKER";
        let content = format!("{}{}{}", prefix, middle, suffix);

        let result = truncate_prompt_if_needed(&content, &logger);

        // Should preserve the end content (most relevant for XSD errors)
        assert!(result.contains("IMPORTANT_END_MARKER"));
    }
}

#[cfg(test)]
mod sanitize_env_tests {
    use super::*;
    use std::collections::HashMap;

    const ANTHROPIC_ENV_VARS_TO_SANITIZE: &[&str] = &[
        "ANTHROPIC_API_KEY",
        "ANTHROPIC_BASE_URL",
        "ANTHROPIC_AUTH_TOKEN",
        "ANTHROPIC_MODEL",
        "ANTHROPIC_DEFAULT_HAIKU_MODEL",
        "ANTHROPIC_DEFAULT_OPUS_MODEL",
        "ANTHROPIC_DEFAULT_SONNET_MODEL",
    ];

    #[test]
    fn test_sanitize_command_env_removes_anthropic_vars_when_not_explicitly_set() {
        // Setup: Environment with GLM-like Anthropic credentials
        let mut env_vars = HashMap::from([
            ("ANTHROPIC_API_KEY".to_string(), "glm-test-key".to_string()),
            ("ANTHROPIC_BASE_URL".to_string(), "https://glm.example.com".to_string()),
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
            ("HOME".to_string(), "/home/user".to_string()),
        ]);
        let agent_env_vars = HashMap::new(); // Agent doesn't set any Anthropic vars

        // Execute: Sanitize environment
        sanitize_command_env(&mut env_vars, &agent_env_vars, ANTHROPIC_ENV_VARS_TO_SANITIZE);

        // Assert: Anthropic vars should be removed, other vars preserved
        assert!(
            !env_vars.contains_key("ANTHROPIC_API_KEY"),
            "ANTHROPIC_API_KEY should be removed when not explicitly set by agent"
        );
        assert!(
            !env_vars.contains_key("ANTHROPIC_BASE_URL"),
            "ANTHROPIC_BASE_URL should be removed when not explicitly set by agent"
        );
        assert_eq!(
            env_vars.get("PATH"),
            Some(&"/usr/bin:/bin".to_string()),
            "Non-Anthropic vars should be preserved"
        );
        assert_eq!(
            env_vars.get("HOME"),
            Some(&"/home/user".to_string()),
            "Non-Anthropic vars should be preserved"
        );
    }

    #[test]
    fn test_sanitize_command_env_preserves_explicitly_set_anthropic_vars() {
        // Setup: Environment with parent Anthropic vars + agent's explicit vars
        let mut env_vars = HashMap::from([
            ("ANTHROPIC_API_KEY".to_string(), "parent-key".to_string()),
            ("ANTHROPIC_BASE_URL".to_string(), "https://parent.example.com".to_string()),
            ("ANTHROPIC_AUTH_TOKEN".to_string(), "parent-token".to_string()),
            ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ]);
        let agent_env_vars = HashMap::from([
            ("ANTHROPIC_API_KEY".to_string(), "agent-specific-key".to_string()),
            ("ANTHROPIC_BASE_URL".to_string(), "https://agent.example.com".to_string()),
        ]);

        // Execute: Sanitize environment
        sanitize_command_env(&mut env_vars, &agent_env_vars, ANTHROPIC_ENV_VARS_TO_SANITIZE);

        // Assert: Explicitly set Anthropic vars should be preserved with agent's values
        assert_eq!(
            env_vars.get("ANTHROPIC_API_KEY"),
            Some(&"agent-specific-key".to_string()),
            "ANTHROPIC_API_KEY explicitly set by agent should be preserved"
        );
        assert_eq!(
            env_vars.get("ANTHROPIC_BASE_URL"),
            Some(&"https://agent.example.com".to_string()),
            "ANTHROPIC_BASE_URL explicitly set by agent should be preserved"
        );
        assert!(
            !env_vars.contains_key("ANTHROPIC_AUTH_TOKEN"),
            "ANTHROPIC_AUTH_TOKEN not explicitly set by agent should be removed"
        );
        assert_eq!(
            env_vars.get("PATH"),
            Some(&"/usr/bin:/bin".to_string()),
            "Non-Anthropic vars should be preserved"
        );
    }

    #[test]
    fn test_sanitize_command_env_handles_empty_env_vars() {
        // Setup: Empty environment
        let mut env_vars = HashMap::new();
        let agent_env_vars = HashMap::new();

        // Execute: Should not panic on empty input
        sanitize_command_env(&mut env_vars, &agent_env_vars, ANTHROPIC_ENV_VARS_TO_SANITIZE);

        // Assert: Environment should remain empty
        assert!(env_vars.is_empty(), "Empty environment should remain empty");
    }

    #[test]
    fn test_sanitize_command_env_handles_all_anthropic_vars() {
        // Setup: Environment with all Anthropic vars
        let mut env_vars = ANTHROPIC_ENV_VARS_TO_SANITIZE
            .iter()
            .map(|&var| (var.to_string(), format!("value-{var}")))
            .collect();
        env_vars.insert("OTHER_VAR".to_string(), "other-value".to_string());

        let agent_env_vars = HashMap::new();

        // Execute: Sanitize all Anthropic vars
        sanitize_command_env(&mut env_vars, &agent_env_vars, ANTHROPIC_ENV_VARS_TO_SANITIZE);

        // Assert: All Anthropic vars should be removed
        for &var in ANTHROPIC_ENV_VARS_TO_SANITIZE {
            assert!(
                !env_vars.contains_key(var),
                "{var} should be removed when not explicitly set"
            );
        }
        assert_eq!(
            env_vars.get("OTHER_VAR"),
            Some(&"other-value".to_string()),
            "Non-Anthropic vars should be preserved"
        );
    }
}
