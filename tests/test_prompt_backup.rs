//! Integration tests for PROMPT.md backup and restore functionality.
//!
//! These tests verify:
//! 1. Backup is created at pipeline start
//! 2. Auto-restore works when PROMPT.md is deleted
//! 3. Backup doesn't get deleted during normal pipeline execution

use predicates::prelude::*;
use std::fs;
use tempfile::TempDir;

mod test_support;
use test_support::init_git_repo;

fn base_env(cmd: &mut assert_cmd::Command) -> &mut assert_cmd::Command {
    cmd.env("RALPH_INTERACTIVE", "0")
        .env("RALPH_DEVELOPER_ITERS", "0")
        .env("RALPH_REVIEWER_REVIEWS", "0")
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@example.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@example.com")
}

#[test]
fn backup_created_at_pipeline_start() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_CMD", "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Verify backup was created
    assert!(dir.path().join(".agent/PROMPT.md.backup").exists());

    // Verify backup content matches original PROMPT.md
    let original = fs::read_to_string(dir.path().join("PROMPT.md")).unwrap();
    let backup = fs::read_to_string(dir.path().join(".agent/PROMPT.md.backup")).unwrap();
    assert_eq!(original, backup);
}

#[test]
fn auto_restore_works_when_prompt_deleted() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let original_content = "# Test Requirements\nTest task";
    let prompt_path = dir.path().join("PROMPT.md");
    let backup_path = dir.path().join(".agent/PROMPT.md.backup");

    // Initial run to create backup
    let mut cmd1 = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd1)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_CMD", "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd1.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Verify backup was created with original content
    assert!(backup_path.exists());
    let backup_content = fs::read_to_string(&backup_path).unwrap();
    assert_eq!(backup_content, original_content);

    // Delete PROMPT.md (simulating accidental deletion)
    fs::remove_file(&prompt_path).unwrap();
    assert!(!prompt_path.exists());

    // Run Ralph again - should auto-restore from backup
    let mut cmd2 = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd2)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_CMD", "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd2.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Verify PROMPT.md was restored
    assert!(prompt_path.exists());
    let restored_content = fs::read_to_string(&prompt_path).unwrap();
    assert_eq!(restored_content, original_content);
}

#[test]
fn backup_not_deleted_during_cleanup() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let backup_path = dir.path().join(".agent/PROMPT.md.backup");

    // Run Ralph to create backup
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_CMD", "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Verify backup exists
    assert!(backup_path.exists());

    // Run Ralph again - cleanup shouldn't delete backup
    let mut cmd2 = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd2)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_CMD", "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd2.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Verify backup still exists (wasn't cleaned up)
    assert!(backup_path.exists());
}

#[test]
fn backup_has_readonly_permissions() {
    let dir = TempDir::new().unwrap();
    init_git_repo(&dir);

    let backup_path = dir.path().join(".agent/PROMPT.md.backup");

    // Run Ralph to create backup
    let mut cmd = assert_cmd::cargo::cargo_bin_cmd!("ralph");
    base_env(&mut cmd)
        .current_dir(dir.path())
        .env("RALPH_DEVELOPER_CMD", "sh -c 'mkdir -p .agent; echo plan > .agent/PLAN.md'")
        .env("RALPH_REVIEWER_CMD", "sh -c 'exit 0'");

    cmd.assert()
        .success()
        .stdout(predicate::str::contains("Pipeline Complete"));

    // Verify backup exists
    assert!(backup_path.exists());

    // On Unix systems, check if file is read-only
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let metadata = fs::metadata(&backup_path).unwrap();
        let permissions = metadata.permissions();
        let mode = permissions.mode();

        // Check if read-only (0o444 means read for all, no write)
        // The file should have read permission but not write permission
        assert!(mode & 0o222 == 0, "Backup file should not have write permissions");
    }

    #[cfg(windows)]
    {
        use std::os::windows::fs::MetadataExt;
        let metadata = fs::metadata(&backup_path).unwrap();
        let attrs = metadata.file_attributes();

        // Check if readonly flag is set (FILE_ATTRIBUTE_READONLY = 0x1)
        assert!(attrs & 0x1 != 0, "Backup file should have readonly attribute set");
    }
}
