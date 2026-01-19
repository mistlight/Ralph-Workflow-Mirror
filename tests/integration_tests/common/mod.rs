//! Common utilities for integration tests

use std::{env, fs, path::PathBuf};

/// Get the path to the ralph binary for testing
///
/// This function locates the ralph binary built by Cargo.
pub fn ralph_cmd() -> assert_cmd::Command {
    let bin_path = ralph_bin_path();
    let mut cmd = assert_cmd::Command::new(bin_path);
    cmd.arg("--test-mode"); // Enable test mode for immediate retries
    cmd
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

    // Then check the current directory's target subdirectory
    let current_dir = env::current_dir().unwrap();
    let mut target_dir = current_dir.join("target");

    // If we're in the tests directory, go up to the workspace root
    if target_dir.join("Cargo.toml").exists() {
        // We're at the workspace level, use target directly
    } else if current_dir.ends_with("tests") {
        // We're in the tests directory, go up to workspace root
        target_dir = current_dir.parent().unwrap().join("target");
    }

    // Use debug or release based on profile
    // During tests, cargo uses the debug profile by default
    let profile = env::var("PROFILE").unwrap_or_else(|_| "debug".to_string());
    target_dir.join(profile)
}

// ============================================================================
// File-Based Mock Helpers
// ============================================================================

/// Mock agent output by pre-creating files that agents would write.
///
/// This approach avoids spawning shell scripts (which violates the
/// integration test guidelines) by directly creating the expected
/// output files that the pipeline reads.
pub struct MockAgentOutput {
    /// The base directory for the test
    pub dir: PathBuf,
}

impl MockAgentOutput {
    /// Create a new mock agent output helper for the given directory.
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    /// Create the .agent directory and any needed subdirectories.
    fn ensure_agent_dir(&self) {
        let agent_dir = self.dir.join(".agent");
        fs::create_dir_all(&agent_dir).expect("Failed to create .agent directory");
        let logs_dir = agent_dir.join("logs");
        fs::create_dir_all(&logs_dir).expect("Failed to create .agent/logs directory");
    }

    /// Write a plan to `.agent/PLAN.md` (developer output).
    pub fn with_plan(&self, content: impl AsRef<str>) -> &Self {
        self.ensure_agent_dir();
        let plan_path = self.dir.join(".agent/PLAN.md");
        fs::write(&plan_path, content.as_ref()).expect("Failed to write PLAN.md");
        self
    }
}

/// Extension trait to make it easy to create mock agent output from TempDir.
pub trait MockAgentOutputExt {
    /// Get a MockAgentOutput helper for this directory.
    fn mock_agent(&self) -> MockAgentOutput;
}

impl MockAgentOutputExt for tempfile::TempDir {
    fn mock_agent(&self) -> MockAgentOutput {
        MockAgentOutput::new(self.path().to_path_buf())
    }
}
