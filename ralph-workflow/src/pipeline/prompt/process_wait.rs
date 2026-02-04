use crate::pipeline::idle_timeout::MonitorResult;
use crate::pipeline::prompt::PipelineRuntime;
use std::io;
use std::sync::Arc;

pub(super) fn wait_for_completion_and_collect_stderr(
    child_arc: Arc<std::sync::Mutex<Box<dyn crate::executor::AgentChild>>>,
    stderr_join_handle: &mut Option<std::thread::JoinHandle<io::Result<String>>>,
    monitor_handle: &mut Option<std::thread::JoinHandle<MonitorResult>>,
    runtime: &PipelineRuntime<'_>,
) -> io::Result<(i32, String, Option<MonitorResult>)> {
    use std::time::Duration;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    enum WaitOutcome {
        Completed(std::process::ExitStatus),
        TimedOut(MonitorResult),
    }

    fn try_take_monitor_result(
        monitor_handle: &mut Option<std::thread::JoinHandle<MonitorResult>>,
    ) -> Option<MonitorResult> {
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

    let check_interval = Duration::from_millis(100);
    let outcome = loop {
        if let Some(monitor_result) = try_take_monitor_result(monitor_handle) {
            if matches!(monitor_result, MonitorResult::TimedOut { .. }) {
                break WaitOutcome::TimedOut(monitor_result);
            }
        }

        let mut child = child_arc.lock().unwrap();
        match child.try_wait()? {
            Some(status) => break WaitOutcome::Completed(status),
            None => {
                drop(child);
                std::thread::sleep(check_interval);
            }
        }
    };

    let status = match outcome {
        WaitOutcome::Completed(status) => status,
        WaitOutcome::TimedOut(monitor_result) => {
            let stderr_output = try_take_stderr_output(stderr_join_handle, runtime);
            return Ok((
                super::SIGTERM_EXIT_CODE,
                stderr_output,
                Some(monitor_result),
            ));
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
