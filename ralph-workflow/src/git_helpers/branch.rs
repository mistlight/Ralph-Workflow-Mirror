//! Git branch detection and default branch resolution.
//!
//! This module provides functionality to:
//! - Get the current branch name
//! - Check if we're on a main/master branch
//! - Detect the default branch from origin/HEAD
//! - Ensure we're on a feature branch when needed
//!
//! Uses libgit2 directly for all operations.

#![deny(unsafe_code)]

use std::io;

/// Convert git2 error to `io::Error`.
fn git2_to_io_error(err: &git2::Error) -> io::Error {
    io::Error::other(err.to_string())
}

/// Check if the current branch is "main" or "master".
///
/// This is used to determine if we're on a protected branch where
/// rebasing should be skipped.
///
/// # Returns
///
/// Returns `Ok(true)` if on main/master, `Ok(false)` if on another branch,
/// or an error if the branch cannot be determined.
pub fn is_main_or_master_branch() -> io::Result<bool> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    let head = repo.head().map_err(|e| git2_to_io_error(&e))?;

    // Get the branch name from the reference name
    // HEAD is usually a symbolic reference like "refs/heads/main"
    let reference_name = head.shorthand().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::NotFound,
            "Could not determine branch name from HEAD",
        )
    })?;

    Ok(reference_name == "main" || reference_name == "master")
}

/// Get the default branch name from the repository.
///
/// This function attempts to detect the default branch by:
/// 1. Checking `refs/remotes/origin/HEAD` (the origin's default branch)
/// 2. Falling back to checking if "main" or "master" exists locally
/// 3. Defaulting to "main" as a last resort
///
/// # Returns
///
/// Returns `Ok(String)` with the default branch name (e.g., "main", "master"),
/// or an error if the repository cannot be opened.
pub fn get_default_branch() -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Try to get the default branch from origin/HEAD
    // This is set when you clone and represents the default branch
    if let Ok(origin_head) = repo.find_reference("refs/remotes/origin/HEAD") {
        if let Some(target) = origin_head.symbolic_target() {
            // target is usually like "refs/remotes/origin/main"
            if let Some(branch_name) = target.strip_prefix("refs/remotes/origin/") {
                return Ok(branch_name.to_string());
            }
        }
    }

    // Fallback: check if "main" or "master" exists as a local branch
    if repo.find_branch("main", git2::BranchType::Local).is_ok() {
        return Ok("main".to_string());
    }

    if repo.find_branch("master", git2::BranchType::Local).is_ok() {
        return Ok("master".to_string());
    }

    // Ultimate fallback: assume "main"
    Ok("main".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_main_or_master_branch_returns_result() {
        // Test that the function returns a Result
        let result = is_main_or_master_branch();
        // We don't assert success/failure since it depends on git state
        let _ = result;
    }

    #[test]
    fn test_get_default_branch_returns_result() {
        // Test that the function returns a Result
        let result = get_default_branch();
        // We don't assert success/failure since it depends on git state
        let _ = result;
    }
}
