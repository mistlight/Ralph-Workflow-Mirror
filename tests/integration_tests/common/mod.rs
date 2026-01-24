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

use clap::Parser;

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
    executor: std::sync::Arc<dyn ralph_workflow::executor::ProcessExecutor>,
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
