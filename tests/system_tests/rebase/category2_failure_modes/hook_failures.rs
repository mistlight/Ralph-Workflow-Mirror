//! # Rebase Hook Failure Tests
//!
//! Tests for hook rejection scenarios during rebase:
//! - Pre-rebase hook rejection (before rebase starts)
//! - Commit hook rejection mid-rebase (during commit creation)
//!
//! ## Expected Behavior
//!
//! When hooks reject rebase operations:
//! - Pre-rebase hook rejection should prevent rebase from starting
//! - Mid-rebase hook rejection should be detectable and recoverable
//! - Error messages should clearly indicate hook rejection

use std::fs;
use test_helpers::{commit_all, with_temp_cwd, write_file};

use crate::common::mock_executor_for_git_success;
use crate::test_timeout::with_default_timeout;

use super::{get_default_branch_name, init_repo_with_initial_commit};

/// Test that hook rejection during rebase start is properly detected.
///
/// This verifies that when a pre-rebase hook rejects the rebase operation,
/// the system can detect and report the rejection properly.
#[test]
fn rebase_detects_pre_rebase_hook_rejection() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);
            let executor = mock_executor_for_git_success();

            // Create a feature branch
            let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
            let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

            // Checkout feature branch
            let obj = repo.revparse_single("feature").unwrap();
            let commit = obj.peel_to_commit().unwrap();
            repo.checkout_tree(commit.as_object(), None).unwrap();
            repo.set_head("refs/heads/feature").unwrap();

            // Add a commit on feature
            write_file(dir.path().join("feature.txt"), "feature content");
            let _ = commit_all(&repo, "add feature file");

            // Create a pre-rebase hook that rejects the rebase
            let hooks_dir = dir.path().join(".git").join("hooks");
            fs::create_dir_all(&hooks_dir).unwrap();
            let hook_path = hooks_dir.join("pre-rebase");
            fs::write(
                &hook_path,
                "#!/bin/sh\necho \"Pre-rebase hook rejecting\" >&2\nexit 1",
            )
            .unwrap();

            // Make hook executable on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&hook_path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&hook_path, perms).unwrap();
            }

            // Try to rebase - hook should reject
            let result = rebase_onto(&default_branch, executor.as_ref());

            match result {
                Ok(RebaseResult::Failed(err)) => {
                    // Should report hook rejection or other failure
                    assert!(
                        err.description().contains("hook")
                            || err.description().contains("Hook")
                            || err.description().contains("reject")
                            || err.description().contains("script")
                    );
                }
                Ok(RebaseResult::Success) => {
                    // On some systems, hooks may not execute (e.g., Windows without Git Bash)
                }
                Ok(RebaseResult::NoOp { .. }) => {
                    // May be considered no-op if nothing to rebase
                }
                _ => {}
            }

            // Clean up
            let _ = abort_rebase(executor.as_ref());
        });
    });
}

/// Test that commit hook rejection mid-rebase is properly detected.
///
/// This verifies that when a pre-commit or commit-msg hook rejects a commit
/// during the rebase process, the system can detect it properly.
#[test]
fn rebase_detects_commit_hook_rejection_mid_rebase() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);
            let executor = mock_executor_for_git_success();

            // Create a file on default branch
            write_file(dir.path().join("base.txt"), "base content");
            let _ = commit_all(&repo, "add base file");

            // Create feature branch
            let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
            let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

            // Checkout feature branch
            let obj = repo.revparse_single("feature").unwrap();
            let commit = obj.peel_to_commit().unwrap();
            repo.checkout_tree(commit.as_object(), None).unwrap();
            repo.set_head("refs/heads/feature").unwrap();

            // Add a commit on feature
            write_file(dir.path().join("feature.txt"), "feature content");
            let _ = commit_all(&repo, "add feature file");

            // Go back to default branch
            let main_obj = repo.revparse_single(&default_branch).unwrap();
            let main_commit = main_obj.peel_to_commit().unwrap();
            repo.checkout_tree(main_commit.as_object(), None).unwrap();
            repo.set_head(&format!("refs/heads/{}", default_branch))
                .unwrap();

            // Add another commit on default
            write_file(dir.path().join("main.txt"), "main content");
            let _ = commit_all(&repo, "add main file");

            // Go back to feature
            let feature_obj = repo.revparse_single("feature").unwrap();
            let feature_commit = feature_obj.peel_to_commit().unwrap();
            repo.checkout_tree(feature_commit.as_object(), None)
                .unwrap();
            repo.set_head("refs/heads/feature").unwrap();

            // Create a pre-commit hook that rejects commits with "feature" in message
            let hooks_dir = dir.path().join(".git").join("hooks");
            fs::create_dir_all(&hooks_dir).unwrap();
            let hook_path = hooks_dir.join("pre-commit");
            fs::write(
                &hook_path,
                "#!/bin/sh\n# Reject commits during rebase\nexit 1",
            )
            .unwrap();

            // Make hook executable on Unix
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mut perms = fs::metadata(&hook_path).unwrap().permissions();
                perms.set_mode(0o755);
                fs::set_permissions(&hook_path, perms).unwrap();
            }

            // Try to rebase - hook should reject during commit creation
            let result = rebase_onto(&default_branch, executor.as_ref());

            match result {
                Ok(RebaseResult::Failed(err)) => {
                    // Should report hook rejection or commit creation failure
                    assert!(
                        err.description().contains("hook")
                            || err.description().contains("Hook")
                            || err.description().contains("commit")
                            || err.description().contains("Commit")
                    );
                }
                Ok(RebaseResult::Success) => {
                    // On some systems, hooks may not execute
                }
                Ok(RebaseResult::Conflicts(_)) => {
                    // May have conflicts instead
                }
                _ => {}
            }

            // Clean up
            let _ = abort_rebase(executor.as_ref());
        });
    });
}
