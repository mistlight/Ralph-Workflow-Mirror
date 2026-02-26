//! Idle timeout detection for agent subprocess execution.
//!
//! This module provides infrastructure to detect when an agent subprocess
//! has stopped producing output, indicating it may be stuck (e.g., waiting
//! for user input in unattended mode).
//!
//! # Design
//!
//! The idle timeout system tracks two types of activity to detect whether an
//! agent is making progress:
//!
//! 1. **Output Activity**: A shared atomic timestamp gets updated whenever
//!    data is read from subprocess stdout OR stderr.
//! 2. **File Activity**: A tracker monitors AI-generated files in `.agent/`
//!    (PLAN.md, ISSUES.md, NOTES.md, commit-message.txt, .agent/tmp/*.xml)
//!    to detect file updates that indicate ongoing work.
//!
//! A monitor thread periodically checks both signals (by default every 30 seconds)
//! and kills the subprocess only if BOTH output and file activity have been idle
//! for longer than the configured timeout (300 seconds).
//!
//! Both stdout and stderr activity are tracked because some agents (e.g., opencode
//! with `--print-logs`) output verbose progress information to stderr while
//! processing, and only produce stdout when complete. Without tracking stderr,
//! such agents would be incorrectly killed as idle.
//!
//! File activity tracking prevents false timeouts when agents produce sparse
//! output but are actively writing files, which is common during planning,
//! commit message generation, and other file-intensive phases.
//!
//! # Timeout Value
//!
//! The default timeout is 5 minutes (300 seconds), which is:
//! - Long enough for complex tool operations and LLM reasoning
//! - Short enough to detect truly stuck agents
//! - Aligned with typical CI/CD step timeouts

mod clock;
mod file_activity;
pub(crate) mod kill;
mod monitor;
mod readers;

pub use clock::{
    is_idle_timeout_exceeded, is_idle_timeout_exceeded_with_clock, new_activity_timestamp,
    new_activity_timestamp_with_clock, new_file_activity_tracker, time_since_activity,
    time_since_activity_with_clock, touch_activity, touch_activity_with_clock, Clock,
    MonotonicClock, SharedActivityTimestamp, SharedFileActivityTracker, IDLE_TIMEOUT_SECS,
};
pub use file_activity::FileActivityTracker;
pub use kill::{KillConfig, DEFAULT_KILL_CONFIG};
pub use monitor::{
    monitor_idle_timeout, monitor_idle_timeout_with_interval,
    monitor_idle_timeout_with_interval_and_kill_config, FileActivityConfig, MonitorConfig,
    MonitorResult,
};
pub use readers::{ActivityTrackingReader, StderrActivityTracker};

#[cfg(test)]
mod tests;
