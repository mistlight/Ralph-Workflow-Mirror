//! Config and init integration tests.
//!
//! These tests verify configuration file creation and initialization behavior.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** via effect capture
//! - Uses `MockAppEffectHandler` AND `MockEffectHandler` for git/filesystem isolation
//! - Uses `MemoryConfigEnvironment` for config path injection
//! - NO `TempDir`, `std::fs`, or real git operations
//! - Tests are deterministic and verify effects, not real filesystem state
//!
//! # Note on Init Commands
//!
//! The --init-legacy and --init-global commands write directly to the filesystem
//! and are not fully mockable via the effect system. Tests for these commands
//! use `MemoryConfigEnvironment` where possible but some legacy behavior tests
//! may need to be in the system tests package instead.

mod init_and_defaults;
mod local_and_validation;
mod modes_and_review;
mod worktree_and_merge;

use ralph_workflow::app::mock_effect_handler::MockAppEffectHandler;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::PipelineState;
use std::path::PathBuf;

/// Standard PROMPT.md content for config tests.
const STANDARD_PROMPT: &str = r"## Goal

Do something.

## Acceptance

- Tests pass
";

/// Create mock handlers with standard setup for config tests.
fn create_config_test_handlers() -> (MockAppEffectHandler, MockEffectHandler) {
    let app_handler = MockAppEffectHandler::new()
        .with_head_oid("a".repeat(40))
        .with_cwd(PathBuf::from("/mock/repo"))
        .with_file("PROMPT.md", STANDARD_PROMPT)
        .with_diff("diff --git a/test.txt b/test.txt\n+new content")
        .with_staged_changes(true);

    let effect_handler = MockEffectHandler::new(PipelineState::initial(0, 0));

    (app_handler, effect_handler)
}
