// Core rebase operations: precondition validation, rebase execution, and lifecycle management.
//
// This file contains:
// - Precondition validation (shallow clone, worktree, submodule, sparse checkout checks)
// - rebase_onto - main rebase execution function
// - abort_rebase - abort an in-progress rebase
// - continue_rebase - continue after conflict resolution
// - get_conflicted_files - retrieve list of conflicted files
// - verify_rebase_completed - verify rebase completion status
// - rebase_in_progress - check if rebase is in progress

/// Validate preconditions before starting a rebase operation.
///
/// This function performs Category 1 (pre-start) validation checks to ensure
/// the repository is in a valid state for rebasing. It checks for common
/// issues that would cause a rebase to fail immediately.
///
/// # Arguments
///
/// * `executor` - Process executor for dependency injection
///
/// # Returns
///
/// Returns `Ok(())` if all preconditions are met, or an error with a
/// descriptive message if validation fails.
///
/// # Validation Checks
///
/// - Repository integrity (valid HEAD, index, object database)
/// - No concurrent Git operations (merge, rebase, cherry-pick, etc.)
/// - Git identity is configured (user.name and user.email)
/// - Working tree is not dirty (no unstaged or staged changes)
/// - Not a shallow clone (shallow clones have limited history)
/// - No worktree conflicts (branch not checked out elsewhere)
/// - Submodules are initialized and in a valid state
/// - Sparse checkout is properly configured (if enabled)
///
/// # Example
///
/// ```ignore
/// use ralph_workflow::git_helpers::rebase::validate_rebase_preconditions;
///
/// match validate_rebase_preconditions(&executor) {
///     Ok(()) => println!("All preconditions met, safe to rebase"),
///     Err(e) => eprintln!("Cannot rebase: {e}"),
/// }
/// ```
#[cfg(any(test, feature = "test-utils"))]
pub fn validate_rebase_preconditions(
    executor: &dyn crate::executor::ProcessExecutor,
) -> io::Result<()> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // 1. Check repository integrity
    validate_git_state()?;

    // 2. Check for concurrent Git operations
    if let Some(concurrent_op) = detect_concurrent_git_operations()? {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Cannot start rebase: {} already in progress. \
                 Please complete or abort the current operation first.",
                concurrent_op.description()
            ),
        ));
    }

    // 3. Check Git identity configuration
    let config = repo.config().map_err(|e| git2_to_io_error(&e))?;

    let user_name = config.get_string("user.name");
    let user_email = config.get_string("user.email");

    if user_name.is_err() && user_email.is_err() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Git identity is not configured. Please set user.name and user.email:\n  \
             git config --global user.name \"Your Name\"\n  \
             git config --global user.email \"you@example.com\"",
        ));
    }

    // 4. Check for dirty working tree using Git CLI via executor
    let status_output = executor.execute("git", &["status", "--porcelain"], &[], None)?;

    if status_output.status.success() {
        let stdout = status_output.stdout.trim();
        if !stdout.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Working tree is not clean. Please commit or stash changes before rebasing.",
            ));
        }
    } else {
        // If git status fails, try with libgit2 as fallback
        let statuses = repo.statuses(None).map_err(|e| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("Failed to check working tree status: {e}"),
            )
        })?;

        if !statuses.is_empty() {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "Working tree is not clean. Please commit or stash changes before rebasing.",
            ));
        }
    }

    // 5. Check for shallow clone (limited history)
    check_shallow_clone()?;

    // 6. Check for worktree conflicts (branch checked out in another worktree)
    check_worktree_conflicts()?;

    // 7. Check submodule state (if submodules exist)
    check_submodule_state()?;

    // 8. Check sparse checkout configuration (if enabled)
    check_sparse_checkout_state()?;

    Ok(())
}

/// Check if the repository is a shallow clone.
///
/// Shallow clones have limited history and may not have all the commits
/// needed for a successful rebase.
///
/// # Returns
///
/// Returns `Ok(())` if the repository is a full clone, or an error if
/// it's a shallow clone.
#[cfg(any(test, feature = "test-utils"))]
fn check_shallow_clone() -> io::Result<()> {
    use std::fs;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    let git_dir = repo.path();

    // Check for shallow marker file
    let shallow_file = git_dir.join("shallow");
    if shallow_file.exists() {
        // This is a shallow clone - read the file to see how many commits we have
        let content = fs::read_to_string(&shallow_file).unwrap_or_default();
        let line_count = content.lines().count();

        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Repository is a shallow clone with {} commits. \
                 Rebasing may fail due to missing history. \
                 Consider running: git fetch --unshallow",
                line_count
            ),
        ));
    }

    Ok(())
}

/// Check if the current branch is checked out in another worktree.
///
/// Git does not allow a branch to be checked out in multiple worktrees
/// simultaneously.
///
/// # Returns
///
/// Returns `Ok(())` if the branch is not checked out elsewhere, or an
/// error if there's a worktree conflict.
#[cfg(any(test, feature = "test-utils"))]
fn check_worktree_conflicts() -> io::Result<()> {
    use std::fs;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Get current branch name
    let head = repo.head().map_err(|e| git2_to_io_error(&e))?;
    let branch_name = match head.shorthand() {
        Some(name) if head.is_branch() => name,
        _ => return Ok(()), // Detached HEAD or unborn branch - skip check
    };

    let git_dir = repo.path();
    let worktrees_dir = git_dir.join("worktrees");

    if !worktrees_dir.exists() {
        return Ok(());
    }

    // Check each worktree to see if our branch is checked out there
    let entries = fs::read_dir(&worktrees_dir).map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Failed to read worktrees directory: {e}"),
        )
    })?;

    for entry in entries.flatten() {
        let worktree_path = entry.path();
        let worktree_head = worktree_path.join("HEAD");

        if worktree_head.exists() {
            if let Ok(content) = fs::read_to_string(&worktree_head) {
                // Check if this worktree has our branch checked out
                if content.contains(&format!("refs/heads/{branch_name}")) {
                    // Extract worktree name from path
                    let worktree_name = worktree_path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");

                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        format!(
                            "Branch '{branch_name}' is already checked out in worktree '{worktree_name}'. \
                             Use 'git worktree add' to create a new worktree for this branch."
                        ),
                    ));
                }
            }
        }
    }

    Ok(())
}

/// Check if submodules are in a valid state.
///
/// Submodules should be initialized and updated before rebasing to avoid
/// conflicts and errors.
///
/// # Returns
///
/// Returns `Ok(())` if submodules are in a valid state or no submodules
/// exist, or an error if there are submodule issues.
#[cfg(any(test, feature = "test-utils"))]
fn check_submodule_state() -> io::Result<()> {
    use std::fs;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    let git_dir = repo.path();

    // Check if .gitmodules exists
    let workdir = repo.workdir().unwrap_or(git_dir);
    let gitmodules_path = workdir.join(".gitmodules");

    if !gitmodules_path.exists() {
        return Ok(()); // No submodules
    }

    // We have submodules - check for common issues
    let modules_dir = git_dir.join("modules");
    if !modules_dir.exists() {
        // .gitmodules exists but .git/modules doesn't - submodules not initialized
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Submodules are not initialized. Run: git submodule update --init --recursive",
        ));
    }

    // Check for orphaned submodule references (common issue after rebasing)
    let gitmodules_content = fs::read_to_string(&gitmodules_path).unwrap_or_default();
    let submodule_count = gitmodules_content.matches("path = ").count();

    if submodule_count > 0 {
        // Verify each submodule directory exists
        for line in gitmodules_content.lines() {
            if line.contains("path = ") {
                if let Some(path) = line.split("path = ").nth(1) {
                    let submodule_path = workdir.join(path.trim());
                    if !submodule_path.exists() {
                        return Err(io::Error::new(
                            io::ErrorKind::InvalidInput,
                            format!(
                                "Submodule '{}' is not initialized. Run: git submodule update --init --recursive",
                                path.trim()
                            ),
                        ));
                    }
                }
            }
        }
    }

    Ok(())
}

/// Check if sparse checkout is properly configured.
///
/// Sparse checkout can cause issues during rebase if files outside the
/// sparse checkout cone are modified.
///
/// # Returns
///
/// Returns `Ok(())` if sparse checkout is not enabled or is properly
/// configured, or an error if there are issues.
#[cfg(any(test, feature = "test-utils"))]
fn check_sparse_checkout_state() -> io::Result<()> {
    use std::fs;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    let git_dir = repo.path();

    // Check if sparse checkout is enabled
    let config = repo.config().map_err(|e| git2_to_io_error(&e))?;

    let sparse_checkout = config.get_bool("core.sparseCheckout");
    let sparse_checkout_cone = config.get_bool("extensions.sparseCheckoutCone");

    match (sparse_checkout, sparse_checkout_cone) {
        (Ok(true), _) | (_, Ok(true)) => {
            // Sparse checkout is enabled - check if it's properly configured
            let info_sparse_dir = git_dir.join("info").join("sparse-checkout");

            if !info_sparse_dir.exists() {
                // Sparse checkout enabled but no config file - this is a problem
                return Err(io::Error::new(
                    io::ErrorKind::InvalidInput,
                    "Sparse checkout is enabled but not configured. \
                     Run: git sparse-checkout init",
                ));
            }

            // Verify the sparse-checkout file has content
            if let Ok(content) = fs::read_to_string(&info_sparse_dir) {
                if content.trim().is_empty() {
                    return Err(io::Error::new(
                        io::ErrorKind::InvalidInput,
                        "Sparse checkout configuration is empty. \
                         Run: git sparse-checkout set <patterns>",
                    ));
                }
            }

            // Sparse checkout is enabled - warn but don't fail
            // Rebase should work with sparse checkout, but conflicts may occur
            // for files outside the sparse checkout cone
            // We return Ok to allow the operation, but the caller should be aware
        }
        (Err(_), _) | (_, Err(_)) => {
            // Config not set - sparse checkout not enabled
        }
        _ => {}
    }

    Ok(())
}

/// Perform a rebase onto the specified upstream branch.
///
/// This function rebases the current branch onto the specified upstream branch.
/// It handles the full rebase process including conflict detection and
/// classifies all known failure modes.
///
/// # Arguments
///
/// * `upstream_branch` - The branch to rebase onto (e.g., "main", "origin/main")
/// * `executor` - Process executor for dependency injection
///
/// # Returns
///
/// Returns `Ok(RebaseResult)` indicating the outcome, or an error if:
/// - The repository cannot be opened
/// - The rebase operation fails in an unexpected way
///
/// # Edge Cases Handled
///
/// - Empty repository (no commits) - Returns `Ok(RebaseResult::NoOp)` with reason
/// - Unborn branch - Returns `Ok(RebaseResult::NoOp)` with reason
/// - Already up-to-date - Returns `Ok(RebaseResult::NoOp)` with reason
/// - Unrelated branches (no shared ancestor) - Returns `Ok(RebaseResult::NoOp)` with reason
/// - On main/master branch - Returns `Ok(RebaseResult::NoOp)` with reason
/// - Conflicts during rebase - Returns `Ok(RebaseResult::Conflicts)` or `Failed` with error kind
/// - Other failures - Returns `Ok(RebaseResult::Failed)` with appropriate error kind
///
/// # Note
///
/// This function uses git CLI for rebase operations as libgit2's rebase API
/// has limitations and complexity that make it unreliable for production use.
/// The git CLI is more robust and better tested for rebase operations.
///
pub fn rebase_onto(
    upstream_branch: &str,
    executor: &dyn crate::executor::ProcessExecutor,
) -> io::Result<RebaseResult> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    rebase_onto_impl(&repo, upstream_branch, executor)
}

/// Implementation of rebase_onto.
fn rebase_onto_impl(
    repo: &git2::Repository,
    upstream_branch: &str,
    executor: &dyn crate::executor::ProcessExecutor,
) -> io::Result<RebaseResult> {
    // Check if we have any commits

    match repo.head() {
        Ok(_) => {}
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // No commits yet - nothing to rebase
            return Ok(RebaseResult::NoOp {
                reason: "Repository has no commits yet (unborn branch)".to_string(),
            });
        }
        Err(e) => return Err(git2_to_io_error(&e)),
    }

    // Get the upstream branch to ensure it exists
    let upstream_object = match repo.revparse_single(upstream_branch) {
        Ok(obj) => obj,
        Err(_) => {
            return Ok(RebaseResult::Failed(RebaseErrorKind::InvalidRevision {
                revision: upstream_branch.to_string(),
            }))
        }
    };

    let upstream_commit = upstream_object
        .peel_to_commit()
        .map_err(|e| git2_to_io_error(&e))?;

    // Get our branch commit
    let head = repo.head().map_err(|e| git2_to_io_error(&e))?;
    let head_commit = head.peel_to_commit().map_err(|e| git2_to_io_error(&e))?;

    // Check if we're already up-to-date
    if repo
        .graph_descendant_of(head_commit.id(), upstream_commit.id())
        .map_err(|e| git2_to_io_error(&e))?
    {
        // Already up-to-date
        return Ok(RebaseResult::NoOp {
            reason: "Branch is already up-to-date with upstream".to_string(),
        });
    }

    // Check if branches share a common ancestor
    // If merge_base fails with NotFound, branches are unrelated
    match repo.merge_base(head_commit.id(), upstream_commit.id()) {
        Err(e)
            if e.class() == git2::ErrorClass::Reference
                && e.code() == git2::ErrorCode::NotFound =>
        {
            // Branches are unrelated - no shared history
            return Ok(RebaseResult::NoOp {
                reason: format!(
                    "No common ancestor between current branch and '{upstream_branch}' (unrelated branches)"
                ),
            });
        }
        Err(e) => return Err(git2_to_io_error(&e)),
        Ok(_) => {}
    }

    // Check if we're on main/master or in a detached HEAD state
    let branch_name = match head.shorthand() {
        Some(name) => name,
        None => {
            // Detached HEAD state - rebase is not applicable
            return Ok(RebaseResult::NoOp {
                reason: "HEAD is detached (not on any branch), rebase not applicable".to_string(),
            });
        }
    };

    if branch_name == "main" || branch_name == "master" {
        return Ok(RebaseResult::NoOp {
            reason: format!("Already on '{branch_name}' branch, rebase not applicable"),
        });
    }

    // Use git CLI for rebase via executor - more reliable than libgit2
    let output = executor.execute("git", &["rebase", upstream_branch], &[], None)?;

    if output.status.success() {
        Ok(RebaseResult::Success)
    } else {
        let stderr = &output.stderr;
        let stdout = &output.stdout;

        // Use classify_rebase_error to determine specific failure mode
        let error_kind = classify_rebase_error(stderr, stdout);

        match error_kind {
            RebaseErrorKind::ContentConflict { .. } => {
                // For conflicts, get of actual conflicted files
                match get_conflicted_files() {
                    Ok(files) if files.is_empty() => {
                        // If we detected a conflict but can't get of files,
                        // return error kind with files from error
                        if let RebaseErrorKind::ContentConflict { files } = error_kind {
                            Ok(RebaseResult::Conflicts(files))
                        } else {
                            Ok(RebaseResult::Conflicts(vec![]))
                        }
                    }
                    Ok(files) => Ok(RebaseResult::Conflicts(files)),
                    Err(_) => Ok(RebaseResult::Conflicts(vec![])),
                }
            }
            RebaseErrorKind::Unknown { .. } => {
                // Check for "up to date" message which is actually a no-op
                if stderr.contains("up to date") {
                    Ok(RebaseResult::NoOp {
                        reason: "Branch is already up-to-date with upstream".to_string(),
                    })
                } else {
                    Ok(RebaseResult::Failed(error_kind))
                }
            }
            _ => Ok(RebaseResult::Failed(error_kind)),
        }
    }
}

/// Abort the current rebase operation.
///
/// This cleans up the rebase state and returns the repository to its
/// pre-rebase condition.
///
/// # Arguments
///
/// * `executor` - Process executor for dependency injection
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if:
/// - No rebase is in progress
/// - The abort operation fails
///
pub fn abort_rebase(executor: &dyn crate::executor::ProcessExecutor) -> io::Result<()> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    abort_rebase_impl(&repo, executor)
}

/// Implementation of abort_rebase.
fn abort_rebase_impl(
    repo: &git2::Repository,
    executor: &dyn crate::executor::ProcessExecutor,
) -> io::Result<()> {
    // Check if a rebase is in progress
    let state = repo.state();
    if state != git2::RepositoryState::Rebase
        && state != git2::RepositoryState::RebaseMerge
        && state != git2::RepositoryState::RebaseInteractive
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "No rebase in progress",
        ));
    }

    // Use git CLI for abort via executor
    let output = executor.execute("git", &["rebase", "--abort"], &[], None)?;

    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Failed to abort rebase: {}",
            output.stderr
        )))
    }
}

/// Get a list of files that have merge conflicts.
///
/// This function queries libgit2's index to find all files that are
/// currently in a conflicted state.
///
/// # Returns
///
/// Returns `Ok(Vec<String>)` containing the paths of conflicted files,
/// or an error if the repository cannot be accessed.
///
pub fn get_conflicted_files() -> io::Result<Vec<String>> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    get_conflicted_files_impl(&repo)
}

/// Implementation of get_conflicted_files.
fn get_conflicted_files_impl(repo: &git2::Repository) -> io::Result<Vec<String>> {
    let index = repo.index().map_err(|e| git2_to_io_error(&e))?;

    let mut conflicted_files = Vec::new();

    // Check if there are any conflicts
    if !index.has_conflicts() {
        return Ok(conflicted_files);
    }

    // Get the list of conflicted files
    let conflicts = index.conflicts().map_err(|e| git2_to_io_error(&e))?;

    for conflict in conflicts {
        let conflict = conflict.map_err(|e| git2_to_io_error(&e))?;
        // The conflict's `our` entry (stage 2) will have the path
        if let Some(our_entry) = conflict.our {
            if let Ok(path) = std::str::from_utf8(&our_entry.path) {
                let path_str = path.to_string();
                if !conflicted_files.contains(&path_str) {
                    conflicted_files.push(path_str);
                }
            }
        }
    }

    Ok(conflicted_files)
}

/// Extract conflict markers from a file.
///
/// This function reads a file and returns the conflict sections,
/// including both versions of the conflicted content.
///
/// # Arguments
///
/// * `path` - Path to the conflicted file (relative to repo root)
///
/// # Returns
///
/// Returns `Ok(String)` containing the conflict sections, or an error
/// if the file cannot be read.
pub fn get_conflict_markers_for_file(path: &Path) -> io::Result<String> {
    use std::fs;
    use std::io::Read;

    let mut file = fs::File::open(path)?;
    let mut content = String::new();
    file.read_to_string(&mut content)?;

    // Extract conflict markers and their content
    let mut conflict_sections = Vec::new();
    let lines: Vec<&str> = content.lines().collect();
    let mut i = 0;

    while i < lines.len() {
        if lines[i].trim_start().starts_with("<<<<<<<") {
            // Found conflict start
            let mut section = Vec::new();
            section.push(lines[i]);

            i += 1;
            // Collect "ours" version
            while i < lines.len() && !lines[i].trim_start().starts_with("=======") {
                section.push(lines[i]);
                i += 1;
            }

            if i < lines.len() {
                section.push(lines[i]); // Add the ======= line
                i += 1;
            }

            // Collect "theirs" version
            while i < lines.len() && !lines[i].trim_start().starts_with(">>>>>>>") {
                section.push(lines[i]);
                i += 1;
            }

            if i < lines.len() {
                section.push(lines[i]); // Add the >>>>>>> line
                i += 1;
            }

            conflict_sections.push(section.join("\n"));
        } else {
            i += 1;
        }
    }

    if conflict_sections.is_empty() {
        // No conflict markers found, return empty string
        Ok(String::new())
    } else {
        Ok(conflict_sections.join("\n\n"))
    }
}

/// Verify that a rebase has completed successfully using LibGit2.
///
/// This function uses LibGit2 exclusively to verify that a rebase operation
/// has completed successfully. It checks:
/// - Repository state is clean (no rebase in progress)
/// - HEAD is valid and not detached (unless expected)
/// - Index has no conflicts
/// - Current branch is descendant of upstream (rebase succeeded)
///
/// # Returns
///
/// Returns `Ok(true)` if rebase is verified as complete, `Ok(false)` if
/// rebase is still in progress (conflicts remain), or an error if the
/// repository state is invalid.
///
/// # Note
///
/// This is the authoritative source for rebase completion verification.
/// It does NOT depend on parsing agent output or any other external signals.
#[cfg(any(test, feature = "test-utils"))]
pub fn verify_rebase_completed(upstream_branch: &str) -> io::Result<bool> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // 1. Check if a rebase is still in progress
    let state = repo.state();
    if state == git2::RepositoryState::Rebase
        || state == git2::RepositoryState::RebaseMerge
        || state == git2::RepositoryState::RebaseInteractive
    {
        // Rebase is still in progress
        return Ok(false);
    }

    // 2. Check if there are any remaining conflicts in the index
    let index = repo.index().map_err(|e| git2_to_io_error(&e))?;
    if index.has_conflicts() {
        // Conflicts remain in the index
        return Ok(false);
    }

    // 3. Verify HEAD is valid
    let head = repo.head().map_err(|e| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Repository HEAD is invalid: {e}"),
        )
    })?;

    // 4. Verify the current branch is a descendant of upstream
    // This confirms the rebase actually succeeded
    if let Ok(head_commit) = head.peel_to_commit() {
        if let Ok(upstream_object) = repo.revparse_single(upstream_branch) {
            if let Ok(upstream_commit) = upstream_object.peel_to_commit() {
                // Check if HEAD is now a descendant of upstream
                // This means the rebase moved our commits on top of upstream
                match repo.graph_descendant_of(head_commit.id(), upstream_commit.id()) {
                    Ok(is_descendant) => {
                        if is_descendant {
                            // HEAD is descendant of upstream - rebase successful
                            return Ok(true);
                        } else {
                            // HEAD is not a descendant - rebase not complete or not applicable
                            // (e.g., diverged branches, feature branch ahead of upstream)
                            return Ok(false);
                        }
                    }
                    Err(e) => {
                        // Can't determine descendant relationship - fall back to conflict check
                        let _ = e; // suppress unused warning
                    }
                }
            }
        }
    }

    // If we can't verify descendant relationship, check for conflicts
    // as a fallback - if no conflicts and no rebase in progress, consider it complete
    Ok(!index.has_conflicts())
}

/// Continue a rebase after conflict resolution.
///
/// This function continues a rebase that was paused due to conflicts.
/// It should be called after all conflicts have been resolved and
/// the resolved files have been staged with `git add`.
///
/// # Returns
///
/// Returns `Ok(())` if successful, or an error if:
/// - No rebase is in progress
/// - Conflicts remain unresolved
/// - The continue operation fails
///
/// **Note:** This function uses the current working directory to discover the repo.
pub fn continue_rebase(executor: &dyn crate::executor::ProcessExecutor) -> io::Result<()> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    continue_rebase_impl(&repo, executor)
}

/// Implementation of continue_rebase.
fn continue_rebase_impl(
    repo: &git2::Repository,
    executor: &dyn crate::executor::ProcessExecutor,
) -> io::Result<()> {
    // Check if a rebase is in progress
    let state = repo.state();
    if state != git2::RepositoryState::Rebase
        && state != git2::RepositoryState::RebaseMerge
        && state != git2::RepositoryState::RebaseInteractive
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "No rebase in progress",
        ));
    }

    // Check if there are still conflicts
    let conflicted = get_conflicted_files()?;
    if !conflicted.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "Cannot continue rebase: {} file(s) still have conflicts",
                conflicted.len()
            ),
        ));
    }

    // Use git CLI for continue via executor
    let output = executor.execute("git", &["rebase", "--continue"], &[], None)?;

    if output.status.success() {
        Ok(())
    } else {
        Err(io::Error::other(format!(
            "Failed to continue rebase: {}",
            output.stderr
        )))
    }
}

/// Check if a rebase is currently in progress.
///
/// This function checks the repository state to determine if a rebase
/// operation is in progress. This is useful for detecting interrupted
/// rebases that need to be resumed or aborted.
///
/// # Returns
///
/// Returns `Ok(true)` if a rebase is in progress, `Ok(false)` otherwise,
/// or an error if the repository cannot be accessed.
///
pub fn rebase_in_progress() -> io::Result<bool> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    rebase_in_progress_impl(&repo)
}

/// Implementation of rebase_in_progress.
fn rebase_in_progress_impl(repo: &git2::Repository) -> io::Result<bool> {
    let state = repo.state();
    Ok(state == git2::RepositoryState::Rebase
        || state == git2::RepositoryState::RebaseMerge
        || state == git2::RepositoryState::RebaseInteractive)
}
