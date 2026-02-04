// Tests for idle timeout monitoring

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::sync::atomic::AtomicU64;
    use std::thread;

    /// Mock clock for testing time-related behavior without real delays.
    struct MockClock {
        current_ms: AtomicU64,
    }

    impl MockClock {
        fn new(initial_ms: u64) -> Self {
            Self {
                current_ms: AtomicU64::new(initial_ms),
            }
        }

        fn advance(&self, delta_ms: u64) {
            self.current_ms.fetch_add(delta_ms, Ordering::SeqCst);
        }

        #[cfg(test)]
        fn jump_back(&self, delta_ms: u64) {
            self.current_ms.fetch_sub(delta_ms, Ordering::SeqCst);
        }
    }

    impl Clock for MockClock {
        fn now_millis(&self) -> u64 {
            self.current_ms.load(Ordering::SeqCst)
        }
    }

    #[test]
    fn test_new_activity_timestamp_with_clock_starts_at_now() {
        let clock = MockClock::new(1234);
        let timestamp = new_activity_timestamp_with_clock(&clock);
        assert_eq!(
            time_since_activity_with_clock(&timestamp, &clock),
            Duration::ZERO
        );
    }

    #[test]
    fn test_touch_activity_with_clock_resets_elapsed_time() {
        let clock = MockClock::new(10_000);
        let timestamp = new_activity_timestamp_with_clock(&clock);

        clock.advance(50);
        assert_eq!(
            time_since_activity_with_clock(&timestamp, &clock),
            Duration::from_millis(50)
        );

        touch_activity_with_clock(&timestamp, &clock);
        assert_eq!(
            time_since_activity_with_clock(&timestamp, &clock),
            Duration::ZERO
        );
    }

    #[test]
    fn test_is_idle_timeout_exceeded_false_when_recent() {
        let timestamp = new_activity_timestamp();
        // Timeout of 1 second, activity just now
        assert!(!is_idle_timeout_exceeded(&timestamp, 1));
    }

    #[test]
    fn test_is_idle_timeout_exceeded_true_after_timeout() {
        // Use mock clock for deterministic testing
        let clock = MockClock::new(100_000); // Start at 100 seconds
        let timestamp = new_activity_timestamp_with_clock(&clock);

        // Advance clock by 2 seconds without touching activity
        clock.advance(2000);

        // Timeout of 1 second should be exceeded
        assert!(is_idle_timeout_exceeded_with_clock(&timestamp, 1, &clock));
    }

    #[test]
    fn test_is_idle_timeout_exceeded_with_mock_clock() {
        let clock = MockClock::new(0);
        let timestamp = new_activity_timestamp_with_clock(&clock);

        // Initially, no timeout
        assert!(!is_idle_timeout_exceeded_with_clock(&timestamp, 5, &clock));

        // Advance by 3 seconds - still no timeout (threshold is 5)
        clock.advance(3000);
        assert!(!is_idle_timeout_exceeded_with_clock(&timestamp, 5, &clock));

        // Advance by 3 more seconds (total 6) - now timeout
        clock.advance(3000);
        assert!(is_idle_timeout_exceeded_with_clock(&timestamp, 5, &clock));

        // Touch activity resets
        touch_activity_with_clock(&timestamp, &clock);
        assert!(!is_idle_timeout_exceeded_with_clock(&timestamp, 5, &clock));
    }

    #[test]
    fn test_activity_tracking_reader_updates_on_read() {
        let data = b"hello world";
        let cursor = Cursor::new(data.to_vec());
        let timestamp = new_activity_timestamp();

        let mut reader = ActivityTrackingReader::new(cursor, timestamp.clone());

        // Use a sentinel value to avoid time-based assertions.
        timestamp.store(u64::MAX, Ordering::Release);

        // Verify timestamp is at sentinel
        assert_eq!(timestamp.load(Ordering::Acquire), u64::MAX);

        // Read some data
        let mut buf = [0u8; 5];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 5);

        // After read, timestamp should no longer be the sentinel.
        assert_ne!(timestamp.load(Ordering::Acquire), u64::MAX);
    }

    #[test]
    fn test_activity_tracking_reader_no_update_on_zero_read() {
        let data = b"";
        let cursor = Cursor::new(data.to_vec());
        let timestamp = new_activity_timestamp();

        let mut reader = ActivityTrackingReader::new(cursor, timestamp.clone());

        // Set timestamp to 0 to simulate very old activity
        timestamp.store(0, Ordering::Release);

        // Verify timestamp is at 0
        assert_eq!(timestamp.load(Ordering::Acquire), 0);

        // Read (should return 0, EOF)
        let mut buf = [0u8; 5];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 0);

        // After zero-read, timestamp should NOT be updated (still 0)
        assert_eq!(
            timestamp.load(Ordering::Acquire),
            0,
            "After zero-read, timestamp should remain 0"
        );
    }

    #[test]
    fn test_activity_tracking_reader_passes_through_data() {
        let data = b"hello world";
        let cursor = Cursor::new(data.to_vec());
        let timestamp = new_activity_timestamp();

        let mut reader = ActivityTrackingReader::new(cursor, timestamp);

        let mut buf = [0u8; 20];
        let n = reader.read(&mut buf).unwrap();

        assert_eq!(n, 11);
        assert_eq!(&buf[..n], b"hello world");
    }

    #[test]
    fn test_idle_timeout_constant_is_five_minutes() {
        assert_eq!(IDLE_TIMEOUT_SECS, 300);
    }

    #[test]
    fn test_monitor_result_variants() {
        // Ensure MonitorResult variants exist and are distinct
        assert_ne!(
            MonitorResult::ProcessCompleted,
            MonitorResult::TimedOut { escalated: false }
        );
        assert_ne!(
            MonitorResult::ProcessCompleted,
            MonitorResult::TimedOut { escalated: true }
        );
        assert_ne!(
            MonitorResult::TimedOut { escalated: false },
            MonitorResult::TimedOut { escalated: true }
        );
    }

    #[test]
    fn test_monitor_stops_when_signaled() {
        use crate::executor::MockAgentChild;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex};

        let timestamp = new_activity_timestamp();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = should_stop.clone();

        // Create mock child
        let mock_child = MockAgentChild::new(0);
        let child = Arc::new(Mutex::new(
            Box::new(mock_child) as Box<dyn crate::executor::AgentChild>
        ));

        // Create a mock executor for the monitor
        let executor: Arc<dyn crate::executor::ProcessExecutor> =
            Arc::new(crate::executor::MockProcessExecutor::new());

        // Use a short check interval (10ms) to speed up the test.
        // With the default 1s interval, this test would block for ~1s even
        // when should_stop is set quickly.
        let check_interval = Duration::from_millis(10);

        // Spawn monitor in a thread with short interval
        let handle = thread::spawn(move || {
            monitor_idle_timeout_with_interval(
                timestamp,
                child,
                60,
                should_stop_clone,
                executor,
                check_interval,
            )
        });

        // Signal stop after a short delay
        thread::sleep(Duration::from_millis(50));
        should_stop.store(true, std::sync::atomic::Ordering::Release);

        // Wait for monitor to complete
        let result = handle.join().expect("Monitor thread panicked");
        assert_eq!(result, MonitorResult::ProcessCompleted);
    }

    #[test]
    #[cfg(unix)]
    fn test_kill_process_returns_failed_when_sigterm_command_exits_nonzero() {
        use std::io;
        use std::path::Path;
        use std::process::ExitStatus;

        #[cfg(unix)]
        use std::os::unix::process::ExitStatusExt;

        #[derive(Debug)]
        struct NonZeroKillExecutor;

        impl ProcessExecutor for NonZeroKillExecutor {
            fn execute(
                &self,
                _command: &str,
                _args: &[&str],
                _env: &[(String, String)],
                _workdir: Option<&Path>,
            ) -> io::Result<crate::executor::ProcessOutput> {
                Ok(crate::executor::ProcessOutput {
                    status: ExitStatus::from_raw(1),
                    stdout: String::new(),
                    stderr: String::new(),
                })
            }
        }

        let executor = NonZeroKillExecutor;
        let result = kill_process(12345, &executor, None, DEFAULT_KILL_CONFIG);
        assert_eq!(result, KillResult::Failed);
    }

    #[test]
    fn test_monitor_does_not_hold_child_lock_while_waiting_between_sigterm_checks() {
        // Regression test for lock contention:
        // monitor_idle_timeout_with_interval() must not hold the child mutex while sleeping
        // during the SIGTERM grace-period polling loop.
        use crate::executor::MockAgentChild;
        use std::sync::atomic::AtomicBool;
        use std::sync::{mpsc, Arc, Barrier, Mutex};
        use std::time::Duration;

        #[derive(Debug)]
        struct CountingChild {
            inner: MockAgentChild,
            try_wait_calls: Arc<std::sync::atomic::AtomicUsize>,
            first_try_wait_gate: Arc<Barrier>,
            entered_first_try_wait: mpsc::Sender<()>,
        }

        impl crate::executor::AgentChild for CountingChild {
            fn id(&self) -> u32 {
                self.inner.id()
            }

            fn wait(&mut self) -> std::io::Result<std::process::ExitStatus> {
                self.inner.wait()
            }

            fn try_wait(&mut self) -> std::io::Result<Option<std::process::ExitStatus>> {
                self.try_wait_calls.fetch_add(1, Ordering::SeqCst);

                // Deterministic sync point: on the first try_wait call we pause while
                // still holding the mutex, so the test can prove the lock is held.
                // After releasing the gate, the monitor will return from try_wait and
                // then sleep outside the lock (the behavior we require).
                if self.try_wait_calls.load(Ordering::SeqCst) == 1 {
                    let _ = self.entered_first_try_wait.send(());
                    self.first_try_wait_gate.wait();
                }
                self.inner.try_wait()
            }
        }

        let (mock_child, running_controller) = MockAgentChild::new_running(0);
        let try_wait_calls = Arc::new(std::sync::atomic::AtomicUsize::new(0));

        let (entered_tx, entered_rx) = mpsc::channel();
        let gate = Arc::new(Barrier::new(2));

        let child = Arc::new(Mutex::new(Box::new(CountingChild {
            inner: mock_child,
            try_wait_calls: Arc::clone(&try_wait_calls),
            first_try_wait_gate: Arc::clone(&gate),
            entered_first_try_wait: entered_tx,
        })
            as Box<dyn crate::executor::AgentChild>));

        let timestamp = new_activity_timestamp();
        timestamp.store(0, Ordering::Release);

        let should_stop = Arc::new(AtomicBool::new(false));
        let executor: Arc<dyn crate::executor::ProcessExecutor> =
            Arc::new(crate::executor::MockProcessExecutor::new());

        // Keep the child alive long enough that the monitor enters and stays in the
        // SIGTERM grace polling loop. We'll stop it after the assertions.

        let child_for_monitor = Arc::clone(&child);
        let timestamp_for_monitor = timestamp.clone();
        let should_stop_for_monitor = Arc::clone(&should_stop);

        let kill_config = KillConfig {
            sigterm_grace: Duration::from_secs(2),
            poll_interval: Duration::from_millis(500),
            sigkill_confirm_timeout: Duration::from_millis(50),
            post_sigkill_hard_cap: Duration::from_secs(5),
            sigkill_resend_interval: Duration::from_secs(1),
        };

        let monitor = thread::spawn(move || {
            monitor_idle_timeout_with_interval_and_kill_config(
                timestamp_for_monitor,
                child_for_monitor,
                0,
                should_stop_for_monitor,
                executor,
                Duration::from_millis(1),
                kill_config,
            )
        });

        // Wait until the monitor reaches the first try_wait() while holding the lock.
        entered_rx
            .recv_timeout(Duration::from_secs(5))
            .expect("expected monitor to call try_wait");

        // Prove the lock is currently held (we're synchronized inside try_wait()).
        assert!(
            child.try_lock().is_err(),
            "expected child mutex to be held during try_wait"
        );

        // Release the gate so the monitor can return from try_wait(). It should then
        // sleep outside the lock, which gives us a deterministic window to acquire it.
        gate.wait();

        // The monitor's poll interval is long enough that we should always be able to
        // acquire the lock after the first try_wait returns. Use a bounded loop so this
        // test fails fast instead of hanging if the lock is held incorrectly.
        let acquired_after_gate = {
            let deadline = std::time::Instant::now() + Duration::from_secs(2);
            let mut acquired = false;
            while std::time::Instant::now() < deadline {
                if let Ok(_guard) = child.try_lock() {
                    acquired = true;
                    break;
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            acquired
        };

        // Stop the monitor and child to avoid hanging the test.
        should_stop.store(true, Ordering::Release);
        running_controller.store(false, Ordering::Release);
        let _ = monitor.join();

        assert!(
            acquired_after_gate,
            "expected to acquire child lock while monitor sleeps"
        );

        // Sanity: we should have observed at least one try_wait call.
        assert!(try_wait_calls.load(Ordering::Acquire) >= 1);
    }

    #[test]
    #[cfg(unix)]
    fn test_monitor_reports_timeout_even_if_sigkill_confirmation_times_out() {
        // Regression test for missed timeout:
        // If SIGKILL is sent successfully but the child isn't observed dead within the
        // confirmation window, the monitor must still classify the run as TimedOut once
        // the child is later observed exited.
        use crate::executor::MockAgentChild;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex};

        let (mock_child, running_controller) = MockAgentChild::new_running(0);
        let child = Arc::new(Mutex::new(
            Box::new(mock_child) as Box<dyn crate::executor::AgentChild>
        ));

        let timestamp = new_activity_timestamp();
        timestamp.store(0, Ordering::Release); // timeout exceeded
        let should_stop = Arc::new(AtomicBool::new(false));

        // Use a concrete executor so we can observe calls.
        let executor = Arc::new(crate::executor::MockProcessExecutor::new());
        let executor_dyn: Arc<dyn crate::executor::ProcessExecutor> = executor.clone();

        // Make SIGTERM grace short and SIGKILL confirmation extremely short so the
        // confirmation loop times out before we flip still_running.
        let kill_config = KillConfig {
            sigterm_grace: Duration::from_millis(10),
            poll_interval: Duration::from_millis(1),
            sigkill_confirm_timeout: Duration::from_millis(1),
            post_sigkill_hard_cap: Duration::from_secs(2),
            sigkill_resend_interval: Duration::from_millis(20),
        };

        let child_for_monitor = Arc::clone(&child);
        let timestamp_for_monitor = timestamp.clone();
        let should_stop_for_monitor = Arc::clone(&should_stop);

        let monitor_handle = thread::spawn(move || {
            monitor_idle_timeout_with_interval_and_kill_config(
                timestamp_for_monitor,
                child_for_monitor,
                0,
                should_stop_for_monitor,
                executor_dyn,
                Duration::from_millis(1),
                kill_config,
            )
        });

        // Wait until SIGKILL is attempted, then shortly after mark the child as dead.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            let calls = executor.execute_calls_for("kill");
            if calls
                .iter()
                .any(|(_, args, _, _)| args.iter().any(|a| a == "-KILL"))
            {
                thread::sleep(Duration::from_millis(5));
                running_controller.store(false, Ordering::Release);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let result = monitor_handle.join().expect("Monitor thread panicked");
        assert_eq!(result, MonitorResult::TimedOut { escalated: true });
        assert!(!running_controller.load(Ordering::Acquire));
    }

    #[test]
    #[cfg(unix)]
    fn test_monitor_treats_try_wait_errors_as_process_gone_during_kill_verification() {
        // Regression test: try_wait() errors during SIGTERM/SIGKILL verification should not
        // cause the monitor to misclassify a timeout-triggered kill as ProcessCompleted.
        use std::io;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex};

        #[derive(Debug)]
        struct TryWaitErrorsChild {
            first: bool,
        }

        impl crate::executor::AgentChild for TryWaitErrorsChild {
            fn id(&self) -> u32 {
                12345
            }

            fn wait(&mut self) -> io::Result<std::process::ExitStatus> {
                Err(io::Error::other("wait should not be called in this test"))
            }

            fn try_wait(&mut self) -> io::Result<Option<std::process::ExitStatus>> {
                if self.first {
                    self.first = false;
                    return Ok(None);
                }

                Err(io::Error::other(
                    "simulated already-reaped / status unavailable",
                ))
            }
        }

        let child =
            Arc::new(Mutex::new(Box::new(TryWaitErrorsChild { first: true })
                as Box<dyn crate::executor::AgentChild>));

        let timestamp = new_activity_timestamp();
        timestamp.store(0, Ordering::Release);
        let should_stop = Arc::new(AtomicBool::new(false));
        let executor: Arc<dyn crate::executor::ProcessExecutor> =
            Arc::new(crate::executor::MockProcessExecutor::new());

        let kill_config = KillConfig {
            sigterm_grace: Duration::from_millis(10),
            poll_interval: Duration::from_millis(1),
            sigkill_confirm_timeout: Duration::from_millis(10),
            post_sigkill_hard_cap: Duration::from_secs(2),
            sigkill_resend_interval: Duration::from_millis(20),
        };

        let result = monitor_idle_timeout_with_interval_and_kill_config(
            timestamp,
            child,
            0,
            should_stop,
            executor,
            Duration::from_millis(1),
            kill_config,
        );

        assert_eq!(result, MonitorResult::TimedOut { escalated: false });
    }

    #[test]
    fn test_stderr_activity_tracker_updates_timestamp() {
        let data = b"debug output\nmore output\n";
        let cursor = Cursor::new(data.to_vec());
        let timestamp = new_activity_timestamp();

        // Use a sentinel value to avoid time-based assertions.
        timestamp.store(u64::MAX, Ordering::Release);

        // Verify timestamp is at sentinel
        assert_eq!(timestamp.load(Ordering::Acquire), u64::MAX);

        // Create stderr tracker and read data
        let mut tracker = StderrActivityTracker::new(cursor, timestamp.clone());
        let mut buf = [0u8; 50];
        let n = tracker.read(&mut buf).unwrap();
        assert!(n > 0);

        // After stderr read, timestamp should no longer be the sentinel.
        assert_ne!(timestamp.load(Ordering::Acquire), u64::MAX);
    }

    #[test]
    fn test_stderr_activity_tracker_no_update_on_zero_read() {
        let data = b"";
        let cursor = Cursor::new(data.to_vec());
        let timestamp = new_activity_timestamp();

        let mut tracker = StderrActivityTracker::new(cursor, timestamp.clone());

        // Set timestamp to 0 after creating tracker
        timestamp.store(0, Ordering::Release);

        // Verify timestamp is at 0
        assert_eq!(timestamp.load(Ordering::Acquire), 0);

        // Read (should return 0, EOF)
        let mut buf = [0u8; 10];
        let n = tracker.read(&mut buf).unwrap();
        assert_eq!(n, 0);

        // After zero-read, timestamp should NOT be updated (still 0)
        assert_eq!(
            timestamp.load(Ordering::Acquire),
            0,
            "After zero-read, timestamp should remain 0"
        );
    }

    #[test]
    fn test_clock_jump_back_does_not_cause_spurious_timeout() {
        // This test verifies that even if we simulate a "clock jump" by
        // manipulating the mock clock, our monotonic time approach handles
        // it correctly with saturating_sub.
        let clock = MockClock::new(100_000); // Start at 100 seconds
        let timestamp = new_activity_timestamp_with_clock(&clock);

        // Activity just happened at t=100s
        touch_activity_with_clock(&timestamp, &clock);

        // Simulate clock jumping back 50 seconds (NTP sync scenario)
        clock.jump_back(50_000);

        // Should NOT trigger timeout because:
        // 1. timestamp stores 100_000
        // 2. current time is now 50_000
        // 3. saturating_sub(50_000, 100_000) = 0
        // 4. elapsed = 0 < timeout threshold
        let elapsed = time_since_activity_with_clock(&timestamp, &clock);
        assert_eq!(elapsed, Duration::ZERO);
        assert!(!is_idle_timeout_exceeded_with_clock(
            &timestamp,
            IDLE_TIMEOUT_SECS,
            &clock
        ));
    }

    #[test]
    fn test_monotonic_clock_only_increases() {
        let clock = MonotonicClock::new();
        let t1 = clock.now_millis();
        let t2 = clock.now_millis();
        assert!(t2 >= t1, "Monotonic clock should never go backwards");
    }

    #[test]
    #[cfg(unix)]
    fn test_monitor_escalates_to_sigkill_when_sigterm_ignored() {
        use crate::executor::MockAgentChild;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex};

        // Create a mock process that "ignores" SIGTERM by staying alive
        let (mock_child, running_controller) = MockAgentChild::new_running(0);
        let child = Arc::new(Mutex::new(
            Box::new(mock_child) as Box<dyn crate::executor::AgentChild>
        ));

        let timestamp = new_activity_timestamp();
        let should_stop = Arc::new(AtomicBool::new(false));

        // Mock executor that reports successful kill commands.
        // Keep a concrete Arc so we can inspect captured calls.
        let executor = Arc::new(crate::executor::MockProcessExecutor::new());
        let executor_dyn: Arc<dyn crate::executor::ProcessExecutor> = executor.clone();

        // Set up the scenario: timestamp is old (timeout exceeded)
        timestamp.store(0, Ordering::Release);

        let child_clone = Arc::clone(&child);
        let timestamp_clone = timestamp.clone();
        let should_stop_clone = should_stop.clone();

        let kill_config = KillConfig {
            sigterm_grace: Duration::from_millis(20),
            poll_interval: Duration::from_millis(1),
            sigkill_confirm_timeout: Duration::from_millis(50),
            post_sigkill_hard_cap: Duration::from_secs(2),
            sigkill_resend_interval: Duration::from_millis(20),
        };

        // Spawn monitor with very short timeout and check interval
        let monitor_handle = thread::spawn(move || {
            monitor_idle_timeout_with_interval_and_kill_config(
                timestamp_clone,
                child_clone,
                0,
                should_stop_clone,
                executor_dyn,
                Duration::from_millis(1),
                kill_config,
            )
        });

        // Wait until SIGKILL is attempted, then simulate termination.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            let calls = executor.execute_calls_for("kill");
            if calls
                .iter()
                .any(|(_, args, _, _)| args.iter().any(|a| a == "-KILL"))
            {
                running_controller.store(false, Ordering::Release);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        // Wait for monitor to complete
        let result = monitor_handle.join().expect("Monitor thread panicked");

        // Verify we got TimedOut with escalation
        assert_eq!(result, MonitorResult::TimedOut { escalated: true });

        // Verify process is no longer running
        assert!(!running_controller.load(Ordering::Acquire));
    }

    #[test]
    #[cfg(unix)]
    fn test_monitor_succeeds_with_sigterm_when_process_terminates() {
        use crate::executor::MockAgentChild;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex};

        // Create a mock process that terminates quickly after SIGTERM
        let (mock_child, running_controller) = MockAgentChild::new_running(0);
        let child = Arc::new(Mutex::new(
            Box::new(mock_child) as Box<dyn crate::executor::AgentChild>
        ));

        let timestamp = new_activity_timestamp();
        let should_stop = Arc::new(AtomicBool::new(false));

        let executor = Arc::new(crate::executor::MockProcessExecutor::new());
        let executor_dyn: Arc<dyn crate::executor::ProcessExecutor> = executor.clone();

        // Set up timeout scenario
        timestamp.store(0, Ordering::Release);

        let child_clone = Arc::clone(&child);
        let timestamp_clone = timestamp.clone();
        let should_stop_clone = should_stop.clone();

        let kill_config = KillConfig {
            sigterm_grace: Duration::from_millis(50),
            poll_interval: Duration::from_millis(1),
            sigkill_confirm_timeout: Duration::from_millis(50),
            post_sigkill_hard_cap: Duration::from_secs(2),
            sigkill_resend_interval: Duration::from_millis(20),
        };

        let monitor_handle = thread::spawn(move || {
            monitor_idle_timeout_with_interval_and_kill_config(
                timestamp_clone,
                child_clone,
                0,
                should_stop_clone,
                executor_dyn,
                Duration::from_millis(1),
                kill_config,
            )
        });

        // Wait until SIGTERM is attempted, then simulate termination.
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        while std::time::Instant::now() < deadline {
            let calls = executor.execute_calls_for("kill");
            if calls
                .iter()
                .any(|(_, args, _, _)| args.iter().any(|a| a == "-TERM"))
            {
                running_controller.store(false, Ordering::Release);
                break;
            }
            thread::sleep(Duration::from_millis(5));
        }

        let result = monitor_handle.join().expect("Monitor thread panicked");

        // Should get TimedOut but WITHOUT escalation
        assert_eq!(result, MonitorResult::TimedOut { escalated: false });
    }

    #[test]
    #[cfg(unix)]
    fn test_monitor_does_not_report_timeout_if_process_still_alive_after_force_kill() {
        // Regression test for a pipeline hang risk:
        // If SIGKILL is sent but the child isn't observed exited, the monitor must not
        // loop forever without an upper bound.
        //
        // Before the fix, this test would time out waiting for the monitor to return.
        // After the fix, the monitor returns TimedOut even if the child never becomes
        // observable as exited, allowing the caller to regain control.
        use crate::executor::MockAgentChild;
        use std::sync::atomic::AtomicBool;
        use std::sync::mpsc;
        use std::sync::{Arc, Mutex};

        let (mock_child, controller) = MockAgentChild::new_running(0);
        let child = Arc::new(Mutex::new(
            Box::new(mock_child) as Box<dyn crate::executor::AgentChild>
        ));

        let timestamp = new_activity_timestamp();
        timestamp.store(0, Ordering::Release); // Timeout exceeded

        let should_stop = Arc::new(AtomicBool::new(false));
        let executor = Arc::new(crate::executor::MockProcessExecutor::new());
        let executor_dyn: Arc<dyn crate::executor::ProcessExecutor> = executor.clone();

        let kill_config = KillConfig {
            sigterm_grace: Duration::from_millis(1),
            poll_interval: Duration::from_millis(1),
            sigkill_confirm_timeout: Duration::from_millis(5),
            post_sigkill_hard_cap: Duration::from_millis(200),
            sigkill_resend_interval: Duration::from_millis(20),
        };

        let (tx, rx) = mpsc::channel();

        let child_for_monitor = Arc::clone(&child);
        let should_stop_for_monitor = Arc::clone(&should_stop);
        let monitor_handle = thread::spawn(move || {
            let result = monitor_idle_timeout_with_interval_and_kill_config(
                timestamp,
                child_for_monitor,
                0,
                should_stop_for_monitor,
                executor_dyn,
                Duration::from_millis(1),
                kill_config,
            );
            let _ = tx.send(result);
        });

        // Before the fix, the monitor would never return while the child stays alive.
        // We assert that it returns within a bounded time.
        let received = rx.recv_timeout(Duration::from_secs(2));

        // Clean up: ensure the monitor thread can join even if the assertion fails.
        controller.store(false, Ordering::Release);
        let _ = monitor_handle.join();

        let result = received.expect("expected monitor to return within bounded time");
        assert_eq!(result, MonitorResult::TimedOut { escalated: true });
    }
}
