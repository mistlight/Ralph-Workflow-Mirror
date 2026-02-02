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
fn wait_for_completion_and_collect_stderr(
    mut child: Box<dyn crate::executor::AgentChild>,
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

    // Get child PID for idle timeout monitoring
    let child_id = agent_handle.inner.id();

    // Set up idle timeout monitoring
    let activity_timestamp = new_activity_timestamp();
    let monitor_should_stop = Arc::new(AtomicBool::new(false));
    let monitor_should_stop_clone = monitor_should_stop.clone();
    let activity_timestamp_clone = activity_timestamp.clone();

    // Create executor for monitor thread to kill the subprocess if needed
    // Use the Arc-wrapped executor from runtime to support mocking in tests
    let monitor_executor: Arc<dyn crate::executor::ProcessExecutor> =
        std::sync::Arc::clone(&runtime.executor_arc);

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

    // Extract stdout and stderr from the handle
    let stdout = agent_handle.stdout;
    let stderr = agent_handle.stderr;
    let inner = agent_handle.inner;

    // Clone activity timestamp for stderr thread to share with stdout tracking.
    // This ensures both stdout AND stderr activity prevent idle timeout kills.
    let stderr_activity_timestamp = activity_timestamp.clone();

    // Spawn stderr collection thread with activity tracking
    let stderr_join_handle = std::thread::spawn(move || -> io::Result<String> {
        const STDERR_MAX_BYTES: usize = 512 * 1024;

        // Wrap stderr with activity tracking to prevent idle timeout when
        // agents produce verbose stderr output while processing.
        let tracked_stderr = StderrActivityTracker::new(stderr, stderr_activity_timestamp);
        let reader = BufReader::new(tracked_stderr);
        collect_stderr_with_cap_and_drain(reader, STDERR_MAX_BYTES)
    });

    // Clone activity_timestamp before passing it to stream function,
    // so we can use it later for the timeout diagnostic message.
    let activity_timestamp_for_timeout = activity_timestamp.clone();

    // Stream agent output using the handle
    streaming::stream_agent_output_from_handle(stdout, cmd, runtime, activity_timestamp)?;

    // Signal monitor to stop (process completed or streaming ended)
    monitor_should_stop.store(true, Ordering::Release);

    let (exit_code, stderr_output) =
        wait_for_completion_and_collect_stderr(inner, Some(stderr_join_handle), runtime)?;

    // Check if monitor killed the process due to idle timeout
    let monitor_result = monitor_handle
        .join()
        .unwrap_or(MonitorResult::ProcessCompleted);

    // If monitor timed out, use SIGTERM exit code regardless of actual exit code
    // and provide detailed diagnostics for debugging
    let final_exit_code = if monitor_result == MonitorResult::TimedOut {
        let idle_duration = time_since_activity(&activity_timestamp_for_timeout);
        runtime.logger.warn(&format!(
            "Agent killed due to idle timeout (no stdout/stderr for {} seconds, \
             last activity {:.1}s ago, process exit code was {}, \
             kill reason: IDLE_TIMEOUT_MONITOR)",
            IDLE_TIMEOUT_SECS,
            idle_duration.as_secs_f64(),
            exit_code
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

    // Extract session_id from the log file (first init event in agent output)
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
