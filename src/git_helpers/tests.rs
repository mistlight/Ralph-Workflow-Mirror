use super::*;
use super::hooks::HOOK_MARKER;
use crate::utils::Logger;
use std::fs::{self, File};
use std::process::Command;
use tempfile::TempDir;

// Note: Tests that change working directory need to run serially.
// Run with: cargo test -- --test-threads=1

#[test]
fn test_git_snapshot() {
    // Create a temp git repo.
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    Command::new("git")
        .arg("init")
        .current_dir(dir_path)
        .output()
        .unwrap();

    // Create an untracked file.
    fs::write(dir_path.join("testfile.txt"), "test").unwrap();

    let output = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .current_dir(dir_path)
        .output()
        .unwrap();
    let snapshot = String::from_utf8_lossy(&output.stdout).to_string();
    assert!(snapshot.contains("??"));
}

#[test]
fn test_install_hook() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    Command::new("git")
        .arg("init")
        .current_dir(dir_path)
        .output()
        .unwrap();

    let hooks_dir = dir_path.join(".git/hooks");
    fs::create_dir_all(&hooks_dir).unwrap();

    let hook_path = hooks_dir.join("pre-commit");
    hooks::install_hook("Commit", &hook_path).unwrap();

    assert!(hook_path.exists());
    let content = fs::read_to_string(&hook_path).unwrap();
    assert!(content.contains(HOOK_MARKER));
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
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();
    let logger = Logger::new(crate::colors::Colors { enabled: false });

    Command::new("git")
        .arg("init")
        .current_dir(dir_path)
        .output()
        .unwrap();

    let hooks_dir = dir_path.join(".git/hooks");
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
}

#[test]
fn test_install_hook_uses_absolute_path() {
    let dir = TempDir::new().unwrap();
    let dir_path = dir.path();

    Command::new("git")
        .arg("init")
        .current_dir(dir_path)
        .output()
        .unwrap();

    let hooks_dir = dir_path.join(".git/hooks");
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
}

#[test]
fn test_cleanup_orphaned_marker() {
    use crate::test_utils::testing::with_temp_cwd;

    with_temp_cwd(|dir| {
        let logger = Logger::new(crate::colors::Colors { enabled: false });
        let dir_path = dir.path();

        Command::new("git")
            .arg("init")
            .current_dir(dir_path)
            .output()
            .unwrap();

        // Create marker.
        let marker_path = dir_path.join(".no_agent_commit");
        File::create(&marker_path).unwrap();
        assert!(marker_path.exists());

        cleanup_orphaned_marker(&logger).unwrap();
        assert!(!marker_path.exists());
    });
}
