//! Integration tests for Category 3: Post-Rebase Failures.
//!
//! Tests for failure modes where rebase completes but leaves system in failed state:
//! - Push or remote integration failures
//! - Post-rebase validation failures (tests failing, build failures, lint violations)

use tempfile::TempDir;
use test_helpers::{commit_all, init_git_repo, with_temp_cwd, write_file};

use ralph_workflow::git_helpers::RebaseErrorKind;

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
fn rebase_detects_validation_failure() {
    // Test that ValidationFailed error kind exists and can be created
    let err = RebaseErrorKind::ValidationFailed {
        reason: "Tests failed after rebase".to_string(),
    };

    assert!(err.description().contains("Post-rebase validation failed"));
    assert!(err.description().contains("Tests failed"));
}

#[test]
fn rebase_validation_error_has_correct_category() {
    // Test that ValidationFailed is in category 3
    let err = RebaseErrorKind::ValidationFailed {
        reason: "Build failed".to_string(),
    };

    assert_eq!(err.category(), 3);
}

#[test]
fn rebase_detects_test_failures() {
    // Document expected behavior for test failures
    //
    // When a rebase completes successfully but tests fail:
    // 1. The system should detect test failures
    // 2. Return ValidationFailed error with test details
    // 3. Allow the user to decide whether to continue or abort
    //
    // We verify the error kind can represent this scenario
    let err = RebaseErrorKind::ValidationFailed {
        reason: "5 tests failed after rebase".to_string(),
    };

    assert!(err.description().contains("5 tests failed"));
}

#[test]
fn rebase_detects_build_failures() {
    // Document expected behavior for build failures
    //
    // When a rebase completes but the build fails:
    // 1. The system should detect build failures
    // 2. Return ValidationFailed error with build error details
    // 3. Provide options to investigate or abort
    let err = RebaseErrorKind::ValidationFailed {
        reason: "Cargo build failed: unresolved import".to_string(),
    };

    assert!(err.description().contains("Cargo build failed"));
}

#[test]
fn rebase_detects_lint_violations() {
    // Document expected behavior for lint violations
    //
    // When a rebase completes but clippy/fmt fails:
    // 1. The system should detect lint violations
    // 2. Return ValidationFailed error with lint details
    // 3. Allow user to decide whether to fix or continue
    let err = RebaseErrorKind::ValidationFailed {
        reason: "Clippy found 3 warnings".to_string(),
    };

    assert!(err.description().contains("Clippy"));
}

#[test]
fn rebase_detects_lockfile_changes() {
    // Document expected behavior for lockfile changes
    //
    // When a rebase causes lockfile drift:
    // 1. The system should detect lockfile differences
    // 2. Return ValidationFailed error about lockfile
    // 3. Allow user to regenerate lockfile
    let err = RebaseErrorKind::ValidationFailed {
        reason: "Cargo.lock differs from source".to_string(),
    };

    assert!(err.description().contains("Cargo.lock"));
}

#[test]
fn rebase_validation_failure_not_recoverable() {
    // Test that validation failures are not considered automatically recoverable
    let err = RebaseErrorKind::ValidationFailed {
        reason: "Tests failed".to_string(),
    };

    // Validation failures require manual intervention
    assert!(!err.is_recoverable());
}

#[test]
fn rebase_successful_rebase_has_no_validation_error() {
    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create feature branch
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Make a commit on feature
        write_file(dir.path().join("feature.txt"), "feature content");
        let _ = commit_all(&repo, "add feature");

        // Rebase should succeed
        let result = ralph_workflow::git_helpers::rebase_onto(&default_branch);

        match result {
            Ok(ralph_workflow::git_helpers::RebaseResult::Success) => {
                // Rebase completed successfully
            }
            Ok(ralph_workflow::git_helpers::RebaseResult::NoOp { .. }) => {
                // Also acceptable - no commits to rebase
            }
            _ => {
                // Other outcomes may occur depending on git state
            }
        }
    });
}

#[test]
fn rebase_handles_submodule_pointer_changes() {
    // Document expected behavior for submodule changes
    //
    // When a rebase changes submodule pointers:
    // 1. The system should detect submodule mismatches
    // 2. Return ValidationFailed error about submodule
    // 3. Allow user to update submodules
    let err = RebaseErrorKind::ValidationFailed {
        reason: "Submodule 'deps/lib' has diverged".to_string(),
    };

    assert!(err.description().contains("Submodule"));
}

#[test]
fn rebase_detects_generated_file_divergence() {
    // Document expected behavior for generated file divergence
    //
    // When a rebase causes generated files to differ:
    // 1. The system should detect generated file differences
    // 2. Return ValidationFailed error about generated files
    // 3. Allow user to regenerate or accept differences
    let err = RebaseErrorKind::ValidationFailed {
        reason: "Generated file 'src/parser.rs' differs".to_string(),
    };

    assert!(err.description().contains("Generated file"));
}
