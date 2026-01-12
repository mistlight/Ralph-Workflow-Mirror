//! Basic git repository operations.
//!
//! Provides fundamental git operations used throughout the application:
//!
//! - Repository detection and root path resolution
//! - Working tree status snapshots (porcelain format)
//! - Staging and committing changes
//!
//! All operations use the `git` CLI directly rather than libgit2 for simplicity
//! and to ensure behavior matches user expectations from the command line.

use std::io;
use std::path::PathBuf;
use std::process::Command;

/// Check if we're in a git repository.
pub(crate) fn require_git_repo() -> io::Result<()> {
    let output = Command::new("git")
        .args(["rev-parse", "--is-inside-work-tree"])
        .output()?;

    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Not inside a git repo",
        ))
    }
}

/// Get the git repository root.
pub(crate) fn get_repo_root() -> io::Result<PathBuf> {
    let output = Command::new("git")
        .args(["rev-parse", "--show-toplevel"])
        .output()?;

    if output.status.success() {
        let path = String::from_utf8_lossy(&output.stdout).trim().to_string();
        Ok(PathBuf::from(path))
    } else {
        Err(io::Error::new(
            io::ErrorKind::NotFound,
            "Not in a git repository",
        ))
    }
}

/// Get a snapshot of the current git status.
pub(crate) fn git_snapshot() -> io::Result<String> {
    let output = Command::new("git")
        .args(["status", "--porcelain=v1"])
        .output()?;

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Stage all changes.
pub(crate) fn git_add_all() -> io::Result<()> {
    let status = Command::new("git").args(["add", "-A"]).status()?;

    if status.success() {
        Ok(())
    } else {
        Err(io::Error::other("git add failed"))
    }
}

/// Create a commit.
pub(crate) fn git_commit(message: &str) -> io::Result<bool> {
    let status = Command::new("git")
        .args(["commit", "-m", message])
        .status()?;

    Ok(status.success())
}
