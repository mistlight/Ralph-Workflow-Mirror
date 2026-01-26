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
//! - Use `MockAppEffectHandler` with `run_ralph_cli_with_handler()` or
//!   `run_ralph_cli_with_handlers()` for filesystem/git isolation
//! - NEVER use `TempDir`, `std::fs`, or real git operations in integration tests
//! - NEVER use `cfg!(test)` branches in production code
//! - Use **dependency injection** for configuration, not environment variables
//!
//! The utilities in this module support proper integration test patterns:
//! - `run_ralph_cli_with_handler()`: Run ralph CLI with MockAppEffectHandler
//! - `run_ralph_cli_with_handlers()`: Run ralph CLI with both app and reducer mock handlers
//! - `create_test_config_struct()`: Create a Config struct directly for dependency injection
//! - `mock_executor_with_success()`: Mock executor for successful agent execution
//!
//! # Configuration via Dependency Injection
//!
//! Tests use `create_test_config_struct()` and `MockAppEffectHandler` to run
//! entirely in-memory, without real filesystem or git operations:
//!
//! ```ignore
//! use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
//!
//! let mut handler = MockAppEffectHandler::new()
//!     .with_head_oid("a".repeat(40))
//!     .with_cwd(PathBuf::from("/mock/repo"))
//!     .with_file("PROMPT.md", "# Test\n## Goal\nTest\n## Acceptance\n- Pass");
//!
//! let config = create_test_config_struct();
//! let executor = mock_executor_with_success();
//!
//! run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();
//!
//! // Verify effects via handler.captured()
//! assert!(handler.captured().iter().any(|e| matches!(e, AppEffect::GitSaveStartCommit)));
//! ```

use clap::error::ErrorKind;
use clap::Parser;
use ralph_workflow::config::ConfigEnvironment;
use std::sync::{Arc, Mutex};

/// Create a MemoryWorkspace from a MockAppEffectHandler.
///
/// This function creates a MemoryWorkspace pre-populated with all files
/// from the handler's in-memory filesystem. This allows the pipeline
/// to use MemoryWorkspace for file operations while the handler remains
/// the source of truth for effect handling.
fn create_workspace_from_handler(
    handler: &ralph_workflow::app::mock_effect_handler::MockAppEffectHandler,
) -> std::sync::Arc<dyn ralph_workflow::workspace::Workspace> {
    let cwd = handler.get_cwd();
    let mut workspace = ralph_workflow::workspace::MemoryWorkspace::new(cwd);

    // Copy all files from the handler to the workspace
    for (path, content) in handler.get_all_files() {
        // MemoryWorkspace stores files relative to root, so convert PathBuf to &str
        if let Some(path_str) = path.to_str() {
            workspace = workspace.with_file(path_str, &content);
        }
    }

    std::sync::Arc::new(workspace)
}

/// Global lock for CWD-changing operations.
///
/// The production code uses relative paths extensively, and CWD is process-global.
/// Tests that change CWD must hold this lock to prevent races.
///
/// This lock is used internally by `run_ralph_cli_injected()` when a `working_dir` is provided.
static CWD_LOCK: Mutex<()> = Mutex::new(());

/// DEPRECATED: Use `run_ralph_cli_with_handler()` or `run_ralph_cli_with_handlers()` instead.
///
/// This function creates a `RealAppEffectHandler` internally, which requires
/// a real git repository and filesystem. Per INTEGRATION_TESTS.md, integration
/// tests MUST use `MockAppEffectHandler` instead of real filesystem operations.
///
/// # Migration Guide
///
/// Replace:
/// ```ignore
/// let dir = TempDir::new().unwrap();
/// let _repo = init_git_repo(&dir);
/// run_ralph_cli_injected(&[], executor, config, Some(dir.path())).unwrap();
/// ```
///
/// With:
/// ```ignore
/// let mut handler = MockAppEffectHandler::new()
///     .with_head_oid("a".repeat(40))
///     .with_cwd(PathBuf::from("/mock/repo"))
///     .with_file("PROMPT.md", "# Test\n## Goal\nTest\n## Acceptance\n- Pass");
/// run_ralph_cli_with_handler(&[], executor, config, &mut handler).unwrap();
/// ```
///
/// For full isolation (both CLI and reducer layers):
/// ```ignore
/// let mut app_handler = MockAppEffectHandler::new()
///     .with_head_oid("a".repeat(40))
///     .with_cwd(PathBuf::from("/mock/repo"))
///     .with_file("PROMPT.md", STANDARD_PROMPT);
/// let mut effect_handler = MockEffectHandler::new(PipelineState::initial(1, 0));
///
/// run_ralph_cli_with_handlers(&[], executor, config, &mut app_handler, &mut effect_handler).unwrap();
/// ```
#[deprecated(
    since = "0.1.0",
    note = "Use run_ralph_cli_with_handler() with MockAppEffectHandler instead. See INTEGRATION_TESTS.md."
)]
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

/// DEPRECATED: Use `run_ralph_cli_with_handler()` with a custom path in MockAppEffectHandler instead.
///
/// This function creates a `RealAppEffectHandler` internally, which requires
/// a real git repository and filesystem. Per INTEGRATION_TESTS.md, integration
/// tests MUST use `MockAppEffectHandler` instead.
///
/// # Migration Guide
///
/// For tests that need custom config paths, configure the MockAppEffectHandler
/// to have the expected files at the right paths:
///
/// ```ignore
/// let mut handler = MockAppEffectHandler::new()
///     .with_head_oid("a".repeat(40))
///     .with_cwd(PathBuf::from("/mock/repo"))
///     .with_file("/mock/repo/ralph-workflow.toml", "# config")
///     .with_file("/mock/repo/PROMPT.md", STANDARD_PROMPT);
///
/// run_ralph_cli_with_handler(&["--init"], executor, config, &mut handler).unwrap();
/// ```
#[deprecated(
    since = "0.1.0",
    note = "Use run_ralph_cli_with_handler() with MockAppEffectHandler instead. See INTEGRATION_TESTS.md."
)]
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

    // Create effect handler for git/filesystem operations
    let mut handler = ralph_workflow::app::effect_handler::RealAppEffectHandler::new();

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
            &mut handler,
            None, // Use default WorkspaceFs
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
            &mut handler,
            None, // Use default WorkspaceFs
        )
    }
}

/// Run ralph workflow with a custom MockAppEffectHandler.
///
/// This function allows tests to inject a mock effect handler to verify
/// that the CLI properly delegates git/filesystem operations to the handler
/// instead of calling git_helpers directly.
///
/// **NOTE:** This function uses `MemoryConfigEnvironment` for full isolation.
/// The config environment is pre-configured with:
/// - PROMPT.md path from the handler's cwd
/// - No pre-existing files (unless added via the handler)
///
/// # Arguments
///
/// * `args` - Command line arguments to pass to ralph
/// * `executor` - Process executor for external process execution
/// * `config` - Pre-built Config struct (bypasses env var loading)
/// * `handler` - Mock effect handler to capture effects
///
/// # Example
///
/// ```ignore
/// use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
///
/// let mut handler = MockAppEffectHandler::new()
///     .with_head_oid("abc123")
///     .with_cwd(PathBuf::from("/mock/repo"));
/// let config = create_test_config_struct();
/// let executor = mock_executor_with_success();
///
/// run_ralph_cli_with_handler(&["--reset-start-commit"], executor, config, &mut handler).unwrap();
///
/// // Verify effects were captured
/// assert!(handler.captured().iter().any(|e| matches!(e, AppEffect::GitResetStartCommit)));
/// ```
pub fn run_ralph_cli_with_handler(
    args: &[&str],
    executor: Arc<dyn ralph_workflow::executor::ProcessExecutor>,
    config: ralph_workflow::config::Config,
    handler: &mut ralph_workflow::app::mock_effect_handler::MockAppEffectHandler,
) -> anyhow::Result<()> {
    // Build argv: binary name + args
    let mut argv: Vec<String> = vec!["ralph".to_string()];
    argv.extend(args.iter().map(|s| s.to_string()));

    // Parse args using clap directly
    let parsed_args = match ralph_workflow::cli::Args::try_parse_from(&argv) {
        Ok(args) => args,
        Err(e) if matches!(e.kind(), ErrorKind::DisplayVersion | ErrorKind::DisplayHelp) => {
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    // Create test registry with built-in agents only
    let registry = create_test_registry();

    // Create a MemoryConfigEnvironment for full isolation.
    // Configure paths based on handler's cwd.
    let cwd = handler.get_cwd();
    let config_env = ralph_workflow::config::MemoryConfigEnvironment::new()
        .with_prompt_path(cwd.join("PROMPT.md"))
        .with_unified_config_path(cwd.join(".config/ralph-workflow.toml"));

    // Create a MemoryWorkspace that syncs with the MockAppEffectHandler's files.
    // This ensures file operations use the handler's in-memory filesystem.
    let workspace = create_workspace_from_handler(handler);

    // Use run_with_config_and_resolver with the provided handler, memory config env, and memory workspace
    ralph_workflow::app::run_with_config_and_resolver(
        parsed_args,
        executor,
        config,
        registry,
        &config_env,
        handler,
        Some(workspace),
    )
}

/// Run ralph workflow with BOTH handlers for full isolation.
///
/// This is the ultimate test entry point that injects:
/// - `MockAppEffectHandler` for CLI-layer operations (git require repo, set cwd, etc.)
/// - `MockEffectHandler` for reducer-layer operations (create commit, run rebase, etc.)
/// - `MemoryConfigEnvironment` for config file operations (init commands, etc.)
///
/// Using both handlers ensures tests make **ZERO real git calls at any layer**.
///
/// # Arguments
///
/// * `args` - Command line arguments to pass to ralph
/// * `executor` - Process executor (use `mock_executor_with_success()`)
/// * `config` - Pre-built Config struct
/// * `app_handler` - Mock handler for CLI-layer operations
/// * `effect_handler` - Mock handler for reducer-layer operations
///
/// # Example
///
/// ```ignore
/// use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
/// use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
/// use ralph_workflow::reducer::PipelineState;
///
/// let mut app_handler = MockAppEffectHandler::new()
///     .with_head_oid("abc123")
///     .with_cwd(PathBuf::from("/mock/repo"))
///     .with_file("PROMPT.md", "# Test");
/// let mut effect_handler = MockEffectHandler::new(PipelineState::initial(1, 0));
///
/// run_ralph_cli_with_handlers(
///     &[],
///     executor,
///     config,
///     &mut app_handler,
///     &mut effect_handler,
/// )?;
///
/// // Verify NO real git operations at either layer
/// assert!(app_handler.captured().iter().any(|e| matches!(e, AppEffect::GitRequireRepo)));
/// assert!(effect_handler.captured_effects().iter().any(|e| matches!(e, Effect::CreateCommit { .. })));
/// ```
pub fn run_ralph_cli_with_handlers(
    args: &[&str],
    executor: Arc<dyn ralph_workflow::executor::ProcessExecutor>,
    config: ralph_workflow::config::Config,
    app_handler: &mut ralph_workflow::app::mock_effect_handler::MockAppEffectHandler,
    effect_handler: &mut ralph_workflow::reducer::mock_effect_handler::MockEffectHandler,
) -> anyhow::Result<()> {
    // Build argv: binary name + args
    let mut argv: Vec<String> = vec!["ralph".to_string()];
    argv.extend(args.iter().map(|s| s.to_string()));

    // Parse args using clap directly
    let parsed_args = match ralph_workflow::cli::Args::try_parse_from(&argv) {
        Ok(args) => args,
        Err(e) if matches!(e.kind(), ErrorKind::DisplayVersion | ErrorKind::DisplayHelp) => {
            return Ok(());
        }
        Err(e) => return Err(e.into()),
    };

    // Create test registry with built-in agents only
    let registry = create_test_registry();

    // Create a MemoryConfigEnvironment for full isolation.
    // Configure paths based on handler's cwd.
    let cwd = app_handler.get_cwd();
    let config_env = ralph_workflow::config::MemoryConfigEnvironment::new()
        .with_prompt_path(cwd.join("PROMPT.md"))
        .with_unified_config_path(cwd.join(".config/ralph-workflow.toml"));

    // Create a MemoryWorkspace that syncs with the MockAppEffectHandler's files.
    let workspace = create_workspace_from_handler(app_handler);

    // Use run_with_config_and_handlers with both handlers and memory workspace
    ralph_workflow::app::run_with_config_and_handlers(
        ralph_workflow::app::RunWithHandlersParams {
            args: parsed_args,
            executor,
            config,
            registry,
            path_resolver: &config_env,
            app_handler,
            effect_handler,
            workspace: Some(workspace),
            _marker: std::marker::PhantomData,
        },
    )
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
