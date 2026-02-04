use crate::agents::{is_glm_like_agent, JsonParserType};
use crate::common::{format_argv_for_log, split_command, truncate_text};
use crate::logger::argv_requests_json;
use crate::pipeline::idle_timeout::{
    monitor_idle_timeout, new_activity_timestamp, time_since_activity, MonitorResult,
    StderrActivityTracker, IDLE_TIMEOUT_SECS,
};
use crate::pipeline::types::CommandResult;
use std::io::{self, BufReader};
use std::path::Path;
use std::sync::Arc;

use super::types::{PipelineRuntime, PromptCommand};

pub(super) fn run_with_agent_spawn(
    cmd: &PromptCommand<'_>,
    runtime: &mut PipelineRuntime<'_>,
    anthropic_env_vars_to_sanitize: &[&str],
) -> io::Result<CommandResult> {
    use std::sync::atomic::{AtomicBool, Ordering};

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

    let logfile_path = Path::new(cmd.logfile);
    if let Some(parent) = logfile_path.parent().filter(|p| !p.as_os_str().is_empty()) {
        runtime.workspace.create_dir_all(parent)?;
    }
    runtime.workspace.write(logfile_path, "")?;

    let mut complete_env: std::collections::HashMap<String, String> = std::env::vars().collect();
    for (key, value) in cmd.env_vars.iter() {
        complete_env.insert(key.clone(), value.clone());
    }
    super::environment::sanitize_command_env(
        &mut complete_env,
        cmd.env_vars,
        anthropic_env_vars_to_sanitize,
    );

    let config = crate::executor::AgentSpawnConfig {
        command: argv[0].clone(),
        args: argv[1..].to_vec(),
        env: complete_env,
        prompt: cmd.prompt.to_string(),
        logfile: cmd.logfile.to_string(),
        parser_type: cmd.parser_type,
    };

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
    let monitor_should_stop_clone = Arc::clone(&monitor_should_stop);
    let activity_timestamp_clone = activity_timestamp.clone();

    let monitor_executor: Arc<dyn crate::executor::ProcessExecutor> =
        std::sync::Arc::clone(&runtime.executor_arc);

    let mut monitor_handle = Some(std::thread::spawn(move || {
        monitor_idle_timeout(
            activity_timestamp_clone,
            child_for_monitor,
            IDLE_TIMEOUT_SECS,
            monitor_should_stop_clone,
            monitor_executor,
        )
    }));

    let stderr_activity_timestamp = activity_timestamp.clone();
    let stderr_cancel = Arc::new(AtomicBool::new(false));
    let stderr_cancel_for_thread = Arc::clone(&stderr_cancel);

    let mut stderr_join_handle = Some(std::thread::spawn(move || -> io::Result<String> {
        const STDERR_MAX_BYTES: usize = 512 * 1024;
        let tracked_stderr = StderrActivityTracker::new(stderr, stderr_activity_timestamp);
        let reader = BufReader::new(tracked_stderr);
        super::stderr_collector::collect_stderr_with_cap_and_drain(
            reader,
            STDERR_MAX_BYTES,
            stderr_cancel_for_thread.as_ref(),
        )
    }));

    let activity_timestamp_for_timeout = activity_timestamp.clone();
    if let Err(e) =
        super::streaming::stream_agent_output_from_handle(stdout, cmd, runtime, activity_timestamp)
    {
        super::cleanup::cleanup_after_agent_failure(
            &child_shared,
            &monitor_should_stop,
            &mut monitor_handle,
            &mut stderr_join_handle,
            &stderr_cancel,
            runtime.executor_arc.as_ref(),
            crate::pipeline::idle_timeout::DEFAULT_KILL_CONFIG,
        );
        return Err(e);
    }

    let (exit_code, stderr_output, monitor_result_early) =
        match super::process_wait::wait_for_completion_and_collect_stderr(
            Arc::clone(&child_shared),
            &mut stderr_join_handle,
            &mut monitor_handle,
            runtime,
        ) {
            Ok(v) => v,
            Err(e) => {
                super::cleanup::cleanup_after_agent_failure(
                    &child_shared,
                    &monitor_should_stop,
                    &mut monitor_handle,
                    &mut stderr_join_handle,
                    &stderr_cancel,
                    runtime.executor_arc.as_ref(),
                    crate::pipeline::idle_timeout::DEFAULT_KILL_CONFIG,
                );
                return Err(e);
            }
        };

    if matches!(monitor_result_early, Some(MonitorResult::TimedOut { .. })) {
        super::cleanup::terminate_child_best_effort(
            &child_shared,
            runtime.executor_arc.as_ref(),
            crate::pipeline::idle_timeout::DEFAULT_KILL_CONFIG,
        );

        super::stderr_collector::cancel_and_join_stderr_collector(
            &stderr_cancel,
            &mut stderr_join_handle,
            std::time::Duration::from_millis(250),
        );
    }

    monitor_should_stop.store(true, Ordering::Release);

    let monitor_result = monitor_result_early
        .or_else(|| monitor_handle.take().and_then(|handle| handle.join().ok()))
        .unwrap_or(MonitorResult::ProcessCompleted);

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
            super::SIGTERM_EXIT_CODE
        }
        MonitorResult::ProcessCompleted => exit_code,
    };

    if runtime.config.verbosity.is_verbose() {
        runtime.logger.info(&format!(
            "Phase elapsed: {}",
            runtime.timer.phase_elapsed_formatted()
        ));
    }

    let session_id =
        super::streaming::extract_session_id_from_logfile(cmd.logfile, runtime.workspace);

    Ok(CommandResult {
        exit_code: final_exit_code,
        stderr: stderr_output,
        session_id,
    })
}
