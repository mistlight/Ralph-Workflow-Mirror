// Activity tracking readers and monitor functions for idle timeout

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

/// Default check interval for the idle monitor (1 second).
const DEFAULT_CHECK_INTERVAL: Duration = Duration::from_secs(1);

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
    monitor_idle_timeout_with_interval(
        activity_timestamp,
        child_id,
        timeout_secs,
        should_stop,
        executor,
        DEFAULT_CHECK_INTERVAL,
    )
}

/// Like [`monitor_idle_timeout`] but with a configurable check interval.
///
/// This variant is primarily used for testing with shorter intervals to avoid
/// long test execution times.
///
/// # Arguments
///
/// * `activity_timestamp` - Shared timestamp updated by the reader
/// * `child_id` - Process ID to kill if timeout exceeded
/// * `timeout_secs` - Maximum seconds of inactivity before killing
/// * `should_stop` - Atomic flag to signal monitor should exit (set when process completes)
/// * `executor` - Process executor for killing the subprocess
/// * `check_interval` - How often to check for timeout/stop signal
pub fn monitor_idle_timeout_with_interval(
    activity_timestamp: SharedActivityTimestamp,
    child_id: u32,
    timeout_secs: u64,
    should_stop: Arc<std::sync::atomic::AtomicBool>,
    executor: Arc<dyn ProcessExecutor>,
    check_interval: Duration,
) -> MonitorResult {
    use std::sync::atomic::Ordering;

    loop {
        std::thread::sleep(check_interval);

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
