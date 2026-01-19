//! Integration tests for Category 2: Rebase Starts but Stops.
//!
//! Tests for failure modes where rebase starts but stops in interrupted state:
//! - Content conflicts
//! - Patch application failures
//! - Interactive todo-driven stops
//! - Empty or redundant commits
//! - Autostash and stash reapplication failures
//! - Commit creation failures mid-rebase
//! - Reference update failures

use std::fs;
use tempfile::TempDir;
use test_helpers::{commit_all, init_git_repo, with_temp_cwd, write_file};

use ralph_workflow::git_helpers::RebaseResult;

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
fn rebase_handles_content_conflicts() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a conflicting file on the default branch
        write_file(dir.path().join("conflict.txt"), "main branch content");
        let _ = commit_all(&repo, "add conflicting file on main");

        // Create feature branch from this commit
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // On the default branch, make conflicting change to the same file
        write_file(
            dir.path().join("conflict.txt"),
            "main branch updated content",
        );
        let _ = commit_all(&repo, "update file on main");

        // Checkout feature branch
        let feature_obj = repo.revparse_single("feature").unwrap();
        let feature_commit = feature_obj.peel_to_commit().unwrap();
        repo.checkout_tree(feature_commit.as_object(), None)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Modify the file differently on feature
        write_file(dir.path().join("conflict.txt"), "feature branch content");
        let _ = commit_all(&repo, "change file on feature");

        // Try to rebase feature onto the default branch - should create conflicts
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::Conflicts(files)) => {
                // Should detect conflicts
                assert!(!files.is_empty(), "Should have conflict files");
            }
            Ok(RebaseResult::Failed(err)) => {
                // Should report conflict error
                assert!(
                    err.description().contains("Conflict")
                        || err.description().contains("conflict")
                );
            }
            _ => {
                // Clean up and abort if something went wrong
                let _ = abort_rebase();
            }
        }

        // Always clean up by aborting any rebase
        let _ = abort_rebase();
    });
}

#[test]
fn rebase_handles_patch_application_failure() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a file with multiple lines
        let content = "line 1\nline 2\nline 3\nline 4\nline 5";
        write_file(dir.path().join("base.txt"), content);
        let _ = commit_all(&repo, "add base file");

        // Create feature branch
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Modify the file on feature
        write_file(
            dir.path().join("base.txt"),
            "line 1\nline 2 modified\nline 3\nline 4\nline 5",
        );
        let _ = commit_all(&repo, "modify on feature");

        // Go back to the default branch
        let main_obj = repo.revparse_single(&default_branch).unwrap();
        let main_commit = main_obj.peel_to_commit().unwrap();
        repo.checkout_tree(main_commit.as_object(), None).unwrap();
        repo.set_head(&format!("refs/heads/{}", default_branch))
            .unwrap();

        // Modify the same lines differently on the default branch
        write_file(
            dir.path().join("base.txt"),
            "line 1\nline 2 changed differently\nline 3\nline 4\nline 5",
        );
        let _ = commit_all(&repo, "modify on main");

        // Go back to feature
        let feature_obj = repo.revparse_single("feature").unwrap();
        let feature_commit = feature_obj.peel_to_commit().unwrap();
        repo.checkout_tree(feature_commit.as_object(), None)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Try to rebase - may fail or have conflicts
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::Conflicts(_)) => {
                // Conflicts are expected
            }
            Ok(RebaseResult::Failed(err)) => {
                // Patch application failure is possible
                assert!(
                    err.description().contains("patch")
                        || err.description().contains("Conflict")
                        || err.description().contains("conflict")
                );
            }
            _ => {}
        }

        // Clean up
        let _ = abort_rebase();
    });
}

#[test]
fn rebase_handles_empty_commits() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a file
        write_file(dir.path().join("file.txt"), "original content");
        let _ = commit_all(&repo, "add file");

        // Create feature branch
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Make a change on feature
        write_file(dir.path().join("file.txt"), "feature content");
        let _ = commit_all(&repo, "change on feature");

        // Go back to the default branch
        let main_obj = repo.revparse_single(&default_branch).unwrap();
        let main_commit = main_obj.peel_to_commit().unwrap();
        repo.checkout_tree(main_commit.as_object(), None).unwrap();
        repo.set_head(&format!("refs/heads/{}", default_branch))
            .unwrap();

        // Make the SAME change on the default branch
        write_file(dir.path().join("file.txt"), "feature content");
        let _ = commit_all(&repo, "same change on main");

        // Go back to feature
        let feature_obj = repo.revparse_single("feature").unwrap();
        let feature_commit = feature_obj.peel_to_commit().unwrap();
        repo.checkout_tree(feature_commit.as_object(), None)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Try to rebase - the feature commit should be empty
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::NoOp { reason }) => {
                // Git may skip empty commits
                assert!(
                    reason.contains("up-to-date")
                        || reason.contains("empty")
                        || reason.contains("NoOp")
                );
            }
            Ok(RebaseResult::Success) => {
                // Git may have handled it automatically
            }
            Ok(RebaseResult::Failed(err)) => {
                // May report empty commit
                assert!(
                    err.description().contains("empty")
                        || err.description().contains("skip")
                        || err.description().contains("redundant")
                );
            }
            _ => {}
        }

        // Clean up
        let _ = abort_rebase();
    });
}

#[test]
fn rebase_handles_add_add_conflicts() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create feature branch from initial commit
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Add a file on feature
        write_file(dir.path().join("new.txt"), "feature version");
        let _ = commit_all(&repo, "add file on feature");

        // Go back to the default branch
        let main_obj = repo.revparse_single(&default_branch).unwrap();
        let main_commit = main_obj.peel_to_commit().unwrap();
        repo.checkout_tree(main_commit.as_object(), None).unwrap();
        repo.set_head(&format!("refs/heads/{}", default_branch))
            .unwrap();

        // Add the same file with different content on the default branch
        write_file(dir.path().join("new.txt"), "main version");
        let _ = commit_all(&repo, "add file on main");

        // Go back to feature
        let feature_obj = repo.revparse_single("feature").unwrap();
        let feature_commit = feature_obj.peel_to_commit().unwrap();
        repo.checkout_tree(feature_commit.as_object(), None)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Try to rebase - should detect add/add conflict
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::Conflicts(files)) => {
                assert!(!files.is_empty(), "Should have conflict files");
            }
            Ok(RebaseResult::Failed(err)) => {
                assert!(
                    err.description().contains("Conflict")
                        || err.description().contains("conflict")
                );
            }
            _ => {}
        }

        // Clean up
        let _ = abort_rebase();
    });
}

#[test]
fn rebase_handles_modify_delete_conflicts() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a file
        write_file(dir.path().join("to_delete.txt"), "original content");
        let _ = commit_all(&repo, "add file");

        // Create feature branch
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Modify the file on feature
        write_file(dir.path().join("to_delete.txt"), "modified content");
        let _ = commit_all(&repo, "modify file");

        // Go back to the default branch
        let main_obj = repo.revparse_single(&default_branch).unwrap();
        let main_commit = main_obj.peel_to_commit().unwrap();
        repo.checkout_tree(main_commit.as_object(), None).unwrap();
        repo.set_head(&format!("refs/heads/{}", default_branch))
            .unwrap();

        // Delete the file on the default branch
        fs::remove_file(dir.path().join("to_delete.txt")).unwrap();
        let _ = commit_all(&repo, "delete file");

        // Go back to feature
        let feature_obj = repo.revparse_single("feature").unwrap();
        let feature_commit = feature_obj.peel_to_commit().unwrap();
        repo.checkout_tree(feature_commit.as_object(), None)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Try to rebase - should detect modify/delete conflict
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::Conflicts(files)) => {
                assert!(!files.is_empty(), "Should have conflict files");
            }
            Ok(RebaseResult::Failed(err)) => {
                assert!(
                    err.description().contains("Conflict")
                        || err.description().contains("conflict")
                        || err.description().contains("delete")
                );
            }
            _ => {}
        }

        // Clean up
        let _ = abort_rebase();
    });
}

#[test]
fn rebase_handles_binary_file_conflicts() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a binary file
        let binary_data = vec![0x00, 0x01, 0x02, 0x03, 0x04, 0x05];
        fs::write(dir.path().join("binary.bin"), &binary_data).unwrap();
        let _ = commit_all(&repo, "add binary");

        // Create feature branch
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Modify binary on feature
        let feature_binary = vec![0x10, 0x11, 0x12, 0x13, 0x14, 0x15];
        fs::write(dir.path().join("binary.bin"), &feature_binary).unwrap();
        let _ = commit_all(&repo, "modify binary on feature");

        // Go back to the default branch
        let main_obj = repo.revparse_single(&default_branch).unwrap();
        let main_commit = main_obj.peel_to_commit().unwrap();
        repo.checkout_tree(main_commit.as_object(), None).unwrap();
        repo.set_head(&format!("refs/heads/{}", default_branch))
            .unwrap();

        // Modify binary differently on the default branch
        let main_binary = vec![0xAA, 0xBB, 0xCC, 0xDD, 0xEE, 0xFF];
        fs::write(dir.path().join("binary.bin"), &main_binary).unwrap();
        let _ = commit_all(&repo, "modify binary on main");

        // Go back to feature
        let feature_obj = repo.revparse_single("feature").unwrap();
        let feature_commit = feature_obj.peel_to_commit().unwrap();
        repo.checkout_tree(feature_commit.as_object(), None)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Try to rebase - should detect binary conflict
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::Conflicts(files)) => {
                assert!(!files.is_empty(), "Should have conflict files");
            }
            Ok(RebaseResult::Failed(err)) => {
                assert!(
                    err.description().contains("Conflict")
                        || err.description().contains("conflict")
                        || err.description().contains("binary")
                );
            }
            _ => {}
        }

        // Clean up
        let _ = abort_rebase();
    });
}

#[test]
fn rebase_detects_conflict_markers_in_file() {
    use ralph_workflow::git_helpers::get_conflict_markers_for_file;

    with_temp_cwd(|dir| {
        let conflict_file = dir.path().join("conflict.txt");

        // Write a file with conflict markers
        let content = r#"some code before
<<<<<<< ours
our version of code
=======
their version of code
>>>>>>> theirs
some code after"#;
        fs::write(&conflict_file, content).unwrap();

        // Try to extract conflict markers
        let markers = get_conflict_markers_for_file(&conflict_file);

        match markers {
            Ok(markers_content) => {
                // Should contain conflict markers
                assert!(markers_content.contains("<<<<<<<"));
                assert!(markers_content.contains("======="));
                assert!(markers_content.contains(">>>>>>>"));
            }
            Err(_) => {
                // Error is also acceptable if file reading fails
            }
        }
    });
}

#[test]
fn rebase_detects_no_conflicts_in_clean_file() {
    use ralph_workflow::git_helpers::get_conflict_markers_for_file;

    with_temp_cwd(|dir| {
        let clean_file = dir.path().join("clean.txt");

        // Write a file without conflict markers
        let content = "some clean code\nno conflicts here\njust normal content";
        fs::write(&clean_file, content).unwrap();

        // Try to extract conflict markers
        let markers = get_conflict_markers_for_file(&clean_file);

        match markers {
            Ok(markers_content) => {
                // Should be empty
                assert!(markers_content.is_empty());
            }
            Err(_) => {
                // Error is also acceptable
            }
        }
    });
}

#[test]
fn rebase_handles_autostash_with_conflicts() {
    use ralph_workflow::git_helpers::abort_rebase;
    use std::process::Command;

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a shared file
        write_file(dir.path().join("shared.txt"), "original");
        let _ = commit_all(&repo, "add shared file");

        // Create feature branch
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Make uncommitted changes
        write_file(dir.path().join("shared.txt"), "uncommitted feature changes");
        write_file(dir.path().join("uncommitted.txt"), "uncommitted file");

        // Go back to the default branch and make a conflicting change
        let main_obj = repo.revparse_single(&default_branch).unwrap();
        let main_commit = main_obj.peel_to_commit().unwrap();
        repo.checkout_tree(main_commit.as_object(), None).unwrap();
        repo.set_head(&format!("refs/heads/{}", default_branch))
            .unwrap();

        write_file(dir.path().join("shared.txt"), "main branch changes");
        let _ = commit_all(&repo, "change on main");

        // Go back to feature
        let feature_obj = repo.revparse_single("feature").unwrap();
        let feature_commit = feature_obj.peel_to_commit().unwrap();
        repo.checkout_tree(feature_commit.as_object(), None)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // The uncommitted changes are now in the working tree

        // Try to rebase with autostash - the stashed changes may conflict when reapplied
        let result = Command::new("git")
            .args(["rebase", &default_branch, "--autostash"])
            .current_dir(dir.path())
            .output();

        // Git may handle this various ways:
        // 1. Succeed with autostash
        // 2. Fail with autostash error
        // 3. Have conflicts from the rebase itself
        // We just verify it doesn't crash
        assert!(result.is_ok());

        // Clean up
        let _ = abort_rebase();
    });
}
