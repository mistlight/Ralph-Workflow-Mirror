// Activity tracking readers and monitor functions for idle timeout
//
// # Threading Model
//
// The idle timeout monitor runs in a separate thread and shares access to the
// child process via `Arc<Mutex<Box<dyn AgentChild>>>`. The monitor:
// 1. Periodically checks if idle timeout has been exceeded
// 2. Acquires the child lock briefly to send kill signals
// 3. Verifies process liveness after SIGTERM using try_wait()
// 4. Escalates to SIGKILL if process doesn't terminate within grace period
// 5. Releases the lock between checks to allow main thread to wait on process
//
// The main thread:
// 1. Streams stdout without holding the child lock
// 2. Acquires the child lock momentarily during try_wait() checks
// 3. Releases the lock between checks to allow monitor to kill process
//
// # Liveness Verification and Escalation
//
// The monitor does NOT assume that a successful kill command means termination.
// After sending SIGTERM, it actively polls the process using try_wait() during
// a grace period (SIGTERM_GRACE_PERIOD_MS). If the process is still running
// after the grace period, it escalates to SIGKILL.
//
// This prevents a critical bug where:
// - Monitor sends SIGTERM, kill command succeeds
// - Monitor immediately returns MonitorResult::TimedOut
// - Process ignores SIGTERM and never exits
// - Main thread blocks forever on stdout streaming or wait()
// - Pipeline hangs instead of timing out
//
// With liveness verification, the monitor doesn't return until either:
// - Process has terminated (verified via try_wait)
// - SIGKILL has been sent (after grace period expires)
//
// This design prevents deadlock while ensuring timeouts are reliably detected
// and enforced, even if subprocesses ignore or handle SIGTERM without exiting.

use crate::executor::AgentChild;

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
    ///
    /// The `escalated` flag indicates whether SIGKILL was required:
    /// - `false`: Process terminated after SIGTERM within grace period
    /// - `true`: Process did not respond to SIGTERM, required SIGKILL escalation
    ///
    /// In both cases, the process was successfully killed (or kill was attempted).
    /// The escalation flag is for diagnostic purposes and does not affect the
    /// timeout handling - both cases result in AgentEvent::TimedOut.
    TimedOut { escalated: bool },
}

/// Default check interval for the idle monitor (1 second).
const DEFAULT_CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// Result of attempting to kill a process.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum KillResult {
    /// Process was successfully killed with SIGTERM.
    TerminatedByTerm,
    /// Process required SIGKILL escalation.
    TerminatedByKill,
    /// Kill attempt failed (process may have already exited).
    Failed,
}

/// Grace period to wait for SIGTERM to take effect before escalating to SIGKILL.
const SIGTERM_GRACE_PERIOD_MS: u64 = 5000; // 5 seconds

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
/// * `child` - Shared mutable reference to the child process for liveness checks
/// * `timeout_secs` - Maximum seconds of inactivity before killing
/// * `should_stop` - Atomic flag to signal monitor should exit (set when process completes)
/// * `executor` - Process executor for killing the subprocess
///
/// # Platform Notes
///
/// Uses `kill -TERM` command on Unix (with SIGKILL escalation) and `taskkill` on Windows
/// via the ProcessExecutor trait.
pub fn monitor_idle_timeout(
    activity_timestamp: SharedActivityTimestamp,
    child: Arc<std::sync::Mutex<Box<dyn AgentChild>>>,
    timeout_secs: u64,
    should_stop: Arc<std::sync::atomic::AtomicBool>,
    executor: Arc<dyn ProcessExecutor>,
) -> MonitorResult {
    monitor_idle_timeout_with_interval(
        activity_timestamp,
        child,
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
/// * `child` - Shared mutable reference to the child process for liveness checks
/// * `timeout_secs` - Maximum seconds of inactivity before killing
/// * `should_stop` - Atomic flag to signal monitor should exit (set when process completes)
/// * `executor` - Process executor for killing the subprocess
/// * `check_interval` - How often to check for timeout/stop signal
pub fn monitor_idle_timeout_with_interval(
    activity_timestamp: SharedActivityTimestamp,
    child: Arc<std::sync::Mutex<Box<dyn AgentChild>>>,
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
            // Get the PID before locking
            let child_id = {
                let locked_child = child.lock().unwrap();
                locked_child.id()
            };

            // Kill the process using platform-specific command with escalation
            let mut locked_child = child.lock().unwrap();
            let kill_result =
                kill_process(child_id, executor.as_ref(), Some(locked_child.as_mut()));
            drop(locked_child); // Release lock

            match kill_result {
                KillResult::TerminatedByTerm => {
                    return MonitorResult::TimedOut { escalated: false };
                }
                KillResult::TerminatedByKill => {
                    return MonitorResult::TimedOut { escalated: true };
                }
                KillResult::Failed => {
                    // Kill failed - process may have already exited
                    if should_stop.load(Ordering::Acquire) {
                        return MonitorResult::ProcessCompleted;
                    }
                    // Kill failed for unknown reason, try again next iteration
                }
            }
        }
    }
}

/// Kill a process by PID using platform-specific commands via executor.
///
/// First attempts SIGTERM, waits for a grace period while verifying liveness,
/// then escalates to SIGKILL if the process hasn't terminated. Returns the kill
/// result indicating whether escalation was required.
///
/// # Liveness Verification
///
/// After sending SIGTERM, this function actively polls the child process using
/// `try_wait()` to verify termination. If the process is still running after the
/// grace period, it escalates to SIGKILL. This prevents the bug where:
/// - Monitor sends SIGTERM and immediately returns TimedOut
/// - Process ignores SIGTERM and never exits
/// - Main thread blocks forever on streaming/wait()
/// - Pipeline hangs instead of timing out
///
/// # Arguments
///
/// * `pid` - Process ID to kill
/// * `executor` - Process executor for running kill commands
/// * `child` - Optional mutable reference to AgentChild for liveness checks
///
/// # Returns
///
/// Returns KillResult indicating how the process was terminated or if it failed.
#[cfg(unix)]
fn kill_process(
    pid: u32,
    executor: &dyn ProcessExecutor,
    child: Option<&mut dyn AgentChild>,
) -> KillResult {
    use std::thread;
    use std::time::Duration;

    // First attempt: send SIGTERM
    let term_result = executor.execute("kill", &["-TERM", &pid.to_string()], &[], None);

    if term_result.is_err() {
        // Kill command itself failed (process may have already exited)
        return KillResult::Failed;
    }

    // Wait for process to terminate gracefully
    if let Some(child_ref) = child {
        let grace_start = std::time::Instant::now();
        let grace_duration = Duration::from_millis(SIGTERM_GRACE_PERIOD_MS);
        let check_interval = Duration::from_millis(100);

        while grace_start.elapsed() < grace_duration {
            match child_ref.try_wait() {
                Ok(Some(_status)) => {
                    // Process has exited
                    return KillResult::TerminatedByTerm;
                }
                Ok(None) => {
                    // Still running, keep waiting
                    thread::sleep(check_interval);
                }
                Err(_) => {
                    // Error checking status - assume process is gone
                    return KillResult::TerminatedByTerm;
                }
            }
        }

        // Grace period expired, process still alive - escalate to SIGKILL
        let kill_result = executor.execute("kill", &["-KILL", &pid.to_string()], &[], None);

        if kill_result.is_ok() {
            // Wait a short time to confirm SIGKILL took effect
            thread::sleep(Duration::from_millis(500));
            return KillResult::TerminatedByKill;
        } else {
            return KillResult::Failed;
        }
    }

    // No child reference provided - assume SIGTERM worked
    // (This path is for backward compatibility with tests that don't provide child)
    KillResult::TerminatedByTerm
}

/// Kill a process by PID using platform-specific commands via executor.
///
/// Windows version uses taskkill /F which is already forceful.
#[cfg(windows)]
fn kill_process(
    pid: u32,
    executor: &dyn ProcessExecutor,
    _child: Option<&mut dyn AgentChild>,
) -> KillResult {
    // Windows taskkill /F is already forceful, no escalation needed
    let result = executor.execute("taskkill", &["/F", "/PID", &pid.to_string()], &[], None);

    if result.map(|o| o.status.success()).unwrap_or(false) {
        KillResult::TerminatedByKill // /F flag is always forceful
    } else {
        KillResult::Failed
    }
}
