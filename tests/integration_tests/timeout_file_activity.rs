//! Integration tests for file-activity-aware idle timeout behavior.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** These tests follow the integration test style guide in
//! **[../INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**.
//! - Tests verify observable timeout behavior
//! - Uses mocked process execution (`MockProcessExecutor`)
//! - Uses in-memory filesystem (`MemoryWorkspace`)

use crate::test_timeout::with_default_timeout;
use ralph_workflow::pipeline::idle_timeout::{
    monitor_idle_timeout_with_interval_and_kill_config, new_activity_timestamp,
    new_file_activity_tracker, FileActivityConfig, KillConfig, MonitorConfig, MonitorResult,
};
use ralph_workflow::workspace::{MemoryWorkspace, Workspace};
use ralph_workflow::{AgentChild, MockAgentChild, MockProcessExecutor, ProcessExecutor};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::Duration;

fn fast_kill_config() -> KillConfig {
    KillConfig::new(
        Duration::from_millis(20),
        Duration::from_millis(5),
        Duration::from_millis(20),
        Duration::from_millis(200),
        Duration::from_millis(20),
    )
}

#[test]
fn active_ai_file_updates_prevent_timeout() {
    with_default_timeout(|| {
        let timestamp = new_activity_timestamp();
        timestamp.store(0, Ordering::Release);

        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_for_monitor = Arc::clone(&should_stop);

        let workspace: Arc<dyn Workspace> =
            Arc::new(MemoryWorkspace::new_test().with_file(".agent/PLAN.md", "# in progress"));

        let file_activity_config = Some(FileActivityConfig {
            tracker: new_file_activity_tracker(),
            workspace,
        });

        let (mock_child, _controller) = MockAgentChild::new_running(0);
        let child = Arc::new(Mutex::new(Box::new(mock_child) as Box<dyn AgentChild>));

        let executor: Arc<dyn ProcessExecutor> = Arc::new(MockProcessExecutor::new());

        let handle = thread::spawn(move || {
            monitor_idle_timeout_with_interval_and_kill_config(
                timestamp,
                file_activity_config,
                child,
                should_stop_for_monitor,
                executor,
                MonitorConfig {
                    timeout_secs: 1,
                    check_interval: Duration::from_millis(10),
                    kill_config: fast_kill_config(),
                },
            )
        });

        thread::sleep(Duration::from_millis(200));
        should_stop.store(true, Ordering::Release);

        let result = handle.join().expect("monitor thread panicked");
        assert_eq!(
            result,
            MonitorResult::ProcessCompleted,
            "recent PLAN.md updates should keep run active"
        );
    });
}

#[test]
fn log_only_activity_does_not_prevent_timeout() {
    with_default_timeout(|| {
        let timestamp = new_activity_timestamp();
        timestamp.store(0, Ordering::Release);

        let should_stop = Arc::new(AtomicBool::new(false));

        let workspace: Arc<dyn Workspace> =
            Arc::new(MemoryWorkspace::new_test().with_file(".agent/pipeline.log", "log churn"));

        let file_activity_config = Some(FileActivityConfig {
            tracker: new_file_activity_tracker(),
            workspace,
        });

        let (mock_child, controller) = MockAgentChild::new_running(0);
        let child = Arc::new(Mutex::new(Box::new(mock_child) as Box<dyn AgentChild>));

        let executor_impl = Arc::new(MockProcessExecutor::new());
        let executor_dyn: Arc<dyn ProcessExecutor> = executor_impl.clone();

        let monitor_handle = thread::spawn(move || {
            monitor_idle_timeout_with_interval_and_kill_config(
                timestamp,
                file_activity_config,
                child,
                should_stop,
                executor_dyn,
                MonitorConfig {
                    timeout_secs: 1,
                    check_interval: Duration::from_millis(10),
                    kill_config: fast_kill_config(),
                },
            )
        });

        let controller_for_watcher = Arc::clone(&controller);
        let executor_for_watcher = Arc::clone(&executor_impl);
        let watcher = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                let saw_term = executor_for_watcher
                    .execute_calls_for("kill")
                    .iter()
                    .any(|(_, args, _, _)| args.iter().any(|a| a == "-TERM"));
                if saw_term {
                    controller_for_watcher.store(false, Ordering::Release);
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
        });

        let result = monitor_handle.join().expect("monitor thread panicked");
        watcher.join().expect("watcher thread panicked");

        assert!(
            matches!(result, MonitorResult::TimedOut { .. }),
            "log-only updates should still time out"
        );
    });
}

#[test]
fn no_output_and_no_ai_files_times_out() {
    with_default_timeout(|| {
        let timestamp = new_activity_timestamp();
        timestamp.store(0, Ordering::Release);

        let should_stop = Arc::new(AtomicBool::new(false));
        let workspace: Arc<dyn Workspace> = Arc::new(MemoryWorkspace::new_test());

        let file_activity_config = Some(FileActivityConfig {
            tracker: new_file_activity_tracker(),
            workspace,
        });

        let (mock_child, controller) = MockAgentChild::new_running(0);
        let child = Arc::new(Mutex::new(Box::new(mock_child) as Box<dyn AgentChild>));

        let executor_impl = Arc::new(MockProcessExecutor::new());
        let executor_dyn: Arc<dyn ProcessExecutor> = executor_impl.clone();

        let monitor_handle = thread::spawn(move || {
            monitor_idle_timeout_with_interval_and_kill_config(
                timestamp,
                file_activity_config,
                child,
                should_stop,
                executor_dyn,
                MonitorConfig {
                    timeout_secs: 1,
                    check_interval: Duration::from_millis(10),
                    kill_config: fast_kill_config(),
                },
            )
        });

        let controller_for_watcher = Arc::clone(&controller);
        let executor_for_watcher = Arc::clone(&executor_impl);
        let watcher = thread::spawn(move || {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            while std::time::Instant::now() < deadline {
                let saw_term = executor_for_watcher
                    .execute_calls_for("kill")
                    .iter()
                    .any(|(_, args, _, _)| args.iter().any(|a| a == "-TERM"));
                if saw_term {
                    controller_for_watcher.store(false, Ordering::Release);
                    return;
                }
                thread::sleep(Duration::from_millis(5));
            }
        });

        let result = monitor_handle.join().expect("monitor thread panicked");
        watcher.join().expect("watcher thread panicked");

        assert!(
            matches!(result, MonitorResult::TimedOut { .. }),
            "no output and no AI files should time out"
        );
    });
}
