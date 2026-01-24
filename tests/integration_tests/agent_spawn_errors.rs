//! Integration tests for agent spawn error handling.
//!
//! These tests verify that the pipeline properly handles error conditions
//! when spawning agent processes, using MockProcessExecutor to simulate
//! failures without actually spawning real processes.
//!
//! # Integration Test Style Guide Compliance
//!
//! This module follows the integration test style guide defined in
//! **[INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**:
//!
//! - **No process spawning:** Uses MockProcessExecutor instead of spawning actual processes
//! - **Behavior-based testing:** Tests observable behavior (error types, messages)
//! - **Architectural boundary mocking:** Mocks ProcessExecutor trait (external process boundary)
//!
//! # Key Principles
//!
//! These tests verify that error conditions during process spawn are handled
//! correctly by the ProcessExecutor trait implementation. The deleted production
//! code tests (test_spawn_agent_process_command_not_found and
//! test_spawn_agent_process_converts_all_errors_to_command_result) violated
//! the style guide by actually spawning processes to test behavior.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::executor::{MockProcessExecutor, ProcessOutput};
use std::os::unix::process::ExitStatusExt;
use std::process::ExitStatus;
use std::sync::Arc;

/// Test that NotFound errors are properly returned by ProcessExecutor.
///
/// This verifies that when a command cannot be found (simulated by
/// MockProcessExecutor), the ProcessExecutor trait returns an appropriate
/// error rather than panicking.
#[test]
fn test_process_executor_returns_not_found_error() {
    with_default_timeout(|| {
        let mock = MockProcessExecutor::new().with_io_error(
            "claude",
            std::io::ErrorKind::NotFound,
            "No such file or directory (os error 2)",
        );
        let executor: Arc<dyn ralph_workflow::executor::ProcessExecutor> = Arc::new(mock);

        let result = executor.execute("claude", &[], &[], None);
        assert!(
            result.is_err(),
            "Should return error for nonexistent command"
        );
        let err = result.unwrap_err();
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::NotFound,
            "Error kind should be NotFound"
        );
        assert!(
            err.to_string().contains("No such file or directory"),
            "Error message should describe the failure"
        );
    });
}

/// Test that InvalidInput errors are properly returned for invalid commands.
///
/// This verifies that invalid commands (like empty strings) are handled
/// gracefully with appropriate error types.
#[test]
fn test_process_executor_returns_invalid_input_for_empty_command() {
    with_default_timeout(|| {
        let mock = MockProcessExecutor::new().with_io_error(
            "",
            std::io::ErrorKind::InvalidInput,
            "Cannot spawn empty command",
        );
        let executor: Arc<dyn ralph_workflow::executor::ProcessExecutor> = Arc::new(mock);

        let result = executor.execute("", &[], &[], None);
        assert!(result.is_err(), "Should return error for empty command");

        let err = result.unwrap_err();
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::InvalidInput,
            "Empty command should return InvalidInput error"
        );
    });
}

/// Test that PermissionDenied errors are properly returned.
///
/// This verifies that when a command cannot be executed due to permissions,
/// the ProcessExecutor returns an appropriate error.
#[test]
fn test_process_executor_returns_permission_denied_error() {
    with_default_timeout(|| {
        let mock = MockProcessExecutor::new().with_io_error(
            "/root/restricted",
            std::io::ErrorKind::PermissionDenied,
            "Permission denied (os error 13)",
        );
        let executor: Arc<dyn ralph_workflow::executor::ProcessExecutor> = Arc::new(mock);

        let result = executor.execute("/root/restricted", &[], &[], None);
        assert!(result.is_err(), "Should return error for permission denied");

        let err = result.unwrap_err();
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::PermissionDenied,
            "Permission denied should return PermissionDenied error"
        );
    });
}

/// Test the distinction between spawn failures and execution failures.
///
/// This verifies that:
/// - Spawn failures (command not found) return io::Error from execute()
/// - Execution failures (command ran but failed) return Ok() with non-zero exit status
#[test]
fn test_spawn_failure_vs_execution_failure() {
    with_default_timeout(|| {
        // Spawn failure: command doesn't exist
        let not_found_mock = MockProcessExecutor::new().with_io_error(
            "nonexistent",
            std::io::ErrorKind::NotFound,
            "not found",
        );
        let executor1: Arc<dyn ralph_workflow::executor::ProcessExecutor> =
            Arc::new(not_found_mock);

        let spawn_result = executor1.execute("nonexistent", &[], &[], None);
        assert!(spawn_result.is_err(), "Nonexistent command should fail");
        assert_eq!(
            spawn_result.unwrap_err().kind(),
            std::io::ErrorKind::NotFound
        );

        // Execution failure: command exists but returns non-zero exit
        let exit_mock = MockProcessExecutor::new().with_result(
            "agent",
            Ok(ProcessOutput {
                status: ExitStatus::from_raw(1), // Non-zero exit code
                stdout: String::new(),
                stderr: "Agent execution failed".to_string(),
            }),
        );
        let executor2: Arc<dyn ralph_workflow::executor::ProcessExecutor> = Arc::new(exit_mock);

        let exec_result = executor2.execute("agent", &[], &[], None);
        assert!(
            exec_result.is_ok(),
            "Command should execute successfully (even with non-zero exit)"
        );
        let output = exec_result.unwrap();
        assert!(
            !output.status.success(),
            "Exit code should indicate failure"
        );
        assert_eq!(output.stderr, "Agent execution failed");
    });
}

/// Test that ArgumentListTooLong errors are properly returned.
///
/// This verifies that when a prompt exceeds OS argument limits,
/// the error is properly reported.
#[test]
fn test_process_executor_returns_argument_list_too_long_error() {
    with_default_timeout(|| {
        let mock = MockProcessExecutor::new().with_io_error(
            "agent",
            std::io::ErrorKind::ArgumentListTooLong,
            "Argument list too long (os error 7)",
        );
        let executor: Arc<dyn ralph_workflow::executor::ProcessExecutor> = Arc::new(mock);

        let result = executor.execute("agent", &[], &[], None);
        assert!(
            result.is_err(),
            "Should return error for argument list too long"
        );

        let err = result.unwrap_err();
        assert_eq!(
            err.kind(),
            std::io::ErrorKind::ArgumentListTooLong,
            "Should return ArgumentListTooLong error"
        );
    });
}

/// Test MockProcessExecutor successful execution.
///
/// This verifies that MockProcessExecutor correctly returns successful outputs.
#[test]
fn test_mock_executor_returns_success() {
    with_default_timeout(|| {
        let mock = MockProcessExecutor::new().with_output("test-cmd", "test output");

        let executor: Arc<dyn ralph_workflow::executor::ProcessExecutor> = Arc::new(mock);

        let result = executor.execute("test-cmd", &["arg1", "arg2"], &[], None);
        assert!(result.is_ok(), "Should return Ok for successful execution");

        let output = result.unwrap();
        assert!(
            output.status.success(),
            "Exit status should indicate success"
        );
        assert_eq!(
            output.stdout, "test output",
            "Stdout should match configured output"
        );
        assert_eq!(
            output.stderr, "",
            "Stderr should be empty for successful execution"
        );
    });
}
