//! Tests for git helper functions.
//!
//! Covers hook installation/uninstallation, marker file operations,
//! and orphaned marker cleanup.
//!
//! Note: Many tests in this module require real git repositories and cannot
//! be converted to use `MemoryWorkspace` because `git2::Repository` requires
//! actual filesystem access. These tests use `with_temp_cwd` helper for isolation.

use super::hooks::HOOK_MARKER;
use super::repo::get_hooks_dir;
use super::*;
use crate::logger::Logger;
use crate::workspace::{MemoryWorkspace, Workspace};
use std::fs::{self, File};
use std::path::Path;

// Note: Tests that change working directory need to run serially.
// Run with: cargo test -- --test-threads=1

#[test]
fn test_git_snapshot() {
    use test_helpers::with_temp_cwd;

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
    use test_helpers::with_temp_cwd;

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
    // Test marker file operations using MemoryWorkspace
    let workspace = MemoryWorkspace::new_test();
    let marker_path = Path::new(".no_agent_commit");

    // Create marker using workspace.
    workspace.write(marker_path, "").unwrap();
    assert!(workspace.exists(marker_path));

    // Remove marker using workspace.
    workspace.remove(marker_path).unwrap();
    assert!(!workspace.exists(marker_path));
}

#[test]
fn test_git_helpers_new() {
    let _helpers = GitHelpers::new();
}

#[test]
fn test_uninstall_hook_restores_original() {
    use test_helpers::with_temp_cwd;
    let logger = Logger::new(crate::logger::Colors { enabled: false });

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
    use test_helpers::with_temp_cwd;

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
    use test_helpers::with_temp_cwd;

    with_temp_cwd(|dir| {
        let logger = Logger::new(crate::logger::Colors { enabled: false });
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

#[test]
fn test_git2_to_io_error_preserves_not_found_kind_for_missing_repo() {
    let missing =
        std::env::temp_dir().join(format!("ralph-nonexistent-repo-{}", std::process::id()));
    let err = match git2::Repository::discover(&missing) {
        Ok(_) => panic!("expected repo discovery to fail for missing path"),
        Err(err) => err,
    };

    let io_err = git2_to_io_error(&err);
    assert_eq!(
        io_err.kind(),
        std::io::ErrorKind::NotFound,
        "expected NotFound kind for missing repo discovery error"
    );
}
