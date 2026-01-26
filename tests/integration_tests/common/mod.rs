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
//! - Use **dependency injection** for configuration, not environment variables
//!
//! The utilities in this module support proper integration test patterns:
//! - `run_ralph_cli_injected()`: Run ralph CLI via app::run_with_config() with injected Config
//! - `create_test_config_struct()`: Create a Config struct directly for dependency injection
//! - `mock_executor_with_success()`: Mock executor for successful agent execution
//! - `mock_executor_for_git_success()`: Mock executor for git command success
//!
//! # Configuration via Dependency Injection
//!
//! Tests use `create_test_config_struct()` to build a Config directly, bypassing
//! environment variable loading. This ensures tests are deterministic and can
//! run in parallel without affecting each other.
//!
//! ```ignore
//! let dir = TempDir::new().unwrap();
//! let config = create_test_config_struct();
//! let executor = mock_executor_with_success();
//! run_ralph_cli_injected(&["--reset-start-commit"], executor, config, Some(dir.path())).unwrap();
//! ```

use clap::error::ErrorKind;
use clap::Parser;
use ralph_workflow::config::ConfigEnvironment;
use std::sync::{Arc, Mutex};

/// Global lock for CWD-changing operations.
///
/// The production code uses relative paths extensively, and CWD is process-global.
/// Tests that change CWD must hold this lock to prevent races.
///
/// This lock is used internally by `run_ralph_cli_injected()` when a `working_dir` is provided.
static CWD_LOCK: Mutex<()> = Mutex::new(());

/// Run ralph workflow with injected Config (no environment variable loading).
///
/// This is the recommended approach for integration tests per the style guide
/// (Rule 3: Use Dependency Injection for Testability). It bypasses all
/// environment variable loading and uses the provided Config directly.
///
/// # Arguments
///
/// * `args` - Command line arguments to pass to ralph
/// * `executor` - Process executor for external process execution
/// * `config` - Pre-built Config struct (bypasses env var loading)
/// * `working_dir` - Optional working directory override
///
/// # Example
///
/// ```ignore
/// use crate::common::{create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected};
///
/// #[test]
/// fn test_workflow() {
///     with_default_timeout(|| {
///         let dir = TempDir::new().unwrap();
///         let config = create_test_config_struct();
///         let executor = mock_executor_with_success();
///         run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
///     });
/// }
/// ```
pub fn run_ralph_cli_injected(
    args: &[&str],
    executor: Arc<dyn ralph_workflow::executor::ProcessExecutor>,
    config: ralph_workflow::config::Config,
    working_dir: Option<&std::path::Path>,
) -> anyhow::Result<()> {
    // Build argv: binary name + args
    let mut argv: Vec<String> = vec!["ralph".to_string()];
    argv.extend(args.iter().map(|s| s.to_string()));

    // Parse args using clap directly
    let mut parsed_args = match ralph_workflow::cli::Args::try_parse_from(&argv) {
        Ok(args) => args,
        Err(e) if matches!(e.kind(), ErrorKind::DisplayVersion | ErrorKind::DisplayHelp) => {
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    // Set working_dir_override if provided
    if let Some(dir) = working_dir {
        parsed_args.working_dir_override = Some(dir.to_path_buf());
    }

    // Use real path resolver by default
    run_ralph_cli_with_resolver(
        parsed_args,
        executor,
        config,
        working_dir,
        &ralph_workflow::config::RealConfigEnvironment,
    )
}

/// Run ralph workflow with injected Config and custom path resolver.
///
/// This is the same as [`run_ralph_cli_injected`] but accepts a custom
/// [`ConfigEnvironment`] for testing init commands that need to create
/// config files at specific paths.
///
/// # Arguments
///
/// * `args` - Command line arguments to pass to ralph
/// * `executor` - Process executor for external process execution
/// * `config` - Pre-built Config struct (bypasses env var loading)
/// * `working_dir` - Optional working directory override
/// * `path_resolver` - Custom path resolver for init commands
///
/// # Example
///
/// ```ignore
/// use crate::common::{create_test_config_struct, mock_executor_with_success, run_ralph_cli_with_path_resolver};
///
/// #[test]
/// fn test_init_workflow() {
///     with_default_timeout(|| {
///         let dir = TempDir::new().unwrap();
///         let config = create_test_config_struct();
///         let executor = mock_executor_with_success();
///         let resolver = TestConfigEnvironment::new()
///             .with_unified_config_path(dir.path().join("ralph-workflow.toml"))
///             .with_prompt_path(dir.path().join("PROMPT.md"));
///         run_ralph_cli_with_path_resolver(&["--init"], executor, config, Some(dir.path()), &resolver).unwrap();
///     });
/// }
/// ```
pub fn run_ralph_cli_with_path_resolver<P: ConfigEnvironment>(
    args: &[&str],
    executor: Arc<dyn ralph_workflow::executor::ProcessExecutor>,
    config: ralph_workflow::config::Config,
    working_dir: Option<&std::path::Path>,
    path_resolver: &P,
) -> anyhow::Result<()> {
    // Build argv: binary name + args
    let mut argv: Vec<String> = vec!["ralph".to_string()];
    argv.extend(args.iter().map(|s| s.to_string()));

    // Parse args using clap directly
    let mut parsed_args = match ralph_workflow::cli::Args::try_parse_from(&argv) {
        Ok(args) => args,
        Err(e) if matches!(e.kind(), ErrorKind::DisplayVersion | ErrorKind::DisplayHelp) => {
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    // Set working_dir_override if provided
    if let Some(dir) = working_dir {
        parsed_args.working_dir_override = Some(dir.to_path_buf());
    }

    run_ralph_cli_with_resolver(parsed_args, executor, config, working_dir, path_resolver)
}

/// Internal helper that runs ralph with parsed args and resolver.
fn run_ralph_cli_with_resolver<P: ConfigEnvironment>(
    parsed_args: ralph_workflow::cli::Args,
    executor: Arc<dyn ralph_workflow::executor::ProcessExecutor>,
    config: ralph_workflow::config::Config,
    working_dir: Option<&std::path::Path>,
    path_resolver: &P,
) -> anyhow::Result<()> {
    // Create test registry with built-in agents only
    let registry = create_test_registry();

    // If working_dir is provided, we need to lock CWD and restore it after
    if working_dir.is_some() {
        let _lock = CWD_LOCK.lock().unwrap_or_else(|e| e.into_inner());
        let original_cwd = std::env::current_dir().ok();

        // Use run_with_config_and_resolver which bypasses env var loading
        let result = ralph_workflow::app::run_with_config_and_resolver(
            parsed_args,
            executor,
            config,
            registry,
            path_resolver,
        );

        if let Some(cwd) = original_cwd {
            let _ = std::env::set_current_dir(cwd);
        }

        result
    } else {
        ralph_workflow::app::run_with_config_and_resolver(
            parsed_args,
            executor,
            config,
            registry,
            path_resolver,
        )
    }
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

/// Create a Config struct directly for dependency injection in tests.
///
/// This function creates a Config struct with test-appropriate defaults,
/// bypassing environment variable loading entirely. This is the recommended
/// approach per the Integration Test Style Guide (Rule 3: Dependency Injection).
///
/// # Returns
///
/// Returns a Config struct with:
/// - `developer_iters = 0` (skip development phase)
/// - `reviewer_reviews = 0` (skip review phase)
/// - `interactive = false` (no prompts)
/// - `isolation_mode = true` (clean context each run)
/// - `checkpoint_enabled = true`
/// - `auto_detect_stack = false`
/// - `verbosity = Quiet`
///
/// # Usage
///
/// ```ignore
/// use crate::common::{create_test_config_struct, mock_executor_with_success, run_ralph_cli_injected};
///
/// #[test]
/// fn test_workflow() {
///     with_default_timeout(|| {
///         let dir = TempDir::new().unwrap();
///         let config = create_test_config_struct();
///         let executor = mock_executor_with_success();
///         run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
///     });
/// }
/// ```
pub fn create_test_config_struct() -> ralph_workflow::config::Config {
    ralph_workflow::config::Config::test_default()
}

/// Create a test Config with custom isolation mode setting.
///
/// This is useful for tests that specifically need to test isolation mode behavior.
pub fn create_test_config_struct_with_isolation(
    isolation_mode: bool,
) -> ralph_workflow::config::Config {
    ralph_workflow::config::Config::test_default().with_isolation_mode(isolation_mode)
}

/// Create a minimal agent registry for tests.
///
/// Returns a registry with built-in agents only (no config file loading).
pub fn create_test_registry() -> ralph_workflow::agents::AgentRegistry {
    ralph_workflow::agents::AgentRegistry::with_builtins_only()
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
