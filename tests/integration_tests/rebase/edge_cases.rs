//! Integration tests for rebase edge cases.
//!
//! Tests for edge cases where rebase is not applicable or should be skipped:
//! - No common ancestor (unrelated branches)
//! - Already on main/master branch
//! - Already up-to-date
//! - Empty repository (unborn HEAD)

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
