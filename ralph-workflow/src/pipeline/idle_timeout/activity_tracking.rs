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
// a grace period (KillConfig::sigterm_grace; default: DEFAULT_KILL_CONFIG). If the
// process is still running after the grace period, it escalates to SIGKILL.
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
    /// Kill signals were sent successfully, but the process was not confirmed exited yet.
    ///
    /// The monitor should continue polling for exit and report `TimedOut` once
    /// the process is observed dead.
    SignalsSentAwaitingExit { escalated: bool },
    /// Kill attempt failed (process may have already exited).
    Failed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct KillConfig {
    sigterm_grace: Duration,
    poll_interval: Duration,
    sigkill_confirm_timeout: Duration,
    post_sigkill_hard_cap: Duration,
    sigkill_resend_interval: Duration,
}

impl KillConfig {
    pub(crate) const fn new(
        sigterm_grace: Duration,
        poll_interval: Duration,
        sigkill_confirm_timeout: Duration,
        post_sigkill_hard_cap: Duration,
        sigkill_resend_interval: Duration,
    ) -> Self {
        Self {
            sigterm_grace,
            poll_interval,
            sigkill_confirm_timeout,
            post_sigkill_hard_cap,
            sigkill_resend_interval,
        }
    }

    pub(crate) fn sigterm_grace(&self) -> Duration {
        self.sigterm_grace
    }

    pub(crate) fn poll_interval(&self) -> Duration {
        self.poll_interval
    }

    pub(crate) fn sigkill_confirm_timeout(&self) -> Duration {
        self.sigkill_confirm_timeout
    }

    pub(crate) fn post_sigkill_hard_cap(&self) -> Duration {
        self.post_sigkill_hard_cap
    }

    pub(crate) fn sigkill_resend_interval(&self) -> Duration {
        self.sigkill_resend_interval
    }
}

/// Default kill configuration.
///
/// - SIGTERM grace: 5s
/// - Poll interval: 100ms
/// - SIGKILL confirm timeout: 500ms
/// - Post-SIGKILL hard cap: 5s
/// - SIGKILL resend interval: 1s
pub(crate) const DEFAULT_KILL_CONFIG: KillConfig = KillConfig::new(
    Duration::from_secs(5),
    Duration::from_millis(100),
    Duration::from_millis(500),
    Duration::from_secs(5),
    Duration::from_secs(1),
);

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
    monitor_idle_timeout_with_interval_and_kill_config(
        activity_timestamp,
        child,
        timeout_secs,
        should_stop,
        executor,
        check_interval,
        DEFAULT_KILL_CONFIG,
    )
}

pub(crate) fn monitor_idle_timeout_with_interval_and_kill_config(
    activity_timestamp: SharedActivityTimestamp,
    child: Arc<std::sync::Mutex<Box<dyn AgentChild>>>,
    timeout_secs: u64,
    should_stop: Arc<std::sync::atomic::AtomicBool>,
    executor: Arc<dyn ProcessExecutor>,
    check_interval: Duration,
    kill_config: KillConfig,
) -> MonitorResult {
    use std::sync::atomic::Ordering;

    #[derive(Debug, Clone, Copy)]
    struct TimeoutEnforcementState {
        pid: u32,
        escalated: bool,
        triggered_at: std::time::Instant,
        last_sigkill_sent_at: Option<std::time::Instant>,
    }

    // Once we have triggered a timeout and sent signals, we must not later
    // misreport normal completion. Keep the timeout classification sticky.
    //
    // IMPORTANT: we keep trying to observe exit, but we also have a hard cap
    // so an unkillable/stuck process can't stall the pipeline indefinitely.
    let mut timeout_triggered: Option<TimeoutEnforcementState> = None;

    loop {
        std::thread::sleep(check_interval);

        // If we already triggered a timeout, keep polling until the child is
        // observed exited (or try_wait fails, which we treat as "no longer running").
        if let Some(mut state) = timeout_triggered.take() {
            let status = {
                let mut locked_child = child.lock().unwrap();
                locked_child.try_wait()
            };

            match status {
                Ok(Some(_)) | Err(_) => {
                    return MonitorResult::TimedOut {
                        escalated: state.escalated,
                    }
                }
                Ok(None) => {
                    // Still running after timeout. Keep trying to regain control.
                    let now = std::time::Instant::now();

                    // Periodically re-send SIGKILL when we've already escalated.
                    if state.escalated {
                        let should_resend = match state.last_sigkill_sent_at {
                            None => true,
                            Some(t) => {
                                now.duration_since(t) >= kill_config.sigkill_resend_interval()
                            }
                        };
                        if should_resend {
                            let _ = force_kill_best_effort(state.pid, executor.as_ref());
                            state.last_sigkill_sent_at = Some(now);
                        }
                    }

                    // Hard cap: stop waiting for an observable exit. The caller should
                    // treat the run as TimedOut and proceed with best-effort cleanup.
                    if now.duration_since(state.triggered_at) >= kill_config.post_sigkill_hard_cap()
                    {
                        // One final best-effort SIGKILL before giving up.
                        if state.escalated {
                            let _ = force_kill_best_effort(state.pid, executor.as_ref());
                        }

                        return MonitorResult::TimedOut {
                            escalated: state.escalated,
                        };
                    }

                    timeout_triggered = Some(state);
                    continue;
                }
            }
        }

        // Check if we should stop (process completed normally)
        if should_stop.load(Ordering::Acquire) {
            return MonitorResult::ProcessCompleted;
        }

        // Check if idle timeout exceeded
        if !is_idle_timeout_exceeded(&activity_timestamp, timeout_secs) {
            continue;
        }

        // Check liveness and capture pid while holding the lock briefly.
        let child_id = {
            let mut locked_child = child.lock().unwrap();

            // If the process already exited, treat as completed.
            match locked_child.try_wait() {
                Ok(Some(_)) | Err(_) => {
                    return MonitorResult::ProcessCompleted;
                }
                Ok(None) => {}
            }

            locked_child.id()
        };

        // Kill using platform-specific command with liveness verification.
        let kill_result = kill_process(child_id, executor.as_ref(), Some(&child), kill_config);

        match kill_result {
            KillResult::TerminatedByTerm => {
                return MonitorResult::TimedOut { escalated: false };
            }
            KillResult::TerminatedByKill => {
                return MonitorResult::TimedOut { escalated: true };
            }
            KillResult::SignalsSentAwaitingExit { escalated } => {
                let now = std::time::Instant::now();
                timeout_triggered = Some(TimeoutEnforcementState {
                    pid: child_id,
                    escalated,
                    triggered_at: now,
                    last_sigkill_sent_at: escalated.then_some(now),
                });
            }
            KillResult::Failed => {
                // Kill failed - this can happen if:
                // - process already exited between checks
                // - kill/taskkill failed
                // - SIGKILL/taskkill succeeded but process is still running (rare)
                //
                // Keep monitoring so we can retry the kill attempt.
                if should_stop.load(Ordering::Acquire) {
                    return MonitorResult::ProcessCompleted;
                }
            }
        }
    }
}

#[cfg(unix)]
fn force_kill_best_effort(pid: u32, executor: &dyn ProcessExecutor) -> bool {
    executor
        .execute("kill", &["-KILL", &pid.to_string()], &[], None)
        .map(|o| o.status.success())
        .unwrap_or(false)
}

#[cfg(windows)]
fn force_kill_best_effort(pid: u32, executor: &dyn ProcessExecutor) -> bool {
    executor
        .execute("taskkill", &["/F", "/PID", &pid.to_string()], &[], None)
        .map(|o| o.status.success())
        .unwrap_or(false)
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
    child: Option<&Arc<std::sync::Mutex<Box<dyn AgentChild>>>>,
    config: KillConfig,
) -> KillResult {
    // First attempt: send SIGTERM
    let term_result = executor.execute("kill", &["-TERM", &pid.to_string()], &[], None);

    let term_ok = term_result.map(|o| o.status.success()).unwrap_or(false);
    if !term_ok {
        return KillResult::Failed;
    }

    // Wait for process to terminate gracefully
    if let Some(child_arc) = child {
        let grace_deadline = std::time::Instant::now() + config.sigterm_grace;

        while std::time::Instant::now() < grace_deadline {
            let status = {
                let mut locked_child = child_arc.lock().unwrap();
                locked_child.try_wait()
            };

            match status {
                Ok(Some(_)) => return KillResult::TerminatedByTerm,
                Ok(None) => std::thread::sleep(config.poll_interval),
                Err(_) => return KillResult::TerminatedByTerm,
            }
        }

        // Grace period expired, process still alive - escalate to SIGKILL
        let kill_result = executor.execute("kill", &["-KILL", &pid.to_string()], &[], None);
        let kill_ok = kill_result.map(|o| o.status.success()).unwrap_or(false);
        if !kill_ok {
            return KillResult::Failed;
        }

        // Confirm SIGKILL took effect with bounded polling.
        let confirm_deadline = std::time::Instant::now() + config.sigkill_confirm_timeout;
        while std::time::Instant::now() < confirm_deadline {
            let status = {
                let mut locked_child = child_arc.lock().unwrap();
                locked_child.try_wait()
            };

            match status {
                Ok(Some(_)) => return KillResult::TerminatedByKill,
                Ok(None) => std::thread::sleep(config.poll_interval),
                Err(_) => return KillResult::TerminatedByKill,
            }
        }

        // SIGKILL was sent but the process may exit shortly after our confirmation window.
        // Keep monitoring so the timeout can be surfaced once the child is observed dead.
        return KillResult::SignalsSentAwaitingExit { escalated: true };
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
    child: Option<&Arc<std::sync::Mutex<Box<dyn AgentChild>>>>,
    config: KillConfig,
) -> KillResult {
    // Windows taskkill /F is already forceful, no escalation needed
    let result = executor.execute("taskkill", &["/F", "/PID", &pid.to_string()], &[], None);

    let kill_ok = result.map(|o| o.status.success()).unwrap_or(false);
    if !kill_ok {
        return KillResult::Failed;
    }

    // If we can verify liveness, do so with bounded polling.
    if let Some(child_arc) = child {
        let confirm_deadline = std::time::Instant::now() + config.sigkill_confirm_timeout;
        while std::time::Instant::now() < confirm_deadline {
            let status = {
                let mut locked_child = child_arc.lock().unwrap();
                locked_child.try_wait()
            };

            match status {
                Ok(Some(_)) => return KillResult::TerminatedByKill,
                Ok(None) => std::thread::sleep(config.poll_interval),
                Err(_) => return KillResult::TerminatedByKill,
            }
        }

        return KillResult::SignalsSentAwaitingExit { escalated: true };
    }

    KillResult::TerminatedByKill
}
