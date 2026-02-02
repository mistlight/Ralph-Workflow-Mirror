// Part 1: Imports, constants, types, and basic operations

use std::io;
use std::path::Path;

use crate::workspace::{Workspace, WorkspaceFs};

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
pub enum StartPoint {
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
///
/// **Note:** This function uses the current working directory to discover the repo.
pub fn get_current_head_oid() -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(|e| to_io_error(&e))?;
    get_current_head_oid_impl(&repo)
}

/// Implementation of get_current_head_oid.
fn get_current_head_oid_impl(repo: &git2::Repository) -> io::Result<String> {
    let head = repo.head().map_err(|e| {
        // Handle UnbornBranch error consistently with git_diff()
        // This provides a clearer error message for empty repositories
        if e.code() == git2::ErrorCode::UnbornBranch {
            io::Error::new(io::ErrorKind::NotFound, "No commits yet (unborn branch)")
        } else {
            to_io_error(&e)
        }
    })?;

    // Get the commit OID
    let head_commit = head.peel_to_commit().map_err(|e| to_io_error(&e))?;

    Ok(head_commit.id().to_string())
}

fn get_current_start_point(repo: &git2::Repository) -> io::Result<StartPoint> {
    let head = repo.head();
    let start_point = match head {
        Ok(head) => {
            let head_commit = head.peel_to_commit().map_err(|e| to_io_error(&e))?;
            StartPoint::Commit(head_commit.id())
        }
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => StartPoint::EmptyRepo,
        Err(e) => return Err(to_io_error(&e)),
    };
    Ok(start_point)
}

/// Save the current HEAD commit as the starting commit.
///
/// Writes the current HEAD OID to `.agent/start_commit` only if it doesn't
/// already exist. This ensures the `start_commit` persists across pipeline runs
/// and is only reset when explicitly requested via `--reset-start-commit`.
///
/// # Errors
///
/// Returns an error if:
/// - The current HEAD cannot be determined
/// - The `.agent` directory cannot be created
/// - The file cannot be written
///
pub fn save_start_commit() -> io::Result<()> {
    let repo = git2::Repository::discover(".").map_err(|e| to_io_error(&e))?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No workdir for repository"))?;
    save_start_commit_impl(&repo, repo_root)
}

/// Implementation of save_start_commit.
fn save_start_commit_impl(repo: &git2::Repository, repo_root: &Path) -> io::Result<()> {
    // If a start commit exists and is valid, preserve it across runs.
    // If it exists but is invalid/corrupt, automatically repair it.
    if load_start_point_impl(repo, repo_root).is_ok() {
        return Ok(());
    }

    write_start_point(repo_root, get_current_start_point(repo)?)
}

fn write_start_commit_with_oid(repo_root: &Path, oid: &str) -> io::Result<()> {
    let workspace = WorkspaceFs::new(repo_root.to_path_buf());
    workspace.write(Path::new(START_COMMIT_FILE), oid)
}

fn write_start_point(repo_root: &Path, start_point: StartPoint) -> io::Result<()> {
    let content = match start_point {
        StartPoint::Commit(oid) => oid.to_string(),
        StartPoint::EmptyRepo => EMPTY_REPO_SENTINEL.to_string(),
    };
    let workspace = WorkspaceFs::new(repo_root.to_path_buf());
    workspace.write(Path::new(START_COMMIT_FILE), &content)
}

/// Write start point to file using workspace abstraction.
///
/// This is the workspace-aware version that should be used in pipeline code
/// where a workspace is available.
fn write_start_point_with_workspace(
    workspace: &dyn Workspace,
    start_point: StartPoint,
) -> io::Result<()> {
    let path = Path::new(START_COMMIT_FILE);
    let content = match start_point {
        StartPoint::Commit(oid) => oid.to_string(),
        StartPoint::EmptyRepo => EMPTY_REPO_SENTINEL.to_string(),
    };
    workspace.write(path, &content)
}

/// Load start point from file using workspace abstraction.
///
/// This version reads the file content via workspace but still validates
/// the commit exists using the provided repository.
///
/// This is the workspace-aware version for pipeline code where git operations
/// are needed for validation.
pub fn load_start_point_with_workspace(
    workspace: &dyn Workspace,
    repo: &git2::Repository,
) -> io::Result<StartPoint> {
    let path = Path::new(START_COMMIT_FILE);
    let content = workspace.read(path)?;
    let raw = content.trim();

    if raw.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "Starting commit file is empty. Run 'ralph --reset-start-commit' to fix.",
        ));
    }

    if raw == EMPTY_REPO_SENTINEL {
        return Ok(StartPoint::EmptyRepo);
    }

    // Validate OID format using libgit2.
    let oid = git2::Oid::from_str(raw).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format\!(
                "Invalid OID format in {}: '{}'. Run 'ralph --reset-start-commit' to fix.",
                START_COMMIT_FILE, raw
            ),
        )
    })?;

    // Ensure the commit still exists in this repository.
    repo.find_commit(oid).map_err(|e| {
        let err_msg = e.message();
        if err_msg.contains("not found") || err_msg.contains("invalid") {
            io::Error::new(
                io::ErrorKind::NotFound,
                format\!(
                    "Start commit '{}' no longer exists (history rewritten). \
                     Run 'ralph --reset-start-commit' to fix.",
                    raw
                ),
            )
        } else {
            to_io_error(&e)
        }
    })?;

    Ok(StartPoint::Commit(oid))
}

/// Save start commit using workspace abstraction.
///
/// This is the workspace-aware version for pipeline code where a workspace
/// is available. If a valid start commit already exists, it is preserved.
pub fn save_start_commit_with_workspace(
    workspace: &dyn Workspace,
    repo: &git2::Repository,
) -> io::Result<()> {
    // If a start commit exists and is valid, preserve it across runs.
    if load_start_point_with_workspace(workspace, repo).is_ok() {
        return Ok(());
    }

    write_start_point_with_workspace(workspace, get_current_start_point(repo)?)
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
///
pub fn load_start_point() -> io::Result<StartPoint> {
    let repo = git2::Repository::discover(".").map_err(|e| {
        io::Error::new(
            io::ErrorKind::NotFound,
            format\!("Git repository error: {e}. Run 'ralph --reset-start-commit' to fix."),
        )
    })?;
    let repo_root = repo
        .workdir()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "No workdir for repository"))?;
    load_start_point_impl(&repo, repo_root)
}

/// Implementation of load_start_point.
fn load_start_point_impl(repo: &git2::Repository, repo_root: &Path) -> io::Result<StartPoint> {
    let workspace = WorkspaceFs::new(repo_root.to_path_buf());
    load_start_point_with_workspace(&workspace, repo)
}

/// Convert git2 error to `io::Error`.
fn to_io_error(err: &git2::Error) -> io::Error {
    io::Error::other(err.to_string())
}
