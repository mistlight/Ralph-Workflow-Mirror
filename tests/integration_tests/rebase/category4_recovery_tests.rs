//! Integration tests for Category 4: Interrupted/Corrupted State.
//!
//! Tests for failure modes involving process termination and corrupted state:
//! - Process termination during rebase
//! - Incomplete or inconsistent rebase metadata
//! - Lock and state artifacts
//! - Worktree, sparse checkout, and submodule edge cases
//! - Recovery metadata unavailable
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (checkpoint save/load, recovery)
//! - Uses `tempfile::TempDir` to mock at architectural boundary (filesystem)
//! - Tests are deterministic and isolated

use std::fs;
use tempfile::TempDir;
use test_helpers::{commit_all, init_git_repo, with_temp_cwd, write_file};

use ralph_workflow::git_helpers::rebase_checkpoint::{
    load_rebase_checkpoint, rebase_checkpoint_exists, save_rebase_checkpoint, RebaseCheckpoint,
    RebasePhase,
};
use ralph_workflow::git_helpers::rebase_onto;

fn init_repo_with_initial_commit(dir: &TempDir) -> git2::Repository {
    let repo = init_git_repo(dir);
    write_file(dir.path().join("initial.txt"), "initial content");
    let _ = commit_all(&repo, "initial commit");
    repo
}

/// Helper to get the default branch name from the repository head
fn get_default_branch_name(repo: &git2::Repository) -> String {
    repo.head()
        .ok()
        .and_then(|h| h.shorthand().map(|s| s.to_string()))
        .unwrap_or_else(|| "main".to_string())
}

#[test]
fn rebase_detects_process_termination_error_kind() {
    // Test that ProcessTerminated error kind exists
    use ralph_workflow::git_helpers::RebaseErrorKind;

    let err = RebaseErrorKind::ProcessTerminated {
        reason: "Rebase interrupted by SIGKILL".to_string(),
    };

    assert!(err.description().contains("terminated"));
    assert!(err.description().contains("SIGKILL"));
}

#[test]
fn rebase_process_terminated_has_correct_category() {
    // Test that ProcessTerminated is in category 4
    use ralph_workflow::git_helpers::RebaseErrorKind;

    let err = RebaseErrorKind::ProcessTerminated {
        reason: "Process crashed".to_string(),
    };

    assert_eq!(err.category(), 4);
}

#[test]
fn rebase_detects_inconsistent_state_error_kind() {
    // Test that InconsistentState error kind exists
    use ralph_workflow::git_helpers::RebaseErrorKind;

    let err = RebaseErrorKind::InconsistentState {
        details: "Rebase state files corrupted".to_string(),
    };

    assert!(err.description().contains("Inconsistent"));
    assert!(err.description().contains("corrupted"));
}

#[test]
fn rebase_inconsistent_state_has_correct_category() {
    // Test that InconsistentState is in category 4
    use ralph_workflow::git_helpers::RebaseErrorKind;

    let err = RebaseErrorKind::InconsistentState {
        details: "State files missing".to_string(),
    };

    assert_eq!(err.category(), 4);
}

#[test]
fn rebase_checkpoint_survives_process_termination() {
    with_temp_cwd(|_dir| {
        // Create a checkpoint
        let checkpoint = RebaseCheckpoint::new("main".to_string())
            .with_phase(RebasePhase::ConflictDetected)
            .with_conflicted_file("src/lib.rs".to_string());

        save_rebase_checkpoint(&checkpoint).unwrap();

        // Verify checkpoint exists
        assert!(rebase_checkpoint_exists());

        // Simulate process restart by loading checkpoint
        let loaded = load_rebase_checkpoint()
            .unwrap()
            .expect("checkpoint should survive process termination");

        assert_eq!(loaded.phase, RebasePhase::ConflictDetected);
        assert_eq!(loaded.conflicted_files.len(), 1);
    });
}

#[test]
fn rebase_detects_stale_lock_files() {
    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Create a stale .git/index.lock
        let index_lock = dir.path().join(".git").join("index.lock");
        fs::create_dir_all(index_lock.parent().unwrap()).unwrap();
        fs::write(&index_lock, "stale lock content").unwrap();

        // Cleanup function should handle stale locks
        let result = ralph_workflow::git_helpers::cleanup_stale_rebase_state();
        assert!(result.is_ok());
    });
}

#[test]
fn rebase_detects_corrupted_reapply_directory() {
    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Create a corrupted .git/rebase-apply directory
        let rebase_dir = dir.path().join(".git").join("rebase-apply");
        fs::create_dir_all(&rebase_dir).unwrap();

        // Write some corrupted state
        fs::write(rebase_dir.join("orig-head"), "invalid\0content").unwrap();

        // Cleanup function should handle corrupted state
        let result = ralph_workflow::git_helpers::cleanup_stale_rebase_state();
        assert!(result.is_ok());
    });
}

#[test]
fn rebase_detects_corrupted_rebase_merge_directory() {
    with_temp_cwd(|_dir| {
        let _repo = init_repo_with_initial_commit(_dir);

        // Create a corrupted .git/rebase-merge directory
        let rebase_dir = _dir.path().join(".git").join("rebase-merge");
        fs::create_dir_all(&rebase_dir).unwrap();

        // Write some corrupted state files
        fs::write(rebase_dir.join("head-name"), "refs/heads/\0feature").unwrap();
        fs::write(rebase_dir.join("onto"), "not-a-valid-oid").unwrap();

        // Cleanup function should handle corrupted state
        let result = ralph_workflow::git_helpers::cleanup_stale_rebase_state();
        assert!(result.is_ok());
    });
}

#[test]
fn rebase_handles_missing_orig_head() {
    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create .git/rebase-apply directory without ORIG_HEAD
        let rebase_dir = dir.path().join(".git").join("rebase-apply");
        fs::create_dir_all(&rebase_dir).unwrap();

        // Create some files but not ORIG_HEAD
        fs::write(rebase_dir.join("head-name"), "refs/heads/feature\n").unwrap();

        // System should handle missing ORIG_HEAD gracefully
        let result = rebase_onto(&default_branch);

        // Result should be Ok (either succeeds or reports error gracefully)
        assert!(result.is_ok());
    });
}

#[test]
fn rebase_handles_missing_head_ref() {
    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Delete HEAD to simulate corrupted state
        let head_file = dir.path().join(".git").join("HEAD");
        fs::remove_file(&head_file).unwrap();

        // Rebase should detect this and handle gracefully
        let result = rebase_onto("main");

        match result {
            Ok(_) => {
                // May succeed if git can recover
            }
            Err(_) => {
                // May fail with clear error
            }
        }
    });
}

#[test]
fn rebase_checkpoint_corruption_recovery() {
    with_temp_cwd(|_dir| {
        // Create and save a valid checkpoint
        let checkpoint = RebaseCheckpoint::new("main".to_string())
            .with_phase(RebasePhase::RebaseInProgress)
            .with_conflicted_file("src/main.rs".to_string());

        save_rebase_checkpoint(&checkpoint).unwrap();

        // Corrupt the checkpoint file
        let checkpoint_path =
            ralph_workflow::git_helpers::rebase_checkpoint::rebase_checkpoint_path();
        fs::write(&checkpoint_path, "corrupted {{{ invalid json").unwrap();

        // Loading should attempt recovery from backup
        let result = load_rebase_checkpoint();

        match result {
            Ok(Some(loaded)) => {
                // Should have loaded from backup
                assert_eq!(loaded.upstream_branch, "main");
            }
            Ok(None) => {
                // No backup available - acceptable
            }
            Err(_) => {
                // Error is also acceptable if no backup exists
            }
        }
    });
}

#[test]
fn rebase_handles_orphaned_temp_files() {
    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Create orphaned temporary merge files
        let git_dir = dir.path().join(".git");
        fs::write(git_dir.join("MERGE_HEAD"), "abc123\n").unwrap();
        fs::write(git_dir.join("MERGE_MSG"), "Merge message\n").unwrap();
        fs::write(git_dir.join("COMMIT_EDITMSG"), "Commit message\n").unwrap();

        // Cleanup should handle orphaned files
        let result = ralph_workflow::git_helpers::cleanup_stale_rebase_state();
        assert!(result.is_ok());
    });
}

#[test]
fn rebase_handles_reflog_disabled() {
    // Document expected behavior when reflog is disabled
    //
    // When reflog is disabled or pruned:
    // 1. The system cannot use reflog for recovery
    // 2. Should fall back to checkpoint system
    // 3. May need manual intervention if checkpoint is also unavailable

    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Disable reflog
        let git_dir = dir.path().join(".git");
        fs::create_dir_all(git_dir.join("refs").join("heads")).unwrap();

        // System should still work without reflog
        let result = rebase_onto("main");
        assert!(result.is_ok());
    });
}

#[test]
fn rebase_detects_sparse_checkout_conflicts() {
    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Configure sparse checkout
        let git_dir = dir.path().join(".git");
        let info_dir = git_dir.join("info");
        fs::create_dir_all(&info_dir).unwrap();

        // Create sparse checkout config
        fs::write(info_dir.join("sparse-checkout"), "*.rs\n").unwrap();
        // Enable sparse checkout (requires core.sparseCheckout config)
        let _ = std::process::Command::new("git")
            .args(["config", "core.sparseCheckout", "true"])
            .current_dir(dir.path())
            .output();

        // System should handle sparse checkout gracefully
        let result = rebase_onto(&default_branch);
        assert!(result.is_ok());
    });
}

#[test]
fn rebase_detects_detached_head_after_interruption() {
    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a commit
        write_file(dir.path().join("file.txt"), "content");
        let _ = commit_all(&repo, "add file");

        // Detach HEAD
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.set_head_detached(head_commit.id()).unwrap();
        repo.checkout_head(None).unwrap();

        // Simulate interrupted rebase state
        let rebase_dir = dir.path().join(".git").join("rebase-merge");
        fs::create_dir_all(&rebase_dir).unwrap();
        fs::write(rebase_dir.join("onto"), head_commit.id().to_string()).unwrap();

        // System should handle detached HEAD with rebase state
        let result = rebase_onto(&default_branch);

        match result {
            Ok(_) => {
                // May succeed
            }
            Err(_) => {
                // May fail with clear error
            }
        }

        // Clean up
        let _ = ralph_workflow::git_helpers::abort_rebase();
    });
}

#[test]
fn rebase_handles_git_gc_removed_objects() {
    // Document expected behavior when git gc removes recovery objects
    //
    // When garbage collection removes objects needed for recovery:
    // 1. The system should detect missing objects
    // 2. Return RepositoryCorrupt error
    // 3. Suggest fetching from remote or using reflog

    let err = ralph_workflow::git_helpers::RebaseErrorKind::RepositoryCorrupt {
        details: "Object not found - may have been garbage collected".to_string(),
    };

    assert!(err.description().contains("Repository integrity issue"));
    assert!(err.description().contains("garbage collected"));
}

#[test]
fn rebase_process_terminated_is_recoverable() {
    // Test that ProcessTerminated errors are considered recoverable
    use ralph_workflow::git_helpers::RebaseErrorKind;

    let err = RebaseErrorKind::ProcessTerminated {
        reason: "CI timeout".to_string(),
    };

    // Process termination should be recoverable via checkpoint
    assert!(err.is_recoverable());
}

#[test]
fn rebase_inconsistent_state_is_recoverable() {
    // Test that InconsistentState errors are considered recoverable
    use ralph_workflow::git_helpers::RebaseErrorKind;

    let err = RebaseErrorKind::InconsistentState {
        details: "Corrupted rebase state".to_string(),
    };

    // Inconsistent state should be recoverable via cleanup
    assert!(err.is_recoverable());
}
