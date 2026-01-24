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
//! - `mock_executor_for_git_success()`: Mock executor for git command success

use clap::error::ErrorKind;
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
/// # Special Cases
///
/// - `--version` and `--help` flags return `Ok(())` without running the app
///   (these are valid clap exit paths that display info and exit successfully)
///
/// # Usage
///
/// ```ignore
/// use crate::common::{run_ralph_cli, mock_executor_with_success};
///
/// #[test]
/// fn test_init() {
///     with_default_timeout(|| {
///         let dir = TempDir::new().unwrap();
///         std::env::set_current_dir(dir.path()).unwrap();
///
///         let executor = mock_executor_with_success();
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
    // Handle --version and --help flags which exit successfully
    let parsed_args = match ralph_workflow::cli::Args::try_parse_from(&argv) {
        Ok(args) => args,
        Err(e) if matches!(e.kind(), ErrorKind::DisplayVersion | ErrorKind::DisplayHelp) => {
            // These are successful exits (version printed or help shown)
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

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
///
/// # Command Mocking
///
/// This executor mocks common external commands to prevent real subprocess spawning:
/// - git commands (status, branch, rev-parse, etc.) - return empty success
/// - whoami - returns "testuser"
/// - hostname - returns "localhost"
/// - cargo commands - return empty success
pub fn mock_executor_with_success() -> Arc<dyn ralph_workflow::executor::ProcessExecutor> {
    Arc::new(
        ralph_workflow::executor::MockProcessExecutor::new()
            // git commands - return empty success (clean working tree)
            .with_output("git", "")
            // whoami - fallback for git identity
            .with_output("whoami", "testuser")
            // hostname - fallback for git identity email
            .with_output("hostname", "localhost")
            // cargo - build/test commands in rebase validation
            .with_output("cargo", "")
            // Agent commands return success
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

/// Create a MockProcessExecutor configured for git command success.
///
/// This helper provides mock responses for common git commands used in
/// rebase and other git operations, preventing real git subprocess spawning.
///
/// # Returns
///
/// Returns an Arc-wrapped MockProcessExecutor that returns successful outputs
/// for common git commands (status, branch, rebase) and all agent types.
///
/// # Usage
///
/// ```ignore
/// use crate::common::mock_executor_for_git_success;
///
/// #[test]
/// fn test_rebase_success() {
///     with_default_timeout(|| {
///         let executor = mock_executor_for_git_success();
///         let result = rebase_onto("main", &executor);
///         // Git commands are mocked - no real git subprocess spawned
///     });
/// }
/// ```
///
/// # Integration Test Style Guide Compliance
///
/// This helper enforces the style guide rule: **NO Process Spawning in Tests**.
/// Git CLI commands are an external dependency that must be mocked in tests.
///
/// # Command Mocking
///
/// This executor mocks common external commands to prevent real subprocess spawning:
/// - git commands (status, branch, rev-parse, rebase, etc.) - return empty success
/// - whoami - returns "testuser"
/// - hostname - returns "localhost"
/// - cargo commands - return empty success
pub fn mock_executor_for_git_success() -> Arc<dyn ralph_workflow::executor::ProcessExecutor> {
    Arc::new(
        ralph_workflow::executor::MockProcessExecutor::new()
            // git status --porcelain (clean working tree)
            .with_output("git", "")
            // whoami - fallback for git identity
            .with_output("whoami", "testuser")
            // hostname - fallback for git identity email
            .with_output("hostname", "localhost")
            // cargo - build/test commands in rebase validation
            .with_output("cargo", "")
            // Agent commands also return success
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
