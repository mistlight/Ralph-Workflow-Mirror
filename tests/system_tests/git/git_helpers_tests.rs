//! System tests for git helper functions requiring real git repositories.
//!
//! These tests use real `git2::Repository` and filesystem operations.
//! Tests that can use `MemoryWorkspace` should remain in the unit tests.

use ralph_workflow::git_helpers::get_hooks_dir;
use ralph_workflow::git_helpers::hooks::HOOK_MARKER;
use ralph_workflow::git_helpers::{
    self, cleanup_orphaned_marker, disable_git_wrapper, end_agent_phase, git_snapshot, hooks,
    start_agent_phase, uninstall_hooks, GitHelpers,
};
use ralph_workflow::logger::Logger;
use std::fs::{self, File};

// Note: Tests that change working directory need to run serially.
// Run with: cargo test -- --test-threads=1

#[test]
fn test_agent_phase_cleanup_removes_git_wrapper_track_file() {
    use test_helpers::with_temp_cwd;

    let logger = Logger::new(ralph_workflow::logger::Colors::with_enabled(false));

    with_temp_cwd(|dir| {
        git2::Repository::init(".").unwrap();

        let mut helpers = GitHelpers::default();
        start_agent_phase(&mut helpers).unwrap();

        // Precondition: wrapper track file exists.
        assert!(
            dir.path().join(".agent/git-wrapper-dir.txt").exists(),
            "expected wrapper track file to exist after start_agent_phase"
        );

        end_agent_phase();
        disable_git_wrapper(&mut helpers);
        uninstall_hooks(&logger).unwrap();

        assert!(
            !dir.path().join(".agent/git-wrapper-dir.txt").exists(),
            "expected wrapper track file to be removed by disable_git_wrapper"
        );
    });
}

#[test]
fn test_disable_git_wrapper_removes_track_file_even_when_cwd_changes() {
    use test_helpers::with_temp_cwd;

    with_temp_cwd(|dir| {
        git2::Repository::init(".").unwrap();

        let mut helpers = GitHelpers::default();
        start_agent_phase(&mut helpers).unwrap();

        let track_file = dir.path().join(".agent/git-wrapper-dir.txt");
        assert!(track_file.exists(), "precondition: track file must exist");

        let other_dir = tempfile::tempdir().expect("create other tempdir");
        std::env::set_current_dir(other_dir.path()).expect("set cwd away from repo");

        // Regression: disable_git_wrapper should remove the track file in the repo root,
        // not relative to the current working directory.
        disable_git_wrapper(&mut helpers);

        assert!(
            !track_file.exists(),
            "expected wrapper track file to be removed even when cwd is not repo root"
        );
    });
}

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
fn test_install_hook_creates_missing_hooks_dir() {
    use test_helpers::with_temp_cwd;

    with_temp_cwd(|_dir| {
        git2::Repository::init(".").unwrap();

        let hooks_dir = get_hooks_dir().unwrap();
        // Simulate a broken repo state where hooks dir is missing.
        // Regression: install_hook must create the directory before canonicalize().
        let _ = fs::remove_dir_all(&hooks_dir);

        let hook_path = hooks_dir.join("pre-commit");
        hooks::install_hook("Commit", &hook_path).expect("install hook should create hooks dir");

        assert!(hook_path.exists());
        let content = fs::read_to_string(&hook_path).unwrap();
        assert!(content.contains(HOOK_MARKER));
    });
}

#[test]
fn test_uninstall_hooks_in_repo_does_not_depend_on_cwd() {
    use test_helpers::with_temp_cwd;

    let logger = Logger::new(ralph_workflow::logger::Colors::with_enabled(false));

    with_temp_cwd(|dir| {
        git2::Repository::init(".").unwrap();

        let hooks_dir = get_hooks_dir().unwrap();
        fs::create_dir_all(&hooks_dir).unwrap();

        let precommit_path = hooks_dir.join("pre-commit");
        let original_hook = "#!/bin/bash\necho 'Original hook'\n";
        fs::write(&precommit_path, original_hook).unwrap();

        // Install Ralph hook (backs up original).
        hooks::install_hook("Commit", &precommit_path).unwrap();
        let content = fs::read_to_string(&precommit_path).unwrap();
        assert!(content.contains(HOOK_MARKER));

        // Change CWD away from the repo root.
        let other_dir = tempfile::tempdir().expect("create other tempdir");
        std::env::set_current_dir(other_dir.path()).expect("set cwd to other tempdir");

        // Regression: uninstalling startup hooks must target the explicit repo root,
        // not the process CWD.
        hooks::uninstall_hooks_in_repo(dir.path(), &logger).unwrap();

        let restored = fs::read_to_string(&precommit_path).unwrap();
        assert_eq!(restored, original_hook);
        assert!(!restored.contains(HOOK_MARKER));
    });
}

#[test]
fn test_uninstall_hook_restores_original() {
    use test_helpers::with_temp_cwd;
    let logger = Logger::new(ralph_workflow::logger::Colors::with_enabled(false));

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
        // The hook script now uses bash-safe single-quoted literals.
        assert!(content.contains("orig='/"));
    });
}

#[test]
fn test_cleanup_orphaned_marker() {
    use test_helpers::with_temp_cwd;

    with_temp_cwd(|dir| {
        let logger = Logger::new(ralph_workflow::logger::Colors::with_enabled(false));
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
    let Err(err) = git2::Repository::discover(&missing) else {
        panic!("expected repo discovery to fail for missing path")
    };

    let io_err = git_helpers::git2_to_io_error(&err);
    assert_eq!(
        io_err.kind(),
        std::io::ErrorKind::NotFound,
        "expected NotFound kind for missing repo discovery error"
    );
}
