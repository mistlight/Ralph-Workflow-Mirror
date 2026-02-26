//! Tests for monitor integration with file activity detection.

use super::super::monitor::MonitorConfig;
use super::super::*;
use crate::executor::{AgentChild, MockAgentChild, MockProcessExecutor};
use crate::workspace::MemoryWorkspace;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread;
use std::time::{Duration, SystemTime};

#[test]
fn monitor_prevents_timeout_with_file_activity() {
    // Setup: Process with no stdout output but files being written
    let timestamp = new_activity_timestamp();
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    // Create workspace with a recently modified file
    let workspace = MemoryWorkspace::new_test().with_file(".agent/PLAN.md", "# Progress");
    let workspace_arc: Arc<dyn crate::workspace::Workspace> = Arc::new(workspace);

    let file_activity_config = Some(FileActivityConfig {
        tracker: new_file_activity_tracker(),
        workspace: Arc::clone(&workspace_arc),
    });

    let mock_child = MockAgentChild::new(0);
    let child = Arc::new(Mutex::new(Box::new(mock_child) as Box<dyn AgentChild>));

    let executor: Arc<dyn crate::executor::ProcessExecutor> = Arc::new(MockProcessExecutor::new());

    // Set a very short timeout (1 second) but fast check interval
    let config = MonitorConfig {
        timeout_secs: 1,
        check_interval: Duration::from_millis(50),
        kill_config: DEFAULT_KILL_CONFIG,
    };

    let handle = thread::spawn(move || {
        monitor_idle_timeout_with_interval_and_kill_config(
            timestamp,
            file_activity_config,
            child,
            should_stop_clone,
            executor,
            config,
        )
    });

    // Wait longer than timeout (1.5 seconds)
    thread::sleep(Duration::from_millis(1500));

    // Signal monitor to stop
    should_stop.store(true, Ordering::Release);

    let result = handle.join().expect("Monitor thread panicked");

    // Should complete normally because file activity prevented timeout
    assert_eq!(
        result,
        MonitorResult::ProcessCompleted,
        "Monitor should not timeout when files are being updated"
    );
}

#[test]
fn monitor_times_out_without_any_activity() {
    // Setup: Process with no output and no file changes
    let timestamp = new_activity_timestamp();
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    // Create workspace with no AI-generated files (only logs)
    let workspace = MemoryWorkspace::new_test().with_file(".agent/pipeline.log", "logs");
    let workspace_arc: Arc<dyn crate::workspace::Workspace> = Arc::new(workspace);

    let file_activity_config = Some(FileActivityConfig {
        tracker: new_file_activity_tracker(),
        workspace: Arc::clone(&workspace_arc),
    });

    let (mock_child, _controller) = MockAgentChild::new_running(0);
    let child = Arc::new(Mutex::new(Box::new(mock_child) as Box<dyn AgentChild>));

    let executor: Arc<dyn crate::executor::ProcessExecutor> = Arc::new(MockProcessExecutor::new());

    // Set a very short timeout for fast test execution
    let config = MonitorConfig {
        timeout_secs: 1,
        check_interval: Duration::from_millis(50),
        kill_config: DEFAULT_KILL_CONFIG,
    };

    let handle = thread::spawn(move || {
        monitor_idle_timeout_with_interval_and_kill_config(
            timestamp,
            file_activity_config,
            child,
            should_stop_clone,
            executor,
            config,
        )
    });

    let result = handle.join().expect("Monitor thread panicked");

    // Should timeout because no activity (stdout or files)
    assert!(
        matches!(result, MonitorResult::TimedOut { .. }),
        "Monitor should timeout when there's no activity"
    );
}

#[test]
fn monitor_respects_output_activity() {
    // Setup: Process with stdout activity (existing behavior)
    let timestamp = new_activity_timestamp();
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    // No file activity config
    let file_activity_config = None;

    let mock_child = MockAgentChild::new(0);
    let child = Arc::new(Mutex::new(Box::new(mock_child) as Box<dyn AgentChild>));

    let executor: Arc<dyn crate::executor::ProcessExecutor> = Arc::new(MockProcessExecutor::new());

    // Set a short timeout
    let config = MonitorConfig {
        timeout_secs: 1,
        check_interval: Duration::from_millis(50),
        kill_config: DEFAULT_KILL_CONFIG,
    };

    // Update activity timestamp periodically to simulate stdout
    let timestamp_clone = timestamp.clone();
    let update_handle = thread::spawn(move || {
        for _ in 0..30 {
            thread::sleep(Duration::from_millis(50));
            touch_activity(&timestamp_clone);
        }
    });

    let handle = thread::spawn(move || {
        monitor_idle_timeout_with_interval_and_kill_config(
            timestamp,
            file_activity_config,
            child,
            should_stop_clone,
            executor,
            config,
        )
    });

    // Wait longer than timeout
    thread::sleep(Duration::from_millis(1500));
    should_stop.store(true, Ordering::Release);

    let result = handle.join().expect("Monitor thread panicked");
    update_handle.join().expect("Update thread panicked");

    // Should complete normally because stdout activity prevented timeout
    assert_eq!(
        result,
        MonitorResult::ProcessCompleted,
        "Monitor should not timeout when stdout activity is present"
    );
}

#[test]
fn monitor_uses_configurable_check_interval() {
    // Setup: Verify that custom check intervals are respected
    let timestamp = new_activity_timestamp();
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    let mock_child = MockAgentChild::new(0);
    let child = Arc::new(Mutex::new(Box::new(mock_child) as Box<dyn AgentChild>));

    let executor: Arc<dyn crate::executor::ProcessExecutor> = Arc::new(MockProcessExecutor::new());

    // Use a custom check interval (30 seconds as per spec)
    let config = MonitorConfig {
        timeout_secs: 60,
        check_interval: Duration::from_secs(30),
        kill_config: DEFAULT_KILL_CONFIG,
    };

    let handle = thread::spawn(move || {
        monitor_idle_timeout_with_interval_and_kill_config(
            timestamp,
            None, // No file activity
            child,
            should_stop_clone,
            executor,
            config,
        )
    });

    // Signal stop quickly
    thread::sleep(Duration::from_millis(50));
    should_stop.store(true, Ordering::Release);

    let result = handle.join().expect("Monitor thread panicked");

    // Should complete normally
    assert_eq!(result, MonitorResult::ProcessCompleted);
}

#[test]
fn monitor_file_activity_with_old_files_times_out() {
    // Setup: Files exist but are old (>timeout window)
    let timestamp = new_activity_timestamp();
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    // Create workspace with old file (400 seconds ago, beyond 300s timeout)
    let old_time = SystemTime::now() - Duration::from_secs(400);
    let workspace =
        MemoryWorkspace::new_test().with_file_at_time(".agent/PLAN.md", "old", old_time);
    let workspace_arc: Arc<dyn crate::workspace::Workspace> = Arc::new(workspace);

    let file_activity_config = Some(FileActivityConfig {
        tracker: new_file_activity_tracker(),
        workspace: Arc::clone(&workspace_arc),
    });

    let (mock_child, _controller) = MockAgentChild::new_running(0);
    let child = Arc::new(Mutex::new(Box::new(mock_child) as Box<dyn AgentChild>));

    let executor: Arc<dyn crate::executor::ProcessExecutor> = Arc::new(MockProcessExecutor::new());

    // Set a short timeout for fast test
    let config = MonitorConfig {
        timeout_secs: 1,
        check_interval: Duration::from_millis(50),
        kill_config: DEFAULT_KILL_CONFIG,
    };

    let handle = thread::spawn(move || {
        monitor_idle_timeout_with_interval_and_kill_config(
            timestamp,
            file_activity_config,
            child,
            should_stop_clone,
            executor,
            config,
        )
    });

    let result = handle.join().expect("Monitor thread panicked");

    // Should timeout because file is too old
    assert!(
        matches!(result, MonitorResult::TimedOut { .. }),
        "Monitor should timeout when files are too old"
    );
}

#[test]
fn monitor_without_file_activity_config_works() {
    // Ensure backward compatibility: monitor works without file activity config
    let timestamp = new_activity_timestamp();
    let should_stop = Arc::new(AtomicBool::new(false));
    let should_stop_clone = Arc::clone(&should_stop);

    let mock_child = MockAgentChild::new(0);
    let child = Arc::new(Mutex::new(Box::new(mock_child) as Box<dyn AgentChild>));

    let executor: Arc<dyn crate::executor::ProcessExecutor> = Arc::new(MockProcessExecutor::new());

    let config = MonitorConfig {
        timeout_secs: 60,
        check_interval: Duration::from_millis(10),
        kill_config: DEFAULT_KILL_CONFIG,
    };

    let handle = thread::spawn(move || {
        monitor_idle_timeout_with_interval_and_kill_config(
            timestamp,
            None, // No file activity config
            child,
            should_stop_clone,
            executor,
            config,
        )
    });

    thread::sleep(Duration::from_millis(50));
    should_stop.store(true, Ordering::Release);

    let result = handle.join().expect("Monitor thread panicked");

    assert_eq!(
        result,
        MonitorResult::ProcessCompleted,
        "Monitor should work without file activity config"
    );
}
