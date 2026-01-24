//! Idle timeout detection for agent subprocess execution.
//!
//! This module provides infrastructure to detect when an agent subprocess
//! has stopped producing output, indicating it may be stuck (e.g., waiting
//! for user input in unattended mode).
//!
//! # Design
//!
//! The idle timeout system uses a shared atomic timestamp that gets updated
//! whenever data is read from the subprocess stdout. A monitor thread
//! periodically checks this timestamp and can kill the subprocess if
//! no output has been received for longer than the configured timeout.
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
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Default idle timeout in seconds (5 minutes).
///
/// This value was chosen because:
/// - Complex tool operations rarely take more than 2 minutes of silence
/// - LLM reasoning models may take 30-60 seconds between outputs
/// - If no output for 5 minutes, the agent is almost certainly stuck
pub const IDLE_TIMEOUT_SECS: u64 = 300;

/// Shared timestamp for tracking last activity.
///
/// Stores milliseconds since UNIX epoch. Use [`ActivityTracker::new`] to create.
pub type SharedActivityTimestamp = Arc<AtomicU64>;

/// Creates a new shared activity timestamp initialized to the current time.
pub fn new_activity_timestamp() -> SharedActivityTimestamp {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64;
    Arc::new(AtomicU64::new(now_ms))
}

/// Updates the shared activity timestamp to the current time.
pub fn touch_activity(timestamp: &SharedActivityTimestamp) {
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64;
    timestamp.store(now_ms, Ordering::Release);
}

/// Returns the duration since the last activity update.
pub fn time_since_activity(timestamp: &SharedActivityTimestamp) -> Duration {
    let last_ms = timestamp.load(Ordering::Acquire);
    let now_ms = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or(Duration::ZERO)
        .as_millis() as u64;

    Duration::from_millis(now_ms.saturating_sub(last_ms))
}

/// Checks if the idle timeout has been exceeded.
pub fn is_idle_timeout_exceeded(timestamp: &SharedActivityTimestamp, timeout_secs: u64) -> bool {
    time_since_activity(timestamp) > Duration::from_secs(timeout_secs)
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
///
/// # Platform Notes
///
/// Uses `kill -TERM` command on Unix and `taskkill` on Windows to avoid unsafe code.
pub fn monitor_idle_timeout(
    activity_timestamp: SharedActivityTimestamp,
    child_id: u32,
    timeout_secs: u64,
    should_stop: Arc<std::sync::atomic::AtomicBool>,
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
            let killed = kill_process(child_id);
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

/// Kill a process by PID using platform-specific commands.
///
/// Returns true if the kill command succeeded, false otherwise.
#[cfg(unix)]
fn kill_process(pid: u32) -> bool {
    // Use kill command to send SIGTERM
    std::process::Command::new("kill")
        .args(["-TERM", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Kill a process by PID using platform-specific commands.
///
/// Returns true if the kill command succeeded, false otherwise.
#[cfg(windows)]
fn kill_process(pid: u32) -> bool {
    std::process::Command::new("taskkill")
        .args(["/F", "/PID", &pid.to_string()])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::thread;

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
        let timestamp = new_activity_timestamp();
        // Set timestamp to 2 seconds ago
        let two_secs_ago = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            - 2000;
        timestamp.store(two_secs_ago, Ordering::Release);

        // Timeout of 1 second should be exceeded
        assert!(is_idle_timeout_exceeded(&timestamp, 1));
    }

    #[test]
    fn test_activity_tracking_reader_updates_on_read() {
        let data = b"hello world";
        let cursor = Cursor::new(data.to_vec());
        let timestamp = new_activity_timestamp();

        let mut reader = ActivityTrackingReader::new(cursor, timestamp.clone());

        // Set timestamp to 1 second ago AFTER creating reader
        // (since new() calls touch_activity)
        let one_sec_ago = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            - 1000;
        timestamp.store(one_sec_ago, Ordering::Release);

        // Before read, should be ~1 second old
        assert!(time_since_activity(&timestamp) >= Duration::from_millis(900));

        // Read some data
        let mut buf = [0u8; 5];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 5);

        // After read, timestamp should be updated (very recent)
        assert!(time_since_activity(&timestamp) < Duration::from_millis(100));
    }

    #[test]
    fn test_activity_tracking_reader_no_update_on_zero_read() {
        let data = b"";
        let cursor = Cursor::new(data.to_vec());
        let timestamp = new_activity_timestamp();

        // Set timestamp to 1 second ago
        let one_sec_ago = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            - 1000;
        timestamp.store(one_sec_ago, Ordering::Release);

        let mut reader = ActivityTrackingReader::new(cursor, timestamp.clone());

        // Note: ActivityTrackingReader::new touches the timestamp, so reset it
        let one_sec_ago = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64
            - 1000;
        timestamp.store(one_sec_ago, Ordering::Release);

        // Read (should return 0, EOF)
        let mut buf = [0u8; 5];
        let n = reader.read(&mut buf).unwrap();
        assert_eq!(n, 0);

        // After zero-read, timestamp should NOT be updated
        assert!(time_since_activity(&timestamp) >= Duration::from_millis(900));
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

        // Spawn monitor in a thread
        let handle =
            thread::spawn(move || monitor_idle_timeout(timestamp, fake_pid, 60, should_stop_clone));

        // Signal stop after a short delay
        thread::sleep(Duration::from_millis(50));
        should_stop.store(true, std::sync::atomic::Ordering::Release);

        // Wait for monitor to complete
        let result = handle.join().expect("Monitor thread panicked");
        assert_eq!(result, MonitorResult::ProcessCompleted);
    }
}
