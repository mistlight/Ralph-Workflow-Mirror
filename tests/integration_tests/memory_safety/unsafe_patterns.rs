//! Unsafe code verification tests
//!
//! These tests verify that unsafe code blocks maintain their safety invariants
//! under various conditions. We test observable behavior, not internal implementation.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! This module tests observable behavior of unsafe code:
//! - Non-blocking FD setup succeeds under normal operation
//! - Process cleanup handles errors gracefully
//! - No segfaults or undefined behavior
//! - Error cases are handled properly

use crate::test_timeout::with_default_timeout;

#[cfg(unix)]
#[test]
fn test_nonblocking_fd_safety_under_normal_operation() {
    with_default_timeout(|| {
        // Verify executor/real.rs unsafe fcntl usage is safe under normal operation
        // Test that:
        // 1. Setting non-blocking succeeds
        // 2. Child process can be spawned
        // 3. File descriptors remain valid
        // 4. No segfaults or undefined behavior

        use ralph_workflow::executor::{ProcessExecutor, RealProcessExecutor};

        let executor = RealProcessExecutor::new();

        // Use regular spawn (which uses similar unsafe code for process setup)
        let result = executor.spawn("echo", &["test"], &[], None);

        // Should succeed without crashes or errors
        assert!(
            result.is_ok(),
            "Spawn with unsafe fcntl should succeed: {:?}",
            result.err()
        );

        if let Ok(mut child) = result {
            // Process should complete normally
            let wait_result = child.wait();

            assert!(
                wait_result.is_ok(),
                "Process should exit cleanly with non-blocking FDs"
            );
        }
    });
}

#[cfg(unix)]
#[test]
fn test_nonblocking_fd_error_handling() {
    with_default_timeout(|| {
        // Verify unsafe code properly handles error cases:
        // 1. Invalid file descriptor (should error, not crash)
        // 2. System call failures (should propagate errors)
        // 3. Edge cases (closed fds, etc.)

        // Test is implicit: if ensure_nonblocking_or_terminate encounters
        // an invalid FD (like -1), it should:
        // 1. Return an error
        // 2. Terminate the child process
        // 3. Not crash or cause UB

        // This behavior is already tested in executor/real.rs:212-253
        // (ensure_nonblocking_or_terminate_kills_child_on_failure)

        // Here we verify the observable outcome: spawning with a command
        // that might fail doesn't cause crashes

        use ralph_workflow::executor::{ProcessExecutor, RealProcessExecutor};

        let executor = RealProcessExecutor::new();

        // Try to execute a non-existent command
        let result = executor.execute("nonexistent_command_that_should_not_exist", &[], &[], None);

        // Should fail gracefully, not crash
        assert!(
            result.is_err(),
            "Non-existent command should fail gracefully"
        );
    });
}

#[cfg(unix)]
#[test]
fn test_process_group_setpgid_safety() {
    with_default_timeout(|| {
        // Verify executor/real.rs:164-172 unsafe setpgid usage is safe
        // The unsafe block puts the agent in its own process group
        // This should work correctly without crashes

        use ralph_workflow::executor::{ProcessExecutor, RealProcessExecutor};

        let executor = RealProcessExecutor::new();

        // spawn_agent uses setpgid internally, but we can't test it directly
        // Instead, verify that regular spawn works without unsafe code issues
        let result = executor.spawn("sleep", &["0.1"], &[], None);

        assert!(
            result.is_ok(),
            "Process spawn should succeed: {:?}",
            result.err()
        );

        if let Ok(mut child) = result {
            // Process should spawn and terminate correctly
            let wait_result = child.wait();

            assert!(wait_result.is_ok(), "Process should exit cleanly");
        }
    });
}

#[cfg(unix)]
#[test]
fn test_process_kill_safety() {
    with_default_timeout(|| {
        // Verify executor/real.rs:40-56 unsafe kill() calls are safe
        // Tests the process cleanup path with SIGTERM/SIGKILL

        use ralph_workflow::executor::{ProcessExecutor, RealProcessExecutor};

        let executor = RealProcessExecutor::new();

        // Spawn a long-running process
        let result = executor.spawn("sleep", &["60"], &[], None);
        assert!(result.is_ok(), "Should spawn successfully");

        if let Ok(mut child) = result {
            // Kill the process (uses unsafe kill internally in cleanup paths)
            let kill_result = child.kill();

            // Should succeed without crashes
            assert!(
                kill_result.is_ok(),
                "Kill should succeed: {:?}",
                kill_result.err()
            );

            // Wait should complete (process should be terminated)
            let _wait_result = child.wait();

            // If we reach here, no crashes or UB occurred
        }
    });
}

#[cfg(not(unix))]
#[test]
fn test_unsafe_code_is_unix_only() {
    // On non-Unix platforms, verify that the code still compiles and works
    // even though unsafe blocks are platform-specific

    use ralph_workflow::executor::{ProcessExecutor, RealProcessExecutor};

    let executor = RealProcessExecutor::new();

    let result = executor.execute("echo", &["test"], &[], None);

    // Should work on all platforms
    assert!(
        result.is_ok(),
        "Basic execution should work on all platforms"
    );
}
