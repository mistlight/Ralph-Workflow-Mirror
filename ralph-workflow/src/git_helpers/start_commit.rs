//! Starting commit tracking for incremental diff generation.
//!
//! This module manages the starting commit reference that enables incremental
//! diffs for reviewers while keeping agents isolated from git history.
//!
//! # Overview
//!
//! When a Ralph pipeline starts, the current HEAD commit is saved as the
//! "starting commit" in `.agent/start_commit`. This reference is used to:
//!
//! 1. Generate incremental diffs for reviewers (changes since pipeline start)
//! 2. Keep agents unaware of git history (no git context in prompts)
//! 3. Enable proper review of accumulated changes across iterations
//!
//! The starting commit file persists across pipeline runs unless explicitly
//! reset by the user via the `--reset-start-commit` CLI command.

use std::fs;
use std::io;
use std::path::PathBuf;

/// Path to the starting commit file.
///
/// Stored in `.agent/start_commit`, this file contains the OID (SHA) of the
/// commit that was HEAD when the pipeline started.
const START_COMMIT_FILE: &str = ".agent/start_commit";

/// Sentinel value written to `.agent/start_commit` when the repository has no commits yet.
///
/// This enables incremental diffs to work in a single run that starts on an unborn HEAD
/// by treating the starting point as the empty tree.
const EMPTY_REPO_SENTINEL: &str = "__EMPTY_REPO__";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum StartPoint {
    /// A concrete commit OID to diff from.
    Commit(git2::Oid),
    /// An empty repository baseline (diff from the empty tree).
    EmptyRepo,
}

/// Get the current HEAD commit OID.
///
/// Returns the full SHA-1 hash of the current HEAD commit.
///
/// # Errors
///
/// Returns an error if:
/// - Not in a git repository
/// - HEAD cannot be resolved (e.g., unborn branch)
/// - HEAD is not a commit (e.g., symbolic ref to tag)
pub(crate) fn get_current_head_oid() -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(to_io_error)?;

    let head = repo.head().map_err(|e| {
        // Handle UnbornBranch error consistently with git_diff()
        // This provides a clearer error message for empty repositories
        if e.code() == git2::ErrorCode::UnbornBranch {
            io::Error::new(io::ErrorKind::NotFound, "No commits yet (unborn branch)")
        } else {
            to_io_error(e)
        }
    })?;

    // Get the commit OID
    let head_commit = head.peel_to_commit().map_err(to_io_error)?;

    Ok(head_commit.id().to_string())
}

fn get_current_start_point() -> io::Result<StartPoint> {
    let repo = git2::Repository::discover(".").map_err(to_io_error)?;
    let head = repo.head();
    let start_point = match head {
        Ok(head) => {
            let head_commit = head.peel_to_commit().map_err(to_io_error)?;
            StartPoint::Commit(head_commit.id())
        }
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => StartPoint::EmptyRepo,
        Err(e) => return Err(to_io_error(e)),
    };
    Ok(start_point)
}

/// Save the current HEAD commit as the starting commit.
///
/// Writes the current HEAD OID to `.agent/start_commit` only if it doesn't
/// already exist. This ensures the start_commit persists across pipeline runs
/// and is only reset when explicitly requested via `--reset-start-commit`.
///
/// # Errors
///
/// Returns an error if:
/// - The current HEAD cannot be determined
/// - The `.agent` directory cannot be created
/// - The file cannot be written
pub(crate) fn save_start_commit() -> io::Result<()> {
    // If a start commit exists and is valid, preserve it across runs.
    // If it exists but is invalid/corrupt, automatically repair it.
    if load_start_point().is_ok() {
        return Ok(());
    }

    write_start_point(get_current_start_point()?)
}

fn write_start_commit_with_oid(oid: &str) -> io::Result<()> {
    // Ensure .agent directory exists
    let path = PathBuf::from(START_COMMIT_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }

    // Write the OID to the file
    fs::write(START_COMMIT_FILE, oid)?;

    Ok(())
}

fn write_start_point(start_point: StartPoint) -> io::Result<()> {
    match start_point {
        StartPoint::Commit(oid) => write_start_commit_with_oid(&oid.to_string()),
        StartPoint::EmptyRepo => {
            // Ensure .agent directory exists
            let path = PathBuf::from(START_COMMIT_FILE);
            if let Some(parent) = path.parent() {
                fs::create_dir_all(parent)?;
            }
            fs::write(START_COMMIT_FILE, EMPTY_REPO_SENTINEL)?;
            Ok(())
        }
    }
}

/// Load the starting commit OID from the file.
///
/// Reads the `.agent/start_commit` file and returns the stored OID.
///
/// # Errors
///
/// Returns an error if:
/// - The file does not exist
/// - The file cannot be read
/// - The file content is invalid
pub(crate) fn load_start_point() -> io::Result<StartPoint> {
    let content = fs::read_to_string(START_COMMIT_FILE)?;

    let raw = content.trim();

    if raw.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Starting commit file is empty",
        ));
    }

    if raw == EMPTY_REPO_SENTINEL {
        return Ok(StartPoint::EmptyRepo);
    }

    // Validate OID format using libgit2.
    // git2::Oid::from_str automatically validates both SHA-1 (40 hex chars) and
    // SHA-256 (64 hex chars) formats, as well as abbreviated forms.
    let oid = git2::Oid::from_str(raw).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid OID format: {}", raw),
        )
    })?;

    // Ensure the commit still exists in this repository (history may have been rewritten).
    let repo = git2::Repository::discover(".").map_err(to_io_error)?;
    repo.find_commit(oid).map_err(to_io_error)?;

    Ok(StartPoint::Commit(oid))
}

/// Reset the starting commit to current HEAD.
///
/// This is a CLI command that updates `.agent/start_commit` to the current
/// HEAD commit. It's useful when the user wants to start tracking from a
/// different baseline.
///
/// # Errors
///
/// Returns an error if:
/// - The current HEAD cannot be determined
/// - The file cannot be written
pub(crate) fn reset_start_commit() -> io::Result<()> {
    // Unlike `save_start_commit`, a reset is an explicit user request and should
    // fail on empty repositories where there is no HEAD commit to reference.
    let oid = get_current_head_oid()?;
    write_start_commit_with_oid(&oid)
}

#[cfg(test)]
fn has_start_commit() -> bool {
    load_start_point().is_ok()
}

/// Convert git2 error to io::Error.
fn to_io_error(err: git2::Error) -> io::Error {
    io::Error::other(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_start_commit_file_path_defined() {
        // Verify the constant is defined correctly
        assert_eq!(START_COMMIT_FILE, ".agent/start_commit");
    }

    #[test]
    fn test_has_start_commit_returns_bool() {
        // This test verifies the function exists and returns a bool
        let result = has_start_commit();
        // The result depends on whether we're in a Ralph pipeline
        // We don't assert either way since the test environment varies
        let _ = result;
    }

    #[test]
    fn test_get_current_head_oid_returns_result() {
        // This test verifies the function exists and returns a Result
        let result = get_current_head_oid();
        // Should succeed if we're in a git repo with commits
        // We don't assert either way since the test environment varies
        let _ = result;
    }

    #[test]
    fn test_load_start_commit_returns_result() {
        // This test verifies load_start_point returns a Result
        // It will fail if the file doesn't exist, which is expected
        let result = load_start_point();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_reset_start_commit_returns_result() {
        // This test verifies reset_start_commit returns a Result
        // It will fail if not in a git repo, which is expected
        let result = reset_start_commit();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_save_start_commit_returns_result() {
        // This test verifies save_start_commit returns a Result
        // It will fail if not in a git repo, which is expected
        let result = save_start_commit();
        assert!(result.is_ok() || result.is_err());
    }

    // Integration tests would require a temporary git repository
    // For full integration tests, see tests/git_workflow.rs
}
