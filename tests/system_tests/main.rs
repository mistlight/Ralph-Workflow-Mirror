//! System tests - real filesystem and git operations.
//!
//! These tests are **NOT** part of CI. Run manually as sanity checks.
//! See `SYSTEM_TESTS.md` for guidelines.
//!
//! # Running System Tests
//!
//! ```bash
//! cargo test -p ralph-workflow-system-tests
//! ```
//!
//! # When to Use System Tests
//!
//! Use system tests ONLY for:
//! - Real git operations (rebase, merge, conflict resolution)
//! - `WorkspaceFs` implementation testing
//! - File permission/symlink edge cases
//! - Cross-platform filesystem behavior
//!
//! # Allowed Patterns
//!
//! Unlike integration tests, system tests MAY use:
//! - `TempDir` for isolated test directories
//! - `std::fs::*` for real filesystem operations
//! - `git2` for real git repository operations
//!
//! # NOT Allowed
//!
//! - Process spawning (`std::process::Command`)
//! - Network calls
//! - Tests over 1000 lines

mod common;
mod test_timeout;

// Test modules using real filesystem/git operations
mod rebase;

// Future modules:
// mod git;
// mod workspace_fs;
