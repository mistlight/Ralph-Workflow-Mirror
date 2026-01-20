//! Test timeout enforcement for integration tests.
//!
//! This module provides a mechanism to enforce maximum execution time
//! for integration tests. Tests that take longer than the specified
//! timeout will be terminated and fail with a clear error message.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** This module is part of the integration test framework and
//! MUST follow the integration test style guide defined in
//! **[../INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (test completion within timeout)
//! - Provides utility for test isolation and determinism
//!
//! # MANDATORY Usage
//!
//! **ALL integration tests MUST use either `with_default_timeout()` or `with_timeout()`
//! to wrap their test code.** This is enforced by code review and the integration test style guide.
//!
//! The timeout wrapper must be the **outermost** wrapper in any test - no test code
//! should execute before the timeout wrapper is invoked.
//!
//! # Usage
//!
//! ```rust,no_run
//! use crate::test_timeout::with_default_timeout;
//!
//! #[test]
//! fn test_name() {
//!     with_default_timeout(|| {
//!         // ALL test code must run inside this closure
//!         let dir = TempDir::new().unwrap();
//!         // ... rest of test
//!     });
//! }
//! ```
//!
//! # Default Timeout
//!
//! The default timeout for all integration tests is 10 seconds.
//! This is enforced via the `DEFAULT_TIMEOUT` constant.
//!
//! Tests that exceed 10 seconds likely have external I/O dependencies
//! (real LLM calls, network requests, shell scripts, long sleeps) which
//! violate the integration test style guide.

use std::panic;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

/// Default timeout for integration tests (10 seconds).
pub const DEFAULT_TIMEOUT: Duration = Duration::from_secs(10);

/// Error type for timeout failures.
#[derive(Debug)]
pub struct TimeoutError {
    pub timeout: Duration,
}

impl std::fmt::Display for TimeoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "Test exceeded timeout of {:?}. \
             This usually indicates the test is waiting for external I/O \
             (e.g., real LLM calls, network requests, or long sleeps). \
             Use mocks to eliminate external dependencies.",
            self.timeout
        )
    }
}

impl std::error::Error for TimeoutError {}

/// Run a closure with a timeout.
///
/// If the closure does not complete within the specified duration,
/// the test will panic with a timeout error.
///
/// # Arguments
///
/// * `f` - The closure to execute
/// * `timeout` - Maximum duration to wait for completion
///
/// # Panics
///
/// Panics if the closure does not complete within the timeout.
///
/// # Example
///
/// ```rust,no_run
/// use crate::test_timeout::with_timeout;
///
/// with_timeout(
///     || {
///         // Fast operation
///         assert_eq!(2 + 2, 4);
///     },
///     std::time::Duration::from_secs(1),
/// );
/// ```
pub fn with_timeout<F>(f: F, timeout: Duration)
where
    F: FnOnce() + Send + 'static,
{
    let completed = Arc::new(AtomicBool::new(false));
    let completed_clone = Arc::clone(&completed);

    let handle = thread::spawn(move || {
        f();
        completed_clone.store(true, Ordering::Release);
    });

    let start = std::time::Instant::now();
    while !completed.load(Ordering::Acquire) {
        if start.elapsed() >= timeout {
            panic!("{}", TimeoutError { timeout }.to_string());
        }
        // Small sleep to avoid busy-waiting
        thread::sleep(Duration::from_millis(50));
    }

    handle.join().unwrap();
}

/// Run a closure with the default 10-second timeout.
///
/// This is a convenience wrapper around `with_timeout` that uses
/// the standard integration test timeout.
///
/// # Example
///
/// ```rust,no_run
/// use crate::test_timeout::with_default_timeout;
///
/// with_default_timeout(|| {
///     // Test code here - must complete in 10 seconds
///     assert_eq!(2 + 2, 4);
/// });
/// ```
pub fn with_default_timeout<F>(f: F)
where
    F: FnOnce() + Send + 'static,
{
    with_timeout(f, DEFAULT_TIMEOUT);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Test that fast operations complete within timeout.
    ///
    /// This verifies that when a fast operation is wrapped with a timeout,
    /// the operation completes successfully without timing out.
    #[test]
    fn test_with_timeout_success() {
        with_timeout(
            || {
                // Fast operation
                assert_eq!(2 + 2, 4);
            },
            Duration::from_secs(1),
        );
    }

    /// Test that fast operations complete within default timeout.
    ///
    /// This verifies that when a fast operation is wrapped with the default timeout,
    /// the operation completes successfully without timing out.
    #[test]
    fn test_with_default_timeout_success() {
        with_default_timeout(|| {
            // Fast operation
            assert_eq!(2 + 2, 4);
        });
    }

    /// Test that slow operations panic with timeout error.
    ///
    /// This verifies that when an operation exceeds the timeout duration,
    /// the test panics with a clear timeout error message.
    #[test]
    #[should_panic(expected = "Test exceeded timeout")]
    fn test_with_timeout_panic_on_slow_operation() {
        with_timeout(
            || {
                thread::sleep(Duration::from_secs(2));
            },
            Duration::from_millis(100),
        );
    }

    /// Test that timeout error displays helpful message.
    ///
    /// This verifies that when a timeout error is displayed,
    /// it includes information about the timeout duration and external I/O.
    #[test]
    fn test_timeout_error_display() {
        let error = TimeoutError {
            timeout: Duration::from_secs(10),
        };
        let msg = format!("{}", error);
        // Duration debug format is "10s", not "10 seconds"
        assert!(msg.contains("10s"));
        assert!(msg.contains("external I/O"));
    }
}
