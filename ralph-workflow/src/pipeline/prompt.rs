//! Prompt-based command execution.

use crate::agents::{is_glm_like_agent, JsonParserType};
use crate::common::{format_argv_for_log, split_command, truncate_text};
use crate::config::Config;
use crate::logger::Colors;
use crate::logger::Logger;
use crate::logger::{argv_requests_json, format_generic_json_for_display};
use crate::pipeline::Timer;
use std::fs::{self, File, OpenOptions};
use std::io::{self, BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStdout, Command, Stdio};

#[cfg(any(test, feature = "test-utils"))]
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
            if let Ok(mut child) = Command::new(clipboard_cmd.binary)
                .args(clipboard_cmd.args)
                .stdin(Stdio::piped())
                .spawn()
            {
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
    command.arg(config.prompt);

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

    // Clear problematic Anthropic env vars that weren't explicitly set by the agent.
    for &var in anthropic_env_vars_to_sanitize {
        if !config.env_vars.contains_key(var) {
            command.env_remove(var);
        }
    }

    Ok((argv, command))
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
fn stream_agent_output(
    stdout: ChildStdout,
    cmd: &PromptCommand<'_>,
    runtime: &PipelineRuntime<'_>,
) -> io::Result<()> {
    // Use StreamingLineReader for real-time streaming instead of BufReader::lines().
    // StreamingLineReader yields lines immediately when newlines are found,
    // enabling character-by-character streaming for agents that output NDJSON gradually.
    let reader = StreamingLineReader::new(stdout);

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

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("Failed to capture stdout"))?;

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

    stream_agent_output(stdout, cmd, runtime)?;

    let (exit_code, stderr_output) =
        wait_for_completion_and_collect_stderr(child, stderr_join_handle, runtime)?;

    if runtime.config.verbosity.is_verbose() {
        runtime.logger.info(&format!(
            "Phase elapsed: {}",
            runtime.timer.phase_elapsed_formatted()
        ));
    }

    Ok(CommandResult {
        exit_code,
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

    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| io::Error::other("Failed to capture stdout"))?;

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

    stream_agent_output(stdout, cmd, runtime)?;

    let (exit_code, stderr_output) =
        wait_for_completion_and_collect_stderr(child, stderr_join_handle, runtime)?;

    if runtime.config.verbosity.is_verbose() {
        runtime.logger.info(&format!(
            "Phase elapsed: {}",
            runtime.timer.phase_elapsed_formatted()
        ));
    }

    Ok(CommandResult {
        exit_code,
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
