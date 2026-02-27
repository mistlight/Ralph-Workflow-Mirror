use std::io;
use std::path::{Path, PathBuf};

use crate::git_helpers::git2_to_io_error;

/// Check if we're in a git repository.
///
/// # Errors
///
/// Returns error if the operation fails.
pub fn require_git_repo() -> io::Result<()> {
    git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    Ok(())
}

/// Get the git repository root.
///
/// # Errors
///
/// Returns error if the operation fails.
pub fn get_repo_root() -> io::Result<PathBuf> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    repo.workdir()
        .map(PathBuf::from)
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No workdir for repository"))
}

/// Get the git hooks directory path.
///
/// Returns the path to the hooks directory inside .git (or the equivalent
/// for worktrees and other configurations).
pub fn get_hooks_dir() -> io::Result<PathBuf> {
    get_hooks_dir_from(Path::new("."))
}

pub fn get_hooks_dir_from(discovery_root: &Path) -> io::Result<PathBuf> {
    let repo = git2::Repository::discover(discovery_root).map_err(|e| git2_to_io_error(&e))?;
    Ok(repo.path().join("hooks"))
}
