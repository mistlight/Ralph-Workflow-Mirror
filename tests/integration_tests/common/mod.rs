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
//! - `ralph_cmd()`: Get a command to invoke the ralph binary for CLI testing
//! - `ralph_bin_path()`: Get the path to the ralph binary for custom invocation

use std::{env, path::PathBuf};

/// Get the path to the ralph binary for testing
///
/// This function locates the ralph binary built by Cargo.
pub fn ralph_cmd() -> assert_cmd::Command {
    let bin_path = ralph_bin_path();
    assert_cmd::Command::new(bin_path)
}

/// Get the path to the ralph binary as a String
///
/// This is useful when you need to use std::process::Command instead of
/// assert_cmd::Command.
pub fn ralph_bin_path() -> String {
    // First, try the environment variable set by Cargo when running tests
    // in the same package as the binary
    if let Ok(path) = env::var("CARGO_BIN_EXE_ralph") {
        return path;
    }

    // Otherwise, find the binary in the target directory
    // This works when tests are in a separate package
    let mut path = find_cargo_target_dir();
    path.push("ralph");

    // On Windows, cargo binaries have .exe extension
    if cfg!(windows) {
        path.set_extension("exe");
    }

    // Verify the binary exists
    if path.exists() {
        path.to_str().unwrap().to_string()
    } else {
        panic!(
            "ralph binary not found at {}; run `cargo build --bin ralph` first",
            path.display()
        )
    }
}

/// Find the Cargo target directory
fn find_cargo_target_dir() -> PathBuf {
    // Check CARGO_TARGET_DIR environment variable first
    if let Ok(target_dir) = env::var("CARGO_TARGET_DIR") {
        return PathBuf::from(target_dir);
    }

    // Use CARGO_MANIFEST_DIR which is set at compile time and points to the
    // package directory (tests/ in this case). This is more reliable than
    // current_dir() which can be affected by test parallelism.
    let manifest_dir = env!("CARGO_MANIFEST_DIR");
    let manifest_path = PathBuf::from(manifest_dir);

    // Go up from tests/ to workspace root, then into target/
    let workspace_root = manifest_path
        .parent()
        .expect("tests/ should have a parent directory");
    let target_dir = workspace_root.join("target");

    // Use debug or release based on profile
    // During tests, cargo uses the debug profile by default
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    target_dir.join(profile)
}
