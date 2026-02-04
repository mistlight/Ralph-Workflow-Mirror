use crate::pipeline::idle_timeout::KillConfig;
use std::io;
use std::sync::Arc;

pub(super) fn terminate_child_best_effort(
    child_arc: &Arc<std::sync::Mutex<Box<dyn crate::executor::AgentChild>>>,
    executor: &dyn crate::executor::ProcessExecutor,
    kill_config: KillConfig,
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

pub(super) fn cleanup_after_agent_failure(
    child_arc: &Arc<std::sync::Mutex<Box<dyn crate::executor::AgentChild>>>,
    monitor_should_stop: &Arc<std::sync::atomic::AtomicBool>,
    monitor_handle: &mut Option<
        std::thread::JoinHandle<crate::pipeline::idle_timeout::MonitorResult>,
    >,
    stderr_join_handle: &mut Option<std::thread::JoinHandle<io::Result<String>>>,
    stderr_cancel: &Arc<std::sync::atomic::AtomicBool>,
    executor: &dyn crate::executor::ProcessExecutor,
    kill_config: KillConfig,
) {
    use std::sync::atomic::Ordering;

    terminate_child_best_effort(child_arc, executor, kill_config);
    monitor_should_stop.store(true, Ordering::Release);

    super::stderr_collector::cancel_and_join_stderr_collector(
        stderr_cancel,
        stderr_join_handle,
        std::time::Duration::from_millis(250),
    );

    if let Some(handle) = monitor_handle.take() {
        let _ = handle.join();
    }
}
