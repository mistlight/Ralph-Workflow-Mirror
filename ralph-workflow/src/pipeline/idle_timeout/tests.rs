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

        // Mock executor that reports successful kill commands
        let executor: Arc<dyn crate::executor::ProcessExecutor> =
            Arc::new(crate::executor::MockProcessExecutor::new());

        // Set up the scenario: timestamp is old (timeout exceeded)
        timestamp.store(0, Ordering::Release);

        let child_clone = Arc::clone(&child);
        let timestamp_clone = timestamp.clone();
        let should_stop_clone = should_stop.clone();
        let running_controller_clone = Arc::clone(&running_controller);

        // Simulate a process that ignores SIGTERM but responds to SIGKILL
        // by terminating well AFTER the grace period expires
        let terminator = thread::spawn(move || {
            // Wait significantly longer than the grace period to ensure
            // the monitor has time to detect the timeout and escalate
            // (5000ms grace + 1000ms to ensure SIGKILL is definitely sent)
            thread::sleep(Duration::from_millis(6000));
            // Mark process as terminated (simulating SIGKILL effect)
            running_controller_clone.store(false, Ordering::Release);
        });

        // Spawn monitor with very short timeout and check interval
        let monitor_handle = thread::spawn(move || {
            monitor_idle_timeout_with_interval(
                timestamp_clone,
                child_clone,
                1, // 1 second timeout
                should_stop_clone,
                executor,
                Duration::from_millis(100), // Fast check interval
            )
        });

        // Wait for monitor to complete
        let result = monitor_handle.join().expect("Monitor thread panicked");

        // Wait for terminator thread to finish
        terminator.join().expect("Terminator thread panicked");

        // Verify we got TimedOut with escalation
        assert_eq!(result, MonitorResult::TimedOut { escalated: true });

        // Verify process is no longer running
        assert!(!running_controller.load(Ordering::Acquire));
    }

    #[test]
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

        let executor: Arc<dyn crate::executor::ProcessExecutor> =
            Arc::new(crate::executor::MockProcessExecutor::new());

        // Set up timeout scenario
        timestamp.store(0, Ordering::Release);

        let child_clone = Arc::clone(&child);
        let timestamp_clone = timestamp.clone();
        let should_stop_clone = should_stop.clone();
        let running_controller_clone = Arc::clone(&running_controller);

        // Spawn monitor
        let monitor_handle = thread::spawn(move || {
            // Simulate process terminating promptly after SIGTERM
            thread::spawn(move || {
                thread::sleep(Duration::from_millis(200)); // Terminate within grace period
                running_controller_clone.store(false, Ordering::Release);
            });

            monitor_idle_timeout_with_interval(
                timestamp_clone,
                child_clone,
                1,
                should_stop_clone,
                executor,
                Duration::from_millis(100),
            )
        });

        let result = monitor_handle.join().expect("Monitor thread panicked");

        // Should get TimedOut but WITHOUT escalation
        assert_eq!(result, MonitorResult::TimedOut { escalated: false });
    }

    #[test]
    fn test_monitor_does_not_hang_on_unresponsive_process() {
        // This test verifies that the monitor thread completes in reasonable time
        // even if the process doesn't terminate. This is the regression test for
        // the original bug where the monitor would return TimedOut but the main
        // thread would hang forever.
        use crate::executor::MockAgentChild;
        use std::sync::atomic::AtomicBool;
        use std::sync::{Arc, Mutex};
        use std::time::Instant;

        let (mock_child, _controller) = MockAgentChild::new_running(0);
        let child = Arc::new(Mutex::new(
            Box::new(mock_child) as Box<dyn crate::executor::AgentChild>
        ));

        let timestamp = new_activity_timestamp();
        timestamp.store(0, Ordering::Release); // Timeout exceeded

        let should_stop = Arc::new(AtomicBool::new(false));
        let executor: Arc<dyn crate::executor::ProcessExecutor> =
            Arc::new(crate::executor::MockProcessExecutor::new());

        let start = Instant::now();

        // The monitor should complete within grace period + overhead
        // even if process never terminates
        let monitor_handle = thread::spawn(move || {
            monitor_idle_timeout_with_interval(
                timestamp,
                child,
                1,
                should_stop,
                executor,
                Duration::from_millis(100),
            )
        });

        let result = monitor_handle.join().expect("Monitor thread panicked");
        let elapsed = start.elapsed();

        // Monitor should complete in reasonable time (grace period + overhead)
        assert!(
            elapsed < Duration::from_secs(10),
            "Monitor took too long: {:?}",
            elapsed
        );

        // Should return TimedOut with escalation attempted
        assert_eq!(result, MonitorResult::TimedOut { escalated: true });
    }
}
