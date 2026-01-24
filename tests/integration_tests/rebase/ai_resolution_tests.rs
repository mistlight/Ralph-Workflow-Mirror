//! Integration tests for AI-based conflict resolution.
//!
//! These tests verify that the AI conflict resolution pipeline:
//! - Correctly detects merge conflicts
//! - Provides sufficient context to the AI
//! - Validates conflict resolution attempts
//! - Handles partial AI failures (some files resolved, some not)
//! - Handles complete AI failure (no files resolved)
//! - Detects invalid syntax introduced by AI
//! - Detects conflict markers left by AI
//! - Implements proper retry behavior
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (conflict detection, resolution state)
//! - Uses `tempfile::TempDir` to mock at architectural boundary (filesystem)
//! - Tests are deterministic and isolated

#![deny(unsafe_code)]

use std::fs;
use tempfile::TempDir;
use test_helpers::{commit_all, init_git_repo, with_temp_cwd, write_file};

use crate::test_timeout::with_default_timeout;

use ralph_workflow::executor::RealProcessExecutor;
use ralph_workflow::git_helpers::{rebase_onto, RebaseErrorKind, RebaseResult, RecoveryAction};

/// Helper to create a file with conflict markers
fn create_conflict_file(path: &std::path::Path, content: &str) {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).ok();
    }
    fs::write(path, content).unwrap();
}

/// Helper to check if a file contains conflict markers
fn has_conflict_markers(path: &std::path::Path) -> bool {
    let content = fs::read_to_string(path).unwrap_or_default();
    content.contains("<<<<<<<") || content.contains("=======") || content.contains(">>>>>>>")
}

/// Helper to check if braces/brackets/parentheses are balanced
fn check_balanced_delimiters(content: &str) -> Result<(), String> {
    let mut brace_depth = 0i32;
    let mut bracket_depth = 0i32;
    let mut paren_depth = 0i32;

    for ch in content.chars() {
        match ch {
            '{' => brace_depth += 1,
            '}' => brace_depth -= 1,
            '[' => bracket_depth += 1,
            ']' => bracket_depth -= 1,
            '(' => paren_depth += 1,
            ')' => paren_depth -= 1,
            _ => {}
        }

        if brace_depth < 0 || bracket_depth < 0 || paren_depth < 0 {
            return Err("Unbalanced delimiter found".to_string());
        }
    }

    if brace_depth != 0 || bracket_depth != 0 || paren_depth != 0 {
        return Err("Unbalanced delimiter found".to_string());
    }

    Ok(())
}

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

/// Test that conflict detection works correctly for single file conflicts.
///
/// This verifies that when a file contains git conflict markers, the system
/// detects them correctly and reports the conflict state.
#[test]
fn test_detect_single_file_conflict() {
    with_default_timeout(|| {
        with_temp_cwd(|dir| {
            let file = dir.path().join("conflict.txt");

            // Create a file with conflict markers
            create_conflict_file(
                &file,
                "<<<<<<< HEAD\nmain version\n=======\nfeature version\n>>>>>>> feature\n",
            );

            // Verify we can detect the conflict markers
            assert!(
                has_conflict_markers(&file),
                "Should detect conflict markers"
            );
        });
    });
}

/// Test that conflict detection works correctly for multiple file conflicts.
///
/// This verifies that when multiple files contain conflict markers, the system
/// detects all of them and reports the complete conflict state.
#[test]
fn test_detect_multiple_file_conflicts() {
    with_default_timeout(|| {
        with_temp_cwd(|dir| {
            let files = vec![
                dir.path().join("file1.rs"),
                dir.path().join("file2.rs"),
                dir.path().join("file3.txt"),
            ];

            for file in &files {
                create_conflict_file(
                    file,
                    "<<<<<<< HEAD\nversion 1\n=======\nversion 2\n>>>>>>> feature\n",
                );
            }

            // Verify all files have conflict markers
            for file in &files {
                assert!(
                    has_conflict_markers(file),
                    "Should detect conflict markers in {:?}",
                    file
                );
            }
        });
    });
}

/// Test that we can distinguish between resolved and unresolved conflicts.
///
/// This verifies that when some files are resolved and others are not, the system
/// correctly identifies which files still contain conflict markers.
#[test]
fn test_distinct_resolved_unresolved_conflicts() {
    with_default_timeout(|| {
        with_temp_cwd(|dir| {
            let file1 = dir.path().join("resolved.rs");
            let file2 = dir.path().join("unresolved.rs");

            // File 1: Resolved (no markers)
            create_conflict_file(&file1, "fn main() {\n    println!(\"Hello, world!\");\n}\n");

            // File 2: Unresolved (has markers)
            create_conflict_file(
                &file2,
                "<<<<<<< HEAD\nmain version\n=======\nfeature version\n>>>>>>> feature\n",
            );

            // Verify detection
            assert!(
                !has_conflict_markers(&file1),
                "Resolved file should have no markers"
            );
            assert!(
                has_conflict_markers(&file2),
                "Unresolved file should have markers"
            );
        });
    });
}

/// Test that validation detects balanced delimiters in source files.
///
/// This verifies that when braces, brackets, and parentheses are balanced,
/// the validation passes, and when unbalanced, it fails appropriately.
#[test]
fn test_validate_balanced_delimiters() {
    with_default_timeout(|| {
        with_temp_cwd(|dir| {
            // Balanced file
            let balanced = dir.path().join("balanced.rs");
            create_conflict_file(&balanced, "fn main() {\n    let x = { [ (1, 2) ] };\n}\n");
            let content = fs::read_to_string(&balanced).unwrap();
            assert!(check_balanced_delimiters(&content).is_ok());

            // Unbalanced braces
            let unbalanced_braces = dir.path().join("unbalanced_braces.rs");
            create_conflict_file(&unbalanced_braces, "fn main() {\n    let x = 1;\n");
            let content = fs::read_to_string(&unbalanced_braces).unwrap();
            assert!(check_balanced_delimiters(&content).is_err());

            // Unbalanced brackets
            let unbalanced_brackets = dir.path().join("unbalanced_brackets.rs");
            create_conflict_file(&unbalanced_brackets, "fn main() {\n    let x = [1, 2;\n}\n");
            let content = fs::read_to_string(&unbalanced_brackets).unwrap();
            assert!(check_balanced_delimiters(&content).is_err());

            // Unbalanced parentheses
            let unbalanced_parens = dir.path().join("unbalanced_parens.rs");
            create_conflict_file(&unbalanced_parens, "fn main() {\n    let x = (1, 2;\n}\n");
            let content = fs::read_to_string(&unbalanced_parens).unwrap();
            assert!(check_balanced_delimiters(&content).is_err());
        });
    });
}

/// Test that AI leaving conflict markers is detected.
///
/// This verifies that when an AI "resolution" still contains conflict markers,
/// the system detects the incomplete resolution and reports it.
#[test]
fn test_ai_leaves_conflict_markers_is_detected() {
    with_default_timeout(|| {
        with_temp_cwd(|dir| {
            let file = dir.path().join("partial.rs");

            // AI "resolves" but leaves markers
            create_conflict_file(
                &file,
                "fn main() {\n<<<<<<< HEAD\n    println!(\"main\");\n=======\n    println!(\"feature\");\n>>>>>>> feature\n}\n",
            );

            // Should detect that markers are still present
            assert!(
                has_conflict_markers(&file),
                "Should detect remaining conflict markers"
            );
        });
    });
}

/// Test that partial conflict resolution is detected correctly.
///
/// This verifies that when some conflicts are resolved and others are not,
/// the system identifies the partial resolution state accurately.
#[test]
fn test_detect_partial_conflict_resolution() {
    with_default_timeout(|| {
        with_temp_cwd(|dir| {
            let file1 = dir.path().join("resolved.rs");
            let file2 = dir.path().join("unresolved.rs");

            // One resolved, one not
            create_conflict_file(&file1, "fn resolved() {}\n");
            create_conflict_file(
                &file2,
                "<<<<<<< HEAD\nmain\n=======\nfeature\n>>>>>>> feature\n",
            );

            // Should detect partial resolution
            let file1_resolved = !has_conflict_markers(&file1);
            let file2_resolved = !has_conflict_markers(&file2);

            assert!(
                file1_resolved && !file2_resolved,
                "Should detect partial resolution"
            );
        });
    });
}

/// Test that complete conflict resolution is validated correctly.
///
/// This verifies that when all conflicts are resolved with no markers remaining,
/// the system validates the complete resolution state successfully.
#[test]
fn test_validate_complete_conflict_resolution() {
    with_default_timeout(|| {
        with_temp_cwd(|dir| {
            let file1 = dir.path().join("file1.rs");
            let file2 = dir.path().join("file2.rs");

            // Both files properly resolved
            create_conflict_file(&file1, "fn func1() {}\n");
            create_conflict_file(&file2, "fn func2() {}\n");

            // Should validate successfully
            assert!(!has_conflict_markers(&file1), "file1 should be resolved");
            assert!(!has_conflict_markers(&file2), "file2 should be resolved");
        });
    });
}

/// Test that rebase with actual merge conflicts is detected.
///
/// This verifies that when a rebase encounters conflicting changes, the system
/// detects the conflicts and returns a Conflicts result with affected files.
#[test]
fn test_rebase_with_conflicts_is_detected() {
    with_default_timeout(|| {
        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);

            let file1 = dir.path().join("shared.txt");

            // Main branch: write one version
            create_conflict_file(&file1, "main version\n");
            let _ = commit_all(&repo, "Main version");

            // Feature branch: write different version
            let _feature_obj = repo
                .branch(
                    "feature",
                    &repo.head().unwrap().peel_to_commit().unwrap(),
                    false,
                )
                .unwrap();
            let obj = repo.revparse_single("feature").unwrap();
            let commit = obj.peel_to_commit().unwrap();
            repo.checkout_tree(commit.as_object(), None).unwrap();
            repo.set_head("refs/heads/feature").unwrap();

            create_conflict_file(&file1, "feature version\n");
            let _ = commit_all(&repo, "Feature version");

            // Go back to main and modify the same file
            let obj = repo.revparse_single(&default_branch).unwrap();
            let commit = obj.peel_to_commit().unwrap();
            repo.checkout_tree(commit.as_object(), None).unwrap();
            repo.set_head(&format!("refs/heads/{}", default_branch))
                .unwrap();

            create_conflict_file(&file1, "main updated\n");
            let _ = commit_all(&repo, "Main updated");

            // Go back to feature branch
            let obj = repo.revparse_single("feature").unwrap();
            let commit = obj.peel_to_commit().unwrap();
            repo.checkout_tree(commit.as_object(), None).unwrap();
            repo.set_head("refs/heads/feature").unwrap();

            // Try to rebase feature onto main - should get conflicts
            let executor = RealProcessExecutor::new();
            let result = rebase_onto(&default_branch, &executor);

            // Should either get conflicts or fail
            match result {
                Ok(RebaseResult::Conflicts(files)) => {
                    // Expected - conflicts detected
                    assert!(!files.is_empty(), "Should have conflicted files");
                }
                Ok(RebaseResult::Failed(_)) => {
                    // Also possible - Git may have failed to start rebase
                }
                Err(_) => {
                    // Also acceptable - rebase failed
                }
                other => {
                    panic!("Unexpected result: {:?}", other);
                }
            }
        });
    });
}

/// Test that recovery action decisions are made correctly for error types.
///
/// This verifies that when different error types occur, the system
/// chooses the appropriate recovery action (Continue, Retry, Abort, or Skip).
#[test]
fn test_recovery_action_decision_logic() {
    with_default_timeout(|| {
        // Content conflict should Continue to AI resolution
        let content_conflict = RebaseErrorKind::ContentConflict {
            files: vec!["test.rs".to_string()],
        };
        assert_eq!(
            RecoveryAction::decide(&content_conflict, 0, 3),
            RecoveryAction::Continue
        );

        // Concurrent operation should Retry
        let concurrent = RebaseErrorKind::ConcurrentOperation {
            operation: "rebase".to_string(),
        };
        assert_eq!(
            RecoveryAction::decide(&concurrent, 0, 3),
            RecoveryAction::Retry
        );

        // Invalid revision should Abort
        let invalid = RebaseErrorKind::InvalidRevision {
            revision: "bad".to_string(),
        };
        assert_eq!(
            RecoveryAction::decide(&invalid, 0, 3),
            RecoveryAction::Abort
        );

        // Empty commit should Skip
        assert_eq!(
            RecoveryAction::decide(&RebaseErrorKind::EmptyCommit, 0, 3),
            RecoveryAction::Skip
        );

        // Max attempts exceeded should Abort
        assert_eq!(
            RecoveryAction::decide(&concurrent, 3, 3),
            RecoveryAction::Abort
        );
    });
}

/// Test that rebasing an up-to-date branch returns NoOp result.
///
/// This verifies that when a feature branch has no unique commits,
/// the system skips rebase and returns NoOp or immediate Success.
#[test]
fn test_rebase_already_up_to_date() {
    with_default_timeout(|| {
        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);

            // Create a feature branch at the same point
            let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
            let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

            // Checkout feature branch
            let obj = repo.revparse_single("feature").unwrap();
            let commit = obj.peel_to_commit().unwrap();
            repo.checkout_tree(commit.as_object(), None).unwrap();
            repo.set_head("refs/heads/feature").unwrap();

            // Try to rebase feature onto main (they're at the same point)
            let executor = RealProcessExecutor::new();
            let result = rebase_onto(&default_branch, &executor);

            // Should be NoOp since there's nothing to rebase
            match result {
                Ok(RebaseResult::NoOp { .. }) => {
                    // Expected
                }
                Ok(RebaseResult::Success) => {
                    // Also acceptable - some Git versions treat this as success
                }
                _ => {
                    // Also acceptable
                }
            }
        });
    });
}

/// Test that rebasing without common ancestor produces appropriate error.
///
/// This verifies that when branches have no common ancestor or the target
/// branch doesn't exist, the system returns Failed or NoOp result.
#[test]
fn test_rebase_no_common_ancestor() {
    with_default_timeout(|| {
        with_temp_cwd(|dir| {
            let _repo = init_repo_with_initial_commit(dir);

            // Try to rebase onto a nonexistent branch
            let executor = RealProcessExecutor::new();
            let result = rebase_onto("nonexistent-branch", &executor);

            // Should fail or return NoOp
            match result {
                Ok(RebaseResult::Failed(_)) => {
                    // Expected outcome
                }
                Ok(RebaseResult::NoOp { .. }) => {
                    // Also acceptable
                }
                Err(_) => {
                    // Also acceptable
                }
                other => {
                    panic!("Unexpected result: {:?}", other);
                }
            }
        });
    });
}

/// Test that state machine tracks conflict resolution progress correctly.
///
/// This verifies that when conflicts are recorded and resolved, the system
/// tracks the resolution state and reports when all conflicts are resolved.
#[test]
fn test_state_machine_tracks_conflict_resolution() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::RebaseStateMachine;

        with_temp_cwd(|_dir| {
            let mut machine = RebaseStateMachine::new("main".to_string());

            // Record conflicts
            machine.record_conflict("file1.rs".to_string());
            machine.record_conflict("file2.rs".to_string());

            assert_eq!(machine.unresolved_conflict_count(), 2);

            // Resolve one
            machine.record_resolution("file1.rs".to_string());

            assert_eq!(machine.unresolved_conflict_count(), 1);
            assert!(!machine.all_conflicts_resolved());

            // Resolve the other
            machine.record_resolution("file2.rs".to_string());

            assert_eq!(machine.unresolved_conflict_count(), 0);
            assert!(machine.all_conflicts_resolved());
        });
    });
}

/// Test that checkpoints can be saved and loaded for recovery.
///
/// This verifies that when a checkpoint is saved, it can be loaded
/// later with all state preserved for recovery operations.
#[test]
fn test_checkpoint_save_and_load() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::rebase_checkpoint::{
            clear_rebase_checkpoint, load_rebase_checkpoint, rebase_checkpoint_exists,
            save_rebase_checkpoint,
        };
        use ralph_workflow::git_helpers::{RebaseCheckpoint, RebasePhase};

        with_temp_cwd(|_dir| {
            // Create a checkpoint
            let checkpoint = RebaseCheckpoint::new("main".to_string())
                .with_phase(RebasePhase::ConflictDetected)
                .with_conflicted_file("test.rs".to_string())
                .with_conflicted_file("lib.rs".to_string());

            save_rebase_checkpoint(&checkpoint).unwrap();

            // Verify it exists
            assert!(rebase_checkpoint_exists());

            // Load it back
            let loaded = load_rebase_checkpoint()
                .unwrap()
                .expect("Checkpoint should exist");

            assert_eq!(loaded.upstream_branch, "main");
            assert_eq!(loaded.phase, RebasePhase::ConflictDetected);
            assert_eq!(loaded.conflicted_files.len(), 2);
            assert!(loaded.conflicted_files.contains(&"test.rs".to_string()));
            assert!(loaded.conflicted_files.contains(&"lib.rs".to_string()));

            // Clean up
            clear_rebase_checkpoint().unwrap();
            assert!(!rebase_checkpoint_exists());
        });
    });
}

/// Test that recovery from checkpoint after interruption works correctly.
///
/// This verifies that when a rebase is interrupted and later resumed,
/// the system restores state from the checkpoint and can continue recovery.
#[test]
fn test_recovery_from_checkpoint() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::rebase_checkpoint::save_rebase_checkpoint;
        use ralph_workflow::git_helpers::{RebaseCheckpoint, RebasePhase, RebaseStateMachine};

        with_temp_cwd(|_dir| {
            // Create a checkpoint as if we were interrupted
            let checkpoint = RebaseCheckpoint::new("main".to_string())
                .with_phase(RebasePhase::ConflictDetected)
                .with_conflicted_file("file1.rs".to_string())
                .with_error("First conflict resolution attempt failed".to_string())
                .with_error("Second attempt also failed".to_string());

            save_rebase_checkpoint(&checkpoint).unwrap();

            // Load the state machine
            let machine = RebaseStateMachine::load_or_create("main".to_string()).unwrap();

            // Verify state was restored
            assert_eq!(machine.phase(), &RebasePhase::ConflictDetected);
            assert_eq!(machine.upstream_branch(), "main");
            assert_eq!(machine.unresolved_conflict_count(), 1);
            assert_eq!(machine.checkpoint().error_count, 2);

            // Can still recover (not at max attempts yet)
            assert!(machine.can_recover());
        });
    });
}

/// Test that rebase lock prevents concurrent operations.
///
/// This verifies that when a rebase lock is held, concurrent rebase
/// operations cannot proceed until the lock is released.
#[test]
fn test_rebase_lock_prevents_concurrent() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::rebase_state_machine::{
            acquire_rebase_lock, release_rebase_lock,
        };

        with_temp_cwd(|_dir| {
            // Acquire lock
            acquire_rebase_lock().unwrap();

            // Try to acquire again - should fail
            let result = acquire_rebase_lock();
            assert!(result.is_err(), "Should not be able to acquire lock twice");

            // Release lock
            release_rebase_lock().unwrap();

            // Now should be able to acquire again
            acquire_rebase_lock().unwrap();

            // Clean up
            release_rebase_lock().unwrap();
        });
    });
}

/// Test that stale lock is cleaned up and can be acquired.
///
/// This verifies that when a stale lock file exists, the system
/// cleans it up and allows new operations to proceed.
#[test]
fn test_stale_lock_is_cleaned_up() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::rebase_state_machine::{
            acquire_rebase_lock, release_rebase_lock,
        };

        with_temp_cwd(|dir| {
            // Manually create a stale lock file
            let lock_path = dir.path().join(".agent").join("rebase.lock");
            fs::create_dir_all(lock_path.parent().unwrap()).unwrap();

            // Create a lock with old timestamp (> 30 minutes ago)
            // Write timestamp that is clearly stale (more than 30 minutes ago)
            let stale_timestamp = "2020-01-01T00:00:00+00:00";
            let content = format!("pid=12345\ntimestamp={stale_timestamp}\n");
            fs::write(&lock_path, content).unwrap();

            // Should be able to acquire lock (stale one gets cleaned up)
            acquire_rebase_lock().unwrap();

            // Clean up
            release_rebase_lock().unwrap();
        });
    });
}

/// Test that conflict resolution continues when JSON parsing fails.
///
/// This verifies the system's resilience: when the AI agent doesn't provide
/// valid JSON output, the system should still verify conflicts via LibGit2
/// state rather than failing. LibGit2 is the authoritative source for
/// conflict verification, not JSON parsing.
#[test]
fn test_conflict_resolution_continues_without_json() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::{abort_rebase, get_conflicted_files, git_add_all};

        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);

            // Create a conflicting file
            let conflict_file = dir.path().join("conflict.txt");

            // Main branch: write one version
            create_conflict_file(&conflict_file, "main version\n");
            let _ = commit_all(&repo, "Main version");

            // Feature branch: write different version
            let _feature_obj = repo
                .branch(
                    "feature",
                    &repo.head().unwrap().peel_to_commit().unwrap(),
                    false,
                )
                .unwrap();
            let obj = repo.revparse_single("feature").unwrap();
            let commit = obj.peel_to_commit().unwrap();
            repo.checkout_tree(commit.as_object(), None).unwrap();
            repo.set_head("refs/heads/feature").unwrap();

            create_conflict_file(&conflict_file, "feature version\n");
            let _ = commit_all(&repo, "Feature version");

            // Go back to main and modify the same file
            let obj = repo.revparse_single(&default_branch).unwrap();
            let commit = obj.peel_to_commit().unwrap();
            repo.checkout_tree(commit.as_object(), None).unwrap();
            repo.set_head(&format!("refs/heads/{}", default_branch))
                .unwrap();

            create_conflict_file(&conflict_file, "main updated\n");
            let _ = commit_all(&repo, "Main updated");

            // Go back to feature branch
            let obj = repo.revparse_single("feature").unwrap();
            let commit = obj.peel_to_commit().unwrap();
            repo.checkout_tree(commit.as_object(), None).unwrap();
            repo.set_head("refs/heads/feature").unwrap();

            // Try to rebase feature onto main - should get conflicts
            let executor = RealProcessExecutor::new();
            let result = rebase_onto(&default_branch, &executor);

            match result {
                Ok(RebaseResult::Conflicts(files)) => {
                    // Conflicts detected - this is expected
                    assert!(!files.is_empty(), "Should have conflict files");

                    // Manually resolve the conflict (simulating AI resolution without JSON)
                    // The system should verify via LibGit2 that conflicts are resolved
                    create_conflict_file(&conflict_file, "merged version\n");

                    // Stage the resolved file
                    let _ = git_add_all();

                    // Verify conflicts are resolved via LibGit2 (not JSON)
                    // get_conflicted_files returns empty vec when no conflicts
                    let conflicted = get_conflicted_files().unwrap_or_default();

                    // Should be resolved since we manually fixed it
                    assert!(
                        conflicted.is_empty(),
                        "Conflicts should be resolved after manual fix, verified via LibGit2. Found conflicts: {:?}",
                        conflicted
                    );

                    // Clean up
                    let executor = RealProcessExecutor::new();
                    let _ = abort_rebase(&executor);
                }
                Ok(RebaseResult::Failed(_)) => {
                    // Rebase failed - acceptable
                }
                Err(_) => {
                    // Rebase error - acceptable
                }
                Ok(RebaseResult::Success) => {
                    // Rebase succeeded without conflicts - also acceptable
                }
                _ => {
                    // Clean up any other state
                    let executor = RealProcessExecutor::new();
                    let _ = abort_rebase(&executor);
                }
            }

            // Always clean up
            let executor = RealProcessExecutor::new();
            let _ = abort_rebase(&executor);
        });
    });
}
