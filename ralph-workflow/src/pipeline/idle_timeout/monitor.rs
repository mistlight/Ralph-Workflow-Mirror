//! Idle-timeout monitor thread.

use super::kill::{
    force_kill_best_effort, kill_process, KillConfig, KillResult, DEFAULT_KILL_CONFIG,
};
use super::{is_idle_timeout_exceeded, SharedActivityTimestamp};
use crate::executor::{AgentChild, ProcessExecutor};
use std::sync::Arc;
use std::time::Duration;

/// Result of idle timeout monitoring.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MonitorResult {
    /// Process completed normally (not killed by monitor).
    ProcessCompleted,
    /// Process was killed due to idle timeout.
    ///
    /// The `escalated` flag indicates whether SIGKILL/taskkill was required:
    /// - `false`: Process terminated after SIGTERM within grace period
    /// - `true`: Process did not respond to SIGTERM, required SIGKILL/taskkill
    TimedOut { escalated: bool },
}

/// Default check interval for the idle monitor (1 second).
const DEFAULT_CHECK_INTERVAL: Duration = Duration::from_secs(1);

/// Monitors activity and kills a process if idle timeout is exceeded.
pub fn monitor_idle_timeout(
    activity_timestamp: SharedActivityTimestamp,
    child: Arc<std::sync::Mutex<Box<dyn AgentChild>>>,
    timeout_secs: u64,
    should_stop: Arc<std::sync::atomic::AtomicBool>,
    executor: Arc<dyn ProcessExecutor>,
) -> MonitorResult {
    monitor_idle_timeout_with_interval_and_kill_config(
        activity_timestamp,
        child,
        timeout_secs,
        should_stop,
        executor,
        DEFAULT_CHECK_INTERVAL,
        DEFAULT_KILL_CONFIG,
    )
}

/// Like [`monitor_idle_timeout`] but with a configurable check interval.
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

pub fn monitor_idle_timeout_with_interval_and_kill_config(
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

    let mut timeout_triggered: Option<TimeoutEnforcementState> = None;

    loop {
        std::thread::sleep(check_interval);

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
                    let now = std::time::Instant::now();

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

                    if now.duration_since(state.triggered_at) >= kill_config.post_sigkill_hard_cap()
                    {
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

        if should_stop.load(Ordering::Acquire) {
            return MonitorResult::ProcessCompleted;
        }

        if !is_idle_timeout_exceeded(&activity_timestamp, timeout_secs) {
            continue;
        }

        let child_id = {
            let mut locked_child = child.lock().unwrap();
            match locked_child.try_wait() {
                Ok(Some(_)) | Err(_) => return MonitorResult::ProcessCompleted,
                Ok(None) => {}
            }
            locked_child.id()
        };

        let kill_result = kill_process(child_id, executor.as_ref(), Some(&child), kill_config);
        match kill_result {
            KillResult::TerminatedByTerm => return MonitorResult::TimedOut { escalated: false },
            KillResult::TerminatedByKill => return MonitorResult::TimedOut { escalated: true },
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
                if should_stop.load(Ordering::Acquire) {
                    return MonitorResult::ProcessCompleted;
                }
            }
        }
    }
}
