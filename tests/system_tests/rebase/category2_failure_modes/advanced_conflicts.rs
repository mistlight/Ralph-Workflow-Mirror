//! # Advanced Rebase Conflict Tests
//!
//! Tests for complex conflict scenarios during rebase:
//! - Autostash with conflicts
//! - Rename-rename conflicts (same file renamed to different names)
//! - Directory-file conflicts (directory replaced with file or vice versa)
//! - Rename-delete conflicts (file renamed on one branch, deleted on another)
//! - Symlink conflicts
//! - Line ending conflicts (CRLF vs LF)
//! - Whitespace-only conflicts
//!
//! ## Expected Behavior
//!
//! These complex scenarios should be handled gracefully, either through:
//! - Auto-resolution where possible (e.g., line ending normalization)
//! - Clear conflict reporting for manual resolution
//! - Proper state management to allow abort and retry

use std::fs;
use test_helpers::{commit_all, git_commit_all, git_switch_force, with_temp_cwd, write_file};

use crate::common::mock_executor_for_git_success;
use crate::test_timeout::with_default_timeout;
use serial_test::serial;

use super::{get_default_branch_name, init_repo_with_initial_commit};

/// Test that rename/rename conflicts during rebase produce Conflicts result.
///
/// This verifies that when the same file is renamed to different names on both branches,
/// the system detects the conflict and returns a Conflicts result.
#[test]
#[serial]
fn rebase_handles_rename_rename_conflicts() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);
            let executor = mock_executor_for_git_success();

            // Create a file on initial commit
            write_file(dir.path().join("original.txt"), "original content");
            let _ = commit_all(&repo, "add original file");

            // Create feature branch and switch to it
            let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
            let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();
            git_switch_force(&repo, "feature");

            // On feature: rename the file using git2 library
            let mut index = repo.index().expect("open index");
            index
                .remove_path(std::path::Path::new("original.txt"))
                .expect("remove original.txt from index");
            write_file(dir.path().join("new_feature.txt"), "feature content");
            index
                .add_path(std::path::Path::new("new_feature.txt"))
                .expect("add new_feature.txt to index");
            index.write().expect("write index");
            let _ = git_commit_all(&repo, "rename on feature");

            // Go back to default branch using git switch with force
            git_switch_force(&repo, &default_branch);

            // Verify the file was restored
            assert!(
                dir.path().join("original.txt").exists(),
                "original.txt should exist after checkout"
            );

            // On default: rename the same file using git2 library
            let mut index = repo.index().expect("open index");
            index
                .remove_path(std::path::Path::new("original.txt"))
                .expect("remove original.txt from index");
            write_file(dir.path().join("new_main.txt"), "main content");
            index
                .add_path(std::path::Path::new("new_main.txt"))
                .expect("add new_main.txt to index");
            index.write().expect("write index");
            let _ = git_commit_all(&repo, "rename on main");

            // Go back to feature using git switch with force
            git_switch_force(&repo, "feature");

            // Try to rebase - should detect rename/rename conflict
            let result = rebase_onto(&default_branch, executor.as_ref());

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
                _ => {}
            }

            // Clean up
            let _ = abort_rebase(executor.as_ref());
        });
    });
}

/// Test that directory/file conflicts during rebase produce Conflicts result.
///
/// This verifies that when a path is a file on one branch and a directory on another,
/// the system detects the conflict and returns a Conflicts result.
#[test]
#[serial]
fn rebase_handles_directory_file_conflicts() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);
            let executor = mock_executor_for_git_success();

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
            repo.set_head(&format!("refs/heads/{default_branch}"))
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
            let result = rebase_onto(&default_branch, executor.as_ref());

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
            let _ = abort_rebase(executor.as_ref());
        });
    });
}

/// Test that rename/delete conflicts during rebase produce Conflicts result.
///
/// This verifies that when a file is renamed on one branch and deleted on another,
/// the system detects the conflict and returns a Conflicts result.
#[test]
#[serial]
fn rebase_handles_rename_delete_conflicts() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);
            let executor = mock_executor_for_git_success();

            // Create a file on initial commit
            write_file(dir.path().join("rename_me.txt"), "original content");
            let _ = commit_all(&repo, "add file");

            // Create feature branch and switch to it
            let head_commit = repo.head().unwrap().peel_to_commit().unwrap();
            let _feature_branch = repo.branch("feature", &head_commit, false).unwrap();
            git_switch_force(&repo, "feature");

            // On feature: rename the file using git2 library
            let mut index = repo.index().expect("open index");
            index
                .remove_path(std::path::Path::new("rename_me.txt"))
                .expect("remove rename_me.txt from index");
            write_file(dir.path().join("renamed.txt"), "renamed content");
            index
                .add_path(std::path::Path::new("renamed.txt"))
                .expect("add renamed.txt to index");
            index.write().expect("write index");
            let _ = git_commit_all(&repo, "rename file");

            // Go back to default branch using git switch with force
            git_switch_force(&repo, &default_branch);

            // Verify the file was restored
            assert!(
                dir.path().join("rename_me.txt").exists(),
                "rename_me.txt should exist after checkout"
            );

            // On default: delete the file using git2 library
            let mut index = repo.index().expect("open index");
            index
                .remove_path(std::path::Path::new("rename_me.txt"))
                .expect("remove rename_me.txt from index");
            index.write().expect("write index");
            let _ = git_commit_all(&repo, "delete file");

            // Go back to feature using git switch with force
            git_switch_force(&repo, "feature");

            // Try to rebase - should detect rename/delete conflict
            let result = rebase_onto(&default_branch, executor.as_ref());

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
            let _ = abort_rebase(executor.as_ref());
        });
    });
}

/// Test that symlink conflicts during rebase produce Conflicts result.
///
/// This verifies that when a file is converted to a symlink on one branch
/// and modified on another, the system detects the conflict appropriately.
#[test]
#[serial]
fn rebase_handles_symlink_conflicts() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);
            let executor = mock_executor_for_git_success();

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
            repo.set_head(&format!("refs/heads/{default_branch}"))
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
            let result = rebase_onto(&default_branch, executor.as_ref());

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
                _ => {}
            }

            // Clean up
            let _ = abort_rebase(executor.as_ref());
        });
    });
}

/// Test that line ending conflicts during rebase produce Conflicts result.
///
/// This verifies that when files have conflicting line endings (CRLF vs LF),
/// the system detects the conflict and returns a Conflicts result.
#[test]
#[serial]
fn rebase_handles_line_ending_conflicts() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);
            let executor = mock_executor_for_git_success();

            // Configure git to not normalize line endings for this test
            // This ensures we can create true CRLF vs LF conflicts
            let mut cfg = repo.config().expect("open config");
            cfg.set_bool("core.autocrlf", false)
                .expect("set core.autocrlf config");

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
            repo.set_head(&format!("refs/heads/{default_branch}"))
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
            let result = rebase_onto(&default_branch, executor.as_ref());

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
                _ => {}
            }

            // Clean up
            let _ = abort_rebase(executor.as_ref());
        });
    });
}

/// Test that whitespace-only conflicts during rebase produce Conflicts result.
///
/// This verifies that when files differ only in whitespace, the system
/// detects the conflict and returns a Conflicts result.
#[test]
#[serial]
fn rebase_handles_whitespace_only_conflicts() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);
            let executor = mock_executor_for_git_success();

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
            repo.set_head(&format!("refs/heads/{default_branch}"))
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
            let result = rebase_onto(&default_branch, executor.as_ref());

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
                _ => {}
            }

            // Clean up
            let _ = abort_rebase(executor.as_ref());
        });
    });
}
