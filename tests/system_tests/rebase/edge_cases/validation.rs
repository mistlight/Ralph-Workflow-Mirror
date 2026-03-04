//! # Rebase Precondition Validation Tests
//!
//! Tests for `validate_rebase_preconditions` function that checks repository
//! state before attempting a rebase:
//! - Shallow clone detection
//! - Uninitialized submodule detection
//! - Sparse checkout configuration validation
//! - Path length validation
//!
//! ## Expected Behavior
//!
//! The validation function should detect problematic repository configurations
//! that could cause rebase failures, allowing the system to fail fast with
//! clear error messages before attempting git operations.

use std::fs;
use test_helpers::{commit_all, with_temp_cwd};

use crate::common::mock_executor_for_git_success;
use crate::test_timeout::with_default_timeout;
use serial_test::serial;

use super::init_repo_with_initial_commit;

/// Test that precondition validation succeeds on a clean repository.
///
/// This verifies that a normal repository with no special configurations
/// passes all validation checks.
#[test]
#[serial]
fn validate_rebase_preconditions_succeeds_on_clean_repo() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::validate_rebase_preconditions;

        with_temp_cwd(|dir| {
            let _repo = init_repo_with_initial_commit(dir);

            // Clean repository should pass precondition validation
            let executor = mock_executor_for_git_success();
            let result = validate_rebase_preconditions(executor.as_ref());

            assert!(
                result.is_ok(),
                "Should pass precondition check with clean repository: {result:?}"
            );
        });
    });
}

/// Test that rebase precondition validation detects shallow clones.
///
/// This verifies that when a repository is a shallow clone with incomplete
/// history, the system fails precondition validation with an appropriate error.
#[test]
#[serial]
fn validate_rebase_preconditions_detects_shallow_clone() {
    with_default_timeout(|| {
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
            let executor = mock_executor_for_git_success();
            let result = validate_rebase_preconditions(executor.as_ref());

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
    });
}

/// Test that rebase precondition validation detects uninitialized submodules.
///
/// This verifies that when .gitmodules exists but submodules are not
/// initialized, the system fails precondition validation appropriately.
#[test]
#[serial]
fn validate_rebase_preconditions_detects_uninitialized_submodules() {
    with_default_timeout(|| {
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
            let executor = mock_executor_for_git_success();
            let result = validate_rebase_preconditions(executor.as_ref());

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
    });
}

/// Test that rebase precondition validation succeeds with initialized submodules.
///
/// This verifies that when .gitmodules exists and submodules are properly
/// initialized, the system passes precondition validation successfully.
#[test]
#[serial]
fn validate_rebase_preconditions_succeeds_with_initialized_submodules() {
    with_default_timeout(|| {
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
            let executor = mock_executor_for_git_success();
            let result = validate_rebase_preconditions(executor.as_ref());

            assert!(
                result.is_ok(),
                "Should pass precondition check with initialized submodules: {result:?}"
            );
        });
    });
}

/// Test that rebase precondition validation succeeds without submodules.
///
/// This verifies that when no .gitmodules file exists (no submodules),
/// the system passes precondition validation successfully.
#[test]
#[serial]
fn validate_rebase_preconditions_succeeds_without_submodules() {
    with_default_timeout(|| {
        use ralph_workflow::git_helpers::validate_rebase_preconditions;

        with_temp_cwd(|dir| {
            let _repo = init_repo_with_initial_commit(dir);

            // No .gitmodules file - no submodules exist
            assert!(!dir.path().join(".gitmodules").exists());

            // Precondition validation should succeed
            let executor = mock_executor_for_git_success();
            let result = validate_rebase_preconditions(executor.as_ref());

            assert!(
                result.is_ok(),
                "Should pass precondition check without submodules: {result:?}"
            );
        });
    });
}

/// Test that rebase precondition validation detects misconfigured sparse checkout.
///
/// This verifies that when sparse checkout is enabled but the sparse-checkout
/// file is missing, the system fails precondition validation appropriately.
#[test]
#[serial]
fn validate_rebase_preconditions_detects_misconfigured_sparse_checkout() {
    with_default_timeout(|| {
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
            let executor = mock_executor_for_git_success();
            let result = validate_rebase_preconditions(executor.as_ref());

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
    });
}

/// Test that rebase precondition validation succeeds with proper sparse checkout.
///
/// This verifies that when sparse checkout is enabled and properly configured
/// with a valid sparse-checkout file, the system passes precondition validation.
#[test]
#[serial]
fn validate_rebase_preconditions_succeeds_with_proper_sparse_checkout() {
    with_default_timeout(|| {
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
            let executor = mock_executor_for_git_success();
            let result = validate_rebase_preconditions(executor.as_ref());

            assert!(
                result.is_ok(),
                "Should pass precondition check with proper sparse checkout: {result:?}"
            );

            // Clean up: remove the config key
            let _ = config.remove("core.sparseCheckout");
        });
    });
}

/// Test that rebase precondition validation detects empty sparse checkout config.
///
/// This verifies that when the sparse-checkout file exists but is empty,
/// the system fails precondition validation with an appropriate error.
#[test]
#[serial]
fn validate_rebase_preconditions_detects_empty_sparse_checkout_config() {
    with_default_timeout(|| {
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
            let executor = mock_executor_for_git_success();
            let result = validate_rebase_preconditions(executor.as_ref());

            assert!(
                result.is_err(),
                "Should fail precondition check with empty sparse checkout config"
            );

            let err_msg = result.unwrap_err().to_string().to_lowercase();
            assert!(
                err_msg.contains("sparse")
                    || err_msg.contains("empty")
                    || err_msg.contains("checkout"),
                "Error message should mention sparse checkout or empty config: {err_msg}"
            );

            // Clean up: remove the config key
            let _ = config.remove("core.sparseCheckout");
        });
    });
}
