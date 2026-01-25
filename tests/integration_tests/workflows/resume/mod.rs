//! Resume functionality integration tests.
//!
//! Tests are organized into focused modules:
//! - checkpoint: Checkpoint creation, content, and cleanup
//! - basic: Resume flags and working directory validation
//! - phases: Resume from different pipeline phases
//! - preservation: Configuration and state preservation
//! - v3: V3 hardened resume features
//! - rebase: Rebase-related resume tests

mod basic;
mod checkpoint;
mod phases;
mod preservation;
mod rebase;
mod v3;

use std::fs;
use tempfile::TempDir;

/// Get the canonical working directory path.
/// This handles macOS symlinks (/var -> /private/var) which cause
/// working directory validation to fail in tests.
pub(crate) fn canonical_working_dir(dir: &TempDir) -> String {
    dir.path()
        .canonicalize()
        .unwrap()
        .to_string_lossy()
        .to_string()
}

/// Pre-create PLAN.md to skip the planning phase and avoid agent execution.
///
/// Integration tests should not spawn real agent processes. This helper
/// creates a minimal PLAN.md so tests can verify behavior without running agents.
pub(crate) fn precreate_plan_file(dir: &TempDir) {
    fs::create_dir_all(dir.path().join(".agent")).unwrap();
    fs::write(
        dir.path().join(".agent/PLAN.md"),
        "# Test Plan\n\nTest task description.\n",
    )
    .unwrap();
}
