//! Tests for git helper functions.
//!
//! Covers hook installation/uninstallation, marker file operations,
//! and orphaned marker cleanup.

use super::hooks::HOOK_MARKER;
use super::repo::get_hooks_dir;
use super::*;
use crate::utils::Logger;
use std::fs::{self, File};
use tempfile::TempDir;

// Note: Tests that change working directory need to run serially.
// Run with: cargo test -- --test-threads=1

#[test]
fn test_git_snapshot() {
    use crate::test_utils::testing::with_temp_cwd;

    with_temp_cwd(|_dir| {
        git2::Repository::init(".").unwrap();

        // Create an untracked file.
        fs::write("testfile.txt", "test").unwrap();

        let snapshot = git_snapshot().unwrap();
        assert!(snapshot.contains("?? testfile.txt"));
    });
}

#[test]
fn test_install_hook() {
    use crate::test_utils::testing::with_temp_cwd;

    with_temp_cwd(|_dir| {
        git2::Repository::init(".").unwrap();

        let hooks_dir = get_hooks_dir().unwrap();
        fs::create_dir_all(&hooks_dir).unwrap();

        let hook_path = hooks_dir.join("pre-commit");
        hooks::install_hook("Commit", &hook_path).unwrap();

        assert!(hook_path.exists());
        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains(HOOK_MARKER));
    });
}

#[test]
fn test_marker_file_operations() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();
    let marker_path = dir_path.join(".no_agent_commit");

    // Create marker.
    File::create(&marker_path).unwrap();
    assert!(marker_path.exists());

    // Remove marker.
    fs::remove_file(&marker_path).unwrap();
    assert!(!marker_path.exists());
}

#[test]
fn test_git_helpers_new() {
    let _helpers = GitHelpers::new();
}

#[test]
fn test_uninstall_hook_restores_original() {
    let logger = Logger::new(crate::colors::Colors { enabled: false });
    use crate::test_utils::testing::with_temp_cwd;

    with_temp_cwd(|_dir| {
        git2::Repository::init(".").unwrap();

        let hooks_dir = get_hooks_dir().unwrap();
        fs::create_dir_all(&hooks_dir).unwrap();

        // Create an original hook.
        let hook_path = hooks_dir.join("pre-commit");
        fs::write(&hook_path, "#!/bin/bash\necho 'Original hook'").unwrap();

        // Install Ralph hook (backs up original).
        hooks::install_hook("Commit", &hook_path).unwrap();

        // Verify Ralph hook is installed.
        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains(HOOK_MARKER));

        // Uninstall hook restores original.
        let restored = hooks::uninstall_hook(&hook_path, &logger).unwrap();
        assert!(restored);

        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains("Original hook"));
        assert!(!content.contains(HOOK_MARKER));
    });
}

#[test]
fn test_install_hook_uses_absolute_path() {
    use crate::test_utils::testing::with_temp_cwd;

    with_temp_cwd(|_dir| {
        git2::Repository::init(".").unwrap();

        let hooks_dir = get_hooks_dir().unwrap();
        fs::create_dir_all(&hooks_dir).unwrap();

        // Create an existing hook.
        let hook_path = hooks_dir.join("pre-commit");
        fs::write(&hook_path, "#!/bin/bash\nexit 0").unwrap();

        // Install Ralph hook.
        hooks::install_hook("TestHook", &hook_path).unwrap();

        // Read the installed hook content.
        let content = fs::read_to_string(&hook_path).unwrap();

        // The orig= line should contain an absolute path (starts with /).
        assert!(content.contains("orig=\"/"));
    });
}

#[test]
fn test_cleanup_orphaned_marker() {
    use crate::test_utils::testing::with_temp_cwd;

    with_temp_cwd(|dir| {
        let logger = Logger::new(crate::colors::Colors { enabled: false });
        let dir_path = dir.path();

        git2::Repository::init(dir_path).unwrap();

        // Create marker.
        let marker_path = dir_path.join(".no_agent_commit");
        File::create(&marker_path).unwrap();
        assert!(marker_path.exists());

        cleanup_orphaned_marker(&logger).unwrap();
        assert!(!marker_path.exists());
    });
}
