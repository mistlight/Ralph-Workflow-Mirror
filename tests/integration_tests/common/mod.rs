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
//!
//! # Working Directory Management
//!
//! Integration tests support parallel execution by passing `working_dir` to
//! `run_ralph_cli()` instead of changing the global current working directory.
//!
//! ```ignore
//! let dir = TempDir::new().unwrap();
//! let executor = mock_executor_with_success();
//! run_ralph_cli(&["--init"], executor, Some(dir.path())).unwrap();
//! ```

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
/// * `working_dir` - Optional working directory override for test parallelism.
///   When provided, ralph uses this path without changing the global CWD.
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
///         // No need to change CWD - pass working_dir instead
///         let executor = mock_executor_with_success();
///         run_ralph_cli(&["--init"], executor, Some(dir.path())).unwrap();
///
///         // Check side effects
///         assert!(dir.path().join("PROMPT.md").exists());
///     });
/// }
/// ```
pub fn run_ralph_cli(
    args: &[&str],
    executor: Arc<dyn ralph_workflow::executor::ProcessExecutor>,
    working_dir: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    run_ralph_cli_with_config(args, executor, None, working_dir)
}

/// Run ralph workflow directly without spawning a process, with optional config path.
///
/// This is the internal implementation that accepts an optional config path to override
/// the default config file location. Used for tests that need to control
/// configuration values like developer_iters and reviewer_reviews.
///
/// # Arguments
///
/// * `args` - Command line arguments to pass to ralph
/// * `executor` - Process executor for external process execution
/// * `config_path` - Optional path to config file (defaults to None)
/// * `working_dir` - Optional working directory override for test parallelism.
///   When provided, ralph uses this path without changing the global CWD.
pub fn run_ralph_cli_with_config(
    args: &[&str],
    executor: Arc<dyn ralph_workflow::executor::ProcessExecutor>,
    config_path: Option<&std::path::Path>,
    working_dir: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    // Build argv: binary name + args
    let mut argv: Vec<String> = vec!["ralph".to_string()];

    // Add config path if provided
    if let Some(path) = config_path {
        argv.push("--config".to_string());
        argv.push(path.to_string_lossy().to_string());
    }

    argv.extend(args.iter().map(|s| s.to_string()));

    // Parse args using clap directly (same as main.rs does)
    // Handle --version and --help flags which exit successfully
    let mut parsed_args = match ralph_workflow::cli::Args::try_parse_from(&argv) {
        Ok(args) => args,
        Err(e) if matches!(e.kind(), ErrorKind::DisplayVersion | ErrorKind::DisplayHelp) => {
            // These are successful exits (version printed or help shown)
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    // Set working_dir_override if provided (enables test parallelism)
    if let Some(dir) = working_dir {
        parsed_args.working_dir_override = Some(dir.to_path_buf());
    }

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
///         run_ralph_cli(&["--init"], executor, Some(dir.path())).unwrap();
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

/// Create a test config file with minimal settings to override user's global config.
///
/// This helper creates a config file in the test directory that sets
/// developer_iters=0 and reviewer_reviews=0, preventing the pipeline from
/// attempting to execute real agents during tests.
///
/// This is necessary because the user's global config file (~/.config/ralph-workflow.toml)
/// has developer_iters=5 which would override environment variables if config loading
/// is not working correctly.
///
/// # Arguments
///
/// * `dir` - TempDir where the config file should be created
///
/// # Returns
///
/// Returns the path to the created config file.
///
/// # Usage
///
/// ```ignore
/// use crate::common::{mock_executor_with_success, run_ralph_cli, create_test_config};
///
/// #[test]
/// fn test_workflow() {
///     with_default_timeout(|| {
///         let dir = TempDir::new().unwrap();
///         let config_path = create_test_config(&dir);
///
///         let executor = mock_executor_with_success();
///         run_ralph_cli_with_config(&[], executor, Some(config_path.as_path()), Some(dir.path())).unwrap();
///     });
/// }
/// ```
pub fn create_test_config(dir: &tempfile::TempDir) -> std::path::PathBuf {
    let config_path = dir.path().join("ralph-workflow-test.toml");
    let config_content = r#"# Test configuration for integration tests
# This overrides the global config to prevent agent execution

[general]
# Disable all agent execution for tests
developer_iters = 0
reviewer_reviews = 0
interactive = false
isolation_mode = true
checkpoint_enabled = true
auto_rebase = false

# Verbosity: quiet for cleaner test output
verbosity = 0

# Stack detection: disabled for tests
auto_detect_stack = false

[agent_chain]
# Use simple agent chain for tests
developer = ["codex"]
reviewer = ["codex"]
commit = ["codex"]
"#;

    std::fs::write(&config_path, config_content).expect("Failed to create test config file");

    config_path
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

/// RAII guard to restore environment variables on drop.
///
/// This struct stores the original values of environment variables
/// and restores them when dropped, ensuring tests don't pollute
/// each other's environment.
///
/// # Example
///
/// ```ignore
/// use crate::common::EnvGuard;
///
/// #[test]
/// fn test_example() {
///     with_default_timeout(|| {
///         let guard = EnvGuard::new(&["VAR1", "VAR2"]);
///         guard.set(&[("VAR1", Some("value1")), ("VAR2", Some("value2"))]);
///         // Test code here
///         // When guard is dropped, VAR1 and VAR2 are restored to original values
///     });
/// }
/// ```
pub struct EnvGuard {
    vars: Vec<(String, Option<String>)>,
}

impl EnvGuard {
    /// Create a new guard for the specified environment variables.
    ///
    /// This captures the current values of all specified variables
    /// so they can be restored when the guard is dropped.
    ///
    /// # Arguments
    ///
    /// * `keys` - Slice of environment variable names to guard
    pub fn new(keys: &[&str]) -> Self {
        let vars = keys
            .iter()
            .map(|k| (k.to_string(), std::env::var(k).ok()))
            .collect();
        Self { vars }
    }

    /// Set environment variables from a list of key-value pairs.
    ///
    /// Each tuple is (key, value) where value can be None to unset.
    ///
    /// # Arguments
    ///
    /// * `settings` - Slice of tuples (key, value) where value is Option<&str>
    pub fn set(&self, settings: &[(&str, Option<&str>)]) {
        for (key, value) in settings {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}

impl Drop for EnvGuard {
    fn drop(&mut self) {
        for (key, value) in &self.vars {
            match value {
                Some(v) => std::env::set_var(key, v),
                None => std::env::remove_var(key),
            }
        }
    }
}
