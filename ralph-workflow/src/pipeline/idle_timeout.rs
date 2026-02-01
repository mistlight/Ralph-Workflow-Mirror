//! Idle timeout detection for agent subprocess execution.
//!
//! This module provides infrastructure to detect when an agent subprocess
//! has stopped producing output, indicating it may be stuck (e.g., waiting
//! for user input in unattended mode).
//!
//! # Design
//!
//! The idle timeout system uses a shared atomic timestamp that gets updated
//! whenever data is read from the subprocess stdout OR stderr. A monitor thread
//! periodically checks this timestamp and can kill the subprocess if
//! no output has been received for longer than the configured timeout.
//!
//! Both stdout and stderr activity are tracked because some agents (e.g., opencode
//! with `--print-logs`) output verbose progress information to stderr while
//! processing, and only produce stdout when complete. Without tracking stderr,
//! such agents would be incorrectly killed as idle.
//!
//! # Timeout Value
//!
//! The default timeout is 5 minutes (300 seconds), which is:
//! - Long enough for complex tool operations and LLM reasoning
//! - Short enough to detect truly stuck agents
//! - Aligned with typical CI/CD step timeouts

use std::io::{self, Read};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

use crate::executor::ProcessExecutor;

/// Clock trait for obtaining current time. Enables testing with mock clocks.
///
/// Prefer implementations that return monotonically increasing millisecond values.
///
/// In practice, some test clocks may simulate backwards jumps. This module
/// treats any backwards movement as *no elapsed time* by using
/// `saturating_sub(now, last)`.
pub trait Clock: Send + Sync {
    /// Returns the current time in milliseconds since an arbitrary epoch.
    ///
    /// If the value goes backwards, elapsed time is clamped to zero.
    fn now_millis(&self) -> u64;
}

/// Production clock using `Instant` for monotonic time.
///
/// Unlike `SystemTime`, `Instant` is guaranteed to be monotonically increasing
/// and is not affected by NTP synchronization, system sleep/wake cycles, or
/// manual clock adjustments. This prevents spurious idle timeout kills caused
/// by clock jumps.
pub struct MonotonicClock {
    /// The reference point for all time measurements.
    epoch: Instant,
}

impl MonotonicClock {
    /// Create a new monotonic clock with the current instant as epoch.
    pub fn new() -> Self {
        Self {
            epoch: Instant::now(),
        }
    }
}

impl Default for MonotonicClock {
    fn default() -> Self {
        Self::new()
    }
}

impl Clock for MonotonicClock {
    fn now_millis(&self) -> u64 {
        self.epoch.elapsed().as_millis() as u64
    }
}

/// Global monotonic clock instance for production use.
///
/// This is lazily initialized on first use and provides monotonic time
/// measurements for the entire process lifetime.
fn global_clock() -> &'static MonotonicClock {
    use std::sync::OnceLock;
    static CLOCK: OnceLock<MonotonicClock> = OnceLock::new();
    CLOCK.get_or_init(MonotonicClock::new)
}

/// Default idle timeout in seconds (5 minutes).
///
/// This value was chosen because:
/// - Complex tool operations rarely take more than 2 minutes of silence
/// - LLM reasoning models may take 30-60 seconds between outputs
/// - If no output for 5 minutes, the agent is almost certainly stuck
pub const IDLE_TIMEOUT_SECS: u64 = 300;

/// Shared timestamp for tracking last activity.
///
/// Stores milliseconds since process start (monotonic time).
/// Use [`new_activity_timestamp`] to create.
pub type SharedActivityTimestamp = Arc<AtomicU64>;

/// Creates a new shared activity timestamp initialized to the current time.
///
/// Uses monotonic time (via `Instant`) to prevent issues with clock jumps
/// from NTP synchronization or system sleep/wake cycles.
pub fn new_activity_timestamp() -> SharedActivityTimestamp {
    Arc::new(AtomicU64::new(global_clock().now_millis()))
}

/// Creates a new shared activity timestamp using a custom clock.
///
/// This variant is primarily used for testing with mock clocks.
pub fn new_activity_timestamp_with_clock(clock: &dyn Clock) -> SharedActivityTimestamp {
    Arc::new(AtomicU64::new(clock.now_millis()))
}

/// Updates the shared activity timestamp to the current time.
///
/// Uses monotonic time to ensure consistent behavior regardless of
/// system clock changes.
pub fn touch_activity(timestamp: &SharedActivityTimestamp) {
    timestamp.store(global_clock().now_millis(), Ordering::Release);
}

/// Updates the shared activity timestamp using a custom clock.
///
/// This variant is primarily used for testing with mock clocks.
pub fn touch_activity_with_clock(timestamp: &SharedActivityTimestamp, clock: &dyn Clock) {
    timestamp.store(clock.now_millis(), Ordering::Release);
}

/// Returns the duration since the last activity update.
///
/// Uses monotonic time, so the result is always non-negative and
/// unaffected by system clock changes.
pub fn time_since_activity(timestamp: &SharedActivityTimestamp) -> Duration {
    let last_ms = timestamp.load(Ordering::Acquire);
    let now_ms = global_clock().now_millis();
    Duration::from_millis(now_ms.saturating_sub(last_ms))
}

/// Returns the duration since the last activity update using a custom clock.
///
/// This variant is primarily used for testing with mock clocks.
///
/// If the clock value goes backwards relative to the stored activity timestamp,
/// the elapsed time is clamped to zero.
pub fn time_since_activity_with_clock(
    timestamp: &SharedActivityTimestamp,
    clock: &dyn Clock,
) -> Duration {
    let last_ms = timestamp.load(Ordering::Acquire);
    let now_ms = clock.now_millis();
    Duration::from_millis(now_ms.saturating_sub(last_ms))
}

/// Checks if the idle timeout has been exceeded.
///
/// Uses monotonic time to prevent spurious timeout triggers from clock jumps.
pub fn is_idle_timeout_exceeded(timestamp: &SharedActivityTimestamp, timeout_secs: u64) -> bool {
    time_since_activity(timestamp) > Duration::from_secs(timeout_secs)
}

/// Checks if the idle timeout has been exceeded using a custom clock.
///
/// This variant is primarily used for testing with mock clocks.
pub fn is_idle_timeout_exceeded_with_clock(
    timestamp: &SharedActivityTimestamp,
    timeout_secs: u64,
    clock: &dyn Clock,
) -> bool {
    time_since_activity_with_clock(timestamp, clock) > Duration::from_secs(timeout_secs)
}

/// A reader wrapper that updates an activity timestamp on every read.
///
/// This wraps any `Read` implementation and updates a shared atomic timestamp
/// whenever data is successfully read. This allows external monitoring of
/// read activity for idle timeout detection.
pub struct ActivityTrackingReader<R: Read> {
    inner: R,
    activity_timestamp: SharedActivityTimestamp,
}

impl<R: Read> ActivityTrackingReader<R> {
    /// Create a new activity-tracking reader.
    ///
    /// The provided timestamp will be updated to the current time
    /// whenever data is successfully read from the inner reader.
    pub fn new(inner: R, activity_timestamp: SharedActivityTimestamp) -> Self {
        // Initialize timestamp to now
        touch_activity(&activity_timestamp);
        Self {
            inner,
            activity_timestamp,
        }
    }
}

impl<R: Read> Read for ActivityTrackingReader<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            touch_activity(&self.activity_timestamp);
        }
        Ok(n)
    }
}

/// A reader wrapper for stderr that updates an activity timestamp on every read.
///
/// This is similar to [`ActivityTrackingReader`] but designed specifically for
/// stderr tracking in a separate thread. It shares the same activity timestamp
/// as the stdout tracker, ensuring that any output (stdout OR stderr) prevents
/// idle timeout kills.
///
/// # Why Separate Struct?
///
/// While functionally identical to `ActivityTrackingReader`, having a distinct
/// type makes the code's intent clearer and enables separate documentation.
/// The stderr tracker runs in its own thread and updates the shared timestamp
/// whenever stderr data is received.
///
/// # Example Use Case
///
/// Some agents (e.g., opencode with `--print-logs`) output progress information
/// to stderr while processing, only producing stdout output when complete.
/// Without tracking stderr activity, such agents would be incorrectly killed
/// after the idle timeout despite being actively working.
pub struct StderrActivityTracker<R: Read> {
    inner: R,
    activity_timestamp: SharedActivityTimestamp,
}

impl<R: Read> StderrActivityTracker<R> {
    /// Create a new stderr activity tracker.
    ///
    /// The provided timestamp should be the same one used by the stdout
    /// `ActivityTrackingReader`, ensuring both streams contribute to
    /// preventing idle timeout kills.
    pub fn new(inner: R, activity_timestamp: SharedActivityTimestamp) -> Self {
        Self {
            inner,
            activity_timestamp,
        }
    }
}

impl<R: Read> Read for StderrActivityTracker<R> {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        let n = self.inner.read(buf)?;
        if n > 0 {
            touch_activity(&self.activity_timestamp);
        }
        Ok(n)
    }
}

/// Result of idle timeout monitoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorResult {
    /// Process completed normally (not killed by monitor).
    ProcessCompleted,
    /// Process was killed due to idle timeout.
    TimedOut,
}

/// Monitors activity and kills a process if idle timeout is exceeded.
///
/// This function runs in a loop, checking the activity timestamp periodically.
/// If the timestamp indicates no activity for longer than `timeout_secs`,
/// it will attempt to kill the process and return `MonitorResult::TimedOut`.
///
/// The function returns when either:
/// - The process is killed due to timeout
/// - The `should_stop` flag is set to true (process completed normally)
///
/// # Arguments
///
/// * `activity_timestamp` - Shared timestamp updated by the reader
/// * `child_id` - Process ID to kill if timeout exceeded
/// * `timeout_secs` - Maximum seconds of inactivity before killing
/// * `should_stop` - Atomic flag to signal monitor should exit (set when process completes)
/// * `executor` - Process executor for killing the subprocess
///
/// # Platform Notes
///
/// Uses `kill -TERM` command on Unix and `taskkill` on Windows via the ProcessExecutor trait.
pub fn monitor_idle_timeout(
    activity_timestamp: SharedActivityTimestamp,
    child_id: u32,
    timeout_secs: u64,
    should_stop: Arc<std::sync::atomic::AtomicBool>,
    executor: Arc<dyn ProcessExecutor>,
) -> MonitorResult {
    use std::sync::atomic::Ordering;

    // Check every second
    const CHECK_INTERVAL: Duration = Duration::from_secs(1);

    loop {
        std::thread::sleep(CHECK_INTERVAL);

        // Check if we should stop (process completed normally)
        if should_stop.load(Ordering::Acquire) {
            return MonitorResult::ProcessCompleted;
        }

        // Check if idle timeout exceeded
        if is_idle_timeout_exceeded(&activity_timestamp, timeout_secs) {
            // Kill the process using platform-specific command
            let killed = kill_process(child_id, executor.as_ref());
            if killed {
                return MonitorResult::TimedOut;
            }
            // If kill failed, process may have already exited - check should_stop again
            if should_stop.load(Ordering::Acquire) {
                return MonitorResult::ProcessCompleted;
            }
            // Kill failed for unknown reason, try again next iteration
        }
    }
}

/// Kill a process by PID using platform-specific commands via executor.
///
/// Returns true if the kill command succeeded, false otherwise.
#[cfg(unix)]
fn kill_process(pid: u32, executor: &dyn ProcessExecutor) -> bool {
    // Use kill command to send SIGTERM via ProcessExecutor
    executor
        .execute("kill", &["-TERM", &pid.to_string()], &[], None)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Kill a process by PID using platform-specific commands via executor.
///
/// Returns true if the kill command succeeded, false otherwise.
#[cfg(windows)]
fn kill_process(pid: u32, executor: &dyn ProcessExecutor) -> bool {
    // Use taskkill to force kill the process via ProcessExecutor
    executor
        .execute("taskkill", &["/F", "/PID", &pid.to_string()], &[], None)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

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
    fn test_new_activity_timestamp_is_recent() {
        let timestamp = new_activity_timestamp();
        let elapsed = time_since_activity(&timestamp);
        // Should be very recent (less than 100ms)
        assert!(elapsed < Duration::from_millis(100));
    }

    #[test]
    fn test_touch_activity_updates_timestamp() {
        let timestamp = new_activity_timestamp();
        // Wait a bit
        thread::sleep(Duration::from_millis(50));
        let before_touch = time_since_activity(&timestamp);

        // Touch should reset the elapsed time
        touch_activity(&timestamp);
        let after_touch = time_since_activity(&timestamp);

        assert!(before_touch >= Duration::from_millis(50));
        assert!(after_touch < Duration::from_millis(10));
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

        // Set timestamp to 0 to simulate very old activity
        timestamp.store(0, Ordering::Release);

        // Verify timestamp is at 0
        assert_eq!(timestamp.load(Ordering::Acquire), 0);

        // Read some data
        let mut buf = [0u8; 5];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 5);

        // After read, timestamp should be updated to current clock time.
        // The actual value depends on when global_clock was initialized.
        // We verify by checking that timestamp == global_clock.now_millis() (within tolerance)
        // because touch_activity sets it to current time.
        let updated = timestamp.load(Ordering::Acquire);
        let current = global_clock().now_millis();
        // Timestamp should be recent (within 100ms of current time)
        assert!(
            current.saturating_sub(updated) < 100,
            "After read, timestamp should be updated to recent time. Updated: {updated}, Current: {current}"
        );
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
        assert_ne!(MonitorResult::ProcessCompleted, MonitorResult::TimedOut);
    }

    #[test]
    fn test_monitor_stops_when_signaled() {
        use std::sync::atomic::AtomicBool;

        let timestamp = new_activity_timestamp();
        let should_stop = Arc::new(AtomicBool::new(false));
        let should_stop_clone = should_stop.clone();

        // Use a fake PID (0 - which won't match any real process)
        let fake_pid = 0u32;

        // Create a mock executor for the monitor
        let executor: Arc<dyn crate::executor::ProcessExecutor> =
            Arc::new(crate::executor::MockProcessExecutor::new());

        // Spawn monitor in a thread
        let handle = thread::spawn(move || {
            monitor_idle_timeout(timestamp, fake_pid, 60, should_stop_clone, executor)
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

        // Set timestamp to 0 to simulate very old activity
        timestamp.store(0, Ordering::Release);

        // Verify timestamp is at 0
        assert_eq!(timestamp.load(Ordering::Acquire), 0);

        // Create stderr tracker and read data
        let mut tracker = StderrActivityTracker::new(cursor, timestamp.clone());
        let mut buf = [0u8; 50];
        let n = tracker.read(&mut buf).unwrap();
        assert!(n > 0);

        // After stderr read, timestamp should be updated to current clock time.
        let updated = timestamp.load(Ordering::Acquire);
        let current = global_clock().now_millis();
        // Timestamp should be recent (within 100ms of current time)
        assert!(
            current.saturating_sub(updated) < 100,
            "After stderr read, timestamp should be updated to recent time. Updated: {updated}, Current: {current}"
        );
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
        thread::sleep(Duration::from_millis(10));
        let t2 = clock.now_millis();
        assert!(t2 >= t1, "Monotonic clock should never go backwards");
    }
}
