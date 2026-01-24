//! Common utilities for integration tests
//!
//! This module provides shared utilities for integration tests across all test modules.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All integration tests MUST follow the style guide defined in
//! **[INTEGRATION_TESTS.md](../INTEGRATION_TESTS.md)**.
//!
//! Before writing, modifying, or debugging any integration test, you MUST read
//! that document. Key principles:
//!
//! - Test **observable behavior**, not implementation details
//! - Mock only at **architectural boundaries** (filesystem, network, external APIs)
//! - Use `TestPrinter` for parser tests (replaces stdout)
//! - Use `TempDir` for filesystem isolation
//! - NEVER use `cfg!(test)` branches in production code
//!
//! The utilities in this module support proper integration test patterns:
//! - `run_ralph_cli()`: Run ralph CLI directly via app::run() without spawning processes
//! - `mock_executor_with_success()`: Mock executor for successful agent execution
//! - `mock_executor_with_agent_failure()`: Mock executor for agent failure scenarios

use clap::Parser;
use std::sync::Arc;

/// Run ralph workflow directly without spawning a process.
///
/// This function calls `ralph_workflow::app::run()` directly instead of
/// spawning the ralph binary process. This eliminates process spawning
/// violations in integration tests.
///
/// For output verification, tests should check:
/// - File side effects (files created/modified)
/// - Error conditions (via returned Result)
/// - Log files (in .agent/logs/)
///
/// # Arguments
///
/// * `args` - Command line arguments to pass to ralph
/// * `executor` - Process executor for external process execution
///
/// # Returns
///
/// Returns `Ok(())` if ralph execution succeeded, or the error if it failed.
///
/// # Panics
///
/// - Panics if args cannot be parsed
///
/// # Usage
///
/// ```ignore
/// use crate::common::run_ralph_cli;
/// use ralph_workflow::executor::RealProcessExecutor;
/// use std::sync::Arc;
///
/// #[test]
/// fn test_init() {
///     with_default_timeout(|| {
///         let dir = TempDir::new().unwrap();
///         std::env::set_current_dir(dir.path()).unwrap();
///
///         let executor = Arc::new(RealProcessExecutor::new());
///         run_ralph_cli(&["--init"], executor).unwrap();
///
///         // Check side effects
///         assert!(dir.path().join("PROMPT.md").exists());
///     });
/// }
/// ```
pub fn run_ralph_cli(
    args: &[&str],
    executor: Arc<dyn ralph_workflow::executor::ProcessExecutor>,
) -> anyhow::Result<()> {
    // Build argv: binary name + args
    let mut argv: Vec<String> = vec!["ralph".to_string()];
    argv.extend(args.iter().map(|s| s.to_string()));

    // Parse args using clap directly (same as main.rs does)
    let parsed_args = ralph_workflow::cli::Args::try_parse_from(&argv).expect("args should parse");

    // Set environment variables for test isolation
    std::env::set_var("RALPH_INTERACTIVE", "0");
    std::env::set_var("RALPH_CI", "1");

    // Call app::run() directly with executor (no process spawning)
    ralph_workflow::app::run(parsed_args, executor)
}

/// Create a MockProcessExecutor configured for successful agent execution.
///
/// This helper prevents tests from spawning real AI agent processes by
/// pre-configuring a MockProcessExecutor with successful results for all
/// common agent types.
///
/// # Returns
///
/// Returns an Arc-wrapped MockProcessExecutor that returns success (exit code 0)
/// for all agent commands (claude, codex, opencode, etc.).
///
/// # Usage
///
/// ```ignore
/// use crate::common::mock_executor_with_success;
///
/// #[test]
/// fn test_workflow_with_agent() {
///     with_default_timeout(|| {
///         let executor = mock_executor_with_success();
///         run_ralph_cli(&["--init"], executor).unwrap();
///         // Agent calls are mocked - no real processes spawned
///     });
/// }
/// ```
///
/// # Integration Test Style Guide Compliance
///
/// This helper enforces the style guide rule: **NO Process Spawning in Tests**.
/// Tests must use MockProcessExecutor instead of RealProcessExecutor to avoid
/// spawning real agent subprocesses.
pub fn mock_executor_with_success() -> Arc<dyn ralph_workflow::executor::ProcessExecutor> {
    Arc::new(
        ralph_workflow::executor::MockProcessExecutor::new()
            .with_agent_result(
                "claude",
                Ok(ralph_workflow::executor::AgentCommandResult::success()),
            )
            .with_agent_result(
                "codex",
                Ok(ralph_workflow::executor::AgentCommandResult::success()),
            )
            .with_agent_result(
                "opencode",
                Ok(ralph_workflow::executor::AgentCommandResult::success()),
            )
            .with_agent_result(
                "glm",
                Ok(ralph_workflow::executor::AgentCommandResult::success()),
            )
            .with_agent_result(
                "aider",
                Ok(ralph_workflow::executor::AgentCommandResult::success()),
            ),
    )
}

/// Create a MockProcessExecutor configured for agent failure scenarios.
///
/// This helper tests error handling when agents fail. Use this to verify
/// that the pipeline properly handles agent errors without hanging or crashing.
///
/// # Arguments
///
/// * `command_pattern` - Pattern to match against agent commands (e.g., "claude", "codex")
/// * `exit_code` - Exit code the agent should return (non-zero = failure)
/// * `stderr` - Stderr content to return from the failed agent
///
/// # Returns
///
/// Returns an Arc-wrapped MockProcessExecutor configured for the specified failure.
///
/// # Usage
///
/// ```ignore
/// use crate::common::mock_executor_with_agent_failure;
///
/// #[test]
/// fn test_agent_failure_handling() {
///     with_default_timeout(|| {
///         let executor = mock_executor_with_agent_failure(
///             "claude",
///             1,
///             "Agent failed to process request"
///         );
///         let result = run_ralph_cli(&["--init"], executor);
///         // Verify error handling
///         assert!(result.is_err());
///     });
/// }
/// ```
///
/// # Integration Test Style Guide Compliance
///
/// This helper enforces the style guide rule: tests must mock at architectural
/// boundaries. Agent spawning is an external dependency, so tests use MockProcessExecutor
/// to simulate failures without actually spawning processes.
pub fn mock_executor_with_agent_failure(
    command_pattern: &str,
    exit_code: i32,
    stderr: &str,
) -> Arc<dyn ralph_workflow::executor::ProcessExecutor> {
    Arc::new(
        ralph_workflow::executor::MockProcessExecutor::new().with_agent_result(
            command_pattern,
            Ok(ralph_workflow::executor::AgentCommandResult::failure(
                exit_code, stderr,
            )),
        ),
    )
}
