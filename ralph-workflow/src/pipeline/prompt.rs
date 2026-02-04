//! Prompt-based command execution.

use crate::agents::{is_glm_like_agent, JsonParserType};
use crate::common::{format_argv_for_log, split_command, truncate_text};
use crate::logger::argv_requests_json;
use crate::pipeline::idle_timeout::{
    monitor_idle_timeout, new_activity_timestamp, time_since_activity, MonitorResult,
    StderrActivityTracker, IDLE_TIMEOUT_SECS,
};
use std::io::{self, BufReader, Read};
use std::path::Path;
use std::sync::Arc;

mod environment;
mod save;
mod streaming;
mod streaming_line_reader;
mod types;

pub use environment::sanitize_command_env;
pub use types::{PipelineRuntime, PromptCommand};

#[cfg(test)]
use streaming_line_reader::StreamingLineReader;

#[cfg(test)]
use save::build_prompt_archive_filename;

#[cfg(test)]
use std::io::BufRead;

#[cfg(test)]
use crate::config::Config;

#[cfg(test)]
use crate::logger::{Colors, Logger};

#[cfg(test)]
use crate::pipeline::Timer;

#[cfg(test)]
use streaming_line_reader::MAX_BUFFER_SIZE;

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
#[cfg(test)]
const MAX_PROMPT_SIZE: usize = 200 * 1024; // 200KB

/// Truncate a prompt that exceeds the safe size limit.
///
/// This function intelligently truncates prompts by:
/// 1. Looking for `{{LAST_OUTPUT}}` marker sections (from XSD retry templates)
/// 2. Truncating from the beginning of LAST_OUTPUT content (keeping the end)
/// 3. If no marker found, truncating from the middle to preserve start/end context
///
/// Returns the original prompt if within limits, or a truncated version with a marker.
#[cfg(test)]
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

use super::types::CommandResult;
use types::{PromptArchiveInfo, PromptSaveOptions};

/// Waits for process completion and collects stderr output.
///
/// Uses try_wait() with polling to avoid holding the child mutex during blocking wait(),
/// which could deadlock with the monitor thread trying to kill the process.
fn wait_for_completion_and_collect_stderr(
    child_arc: Arc<std::sync::Mutex<Box<dyn crate::executor::AgentChild>>>,
    stderr_join_handle: &mut Option<std::thread::JoinHandle<io::Result<String>>>,
    monitor_handle: &mut Option<
        std::thread::JoinHandle<crate::pipeline::idle_timeout::MonitorResult>,
    >,
    runtime: &PipelineRuntime<'_>,
) -> io::Result<(
    i32,
    String,
    Option<crate::pipeline::idle_timeout::MonitorResult>,
)> {
    use std::time::Duration;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum WaitOutcome {
        Completed(std::process::ExitStatus),
        TimedOut(crate::pipeline::idle_timeout::MonitorResult),
    }

    fn try_take_monitor_result(
        monitor_handle: &mut Option<
            std::thread::JoinHandle<crate::pipeline::idle_timeout::MonitorResult>,
        >,
    ) -> Option<crate::pipeline::idle_timeout::MonitorResult> {
        let finished = match monitor_handle.as_ref() {
            Some(h) => h.is_finished(),
            None => false,
        };
        if !finished {
            return None;
        }

        monitor_handle.take().and_then(|h| h.join().ok())
    }

    fn try_take_stderr_output(
        stderr_join_handle: &mut Option<std::thread::JoinHandle<io::Result<String>>>,
        runtime: &PipelineRuntime<'_>,
    ) -> String {
        let finished = match stderr_join_handle.as_ref() {
            Some(h) => h.is_finished(),
            None => false,
        };
        if !finished {
            return String::new();
        }

        match stderr_join_handle.take() {
            Some(handle) => match handle.join() {
                Ok(result) => result.unwrap_or_else(|e| {
                    runtime
                        .logger
                        .warn(&format!("Stderr collection failed after timeout: {e}"));
                    String::new()
                }),
                Err(_) => String::new(),
            },
            None => String::new(),
        }
    }

    // Poll for process completion without holding the lock continuously.
    // This allows the monitor thread to acquire the lock to kill the process.
    let check_interval = Duration::from_millis(100);
    let outcome = loop {
        if let Some(monitor_result) = try_take_monitor_result(monitor_handle) {
            if matches!(
                monitor_result,
                crate::pipeline::idle_timeout::MonitorResult::TimedOut { .. }
            ) {
                break WaitOutcome::TimedOut(monitor_result);
            }
        }

        let mut child = child_arc.lock().unwrap();
        match child.try_wait()? {
            Some(status) => break WaitOutcome::Completed(status),
            None => {
                // Release lock before sleeping to allow monitor thread to kill process
                drop(child);
                std::thread::sleep(check_interval);
            }
        }
    };

    let status = match outcome {
        WaitOutcome::Completed(status) => status,
        WaitOutcome::TimedOut(monitor_result) => {
            // The monitor has decided this run timed out. Avoid waiting indefinitely for
            // an observable exit; return control to the caller.
            let stderr_output = try_take_stderr_output(stderr_join_handle, runtime);
            return Ok((SIGTERM_EXIT_CODE, stderr_output, Some(monitor_result)));
        }
    };

    let exit_code = status.code().unwrap_or(1);

    if status.code().is_none() && runtime.config.verbosity.is_debug() {
        runtime
            .logger
            .warn("Process terminated by signal (no exit code), treating as failure");
    }

    let stderr_output = match stderr_join_handle.take() {
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

    Ok((exit_code, stderr_output, None))
}

fn terminate_child_best_effort(
    child_arc: &Arc<std::sync::Mutex<Box<dyn crate::executor::AgentChild>>>,
    executor: &dyn crate::executor::ProcessExecutor,
    kill_config: crate::pipeline::idle_timeout::KillConfig,
) {
    use std::time::Instant;

    let pid = {
        let locked_child = child_arc.lock().unwrap();
        locked_child.id()
    };

    #[cfg(unix)]
    {
        let pid_str = pid.to_string();
        let _ = executor.execute("kill", &["-TERM", &pid_str], &[], None);

        let term_deadline = Instant::now() + kill_config.sigterm_grace();
        while Instant::now() < term_deadline {
            let status = {
                let mut locked_child = child_arc.lock().unwrap();
                locked_child.try_wait()
            };

            match status {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => std::thread::sleep(kill_config.poll_interval()),
            }
        }

        let _ = executor.execute("kill", &["-KILL", &pid_str], &[], None);
        let kill_deadline = Instant::now() + kill_config.sigkill_confirm_timeout();
        while Instant::now() < kill_deadline {
            let status = {
                let mut locked_child = child_arc.lock().unwrap();
                locked_child.try_wait()
            };

            match status {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => std::thread::sleep(kill_config.poll_interval()),
            }
        }
    }

    #[cfg(windows)]
    {
        let pid_str = pid.to_string();
        let _ = executor.execute("taskkill", &["/F", "/PID", &pid_str], &[], None);

        let deadline = Instant::now() + kill_config.sigkill_confirm_timeout();
        while Instant::now() < deadline {
            let status = {
                let mut locked_child = child_arc.lock().unwrap();
                locked_child.try_wait()
            };

            match status {
                Ok(Some(_)) | Err(_) => return,
                Ok(None) => std::thread::sleep(kill_config.poll_interval()),
            }
        }
    }
}

fn cleanup_after_agent_failure(
    child_arc: &Arc<std::sync::Mutex<Box<dyn crate::executor::AgentChild>>>,
    monitor_should_stop: &Arc<std::sync::atomic::AtomicBool>,
    monitor_handle: &mut Option<
        std::thread::JoinHandle<crate::pipeline::idle_timeout::MonitorResult>,
    >,
    stderr_join_handle: &mut Option<std::thread::JoinHandle<io::Result<String>>>,
    executor: &dyn crate::executor::ProcessExecutor,
    kill_config: crate::pipeline::idle_timeout::KillConfig,
) {
    use std::sync::atomic::Ordering;

    // Ensure the child isn't left running when we abort early.
    // Keep the idle-timeout monitor alive during this best-effort termination so it can
    // provide redundant enforcement if termination is slow or fails.
    terminate_child_best_effort(child_arc, executor, kill_config);

    // After we've attempted termination, stop the monitor so it can't fire later.
    monitor_should_stop.store(true, Ordering::Release);

    // Join stderr collector so it doesn't outlive the caller.
    if let Some(handle) = stderr_join_handle.take() {
        let _ = handle.join();
    }

    // Join monitor to prevent delayed kills after we returned an error.
    if let Some(handle) = monitor_handle.take() {
        let _ = handle.join();
    }
}

fn collect_stderr_with_cap_and_drain<R: Read>(
    mut reader: R,
    max_bytes: usize,
) -> io::Result<String> {
    let mut buf = [0u8; 8192];
    let mut collected = Vec::<u8>::new();
    let mut truncated = false;

    loop {
        let n = reader.read(&mut buf)?;
        if n == 0 {
            break;
        }

        if collected.len() < max_bytes {
            let remaining = max_bytes - collected.len();
            let to_take = remaining.min(n);
            collected.extend_from_slice(&buf[..to_take]);
            if to_take < n {
                truncated = true;
            }
        } else {
            truncated = true;
        }
        // Always continue reading to EOF.
        // If we stop reading after reaching max_bytes, the stderr pipe can fill and
        // block the subprocess, causing a self-inflicted idle timeout.
    }

    let mut stderr_output = String::from_utf8_lossy(&collected).into_owned();
    if truncated {
        if !stderr_output.ends_with('\n') {
            stderr_output.push('\n');
        }
        stderr_output.push_str("<stderr truncated>");
    }

    Ok(stderr_output)
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

    let options = PromptSaveOptions {
        archive_info: Some(PromptArchiveInfo {
            phase_label: cmd.label,
            agent_name: cmd.display_name,
            log_prefix: cmd.log_prefix,
            model_index: cmd.model_index,
            attempt: cmd.attempt,
        }),
        interactive: runtime.config.behavior.interactive,
        colors: *runtime.colors,
    };

    save::save_prompt_to_file_and_clipboard(
        cmd.prompt,
        &runtime.config.prompt_path,
        options,
        runtime.logger,
        runtime.executor,
        runtime.workspace,
    )?;

    // Use ProcessExecutor for agent spawning
    // In production: spawns real process via RealProcessExecutor
    // In tests: uses mock result via MockProcessExecutor
    run_with_agent_spawn(cmd, runtime, ANTHROPIC_ENV_VARS_TO_SANITIZE)
}

/// Exit code returned when a process is killed due to SIGTERM.
const SIGTERM_EXIT_CODE: i32 = 143;

/// Run agent using ProcessExecutor.spawn_agent().
///
/// This function uses the ProcessExecutor trait to spawn agents,
/// allowing real process spawning in production and mock results in tests.
fn run_with_agent_spawn(
    cmd: &PromptCommand<'_>,
    runtime: &mut PipelineRuntime<'_>,
    anthropic_env_vars_to_sanitize: &[&str],
) -> io::Result<CommandResult> {
    use std::sync::atomic::{AtomicBool, Ordering};

    // Build spawn config (not a Command object!)
    let argv = split_command(cmd.cmd_str)?;
    if argv.is_empty() || cmd.cmd_str.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Agent command is empty or contains only whitespace",
        ));
    }

    let mut argv_for_log = argv.clone();
    argv_for_log.push("<PROMPT>".to_string());
    let display_cmd = truncate_text(&format_argv_for_log(&argv_for_log), 160);
    runtime.logger.info(&format!(
        "Executing: {}{}{}",
        runtime.colors.dim(),
        display_cmd,
        runtime.colors.reset()
    ));

    // GLM-specific debug logging (only for CCS/Claude-based GLM)
    let is_glm_cmd = is_glm_like_agent(cmd.cmd_str);
    if is_glm_cmd {
        runtime
            .logger
            .info(&format!("GLM command details: {display_cmd}"));
        if argv.iter().any(|arg| arg == "-p") {
            runtime
                .logger
                .info("GLM command includes '-p' flag (correct)");
        } else {
            runtime.logger.warn("GLM command may be missing '-p' flag");
        }
    }

    let _uses_json = cmd.parser_type != JsonParserType::Generic || argv_requests_json(&argv);
    runtime
        .logger
        .info(&format!("Using {} parser...", cmd.parser_type));

    // Create log file using workspace
    let logfile_path = Path::new(cmd.logfile);
    if let Some(parent) = logfile_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        runtime.workspace.create_dir_all(parent)?;
    }
    runtime.workspace.write(logfile_path, "")?;

    // Build sanitized environment map
    let mut complete_env: std::collections::HashMap<String, String> = std::env::vars().collect();
    for (key, value) in cmd.env_vars.iter() {
        complete_env.insert(key.clone(), value.clone());
    }
    sanitize_command_env(
        &mut complete_env,
        cmd.env_vars,
        anthropic_env_vars_to_sanitize,
    );

    // Build spawn config for ProcessExecutor
    let config = crate::executor::AgentSpawnConfig {
        command: argv[0].clone(),
        args: argv[1..].to_vec(),
        env: complete_env,
        prompt: cmd.prompt.to_string(),
        logfile: cmd.logfile.to_string(),
        parser_type: cmd.parser_type,
    };

    // Use ProcessExecutor - spawns real process in prod, mocks in test
    let agent_handle = match runtime.executor.spawn_agent(&config) {
        Ok(handle) => handle,
        Err(e) => {
            // Convert spawn errors to CommandResult so fallback can handle them.
            // This prevents spawn failures from crashing the entire pipeline.
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

            return Ok(CommandResult {
                exit_code,
                stderr: format!("{}: {} - {}", argv[0], detail, e),
                session_id: None,
            });
        }
    };

    // Extract stdout and stderr from the handle
    let stdout = agent_handle.stdout;
    let stderr = agent_handle.stderr;
    let inner = agent_handle.inner;

    // Wrap child in Arc<Mutex> for shared access between monitor and main thread
    let child_shared = Arc::new(std::sync::Mutex::new(inner));
    let child_for_monitor = Arc::clone(&child_shared);

    // Set up idle timeout monitoring
    let activity_timestamp = new_activity_timestamp();
    let monitor_should_stop = Arc::new(AtomicBool::new(false));
    let monitor_should_stop_clone = monitor_should_stop.clone();
    let activity_timestamp_clone = activity_timestamp.clone();

    // Create executor for monitor thread to kill the subprocess if needed
    // Use the Arc-wrapped executor from runtime to support mocking in tests
    let monitor_executor: Arc<dyn crate::executor::ProcessExecutor> =
        std::sync::Arc::clone(&runtime.executor_arc);

    // Spawn idle timeout monitor thread with child reference
    let mut monitor_handle = Some(std::thread::spawn(move || {
        monitor_idle_timeout(
            activity_timestamp_clone,
            child_for_monitor,
            IDLE_TIMEOUT_SECS,
            monitor_should_stop_clone,
            monitor_executor,
        )
    }));

    // Clone activity timestamp for stderr thread to share with stdout tracking.
    // This ensures both stdout AND stderr activity prevent idle timeout kills.
    let stderr_activity_timestamp = activity_timestamp.clone();

    // Spawn stderr collection thread with activity tracking
    let mut stderr_join_handle = Some(std::thread::spawn(move || -> io::Result<String> {
        const STDERR_MAX_BYTES: usize = 512 * 1024;

        // Wrap stderr with activity tracking to prevent idle timeout when
        // agents produce verbose stderr output while processing.
        let tracked_stderr = StderrActivityTracker::new(stderr, stderr_activity_timestamp);
        let reader = BufReader::new(tracked_stderr);
        collect_stderr_with_cap_and_drain(reader, STDERR_MAX_BYTES)
    }));

    // Clone activity_timestamp before passing it to stream function,
    // so we can use it later for the timeout diagnostic message.
    let activity_timestamp_for_timeout = activity_timestamp.clone();

    // Stream agent output using the handle
    if let Err(e) =
        streaming::stream_agent_output_from_handle(stdout, cmd, runtime, activity_timestamp)
    {
        cleanup_after_agent_failure(
            &child_shared,
            &monitor_should_stop,
            &mut monitor_handle,
            &mut stderr_join_handle,
            runtime.executor_arc.as_ref(),
            crate::pipeline::idle_timeout::DEFAULT_KILL_CONFIG,
        );
        return Err(e);
    }

    // After streaming completes, wait for child
    // Pass the Arc to allow lock release between try_wait() polls
    let (exit_code, stderr_output, monitor_result_early) =
        match wait_for_completion_and_collect_stderr(
            Arc::clone(&child_shared),
            &mut stderr_join_handle,
            &mut monitor_handle,
            runtime,
        ) {
            Ok(v) => v,
            Err(e) => {
                cleanup_after_agent_failure(
                    &child_shared,
                    &monitor_should_stop,
                    &mut monitor_handle,
                    &mut stderr_join_handle,
                    runtime.executor_arc.as_ref(),
                    crate::pipeline::idle_timeout::DEFAULT_KILL_CONFIG,
                );
                return Err(e);
            }
        };

    // If the monitor timed out but the child never became observable as exited, we
    // intentionally stop waiting here to guarantee the pipeline regains control.
    if matches!(monitor_result_early, Some(MonitorResult::TimedOut { .. })) {
        terminate_child_best_effort(
            &child_shared,
            runtime.executor_arc.as_ref(),
            crate::pipeline::idle_timeout::DEFAULT_KILL_CONFIG,
        );

        // Avoid blocking on stderr thread in this extreme case.
        if let Some(handle) = stderr_join_handle.as_ref() {
            if handle.is_finished() {
                let _ = stderr_join_handle.take().and_then(|h| h.join().ok());
            } else {
                let _ = stderr_join_handle.take();
            }
        }
    }

    // Signal monitor to stop only after the child has exited.
    // If stdout closes early but the child remains running, the monitor must
    // continue enforcing the idle timeout through process termination.
    monitor_should_stop.store(true, Ordering::Release);

    // Check if monitor killed the process due to idle timeout
    let monitor_result = monitor_result_early
        .or_else(|| monitor_handle.take().and_then(|handle| handle.join().ok()))
        .unwrap_or(MonitorResult::ProcessCompleted);

    // Handle timeout with escalation diagnostics
    let final_exit_code = match monitor_result {
        MonitorResult::TimedOut { escalated } => {
            let idle_duration = time_since_activity(&activity_timestamp_for_timeout);
            let escalation_msg = if escalated {
                if cfg!(windows) {
                    ", force killed (taskkill /F)"
                } else {
                    ", escalated to SIGKILL after SIGTERM grace period"
                }
            } else {
                ""
            };
            runtime.logger.warn(&format!(
                "Agent killed due to idle timeout (no stdout/stderr for {} seconds, \
                 last activity {:.1}s ago, process exit code was {}{}, \
                 kill reason: IDLE_TIMEOUT_MONITOR)",
                IDLE_TIMEOUT_SECS,
                idle_duration.as_secs_f64(),
                exit_code,
                escalation_msg
            ));
            SIGTERM_EXIT_CODE
        }
        MonitorResult::ProcessCompleted => exit_code,
    };

    if runtime.config.verbosity.is_verbose() {
        runtime.logger.info(&format!(
            "Phase elapsed: {}",
            runtime.timer.phase_elapsed_formatted()
        ));
    }

    // Extract session_id from the log file (first init event in agent output)
    let session_id = streaming::extract_session_id_from_logfile(cmd.logfile, runtime.workspace);

    Ok(CommandResult {
        exit_code: final_exit_code,
        stderr: stderr_output,
        session_id,
    })
}

#[cfg(test)]
fn run_with_agent_spawn_with_monitor_config(
    cmd: &PromptCommand<'_>,
    runtime: &mut PipelineRuntime<'_>,
    anthropic_env_vars_to_sanitize: &[&str],
    idle_timeout_secs: u64,
    monitor_check_interval: std::time::Duration,
    kill_config: crate::pipeline::idle_timeout::KillConfig,
) -> io::Result<CommandResult> {
    use std::sync::atomic::{AtomicBool, Ordering};

    // Build spawn config (not a Command object!)
    let argv = split_command(cmd.cmd_str)?;
    if argv.is_empty() || cmd.cmd_str.trim().is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Agent command is empty or contains only whitespace",
        ));
    }

    let mut argv_for_log = argv.clone();
    argv_for_log.push("<PROMPT>".to_string());
    let display_cmd = truncate_text(&format_argv_for_log(&argv_for_log), 160);
    runtime.logger.info(&format!(
        "Executing: {}{}{}",
        runtime.colors.dim(),
        display_cmd,
        runtime.colors.reset()
    ));

    // Create log file using workspace
    let logfile_path = Path::new(cmd.logfile);
    if let Some(parent) = logfile_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        runtime.workspace.create_dir_all(parent)?;
    }
    runtime.workspace.write(logfile_path, "")?;

    // Build sanitized environment map
    let mut complete_env: std::collections::HashMap<String, String> = std::env::vars().collect();
    for (key, value) in cmd.env_vars.iter() {
        complete_env.insert(key.clone(), value.clone());
    }
    sanitize_command_env(
        &mut complete_env,
        cmd.env_vars,
        anthropic_env_vars_to_sanitize,
    );

    // Build spawn config for ProcessExecutor
    let config = crate::executor::AgentSpawnConfig {
        command: argv[0].clone(),
        args: argv[1..].to_vec(),
        env: complete_env,
        prompt: cmd.prompt.to_string(),
        logfile: cmd.logfile.to_string(),
        parser_type: cmd.parser_type,
    };

    // Use ProcessExecutor - spawns real process in prod, mocks in test
    let agent_handle = match runtime.executor.spawn_agent(&config) {
        Ok(handle) => handle,
        Err(e) => {
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

            return Ok(CommandResult {
                exit_code,
                stderr: format!("{}: {} - {}", argv[0], detail, e),
                session_id: None,
            });
        }
    };

    let stdout = agent_handle.stdout;
    let stderr = agent_handle.stderr;
    let inner = agent_handle.inner;

    let child_shared = Arc::new(std::sync::Mutex::new(inner));
    let child_for_monitor = Arc::clone(&child_shared);

    let activity_timestamp = new_activity_timestamp();
    let monitor_should_stop = Arc::new(AtomicBool::new(false));
    let monitor_should_stop_clone = monitor_should_stop.clone();
    let activity_timestamp_clone = activity_timestamp.clone();

    let monitor_executor: Arc<dyn crate::executor::ProcessExecutor> =
        std::sync::Arc::clone(&runtime.executor_arc);

    let mut monitor_handle = Some(std::thread::spawn(move || {
        crate::pipeline::idle_timeout::monitor_idle_timeout_with_interval_and_kill_config(
            activity_timestamp_clone,
            child_for_monitor,
            idle_timeout_secs,
            monitor_should_stop_clone,
            monitor_executor,
            monitor_check_interval,
            kill_config,
        )
    }));

    let stderr_activity_timestamp = activity_timestamp.clone();
    let mut stderr_join_handle = Some(std::thread::spawn(move || -> io::Result<String> {
        const STDERR_MAX_BYTES: usize = 512 * 1024;
        let tracked_stderr = StderrActivityTracker::new(stderr, stderr_activity_timestamp);
        let reader = BufReader::new(tracked_stderr);
        collect_stderr_with_cap_and_drain(reader, STDERR_MAX_BYTES)
    }));

    let activity_timestamp_for_timeout = activity_timestamp.clone();
    if let Err(e) =
        streaming::stream_agent_output_from_handle(stdout, cmd, runtime, activity_timestamp)
    {
        cleanup_after_agent_failure(
            &child_shared,
            &monitor_should_stop,
            &mut monitor_handle,
            &mut stderr_join_handle,
            runtime.executor_arc.as_ref(),
            kill_config,
        );
        return Err(e);
    }

    let (exit_code, stderr_output, monitor_result_early) =
        match wait_for_completion_and_collect_stderr(
            Arc::clone(&child_shared),
            &mut stderr_join_handle,
            &mut monitor_handle,
            runtime,
        ) {
            Ok(v) => v,
            Err(e) => {
                cleanup_after_agent_failure(
                    &child_shared,
                    &monitor_should_stop,
                    &mut monitor_handle,
                    &mut stderr_join_handle,
                    runtime.executor_arc.as_ref(),
                    kill_config,
                );
                return Err(e);
            }
        };

    if matches!(
        monitor_result_early,
        Some(crate::pipeline::idle_timeout::MonitorResult::TimedOut { .. })
    ) {
        terminate_child_best_effort(&child_shared, runtime.executor_arc.as_ref(), kill_config);

        if let Some(handle) = stderr_join_handle.as_ref() {
            if handle.is_finished() {
                let _ = stderr_join_handle.take().and_then(|h| h.join().ok());
            } else {
                let _ = stderr_join_handle.take();
            }
        }
    }

    // Signal monitor to stop only after the child has exited.
    monitor_should_stop.store(true, Ordering::Release);

    let monitor_result = monitor_result_early
        .or_else(|| monitor_handle.take().and_then(|handle| handle.join().ok()))
        .unwrap_or(crate::pipeline::idle_timeout::MonitorResult::ProcessCompleted);

    let final_exit_code = match monitor_result {
        MonitorResult::TimedOut { escalated } => {
            let idle_duration = time_since_activity(&activity_timestamp_for_timeout);
            let escalation_msg = if escalated {
                if cfg!(windows) {
                    ", force killed (taskkill /F)"
                } else {
                    ", escalated to SIGKILL after SIGTERM grace period"
                }
            } else {
                ""
            };
            runtime.logger.warn(&format!(
                "Agent killed due to idle timeout (no stdout/stderr for {} seconds, \
                 last activity {:.1}s ago, process exit code was {}{}, \
                 kill reason: IDLE_TIMEOUT_MONITOR)",
                idle_timeout_secs,
                idle_duration.as_secs_f64(),
                exit_code,
                escalation_msg
            ));
            SIGTERM_EXIT_CODE
        }
        MonitorResult::ProcessCompleted => exit_code,
    };

    let session_id = streaming::extract_session_id_from_logfile(cmd.logfile, runtime.workspace);

    Ok(CommandResult {
        exit_code: final_exit_code,
        stderr: stderr_output,
        session_id,
    })
}

#[cfg(test)]
#[path = "prompt/tests.rs"]
mod tests;

#[cfg(test)]
#[path = "prompt/sanitize_env_tests.rs"]
mod sanitize_env_tests;
