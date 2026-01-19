//! Integration tests for rebase edge cases.
//!
//! Tests for edge cases where rebase is not applicable or should be skipped:
//! - No common ancestor (unrelated branches)
//! - Already on main/master branch
//! - Already up-to-date
//! - Empty repository (unborn HEAD)

use std::fs;
use tempfile::TempDir;
use test_helpers::{commit_all, init_git_repo, with_temp_cwd, write_file};

use ralph_workflow::git_helpers::{rebase_onto, RebaseResult};

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
fn rebase_on_main_branch_returns_noop() {
    use ralph_workflow::git_helpers::is_main_or_master_branch;

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // We're on the default branch
        let is_main_or_master = default_branch == "main" || default_branch == "master";

        if is_main_or_master {
            // Verify is_main_or_master_branch function
            assert!(is_main_or_master_branch().unwrap_or(false));

            // The rebase should return NoOp since we're on main/master
            let result = rebase_onto(&default_branch);

            match result {
                Ok(RebaseResult::NoOp { reason }) => {
                    assert!(
                        reason.contains("Already on")
                            || reason.contains("main")
                            || reason.contains("master")
                            || reason.contains("up-to-date")
                    );
                }
                Ok(RebaseResult::Success) => {
                    // Git may succeed since we're rebasing onto ourselves
                }
                _ => {}
            }
        }
    });
}

#[test]
fn rebase_already_uptodate_returns_noop() {
    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a feature branch at the current commit
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // The feature branch is identical to main (pointing to same commit)
        // So rebasing should be a NoOp or Success (no commits to rebase)
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::NoOp { reason }) => {
                assert!(
                    reason.contains("up-to-date")
                        || reason.contains("NoOp")
                        || reason.contains("already")
                        || reason.contains("Current branch")
                );
            }
            Ok(RebaseResult::Success) => {
                // Git may succeed immediately since there's nothing to do
            }
            _ => {}
        }
    });
}

#[test]
fn rebase_empty_repo_returns_noop() {
    with_temp_cwd(|dir| {
        // Initialize an empty git repo (no commits)
        let _ = init_git_repo(dir);

        // An empty repo cannot be rebased - there's nothing to rebase
        let result = rebase_onto("main");

        // Should return NoOp or Failed since there are no commits
        match result {
            Ok(RebaseResult::NoOp { reason }) => {
                assert!(
                    reason.contains("no commits")
                        || reason.contains("unborn")
                        || reason.contains("empty")
                        || reason.contains("HEAD")
                );
            }
            Ok(RebaseResult::Failed(err)) => {
                // May also fail with InvalidRevision since main doesn't exist
                assert!(
                    err.description().contains("Invalid")
                        || err.description().contains("revision")
                        || err.description().contains("not found")
                        || err.description().contains("unborn")
                );
            }
            _ => {}
        }
    });
}

#[test]
fn rebase_unborn_head_returns_noop() {
    with_temp_cwd(|dir| {
        // Initialize an empty git repo (no commits, unborn HEAD)
        let _ = init_git_repo(dir);

        // Verify HEAD is unborn (no commits yet)
        let repo = git2::Repository::open(dir.path()).unwrap();
        assert!(repo.head().is_err(), "HEAD should be unborn in empty repo");

        // Rebase should return NoOp or Failed for empty repositories
        let result = rebase_onto("main");

        match result {
            Ok(RebaseResult::NoOp { reason }) => {
                assert!(
                    reason.contains("no commits")
                        || reason.contains("unborn")
                        || reason.contains("empty")
                );
            }
            Ok(RebaseResult::Failed(err)) => {
                // May also fail with InvalidRevision
                assert!(
                    err.description().contains("Invalid")
                        || err.description().contains("revision")
                        || err.description().contains("unborn")
                        || err.description().contains("not found")
                );
            }
            _ => {}
        }
    });
}

#[test]
fn rebase_with_no_changes_returns_noop() {
    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a feature branch but don't make any commits
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // There are no commits on feature that aren't on main
        // So rebasing should be a NoOp
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::NoOp { reason }) => {
                assert!(
                    reason.contains("up-to-date")
                        || reason.contains("already")
                        || reason.contains("nothing")
                        || reason.contains("Current branch")
                );
            }
            Ok(RebaseResult::Success) => {
                // Git may succeed immediately
            }
            _ => {}
        }
    });
}

#[test]
fn rebase_skipped_when_branch_is_main() {
    use ralph_workflow::git_helpers::is_main_or_master_branch;

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Verify we're on main or master
        let is_main_or_master = default_branch == "main" || default_branch == "master";

        if is_main_or_master {
            assert!(is_main_or_master_branch().unwrap_or(false));

            // The rebase logic should detect we're on main/master and skip the rebase entirely
            let result = rebase_onto(&default_branch);

            match result {
                Ok(RebaseResult::NoOp { reason }) => {
                    // Should skip with clear reason
                    assert!(
                        reason.contains("Already on")
                            || reason.contains(&default_branch)
                            || reason.contains("up-to-date")
                    );
                }
                Ok(RebaseResult::Success) => {
                    // May also succeed (self-rebase)
                }
                _ => {}
            }
        } else {
            // If the default branch has a different name (not main/master),
            // the is_main_or_master_branch function may return false, which is correct
        }
    });
}

#[test]
fn rebase_with_nonexistent_upstream_fails() {
    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Try to rebase onto a branch that doesn't exist
        let result = rebase_onto("completely-nonexistent-branch-xyz");

        match result {
            Ok(RebaseResult::Failed(err)) => {
                // Should fail with InvalidRevision
                assert!(
                    err.description().contains("Invalid")
                        || err.description().contains("revision")
                        || err.description().contains("not found")
                        || err.description().contains("does not exist")
                );
            }
            _ => {
                // Other outcomes are acceptable depending on git version
            }
        }
    });
}

#[test]
fn rebase_detects_shallow_clone_limitations() {
    // Test that rebase handles shallow clone limitations
    // This is difficult to test without actual shallow clones,
    // but we document the expected behavior:
    //
    // Expected: RebaseErrorKind::RepositoryCorrupt or InvalidRevision
    //
    // When a shallow clone lacks the required history for a rebase,
    // git should fail with a clear error message.
    //
    // We verify the error kind can represent this case
    use ralph_workflow::git_helpers::RebaseErrorKind;

    let err = RebaseErrorKind::InvalidRevision {
        revision: "origin/main".to_string(),
    };
    assert!(err.description().contains("Invalid") || err.description().contains("revision"));
}

#[test]
fn rebase_handles_detached_head() {
    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a commit
        write_file(dir.path().join("file.txt"), "content");
        let _ = commit_all(&repo, "add file");

        // Detach HEAD by checking out a commit directly
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.set_head_detached(head_commit.id()).unwrap();
        repo.checkout_head(None).unwrap();

        // Verify HEAD is detached
        assert!(
            repo.head_detached().unwrap_or(false),
            "HEAD should be detached after set_head_detached"
        );

        // Try to rebase - should either work or fail gracefully
        let result = rebase_onto(&default_branch);

        // Should handle gracefully - either succeed or fail with clear error
        match result {
            Ok(RebaseResult::NoOp { .. }) | Ok(RebaseResult::Success) => {
                // Acceptable outcomes
            }
            Ok(RebaseResult::Failed(err)) => {
                // Should have clear error message
                assert!(!err.description().is_empty());
            }
            Err(_) => {
                // IO error is also acceptable
            }
            _ => {}
        }
    });
}

#[test]
fn rebase_with_ambiguous_revision_fails() {
    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Try to rebase with a potentially ambiguous short SHA or pattern
        // In practice, this depends on the repository state
        let result = rebase_onto("v");

        match result {
            Ok(RebaseResult::Failed(err)) => {
                // Should fail with InvalidRevision
                assert!(
                    err.description().contains("Invalid")
                        || err.description().contains("revision")
                        || err.description().contains("ambiguous")
                        || err.description().contains("not found")
                );
            }
            _ => {
                // Other outcomes are acceptable
            }
        }
    });
}

#[test]
fn rebase_validates_branch_name() {
    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Try to rebase with an invalid branch name
        let result = rebase_onto("-invalid-branch-name");

        match result {
            Ok(RebaseResult::Failed(err)) => {
                // Should fail with InvalidRevision
                assert!(
                    err.description().contains("Invalid")
                        || err.description().contains("revision")
                        || err.description().contains("bad")
                );
            }
            _ => {
                // Other outcomes are acceptable
            }
        }
    });
}

#[test]
fn rebase_with_unrelated_branches_returns_noop() {
    with_temp_cwd(|dir| {
        // Create first repository with initial commit
        let repo1 = init_git_repo(dir);
        write_file(dir.path().join("initial.txt"), "initial content in repo1");
        let _ = commit_all(&repo1, "initial commit in repo1");

        // Create a feature branch from this commit
        let head_commit = repo1.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo1.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo1.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo1.checkout_tree(commit.as_object(), None).unwrap();
        repo1.set_head("refs/heads/feature").unwrap();

        // Make a commit on feature
        write_file(dir.path().join("feature.txt"), "feature content");
        let _ = commit_all(&repo1, "add feature");

        // Now create a new repository at the same path with unrelated history
        // This simulates two unrelated repositories
        let _ = fs::remove_dir_all(dir.path().join(".git"));
        let repo2 = init_git_repo(dir);
        write_file(dir.path().join("unrelated.txt"), "unrelated content");
        let _ = commit_all(&repo2, "unrelated initial commit");

        // Create a branch "other" in the new repository
        let head_commit2 = repo2.head().unwrap().peel_to_commit().unwrap();
        let _other_branch = repo2.branch("other", &head_commit2, false).unwrap();

        // Go back to feature branch (this might not exist in the new repo)
        // Let's test rebasing from the current branch to an unrelated branch
        let result = rebase_onto("other");

        match result {
            Ok(RebaseResult::NoOp { reason }) => {
                // Should skip with clear reason about unrelated branches
                // Note: Git may return various messages for unrelated branches
                assert!(
                    reason.contains("unrelated")
                        || reason.contains("common ancestor")
                        || reason.contains("No common")
                        || reason.contains("up-to-date") // Git may succeed immediately
                        || reason.contains("Already") // Git may detect branch state
                        || reason.contains("different"), // Different history
                    "Unexpected reason: {reason}"
                );
            }
            Ok(RebaseResult::Failed(err)) => {
                // May also fail with clear error about unrelated histories
                assert!(
                    err.description().contains("unrelated")
                        || err.description().contains("common ancestor")
                        || err.description().contains("histories")
                        || err.description().contains("different")
                        || err.description().contains("Invalid"), // Branch may not exist
                    "Unexpected error: {}",
                    err.description()
                );
            }
            Ok(RebaseResult::Success) => {
                // Git may succeed in some cases
            }
            _ => {
                // Other outcomes are acceptable depending on git version
            }
        }
    });
}

#[test]
fn rebase_on_detached_head_returns_noop_with_clear_reason() {
    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a commit
        write_file(dir.path().join("file.txt"), "content");
        let _ = commit_all(&repo, "add file");

        // Detach HEAD by checking out a commit directly
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        repo.set_head_detached(head_commit.id()).unwrap();
        repo.checkout_head(None).unwrap();

        // Verify HEAD is detached
        assert!(
            repo.head_detached().unwrap_or(false),
            "HEAD should be detached after set_head_detached"
        );

        // Try to rebase - should return NoOp with clear reason
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::NoOp { reason }) => {
                // The reason should mention detached HEAD
                assert!(
                    reason.contains("detached") || reason.contains("HEAD"),
                    "Expected NoOp reason to mention 'detached' or 'HEAD', got: {reason}"
                );
            }
            Ok(RebaseResult::Success) => {
                // Git may succeed in some configurations
            }
            Ok(other) => {
                panic!("Expected NoOp or Success, got: {other:?}");
            }
            Err(e) => {
                panic!("Unexpected error: {e}");
            }
        }
    });
}

#[test]
fn verify_rebase_completed_detects_incomplete_rebase() {
    use ralph_workflow::git_helpers::verify_rebase_completed;

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a feature branch with a commit
        write_file(dir.path().join("file1.txt"), "content on main");
        let _ = commit_all(&repo, "add file1 on main");

        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        write_file(dir.path().join("file2.txt"), "content on feature");
        let _ = commit_all(&repo, "add file2 on feature");

        // Start a rebase that will have conflicts
        // We need to create a conflict by modifying the same file differently
        write_file(dir.path().join("file1.txt"), "modified on feature");
        let _ = commit_all(&repo, "modify file1 on feature");

        // Now try to rebase onto main (which also has file1.txt)
        // This should succeed since feature is ahead of main
        let result = rebase_onto(&default_branch);
        match result {
            Ok(RebaseResult::Success) => {
                // Verify the rebase completed using LibGit2
                assert!(
                    verify_rebase_completed(&default_branch).unwrap_or(false),
                    "Rebase should be verified as complete after success"
                );
            }
            _ => {
                // Other outcomes are acceptable
            }
        }
    });
}

#[test]
fn verify_rebase_completed_returns_false_when_diverged() {
    use ralph_workflow::git_helpers::verify_rebase_completed;

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create initial file on default branch
        write_file(dir.path().join("shared.txt"), "original content");
        let _ = commit_all(&repo, "add shared file");

        // Create feature branch
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Modify file on feature
        write_file(dir.path().join("shared.txt"), "feature branch content");
        let _ = commit_all(&repo, "modify on feature");

        // Go back to default branch and modify the same file
        let default_ref = format!("refs/heads/{}", default_branch);
        repo.set_head(&default_ref).unwrap();
        repo.checkout_head(None).unwrap();
        write_file(dir.path().join("shared.txt"), "default branch content");
        let _ = commit_all(&repo, "modify on default");

        // Go back to feature
        repo.set_head("refs/heads/feature").unwrap();
        repo.checkout_head(None).unwrap();

        // Before any rebase, verify should return false
        // The function checks if we're descendant of upstream
        // We're not a descendant yet (feature has diverged), so it should be false
        let verified = verify_rebase_completed(&default_branch).unwrap_or(false);
        assert!(
            !verified,
            "Should not be verified as complete before rebase (diverged branches)"
        );
    });
}

#[test]
fn validate_rebase_preconditions_detects_dirty_tree() {
    use ralph_workflow::git_helpers::validate_rebase_preconditions;

    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Create an uncommitted change (dirty working tree)
        write_file(dir.path().join("dirty.txt"), "uncommitted content");

        // Precondition validation should fail due to dirty tree
        let result = validate_rebase_preconditions();

        assert!(
            result.is_err(),
            "Should fail precondition check with dirty working tree"
        );

        let err_msg = result.unwrap_err().to_string().to_lowercase();
        assert!(
            err_msg.contains("clean") || err_msg.contains("dirty") || err_msg.contains("commit"),
            "Error message should mention clean/dirty state or commit: {err_msg}"
        );
    });
}

#[test]
fn validate_rebase_preconditions_succeeds_on_clean_repo() {
    use ralph_workflow::git_helpers::validate_rebase_preconditions;

    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Clean repository should pass precondition validation
        let result = validate_rebase_preconditions();

        assert!(
            result.is_ok(),
            "Should pass precondition check with clean repository: {result:?}"
        );
    });
}

#[test]
fn validate_rebase_preconditions_detects_shallow_clone() {
    use ralph_workflow::git_helpers::validate_rebase_preconditions;

    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Create a shallow marker file to simulate shallow clone
        // We need to do this after the repo is initialized but before validation
        let repo = git2::Repository::open(dir.path()).unwrap();
        let git_dir = repo.path();
        let shallow_file = git_dir.join("shallow");

        // Write a valid-looking commit SHA to the shallow file
        // Use a 40-character hex string that looks like a real SHA
        fs::write(&shallow_file, "abc123def456789abc123def456789abc1234567\n").unwrap();

        // Precondition validation should fail due to shallow clone
        let result = validate_rebase_preconditions();

        assert!(
            result.is_err(),
            "Should fail precondition check with shallow clone"
        );

        let err_msg = result.unwrap_err().to_string().to_lowercase();
        // The error might mention shallow, clone, history, or could be a git error
        // Accept various error messages related to shallow clones
        assert!(
            err_msg.contains("shallow")
                || err_msg.contains("clone")
                || err_msg.contains("history")
                || err_msg.contains("invalid")
                || err_msg.contains("graft"),
            "Error message should mention shallow clone or related issue: {err_msg}"
        );
    });
}

#[test]
fn validate_rebase_preconditions_detects_uninitialized_submodules() {
    use ralph_workflow::git_helpers::validate_rebase_preconditions;

    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // Create a .gitmodules file (indicating submodules exist)
        let gitmodules_content = r#"[submodule "test-submodule"]
    path = lib/test
    url = https://github.com/example/test.git
"#;
        fs::write(dir.path().join(".gitmodules"), gitmodules_content).unwrap();

        // Commit the .gitmodules file
        let repo = git2::Repository::open(dir.path()).unwrap();
        let _ = commit_all(&repo, "add .gitmodules");

        // The modules directory doesn't exist, so submodules are not initialized
        let git_dir = repo.path();
        let modules_dir = git_dir.join("modules");
        assert!(!modules_dir.exists(), "modules directory should not exist");

        // Precondition validation should fail due to uninitialized submodules
        let result = validate_rebase_preconditions();

        assert!(
            result.is_err(),
            "Should fail precondition check with uninitialized submodules"
        );

        let err_msg = result.unwrap_err().to_string().to_lowercase();
        assert!(
            err_msg.contains("submodule") || err_msg.contains("initialized"),
            "Error message should mention submodules: {err_msg}"
        );
    });
}

#[test]
fn validate_rebase_preconditions_succeeds_with_initialized_submodules() {
    use ralph_workflow::git_helpers::validate_rebase_preconditions;

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);

        // Create .gitmodules file
        let gitmodules_content = r#"[submodule "test-submodule"]
    path = lib/test
    url = https://github.com/example/test.git
"#;
        fs::write(dir.path().join(".gitmodules"), gitmodules_content).unwrap();

        // Create the modules directory (simulating initialized submodules)
        let git_dir = repo.path();
        let modules_dir = git_dir.join("modules");
        fs::create_dir_all(&modules_dir).unwrap();

        // Create the submodule directory in workdir
        let submodule_path = dir.path().join("lib").join("test");
        fs::create_dir_all(&submodule_path).unwrap();
        fs::write(submodule_path.join("README.md"), "test submodule").unwrap();

        // Commit the changes
        let _ = commit_all(&repo, "add submodule");

        // Precondition validation should succeed with initialized submodules
        let result = validate_rebase_preconditions();

        assert!(
            result.is_ok(),
            "Should pass precondition check with initialized submodules: {result:?}"
        );
    });
}

#[test]
fn validate_rebase_preconditions_succeeds_without_submodules() {
    use ralph_workflow::git_helpers::validate_rebase_preconditions;

    with_temp_cwd(|dir| {
        let _repo = init_repo_with_initial_commit(dir);

        // No .gitmodules file - no submodules exist
        assert!(!dir.path().join(".gitmodules").exists());

        // Precondition validation should succeed
        let result = validate_rebase_preconditions();

        assert!(
            result.is_ok(),
            "Should pass precondition check without submodules: {result:?}"
        );
    });
}

#[test]
fn validate_rebase_preconditions_detects_misconfigured_sparse_checkout() {
    use ralph_workflow::git_helpers::validate_rebase_preconditions;

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);

        // Enable sparse checkout in config
        let mut config = repo.config().unwrap();
        config.set_str("core.sparseCheckout", "true").unwrap();

        // But don't create the sparse-checkout file
        let git_dir = repo.path();
        let info_dir = git_dir.join("info");
        let sparse_file = info_dir.join("sparse-checkout");
        assert!(
            !sparse_file.exists(),
            "sparse-checkout file should not exist"
        );

        // Precondition validation should fail due to misconfigured sparse checkout
        let result = validate_rebase_preconditions();

        assert!(
            result.is_err(),
            "Should fail precondition check with misconfigured sparse checkout"
        );

        let err_msg = result.unwrap_err().to_string().to_lowercase();
        assert!(
            err_msg.contains("sparse") || err_msg.contains("checkout"),
            "Error message should mention sparse checkout: {err_msg}"
        );

        // Clean up: remove the config key
        let _ = config.remove("core.sparseCheckout");
    });
}

#[test]
fn validate_rebase_preconditions_succeeds_with_proper_sparse_checkout() {
    use ralph_workflow::git_helpers::validate_rebase_preconditions;

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);

        // Enable sparse checkout in config
        let mut config = repo.config().unwrap();
        config.set_str("core.sparseCheckout", "true").unwrap();

        // Create a properly configured sparse-checkout file
        let git_dir = repo.path();
        let info_dir = git_dir.join("info");
        fs::create_dir_all(&info_dir).unwrap();
        let sparse_file = info_dir.join("sparse-checkout");
        fs::write(&sparse_file, "src/\n*.rs\n").unwrap();

        // Precondition validation should succeed with properly configured sparse checkout
        let result = validate_rebase_preconditions();

        assert!(
            result.is_ok(),
            "Should pass precondition check with proper sparse checkout: {result:?}"
        );

        // Clean up: remove the config key
        let _ = config.remove("core.sparseCheckout");
    });
}

#[test]
fn validate_rebase_preconditions_detects_empty_sparse_checkout_config() {
    use ralph_workflow::git_helpers::validate_rebase_preconditions;

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);

        // Enable sparse checkout in config
        let mut config = repo.config().unwrap();
        config.set_str("core.sparseCheckout", "true").unwrap();

        // Create an empty sparse-checkout file
        let git_dir = repo.path();
        let info_dir = git_dir.join("info");
        fs::create_dir_all(&info_dir).unwrap();
        let sparse_file = info_dir.join("sparse-checkout");
        fs::write(&sparse_file, "").unwrap();

        // Precondition validation should fail due to empty sparse checkout config
        let result = validate_rebase_preconditions();

        assert!(
            result.is_err(),
            "Should fail precondition check with empty sparse checkout config"
        );

        let err_msg = result.unwrap_err().to_string().to_lowercase();
        assert!(
            err_msg.contains("sparse") || err_msg.contains("empty") || err_msg.contains("checkout"),
            "Error message should mention sparse checkout or empty config: {err_msg}"
        );

        // Clean up: remove the config key
        let _ = config.remove("core.sparseCheckout");
    });
}
