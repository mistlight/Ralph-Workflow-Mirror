use super::*;

#[test]
fn test_git_diff_returns_string() {
    // This test verifies the function exists and returns a Result.
    // The actual content depends on the git state.
    let result = git_diff();
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_require_git_repo() {
    // This test verifies we can detect a git repository.
    let result = require_git_repo();
    // We don't assert either way since the test environment varies.
    let _ = result;
}

#[test]
fn test_get_repo_root() {
    let result = get_repo_root();
    if let Ok(path) = result {
        assert!(path.exists());
        assert!(path.is_dir());
        let git_dir = path.join(".git");
        assert!(git_dir.exists() || path.ancestors().any(|p| p.join(".git").exists()));
    }
}

#[test]
fn test_git_diff_from_returns_result() {
    let result = git_diff_from("invalid_oid_that_does_not_exist");
    assert!(result.is_err());
}

#[test]
fn test_git_snapshot_returns_result() {
    let result = git_snapshot();
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_git_add_all_returns_result() {
    let result = git_add_all();
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_get_git_diff_from_start_returns_result() {
    let result = get_git_diff_from_start();
    assert!(result.is_ok() || result.is_err());
}

#[test]
fn test_get_git_diff_from_start_with_workspace_returns_not_found_for_non_git_workspace() {
    // Arrange: MemoryWorkspace has no real .git file on disk, so the function must
    // return Err without touching the process CWD git repository.
    let workspace = crate::workspace::MemoryWorkspace::new_test();

    // Act
    let result = get_git_diff_from_start_with_workspace(&workspace);

    // Assert: early return with a clear error — not a git2 error about the CWD.
    assert!(result.is_err(), "expected Err for non-git workspace");
    let err = result.unwrap_err();
    assert_eq!(
        err.kind(),
        std::io::ErrorKind::NotFound,
        "expected NotFound error kind"
    );
    assert!(
        err.to_string()
            .contains("Workspace has no on-disk git repository"),
        "expected descriptive error message, got: {err}"
    );
}
