use std::io;

use crate::git_helpers::git2_to_io_error;
use std::path::Path;

/// Get a snapshot of the current git status.
///
/// Returns status in porcelain format (similar to `git status --porcelain=v1`).
pub fn git_snapshot() -> io::Result<String> {
    git_snapshot_in_repo(Path::new("."))
}

/// Get a snapshot of git status for a specific repository root.
///
/// Prefer this in pipeline code where `ctx.repo_root` is known, to avoid
/// accidentally discovering/inspecting the wrong repository.
pub fn git_snapshot_in_repo(repo_root: &Path) -> io::Result<String> {
    let repo = git2::Repository::discover(repo_root).map_err(|e| git2_to_io_error(&e))?;
    git_snapshot_impl(&repo)
}

/// Implementation of git snapshot.
fn git_snapshot_impl(repo: &git2::Repository) -> io::Result<String> {
    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|e| git2_to_io_error(&e))?;

    let mut result = String::new();
    for entry in statuses.iter() {
        let status = entry.status();
        let path = entry.path().unwrap_or("").to_string();

        // Convert git2 status to porcelain format.
        // Untracked files are represented as "??" in porcelain v1.
        if status.contains(git2::Status::WT_NEW) {
            result.push('?');
            result.push('?');
            result.push(' ');
            result.push_str(&path);
            result.push('\n');
            continue;
        }

        // Index status
        let index_status = if status.contains(git2::Status::INDEX_NEW) {
            'A'
        } else if status.contains(git2::Status::INDEX_MODIFIED) {
            'M'
        } else if status.contains(git2::Status::INDEX_DELETED) {
            'D'
        } else if status.contains(git2::Status::INDEX_RENAMED) {
            'R'
        } else if status.contains(git2::Status::INDEX_TYPECHANGE) {
            'T'
        } else {
            ' '
        };

        // Worktree status
        let wt_status = if status.contains(git2::Status::WT_MODIFIED) {
            'M'
        } else if status.contains(git2::Status::WT_DELETED) {
            'D'
        } else if status.contains(git2::Status::WT_RENAMED) {
            'R'
        } else if status.contains(git2::Status::WT_TYPECHANGE) {
            'T'
        } else {
            ' '
        };

        result.push(index_status);
        result.push(wt_status);
        result.push(' ');
        result.push_str(&path);
        result.push('\n');
    }

    Ok(result)
}
