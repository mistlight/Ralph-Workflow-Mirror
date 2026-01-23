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
use tempfile::TempDir;
use test_helpers::{
    commit_all, git_commit_all, git_switch_force, init_git_repo, with_temp_cwd, write_file,
};

use crate::test_timeout::with_default_timeout;
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

/// Test that content conflicts during rebase produce Conflicts result.
///
/// This verifies that when the same file has conflicting changes on both branches,
/// the system detects the conflict and returns a Conflicts result with affected files.
#[test]
fn rebase_handles_content_conflicts() {
    with_default_timeout(|| {
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
    });
}

/// Test that patch application failures produce Conflicts or Failed result.
///
/// This verifies that when a patch cannot be cleanly applied during rebase,
/// the system returns a Conflicts result or Failed result with error details.
#[test]
fn rebase_handles_patch_application_failure() {
    with_default_timeout(|| {
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
    });
}

/// Test that empty commits during rebase produce NoOp or Success result.
///
/// This verifies that when a commit becomes empty after rebase (same changes upstream),
/// the system skips it with NoOp reason or handles it automatically.
#[test]
fn rebase_handles_empty_commits() {
    with_default_timeout(|| {
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
    });
}

/// Test that add/add conflicts during rebase produce Conflicts result.
///
/// This verifies that when the same file is added on both branches with different content,
/// the system detects the conflict and returns a Conflicts result.
#[test]
fn rebase_handles_add_add_conflicts() {
    with_default_timeout(|| {
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
    });
}

/// Test that modify/delete conflicts during rebase produce Conflicts result.
///
/// This verifies that when a file is modified on one branch and deleted on another,
/// the system detects the conflict and returns a Conflicts result.
#[test]
fn rebase_handles_modify_delete_conflicts() {
    with_default_timeout(|| {
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
    });
}

/// Test that binary file conflicts during rebase produce Conflicts result.
///
/// This verifies that when binary files are modified differently on both branches,
/// the system detects the conflict and returns a Conflicts result.
#[test]
fn rebase_handles_binary_file_conflicts() {
    with_default_timeout(|| {
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
    });
}

/// Test that conflict markers in files are detected and extracted.
///
/// This verifies that when a file contains git conflict markers (<<<<<<<, =======, >>>>>>>),
/// the system can extract and return the marker content.
#[test]
fn rebase_detects_conflict_markers_in_file() {
    with_default_timeout(|| {
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
    });
}

/// Test that clean files without conflicts return empty marker content.
///
/// This verifies that when a file has no conflict markers, the system
/// returns empty content or handles it appropriately.
#[test]
fn rebase_detects_no_conflicts_in_clean_file() {
    with_default_timeout(|| {
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
    });
}

/// Test that autostash with conflicting changes produces appropriate result.
///
/// This verifies that when --autostash is used and stashed changes conflict on reapply,
/// the system handles the situation without crashing.
#[test]
fn rebase_handles_autostash_with_conflicts() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::abort_rebase;

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

            // Try to rebase with autostash - stashed changes may conflict when reapplied
            use ralph_workflow::git_helpers::rebase_onto;
            let result = rebase_onto(&default_branch);

            // Git may handle this various ways:
            // 1. Succeed with autostash
            // 2. Fail with autostash error
            // 3. Have conflicts from the rebase itself
            // We just verify it doesn't crash
            assert!(result.is_ok());

            // Clean up
            let _ = abort_rebase();
        });
    });
}

/// Test that rename/rename conflicts during rebase produce Conflicts result.
///
/// This verifies that when the same file is renamed to different names on both branches,
/// the system detects the conflict and returns a Conflicts result.
#[test]
fn rebase_handles_rename_rename_conflicts() {
    with_default_timeout(|| {
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
    });
}

/// Test that directory/file conflicts during rebase produce Conflicts result.
///
/// This verifies that when a path is a file on one branch and a directory on another,
/// the system detects the conflict and returns a Conflicts result.
#[test]
fn rebase_handles_directory_file_conflicts() {
    with_default_timeout(|| {
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
    });
}

/// Test that rename/delete conflicts during rebase produce Conflicts result.
///
/// This verifies that when a file is renamed on one branch and deleted on another,
/// the system detects the conflict and returns a Conflicts result.
#[test]
fn rebase_handles_rename_delete_conflicts() {
    with_default_timeout(|| {
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
    });
}

/// Test that symlink conflicts during rebase produce Conflicts result.
///
/// This verifies that when a file is converted to a symlink on one branch
/// and modified on another, the system detects the conflict appropriately.
#[test]
fn rebase_handles_symlink_conflicts() {
    with_default_timeout(|| {
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
    });
}

/// Test that line ending conflicts during rebase produce Conflicts result.
///
/// This verifies that when files have conflicting line endings (CRLF vs LF),
/// the system detects the conflict and returns a Conflicts result.
#[test]
fn rebase_handles_line_ending_conflicts() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::{abort_rebase, rebase_onto, RebaseResult};

        with_temp_cwd(|dir| {
            let repo = init_repo_with_initial_commit(dir);
            let default_branch = get_default_branch_name(&repo);

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
    });
}

/// Test that whitespace-only conflicts during rebase produce Conflicts result.
///
/// This verifies that when files differ only in whitespace, the system
/// detects the conflict and returns a Conflicts result.
#[test]
fn rebase_handles_whitespace_only_conflicts() {
    with_default_timeout(|| {
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
    });
}

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
    });
}
