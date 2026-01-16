//! Pipeline Execution Module
//!
//! This module contains the core pipeline execution infrastructure:
//! - Types for tracking pipeline statistics and RAII cleanup
//! - Model flag resolution utilities
//! - Command execution helpers with fault-tolerant fallback chains
//! - Timer utilities for tracking execution duration
//!
//! # Module Structure
//!
//! - [`model_flag`] - Model flag resolution and provider detection
//! - [`runner`] - Pipeline runtime and command execution with fallback
//! - [`types`] - Pipeline statistics tracking and RAII guards

#![deny(unsafe_code)]

mod clipboard;
mod fallback;
mod model_flag;
mod prompt;
mod runner;
mod types;

pub use fallback::OutputValidator;
pub use prompt::{run_with_prompt, PipelineRuntime, PromptCommand};
pub use runner::{run_with_fallback, run_with_fallback_and_validator, FallbackConfig};
pub use types::{AgentPhaseGuard, Stats};

// ===== Timer Utilities =====

use std::time::{Duration, Instant};

/// Timer for tracking execution duration
#[derive(Clone)]
pub struct Timer {
    start_time: Instant,
    phase_start: Instant,
}

impl Timer {
    /// Create a new timer, starting now
    pub fn new() -> Self {
        let now = Instant::now();
        Self {
            start_time: now,
            phase_start: now,
        }
    }

    /// Start a new phase timer
    pub fn start_phase(&mut self) {
        self.phase_start = Instant::now();
    }

    /// Get elapsed time since timer start
    pub fn elapsed(&self) -> Duration {
        self.start_time.elapsed()
    }

    /// Get elapsed time since phase start
    pub fn phase_elapsed(&self) -> Duration {
        self.phase_start.elapsed()
    }

    /// Format a duration as "Xm YYs"
    pub fn format_duration(duration: Duration) -> String {
        let total_secs = duration.as_secs();
        let mins = total_secs / 60;
        let secs = total_secs % 60;
        format!("{mins}m {secs:02}s")
    }

    /// Get formatted elapsed time since start
    pub fn elapsed_formatted(&self) -> String {
        Self::format_duration(self.elapsed())
    }

    /// Get formatted elapsed time since phase start
    pub fn phase_elapsed_formatted(&self) -> String {
        Self::format_duration(self.phase_elapsed())
    }
}

impl Default for Timer {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod timer_tests {
    use super::*;
    use std::thread;

    #[test]
    fn test_format_duration_zero() {
        let d = Duration::from_secs(0);
        assert_eq!(Timer::format_duration(d), "0m 00s");
    }

    #[test]
    fn test_format_duration_seconds() {
        let d = Duration::from_secs(30);
        assert_eq!(Timer::format_duration(d), "0m 30s");
    }

    #[test]
    fn test_format_duration_minutes() {
        let d = Duration::from_secs(65);
        assert_eq!(Timer::format_duration(d), "1m 05s");
    }

    #[test]
    fn test_format_duration_large() {
        let d = Duration::from_secs(3661);
        assert_eq!(Timer::format_duration(d), "61m 01s");
    }

    #[test]
    fn test_timer_elapsed() {
        let timer = Timer::new();
        thread::sleep(Duration::from_millis(10));
        assert!(timer.elapsed() >= Duration::from_millis(10));
    }

    #[test]
    fn test_timer_phase() {
        let mut timer = Timer::new();
        thread::sleep(Duration::from_millis(10));
        timer.start_phase();
        thread::sleep(Duration::from_millis(10));
        // Phase elapsed should be less than total elapsed
        assert!(timer.phase_elapsed() < timer.elapsed());
    }
}

#[cfg(test)]
mod tests;
