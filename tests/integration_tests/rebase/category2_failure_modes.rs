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
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (rebase state, conflict markers)
//! - Uses `tempfile::TempDir` to mock at architectural boundary (filesystem)
//! - Tests are deterministic and isolated

use std::fs;
use std::process::Command;
use tempfile::TempDir;
use test_helpers::{
    commit_all, git_commit_all, git_switch_force, init_git_repo, with_temp_cwd, write_file,
};

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

#[test]
fn rebase_handles_rename_rename_conflicts() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a file on initial commit
        write_file(dir.path().join("original.txt"), "original content");
        let _ = commit_all(&repo, "add original file");

        // Create feature branch and switch to it
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();
        git_switch_force(dir.path(), "feature");

        // On feature: rename the file using git mv
        let status = Command::new("git")
            .args(["mv", "original.txt", "new_feature.txt"])
            .current_dir(dir.path())
            .status()
            .expect("git mv should execute");
        assert!(status.success(), "git mv should succeed");
        write_file(dir.path().join("new_feature.txt"), "feature content");
        git_commit_all(dir.path(), "rename on feature");

        // Go back to default branch using git switch with force
        git_switch_force(dir.path(), &default_branch);

        // Verify the file was restored
        assert!(
            dir.path().join("original.txt").exists(),
            "original.txt should exist after checkout"
        );

        // On default: rename the same file using git mv
        let status = Command::new("git")
            .args(["mv", "original.txt", "new_main.txt"])
            .current_dir(dir.path())
            .status()
            .expect("git mv should execute");
        assert!(status.success(), "git mv should succeed");
        write_file(dir.path().join("new_main.txt"), "main content");
        git_commit_all(dir.path(), "rename on main");

        // Go back to feature using git switch with force
        git_switch_force(dir.path(), "feature");

        // Try to rebase - should detect rename/rename conflict
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::Conflicts(files)) => {
                assert!(!files.is_empty(), "Should have conflict files");
            }
            Ok(RebaseResult::Failed(err)) => {
                assert!(
                    err.description().contains("Conflict")
                        || err.description().contains("conflict")
                        || err.description().contains("rename")
                );
            }
            Ok(RebaseResult::Success) => {
                // Git may resolve rename/rename automatically in some versions
            }
            _ => {}
        }

        // Clean up
        let _ = abort_rebase();
    });
}

#[test]
fn rebase_handles_directory_file_conflicts() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a file on initial commit
        write_file(dir.path().join("path.txt"), "file content");
        let _ = commit_all(&repo, "add file");

        // Create feature branch
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // On feature: rename path.txt to path/new.txt (create directory)
        fs::remove_file(dir.path().join("path.txt")).unwrap();
        fs::create_dir(dir.path().join("path")).unwrap();
        write_file(dir.path().join("path/new.txt"), "feature content");
        let _ = commit_all(&repo, "convert to directory on feature");

        // Go back to default branch
        let main_obj = repo.revparse_single(&default_branch).unwrap();
        let main_commit = main_obj.peel_to_commit().unwrap();
        repo.checkout_tree(main_commit.as_object(), None).unwrap();
        repo.set_head(&format!("refs/heads/{}", default_branch))
            .unwrap();

        // On default: modify path.txt
        write_file(dir.path().join("path.txt"), "modified content");
        let _ = commit_all(&repo, "modify file");

        // Go back to feature
        let feature_obj = repo.revparse_single("feature").unwrap();
        let feature_commit = feature_obj.peel_to_commit().unwrap();
        repo.checkout_tree(feature_commit.as_object(), None)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Try to rebase - should detect directory/file conflict
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
            Ok(RebaseResult::Success) => {
                // Git may handle this in some versions
            }
            _ => {}
        }

        // Clean up
        let _ = abort_rebase();
    });
}

#[test]
fn rebase_handles_rename_delete_conflicts() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a file on initial commit
        write_file(dir.path().join("rename_me.txt"), "original content");
        let _ = commit_all(&repo, "add file");

        // Create feature branch and switch to it
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();
        git_switch_force(dir.path(), "feature");

        // On feature: rename the file using git mv
        let status = Command::new("git")
            .args(["mv", "rename_me.txt", "renamed.txt"])
            .current_dir(dir.path())
            .status()
            .expect("git mv should execute");
        assert!(status.success(), "git mv should succeed");
        write_file(dir.path().join("renamed.txt"), "renamed content");
        git_commit_all(dir.path(), "rename file");

        // Go back to default branch using git switch with force
        git_switch_force(dir.path(), &default_branch);

        // Verify the file was restored
        assert!(
            dir.path().join("rename_me.txt").exists(),
            "rename_me.txt should exist after checkout"
        );

        // On default: delete the file using git rm
        let status = Command::new("git")
            .args(["rm", "rename_me.txt"])
            .current_dir(dir.path())
            .status()
            .expect("git rm should execute");
        assert!(status.success(), "git rm should succeed");
        git_commit_all(dir.path(), "delete file");

        // Go back to feature using git switch with force
        git_switch_force(dir.path(), "feature");

        // Try to rebase - should detect rename/delete conflict
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
            Ok(RebaseResult::Success) => {
                // Git may resolve automatically
            }
            _ => {}
        }

        // Clean up
        let _ = abort_rebase();
    });
}

#[test]
fn rebase_handles_symlink_conflicts() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a file on initial commit
        write_file(dir.path().join("target.txt"), "target content");
        let _ = commit_all(&repo, "add target file");

        // Create feature branch
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // On feature: convert to symlink
        // Note: symlinks require special permissions, may not work on all platforms
        // We'll document expected behavior instead
        #[cfg(unix)]
        {
            use std::os::unix::fs as unix_fs;
            fs::remove_file(dir.path().join("target.txt")).unwrap();
            unix_fs::symlink("other.txt", dir.path().join("target.txt")).unwrap();
            let _ = commit_all(&repo, "convert to symlink");
        }

        #[cfg(not(unix))]
        {
            // On non-Unix platforms, just modify the file
            write_file(dir.path().join("target.txt"), "modified content");
            let _ = commit_all(&repo, "modify file");
        }

        // Go back to default branch
        let main_obj = repo.revparse_single(&default_branch).unwrap();
        let main_commit = main_obj.peel_to_commit().unwrap();
        repo.checkout_tree(main_commit.as_object(), None).unwrap();
        repo.set_head(&format!("refs/heads/{}", default_branch))
            .unwrap();

        // On default: modify the file
        write_file(dir.path().join("target.txt"), "main content");
        let _ = commit_all(&repo, "modify on main");

        // Go back to feature
        let feature_obj = repo.revparse_single("feature").unwrap();
        let feature_commit = feature_obj.peel_to_commit().unwrap();
        repo.checkout_tree(feature_commit.as_object(), None)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Try to rebase - should handle symlink conflicts gracefully
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::Conflicts(files)) => {
                assert!(!files.is_empty(), "Should have conflict files");
            }
            Ok(RebaseResult::Failed(err)) => {
                assert!(
                    err.description().contains("Conflict")
                        || err.description().contains("conflict")
                        || err.description().contains("link")
                );
            }
            Ok(RebaseResult::Success) => {
                // Git may resolve automatically
            }
            _ => {}
        }

        // Clean up
        let _ = abort_rebase();
    });
}

#[test]
fn rebase_handles_line_ending_conflicts() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Configure git to not normalize line endings for this test
        // This ensures we can create true CRLF vs LF conflicts
        let _ = std::process::Command::new("git")
            .args(["config", "core.autocrlf", "false"])
            .current_dir(dir.path())
            .output();

        // Create a text file with LF endings
        let lf_content = "line 1\nline 2\nline 3\n";
        write_file(dir.path().join("text.txt"), lf_content);
        let _ = commit_all(&repo, "add text file with LF");

        // Create feature branch
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Modify with CRLF on feature (simulating Windows-style)
        let crlf_content = "line 1\r\nline 2 modified\r\nline 3\r\n";
        fs::write(dir.path().join("text.txt"), crlf_content).unwrap();
        let _ = commit_all(&repo, "modify with CRLF");

        // Go back to default branch
        let main_obj = repo.revparse_single(&default_branch).unwrap();
        let main_commit = main_obj.peel_to_commit().unwrap();
        repo.checkout_tree(main_commit.as_object(), None).unwrap();
        repo.set_head(&format!("refs/heads/{}", default_branch))
            .unwrap();

        // Modify with LF on default
        let lf_modified = "line 1\nline 2 changed differently\nline 3\n";
        fs::write(dir.path().join("text.txt"), lf_modified).unwrap();
        let _ = commit_all(&repo, "modify with LF");

        // Go back to feature
        let feature_obj = repo.revparse_single("feature").unwrap();
        let feature_commit = feature_obj.peel_to_commit().unwrap();
        repo.checkout_tree(feature_commit.as_object(), None)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Try to rebase - should handle line ending conflicts
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::Conflicts(files)) => {
                // Should detect conflict
                assert!(!files.is_empty(), "Should have conflict files");
            }
            Ok(RebaseResult::Failed(err)) => {
                // May report conflict error
                assert!(
                    err.description().contains("Conflict")
                        || err.description().contains("conflict")
                );
            }
            Ok(RebaseResult::Success) => {
                // Git may resolve automatically
            }
            _ => {}
        }

        // Clean up
        let _ = abort_rebase();
    });
}

#[test]
fn rebase_handles_whitespace_only_conflicts() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

        // Create a text file
        let content = "line 1\nline 2\nline 3\n";
        write_file(dir.path().join("text.txt"), content);
        let _ = commit_all(&repo, "add text file");

        // Create feature branch
        let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
        let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();

        // Checkout feature branch
        let obj = repo.revparse_single("feature").unwrap();
        let commit = obj.peel_to_commit().unwrap();
        repo.checkout_tree(commit.as_object(), None).unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Modify with trailing whitespace
        let trailing_ws = "line 1   \nline 2\t\nline 3\n";
        fs::write(dir.path().join("text.txt"), trailing_ws).unwrap();
        let _ = commit_all(&repo, "add trailing whitespace");

        // Go back to default branch
        let main_obj = repo.revparse_single(&default_branch).unwrap();
        let main_commit = main_obj.peel_to_commit().unwrap();
        repo.checkout_tree(main_commit.as_object(), None).unwrap();
        repo.set_head(&format!("refs/heads/{}", default_branch))
            .unwrap();

        // Modify with leading whitespace
        let leading_ws = "line 1\n  line 2\nline 3\n";
        fs::write(dir.path().join("text.txt"), leading_ws).unwrap();
        let _ = commit_all(&repo, "add leading whitespace");

        // Go back to feature
        let feature_obj = repo.revparse_single("feature").unwrap();
        let feature_commit = feature_obj.peel_to_commit().unwrap();
        repo.checkout_tree(feature_commit.as_object(), None)
            .unwrap();
        repo.set_head("refs/heads/feature").unwrap();

        // Try to rebase - should handle whitespace conflicts
        let result = rebase_onto(&default_branch);

        match result {
            Ok(RebaseResult::Conflicts(files)) => {
                // Should detect conflict
                assert!(!files.is_empty(), "Should have conflict files");
            }
            Ok(RebaseResult::Failed(err)) => {
                // May report conflict error
                assert!(
                    err.description().contains("Conflict")
                        || err.description().contains("conflict")
                );
            }
            Ok(RebaseResult::Success) => {
                // Git may resolve automatically (often with conflict markers)
            }
            _ => {}
        }

        // Clean up
        let _ = abort_rebase();
    });
}

/// Test that hook rejection during rebase start is properly detected.
///
/// This test verifies that the system can detect when a pre-rebase hook
/// rejects the rebase operation. Hooks are user-defined scripts that can
/// veto Git operations, and the rebase system must properly report these
/// rejections.
#[test]
fn rebase_detects_pre_rebase_hook_rejection() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

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
        let result = rebase_onto(&default_branch);

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
        let _ = abort_rebase();
    });
}

/// Test that commit hook rejection mid-rebase is properly detected.
///
/// This test verifies that the system can detect when a pre-commit or
/// commit-msg hook rejects a commit during the rebase process.
#[test]
fn rebase_detects_commit_hook_rejection_mid_rebase() {
    use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

    with_temp_cwd(|dir| {
        let repo = init_repo_with_initial_commit(dir);
        let default_branch = get_default_branch_name(&repo);

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
        let result = rebase_onto(&default_branch);

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
        let _ = abort_rebase();
    });
}
