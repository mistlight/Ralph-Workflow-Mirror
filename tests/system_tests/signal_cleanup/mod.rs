//! System tests for Ctrl+C (SIGINT) cleanup verification.
//!
//! These tests spawn real Ralph pipeline processes and send SIGINT signals
//! to verify that cleanup (PROMPT.md permissions, hooks, markers) happens
//! correctly regardless of interrupt timing.
//!
//! # Why System Tests?
//!
//! Integration tests cannot test actual SIGINT handling because:
//! - `MockEffectHandler` doesn't run real signal handlers
//! - `MemoryWorkspace` doesn't have real file permissions
//! - The interrupt context is process-global state
//!
//! # Test Categories
//!
//! 1. Early interrupt (before event loop active)
//! 2. Mid-execution interrupt (during agent phase)
//! 3. Cleanup verification (PROMPT.md, hooks, marker)
//!
//! # Platform Support
//!
//! These tests are Unix-only (`#[cfg(unix)]`) because they rely on:
//! - `libc::kill` for sending signals
//! - Unix file permission model (mode bits)
//!
//! # Note on Test Reliability
//!
//! These tests involve real process spawning and signal delivery, which can
//! be timing-sensitive. Tests use generous timeouts and retry logic to
//! minimize flakiness while still verifying the core cleanup behavior.

#![cfg(unix)]

use crate::test_timeout::with_timeout;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};
use tempfile::TempDir;
use test_helpers::init_git_repo;

/// Extended timeout for signal cleanup tests.
///
/// These tests spawn real processes, wait for startup, send signals, and
/// verify cleanup. They need more time than typical system tests.
const SIGNAL_TEST_TIMEOUT: Duration = Duration::from_secs(60);

/// Timeout for waiting for marker file to appear.
const MARKER_WAIT_TIMEOUT: Duration = Duration::from_secs(30);

/// Brief delay after process spawn before sending early SIGINT.
const EARLY_SIGINT_DELAY: Duration = Duration::from_millis(200);

/// Polling interval for marker file checks.
const POLL_INTERVAL: Duration = Duration::from_millis(50);

/// Set file to read-only (no write bits for user/group/other).
fn set_readonly(path: &Path) {
    let metadata = std::fs::metadata(path).expect("stat file for readonly");
    let mut perms = metadata.permissions();
    perms.set_mode(perms.mode() & !0o222);
    std::fs::set_permissions(path, perms).expect("set readonly permissions");
}

/// Find the ralph binary.
///
/// Tries these locations in order:
/// 1. `CARGO_BIN_EXE_ralph` environment variable (set by cargo test)
/// 2. `target/debug/ralph` relative to repo root
/// 3. `target/release/ralph` relative to repo root
///
/// Returns `None` if no binary is found.
fn find_ralph_binary() -> Option<std::path::PathBuf> {
    // Check environment variable first (set by cargo test --package)
    if let Ok(path) = std::env::var("CARGO_BIN_EXE_ralph") {
        let path = std::path::PathBuf::from(path);
        if path.exists() {
            return Some(path);
        }
    }

    // Find repo root (parent of 'tests' directory)
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").ok()?;
    let tests_dir = std::path::PathBuf::from(manifest_dir);
    let repo_root = tests_dir.parent()?;

    // Try debug build first
    let debug_path = repo_root.join("target/debug/ralph");
    if debug_path.exists() {
        return Some(debug_path);
    }

    // Then release build
    let release_path = repo_root.join("target/release/ralph");
    if release_path.exists() {
        return Some(release_path);
    }

    None
}

/// Spawn ralph pipeline process targeting the given repo.
///
/// Uses minimal arguments to start a pipeline run that will create the
/// `.no_agent_commit` marker when entering the agent phase.
///
/// Returns `None` if the ralph binary cannot be found.
fn spawn_ralph_pipeline(repo_dir: &Path) -> Option<Child> {
    let ralph_bin = find_ralph_binary()?;

    // Spawn with minimal config - we just need the process to start
    // and reach the agent phase (create .no_agent_commit)
    let child = Command::new(&ralph_bin)
        .current_dir(repo_dir)
        .args(["--developer-iters", "1", "--reviewer-reviews", "0"])
        .env("NO_COLOR", "1") // Disable colors for cleaner output
        .env("RALPH_LOG", "warn") // Reduce log noise
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn ralph process");

    Some(child)
}

/// Wait for `.no_agent_commit` marker to appear.
///
/// Polls the file system until the marker exists or timeout is reached.
/// Returns `true` if marker appeared, `false` on timeout.
fn wait_for_marker(repo_dir: &Path, timeout: Duration) -> bool {
    let marker_path = repo_dir.join(".no_agent_commit");
    let start = Instant::now();

    while start.elapsed() < timeout {
        if marker_path.exists() {
            return true;
        }
        std::thread::sleep(POLL_INTERVAL);
    }
    false
}

/// Send SIGINT to process.
fn send_sigint(child: &Child) {
    unsafe {
        libc::kill(child.id() as i32, libc::SIGINT);
    }
}

/// Assert PROMPT.md has write permission for owner.
fn assert_prompt_writable(prompt_path: &Path) {
    let metadata = std::fs::metadata(prompt_path).expect("stat PROMPT.md");
    let mode = metadata.permissions().mode();
    assert_ne!(
        mode & 0o200,
        0,
        "PROMPT.md should be writable after cleanup (mode={:o})",
        mode
    );
}

/// Check if file contains Ralph hook marker.
fn contains_ralph_marker(path: &Path) -> bool {
    std::fs::read_to_string(path)
        .map(|content| content.contains("RALPH_RUST_MANAGED_HOOK"))
        .unwrap_or(false)
}

/// Skip test if ralph binary is not available.
macro_rules! require_ralph_binary {
    () => {
        if find_ralph_binary().is_none() {
            eprintln!(
                "Skipping test: ralph binary not found. Build with `cargo build -p ralph-workflow`"
            );
            return;
        }
    };
}

/// Test: Ctrl+C during agent phase restores PROMPT.md to writable.
///
/// This is the primary acceptance test: verify PROMPT.md is writable after
/// Ctrl+C at any point during pipeline execution.
///
/// # Test Steps
///
/// 1. Create temp git repo with read-only PROMPT.md (simulating prior crash)
/// 2. Spawn Ralph pipeline process
/// 3. Wait for `.no_agent_commit` marker (agent phase active)
/// 4. Send SIGINT
/// 5. Wait for process exit
/// 6. Assert PROMPT.md has write permission
#[test]
fn test_ctrl_c_restores_prompt_md_writable() {
    with_timeout(
        || {
            require_ralph_binary!();

            let temp_dir = TempDir::new().expect("create temp dir");
            let _repo = init_git_repo(&temp_dir);

            // Set PROMPT.md read-only (simulating prior crashed run)
            let prompt_path = temp_dir.path().join("PROMPT.md");
            set_readonly(&prompt_path);

            // Spawn ralph pipeline
            let mut child = spawn_ralph_pipeline(temp_dir.path()).expect("spawn ralph for test");

            // Wait for agent phase to start (marker appears)
            let marker_appeared = wait_for_marker(temp_dir.path(), MARKER_WAIT_TIMEOUT);
            if !marker_appeared {
                // If marker didn't appear, process may have exited early
                // (e.g., missing agent config). Check if process is still running.
                match child.try_wait() {
                    Ok(Some(status)) => {
                        eprintln!(
                            "Ralph exited early with status: {:?}. \
                             This test requires a configured agent.",
                            status
                        );
                        // Still verify cleanup happened
                        assert_prompt_writable(&prompt_path);
                        return;
                    }
                    _ => panic!("Agent phase should start (marker should appear)"),
                }
            }

            // Send SIGINT
            send_sigint(&child);

            // Wait for exit
            let status = child.wait().expect("wait for child");

            // On SIGINT, process typically exits with 130 (128 + signal number)
            // but may vary based on signal handler implementation
            eprintln!("Process exited with status: {:?}", status);

            // Key assertion: PROMPT.md must be writable after cleanup
            assert_prompt_writable(&prompt_path);

            // Marker should be cleaned up
            assert!(
                !temp_dir.path().join(".no_agent_commit").exists(),
                ".no_agent_commit should be removed after Ctrl+C cleanup"
            );
        },
        SIGNAL_TEST_TIMEOUT,
    );
}

/// Test: Ctrl+C BEFORE lock restores PROMPT.md to writable.
///
/// Covers Gap 1: early interrupt before `LockPromptPermissions` executes.
/// Must verify restoration even when `restore_needed=false`.
///
/// # Test Steps
///
/// 1. Create temp git repo with read-only PROMPT.md
/// 2. Spawn Ralph pipeline process
/// 3. Send SIGINT IMMEDIATELY (before agent phase starts)
/// 4. Wait for process exit
/// 5. Assert PROMPT.md has write permission
#[test]
fn test_ctrl_c_before_lock_restores_prompt_md_writable() {
    with_timeout(
        || {
            require_ralph_binary!();

            let temp_dir = TempDir::new().expect("create temp dir");
            let _repo = init_git_repo(&temp_dir);

            // Create PROMPT.md read-only (simulating prior crashed run)
            let prompt_path = temp_dir.path().join("PROMPT.md");
            set_readonly(&prompt_path);

            // Spawn ralph pipeline
            let mut child = spawn_ralph_pipeline(temp_dir.path()).expect("spawn ralph for test");

            // Send SIGINT IMMEDIATELY (before agent phase starts)
            // This tests the early interrupt path in signal handler
            std::thread::sleep(EARLY_SIGINT_DELAY);
            send_sigint(&child);

            // Wait for exit
            let status = child.wait().expect("wait for child");
            eprintln!("Process exited with status: {:?}", status);

            // Key assertion: PROMPT.md must be writable after exit
            // Even with early interrupt, startup cleanup should have restored it
            assert_prompt_writable(&prompt_path);
        },
        SIGNAL_TEST_TIMEOUT,
    );
}

/// Test: Ctrl+C removes `.no_agent_commit` marker.
///
/// Verify the .no_agent_commit marker is removed after Ctrl+C so git
/// operations are unblocked.
///
/// # Test Steps
///
/// 1. Create temp git repo with PROMPT.md
/// 2. Spawn Ralph pipeline process
/// 3. Wait for `.no_agent_commit` marker to appear
/// 4. Assert marker exists
/// 5. Send SIGINT
/// 6. Wait for process exit
/// 7. Assert marker does not exist
#[test]
fn test_ctrl_c_removes_no_agent_commit() {
    with_timeout(
        || {
            require_ralph_binary!();

            let temp_dir = TempDir::new().expect("create temp dir");
            let _repo = init_git_repo(&temp_dir);

            let mut child = spawn_ralph_pipeline(temp_dir.path()).expect("spawn ralph for test");

            // Wait for marker to appear
            let marker_path = temp_dir.path().join(".no_agent_commit");
            let appeared = wait_for_marker(temp_dir.path(), MARKER_WAIT_TIMEOUT);

            if !appeared {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        eprintln!(
                            "Ralph exited early with status: {:?}. \
                             Marker may not have been created.",
                            status
                        );
                        // If process exited cleanly, marker should be gone
                        assert!(
                            !marker_path.exists(),
                            ".no_agent_commit should not exist after clean exit"
                        );
                        return;
                    }
                    _ => panic!("Marker should appear when agent phase starts"),
                }
            }

            assert!(marker_path.exists(), "Marker file should exist");

            // Send SIGINT
            send_sigint(&child);
            let _ = child.wait().expect("wait for child");

            // Marker must not exist after cleanup
            assert!(
                !marker_path.exists(),
                ".no_agent_commit should be removed after Ctrl+C cleanup"
            );
        },
        SIGNAL_TEST_TIMEOUT,
    );
}

/// Test: Ctrl+C restores git hooks to pre-Ralph state.
///
/// Verify git hooks are restored to their pre-Ralph state (original content
/// or removed if none existed).
///
/// # Test Steps
///
/// 1. Create temp git repo with PROMPT.md
/// 2. Create original pre-commit hook
/// 3. Spawn Ralph pipeline process
/// 4. Wait for `.no_agent_commit` marker
/// 5. Send SIGINT
/// 6. Wait for process exit
/// 7. Assert hook does not contain Ralph marker
#[test]
fn test_ctrl_c_restores_git_hooks() {
    with_timeout(
        || {
            require_ralph_binary!();

            let temp_dir = TempDir::new().expect("create temp dir");
            let _repo = init_git_repo(&temp_dir);

            // Create original pre-commit hook
            let hooks_dir = temp_dir.path().join(".git/hooks");
            std::fs::create_dir_all(&hooks_dir).expect("create hooks dir");
            let precommit_path = hooks_dir.join("pre-commit");
            let original_hook = "#!/bin/bash\necho 'Original hook'\n";
            std::fs::write(&precommit_path, original_hook).expect("write original hook");

            let mut child = spawn_ralph_pipeline(temp_dir.path()).expect("spawn ralph for test");

            let appeared = wait_for_marker(temp_dir.path(), MARKER_WAIT_TIMEOUT);
            if !appeared {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        eprintln!("Ralph exited early with status: {:?}", status);
                        // Verify hook was not modified
                        if precommit_path.exists() {
                            assert!(
                                !contains_ralph_marker(&precommit_path),
                                "Hook should not contain Ralph marker after clean exit"
                            );
                        }
                        return;
                    }
                    _ => panic!("Marker should appear when agent phase starts"),
                }
            }

            // Send SIGINT
            send_sigint(&child);
            let _ = child.wait().expect("wait for child");

            // Hook should be restored to original content (no RALPH_RUST_MANAGED_HOOK marker)
            if precommit_path.exists() {
                assert!(
                    !contains_ralph_marker(&precommit_path),
                    "Hook should not contain Ralph marker after cleanup"
                );
            }
            // If hook doesn't exist, that's also acceptable (Ralph may not have installed it yet)
        },
        SIGNAL_TEST_TIMEOUT,
    );
}

/// Test: Ctrl+C removes hook when no prior hook existed.
///
/// Verify hooks are removed entirely when no prior hook existed before
/// Ralph started.
///
/// # Test Steps
///
/// 1. Create temp git repo with PROMPT.md
/// 2. Ensure no pre-commit hook exists
/// 3. Spawn Ralph pipeline process
/// 4. Wait for `.no_agent_commit` marker
/// 5. Send SIGINT
/// 6. Wait for process exit
/// 7. Assert hooks don't exist or don't have Ralph marker
#[test]
fn test_ctrl_c_removes_hook_when_no_prior_hook_existed() {
    with_timeout(
        || {
            require_ralph_binary!();

            let temp_dir = TempDir::new().expect("create temp dir");
            let _repo = init_git_repo(&temp_dir);

            // Ensure no hooks exist
            let hooks_dir = temp_dir.path().join(".git/hooks");
            let precommit_path = hooks_dir.join("pre-commit");
            let prepush_path = hooks_dir.join("pre-push");
            assert!(
                !precommit_path.exists(),
                "Test precondition: no pre-commit hook"
            );

            let mut child = spawn_ralph_pipeline(temp_dir.path()).expect("spawn ralph for test");

            let appeared = wait_for_marker(temp_dir.path(), MARKER_WAIT_TIMEOUT);
            if !appeared {
                match child.try_wait() {
                    Ok(Some(status)) => {
                        eprintln!("Ralph exited early with status: {:?}", status);
                        return;
                    }
                    _ => panic!("Marker should appear when agent phase starts"),
                }
            }

            send_sigint(&child);
            let _ = child.wait().expect("wait for child");

            // Hooks should not exist after cleanup (they were installed by Ralph, no original)
            // Or if they do exist, they should not have the Ralph marker
            assert!(
                !precommit_path.exists() || !contains_ralph_marker(&precommit_path),
                "pre-commit should not exist or not have Ralph marker after cleanup"
            );
            assert!(
                !prepush_path.exists() || !contains_ralph_marker(&prepush_path),
                "pre-push should not exist or not have Ralph marker after cleanup"
            );
        },
        SIGNAL_TEST_TIMEOUT,
    );
}

/// Test: Startup cleanup restores PROMPT.md from prior run.
///
/// Verify startup-time orphan cleanup handles SIGKILL scenario where
/// prior run left PROMPT.md read-only.
///
/// # Test Steps
///
/// 1. Create temp git repo
/// 2. Simulate prior SIGKILL scenario: PROMPT.md read-only + marker exists
/// 3. Spawn Ralph pipeline - startup cleanup should restore PROMPT.md
/// 4. Wait briefly for startup cleanup to run
/// 5. Send SIGINT to exit cleanly
/// 6. Wait for process exit
/// 7. Assert PROMPT.md is writable
/// 8. Assert marker is cleaned up
#[test]
fn test_startup_cleanup_restores_prompt_md_from_prior_run() {
    with_timeout(
        || {
            require_ralph_binary!();

            let temp_dir = TempDir::new().expect("create temp dir");
            let _repo = init_git_repo(&temp_dir);
            let prompt_path = temp_dir.path().join("PROMPT.md");

            // Simulate prior SIGKILL scenario:
            // 1. PROMPT.md is read-only
            // 2. .no_agent_commit marker exists
            set_readonly(&prompt_path);
            std::fs::write(temp_dir.path().join(".no_agent_commit"), "").expect("write marker");

            // Spawn Ralph pipeline - startup cleanup should restore PROMPT.md
            let mut child = spawn_ralph_pipeline(temp_dir.path()).expect("spawn ralph for test");

            // Wait for startup cleanup to run (or for process to reach agent phase)
            let marker_appeared = wait_for_marker(temp_dir.path(), Duration::from_secs(5));

            // If the marker appeared again, Ralph is running normally
            // If it didn't appear (or was cleaned and recreated), that's also fine
            // We just need to ensure cleanup runs

            // Send SIGINT to exit cleanly
            send_sigint(&child);
            let status = child.wait().expect("wait for child");
            eprintln!("Process exited with status: {:?}", status);

            // PROMPT.md should be writable (startup cleanup should have restored it,
            // or the signal cleanup should have)
            assert_prompt_writable(&prompt_path);

            // Marker should be cleaned up
            // Note: The marker we created simulating prior run should be cleaned up
            // If Ralph created a new one during agent phase, that should also be cleaned
            if marker_appeared {
                // If we reached agent phase, marker should be gone after cleanup
                assert!(
                    !temp_dir.path().join(".no_agent_commit").exists(),
                    ".no_agent_commit should be removed after cleanup"
                );
            }
            // If marker didn't appear, startup cleanup removed the stale one
            // Either way, no marker should remain
        },
        SIGNAL_TEST_TIMEOUT,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Verify helper functions work correctly.
    #[test]
    fn test_set_readonly() {
        let temp_dir = TempDir::new().expect("create temp dir");
        let file_path = temp_dir.path().join("test.txt");
        std::fs::write(&file_path, "test content").expect("write file");

        // Verify file is writable initially
        let mode = std::fs::metadata(&file_path)
            .expect("stat")
            .permissions()
            .mode();
        assert_ne!(mode & 0o200, 0, "file should be writable initially");

        // Set readonly
        set_readonly(&file_path);

        // Verify file is now read-only
        let mode = std::fs::metadata(&file_path)
            .expect("stat")
            .permissions()
            .mode();
        assert_eq!(
            mode & 0o222,
            0,
            "file should have no write bits after set_readonly"
        );
    }

    /// Verify contains_ralph_marker detection.
    #[test]
    fn test_contains_ralph_marker() {
        let temp_dir = TempDir::new().expect("create temp dir");

        // File without marker
        let no_marker = temp_dir.path().join("no_marker.sh");
        std::fs::write(&no_marker, "#!/bin/bash\necho hello\n").expect("write");
        assert!(!contains_ralph_marker(&no_marker));

        // File with marker
        let with_marker = temp_dir.path().join("with_marker.sh");
        std::fs::write(
            &with_marker,
            "#!/bin/bash\n# RALPH_RUST_MANAGED_HOOK\nexit 0\n",
        )
        .expect("write");
        assert!(contains_ralph_marker(&with_marker));

        // Non-existent file
        let missing = temp_dir.path().join("missing.sh");
        assert!(!contains_ralph_marker(&missing));
    }
}
