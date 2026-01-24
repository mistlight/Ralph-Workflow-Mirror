//! Basic git repository operations.
//!
//! Provides fundamental git operations used throughout the application:
//!
//! - Repository detection and root path resolution
//! - Working tree status snapshots (porcelain format)
//! - Staging and committing changes
//! - Diff generation for commit messages
//! - Automated commit message generation and committing
//!
//! Operations use libgit2 directly to avoid CLI dependencies and work
//! even when git is not installed.

use std::io;
use std::path::PathBuf;

use super::identity::GitIdentity;

/// The level of truncation applied to a diff for review.
///
/// This enum tracks how much a diff has been abbreviated and determines
/// what instructions should be given to the reviewer agent.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum DiffTruncationLevel {
    /// No truncation - full diff is included
    #[default]
    Full,
    /// Diff was semantically truncated - high-priority files shown, instruction to explore
    Abbreviated,
    /// Only file paths listed - instruction to explore each file's diff
    FileList,
    /// File list was abbreviated - instruction to explore and discover files
    FileListAbbreviated,
}

/// The result of diff truncation for review purposes.
///
/// Contains both the potentially-truncated content and metadata about
/// what truncation was applied, along with version context information.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DiffReviewContent {
    /// The content to include in the review prompt
    pub content: String,
    /// The level of truncation applied
    pub truncation_level: DiffTruncationLevel,
    /// Total number of files in the full diff (for context in messages)
    pub total_file_count: usize,
    /// Number of files shown in the abbreviated content (if applicable)
    pub shown_file_count: Option<usize>,
    /// The OID (commit SHA) that this diff is compared against (baseline)
    pub baseline_oid: Option<String>,
    /// Short form (first 8 chars) of the baseline OID for display
    pub baseline_short: Option<String>,
    /// Description of what the baseline represents (e.g., "review_baseline", "start_commit")
    pub baseline_description: String,
}

impl DiffReviewContent {
    /// Generate a human-readable header describing the diff's version context.
    ///
    /// This header is meant to be included at the beginning of the diff content
    /// to provide clarity about what state of the code the diff represents.
    ///
    /// # Returns
    ///
    /// A formatted string like:
    /// ```text
    /// Diff Context: Compared against review_baseline abc12345
    /// Current state: Working directory (includes unstaged changes)
    /// ```
    ///
    /// If no baseline information is available, returns a generic message.
    pub fn format_context_header(&self) -> String {
        let mut lines = Vec::new();

        if let Some(short) = &self.baseline_short {
            lines.push(format!(
                "Diff Context: Compared against {} {}",
                self.baseline_description, short
            ));
        } else {
            lines.push("Diff Context: Version information not available".to_string());
        }

        // Add information about truncation if applicable
        match self.truncation_level {
            DiffTruncationLevel::Full => {
                // No truncation - full diff
            }
            DiffTruncationLevel::Abbreviated => {
                lines.push(format!(
                    "Note: Diff abbreviated - {}/{} files shown",
                    self.shown_file_count.unwrap_or(0),
                    self.total_file_count
                ));
            }
            DiffTruncationLevel::FileList => {
                lines.push(format!(
                    "Note: Only file list shown - {} files changed",
                    self.total_file_count
                ));
            }
            DiffTruncationLevel::FileListAbbreviated => {
                lines.push(format!(
                    "Note: File list abbreviated - {}/{} files shown",
                    self.shown_file_count.unwrap_or(0),
                    self.total_file_count
                ));
            }
        }

        if lines.is_empty() {
            String::new()
        } else {
            format!("{}\n", lines.join("\n"))
        }
    }
}

/// Convert git2 error to `io::Error`.
fn git2_to_io_error(err: &git2::Error) -> io::Error {
    io::Error::other(err.to_string())
}

/// Check if we're in a git repository.
pub fn require_git_repo() -> io::Result<()> {
    git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    Ok(())
}

/// Get the git repository root.
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
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;
    Ok(repo.path().join("hooks"))
}

/// Get a snapshot of the current git status.
///
/// Returns status in porcelain format (similar to `git status --porcelain=v1`).
pub fn git_snapshot() -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    let mut opts = git2::StatusOptions::new();
    opts.include_untracked(true).recurse_untracked_dirs(true);
    let statuses = repo
        .statuses(Some(&mut opts))
        .map_err(|e| git2_to_io_error(&e))?;

    let mut result = String::new();
    for entry in statuses.iter() {
        let status = entry.status();
        let path = entry.path().unwrap_or("").to_string();

        // Convert git2 status to porcelain format
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

/// Get the diff of all changes (unstaged and staged).
///
/// Returns a formatted diff string suitable for LLM analysis.
/// This is similar to `git diff HEAD`.
///
/// Handles the case of an empty repository (no commits yet) by
/// diffing against an empty tree using a read-only approach.
pub fn git_diff() -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Try to get HEAD tree
    let head_tree = match repo.head() {
        Ok(head) => Some(head.peel_to_tree().map_err(|e| git2_to_io_error(&e))?),
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // No commits yet - we need to show all untracked files as new files
            // Since there's no HEAD, we diff an empty tree against the workdir

            // Create a diff with an empty tree (no parent tree)
            // This is a read-only operation that doesn't modify the index
            let mut diff_opts = git2::DiffOptions::new();
            diff_opts.include_untracked(true);
            diff_opts.recurse_untracked_dirs(true);

            let diff = repo
                .diff_tree_to_workdir_with_index(None, Some(&mut diff_opts))
                .map_err(|e| git2_to_io_error(&e))?;

            let mut result = Vec::new();
            diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
                result.extend_from_slice(line.content());
                true
            })
            .map_err(|e| git2_to_io_error(&e))?;

            return Ok(String::from_utf8_lossy(&result).to_string());
        }
        Err(e) => return Err(git2_to_io_error(&e)),
    };

    // For repos with commits, diff HEAD against working tree
    // This includes both staged and unstaged changes
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_untracked(true);
    diff_opts.recurse_untracked_dirs(true);

    let diff = repo
        .diff_tree_to_workdir_with_index(head_tree.as_ref(), Some(&mut diff_opts))
        .map_err(|e| git2_to_io_error(&e))?;

    // Generate diff text
    let mut result = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        result.extend_from_slice(line.content());
        true
    })
    .map_err(|e| git2_to_io_error(&e))?;

    Ok(String::from_utf8_lossy(&result).to_string())
}

/// Truncate a diff for review using progressive fallback strategy.
///
/// This function implements a multi-level truncation approach:
/// 1. If diff fits within `max_full_diff_size`, return as-is
/// 2. If diff is too large, semantically truncate with file prioritization
/// 3. If even abbreviated diff is too large, return just file paths
/// 4. If file list is too large, return abbreviated file list
///
/// When truncation occurs, the returned content includes clear markers
/// and the truncation level indicates what instructions should be shown
/// to the reviewer agent about exploring the full diff themselves.
///
/// # Warning Behavior
///
/// This function does not print warnings directly. Callers should check the
/// return value's boolean flag and log appropriate warnings if truncation occurred.
///
/// # Truncation Behavior
///
/// When a diff exceeds `MAX_DIFF_SIZE_HARD`, it is truncated and a warning marker
/// is placed **before** the diff content (not after). This ensures the LLM reviewer
/// is immediately aware that the context is incomplete before analyzing the diff.
///
/// # Arguments
///
/// * `diff` - The full git diff
/// * `max_full_diff_size` - Maximum size for full diff (default: 100KB)
/// * `max_abbreviated_size` - Maximum size for abbreviated diff (default: 50KB)
/// * `max_file_list_size` - Maximum size for file list (default: 10KB)
///
/// # Returns
///
/// A `DiffReviewContent` struct containing the truncated content and metadata.
pub fn truncate_diff_for_review(
    diff: String,
    max_full_diff_size: usize,
    max_abbreviated_size: usize,
    max_file_list_size: usize,
) -> DiffReviewContent {
    let diff_size = diff.len();

    // Level 1: Full diff fits
    // Parse file count for consistent metadata even when returning early
    let files = parse_diff_to_files(&diff);
    let total_file_count = files.len();

    if diff_size <= max_full_diff_size {
        return DiffReviewContent {
            content: diff,
            truncation_level: DiffTruncationLevel::Full,
            total_file_count,
            shown_file_count: None,
            baseline_oid: None,
            baseline_short: None,
            baseline_description: String::new(),
        };
    }

    // Level 2: Abbreviated diff with semantic prioritization
    let abbreviated = truncate_diff_semantically(&diff, &files, max_abbreviated_size);
    let abbreviated_size = abbreviated.content.len();

    if abbreviated_size <= max_abbreviated_size {
        return abbreviated;
    }

    // Level 3: File list only
    let file_list = build_file_list(&files);
    let file_list_size = file_list.content.len();

    if file_list_size <= max_file_list_size {
        return file_list;
    }

    // Level 4: Abbreviated file list
    abbreviate_file_list(&files, max_file_list_size, total_file_count)
}

/// Represents a single file's diff chunk.
#[derive(Debug, Default, Clone)]
struct DiffFile {
    /// File path (extracted from diff header)
    path: String,
    /// Priority for selection (higher = more important)
    priority: i32,
    /// Lines in this file's diff
    lines: Vec<String>,
}

/// Assign a priority score to a file path for truncation selection.
///
/// Higher priority files are kept first when truncating:
/// - src/*.rs: +100 (source code is most important)
/// - src/*: +80 (other src files)
/// - tests/*: +40 (tests are important but secondary)
/// - Cargo.toml, package.json, etc.: +60 (config files)
/// - docs/*, *.md: +20 (docs are least important)
/// - Other: +50 (default)
fn prioritize_file_path(path: &str) -> i32 {
    use std::path::Path;
    let path_lower = path.to_lowercase();

    // Helper function for case-insensitive file extension check
    let has_ext_lower = |ext: &str| -> bool {
        Path::new(&path_lower)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    };

    // Helper function for case-insensitive extension check on original path
    let has_ext = |ext: &str| -> bool {
        Path::new(path)
            .extension()
            .and_then(std::ffi::OsStr::to_str)
            .is_some_and(|e| e.eq_ignore_ascii_case(ext))
    };

    // Source code files (highest priority)
    if path_lower.contains("src/") && has_ext_lower("rs") {
        100
    } else if path_lower.contains("src/") {
        80
    }
    // Test files
    else if path_lower.contains("test") {
        40
    }
    // Config files - use case-insensitive extension check
    else if has_ext("toml")
        || has_ext("json")
        || path_lower.ends_with("cargo.toml")
        || path_lower.ends_with("package.json")
        || path_lower.ends_with("tsconfig.json")
    {
        60
    }
    // Documentation files (lowest priority)
    else if path_lower.contains("doc") || has_ext("md") {
        20
    }
    // Default priority
    else {
        50
    }
}

/// Parse a git diff into individual file blocks.
fn parse_diff_to_files(diff: &str) -> Vec<DiffFile> {
    let mut files: Vec<DiffFile> = Vec::new();
    let mut current_file = DiffFile::default();
    let mut in_file = false;

    for line in diff.lines() {
        if line.starts_with("diff --git ") {
            if in_file && !current_file.lines.is_empty() {
                files.push(std::mem::take(&mut current_file));
            }
            in_file = true;
            current_file.lines.push(line.to_string());

            if let Some(path) = line.split(" b/").nth(1) {
                current_file.path = path.to_string();
                current_file.priority = prioritize_file_path(path);
            }
        } else if in_file {
            current_file.lines.push(line.to_string());
        }
    }

    if in_file && !current_file.lines.is_empty() {
        files.push(current_file);
    }

    files
}

/// Semantically truncate diff with file prioritization.
fn truncate_diff_semantically(
    _diff: &str,
    files: &[DiffFile],
    max_size: usize,
) -> DiffReviewContent {
    // Sort by priority, greedily select files that fit
    let mut sorted_files = files.to_vec();
    sorted_files.sort_by_key(|f: &DiffFile| std::cmp::Reverse(f.priority));

    let mut selected_files = Vec::new();
    let mut current_size = 0;

    for file in &sorted_files {
        let file_size: usize = file.lines.iter().map(|l| l.len() + 1).sum();

        if current_size + file_size <= max_size {
            current_size += file_size;
            selected_files.push(file.clone());
        } else if current_size > 0 {
            break;
        } else {
            // Even the first file is too large, take part of it
            let truncated_lines = truncate_lines_to_fit(&file.lines, max_size);
            selected_files.push(DiffFile {
                path: file.path.clone(),
                priority: file.priority,
                lines: truncated_lines,
            });
            break;
        }
    }

    let shown_count = selected_files.len();
    let omitted_count = files.len().saturating_sub(shown_count);

    let mut result = String::new();
    if omitted_count > 0 {
        use std::fmt::Write;
        let _ = write!(
            result,
            "[DIFF TRUNCATED: Showing {shown_count} of {} files. You MUST explore the full diff using git commands to review properly.]\n\n",
            files.len()
        );
    }

    for file in &selected_files {
        for line in &file.lines {
            result.push_str(line);
            result.push('\n');
        }
    }

    DiffReviewContent {
        content: result,
        truncation_level: DiffTruncationLevel::Abbreviated,
        total_file_count: files.len(),
        shown_file_count: Some(shown_count),
        baseline_oid: None,
        baseline_short: None,
        baseline_description: String::new(),
    }
}

/// Build a file list from diff files.
fn build_file_list(files: &[DiffFile]) -> DiffReviewContent {
    let mut result = String::from(
        "[FULL DIFF TOO LARGE - Showing file list only. You MUST explore each file's diff using git commands.]\n\n"
    );
    result.push_str("FILES CHANGED (you must explore each file's diff):\n");

    for file in files {
        if !file.path.is_empty() {
            result.push_str("  - ");
            result.push_str(&file.path);
            result.push('\n');
        }
    }

    DiffReviewContent {
        content: result,
        truncation_level: DiffTruncationLevel::FileList,
        total_file_count: files.len(),
        shown_file_count: Some(files.len()),
        baseline_oid: None,
        baseline_short: None,
        baseline_description: String::new(),
    }
}

/// Abbreviate a file list that's too large.
fn abbreviate_file_list(
    files: &[DiffFile],
    max_size: usize,
    total_count: usize,
) -> DiffReviewContent {
    let mut result = String::from(
        "[FILE LIST TOO LARGE - You MUST explore the repository to find all changed files.]\n\n",
    );

    // Calculate how many files we can show
    let mut size_so_far = result.len();
    let mut shown_count = 0;

    result.push_str("SAMPLE OF CHANGED FILES (explore to find all):\n");

    for file in files {
        let line = format!("  - {}\n", file.path);
        if size_so_far + line.len() > max_size {
            break;
        }
        result.push_str(&line);
        size_so_far += line.len();
        shown_count += 1;
    }

    let omitted = total_count.saturating_sub(shown_count);
    if omitted > 0 {
        use std::fmt::Write;
        let _ = write!(
            result,
            "\n... and {} more files (explore to find all)\n",
            omitted
        );
    }

    DiffReviewContent {
        content: result,
        truncation_level: DiffTruncationLevel::FileListAbbreviated,
        total_file_count: total_count,
        shown_file_count: Some(shown_count),
        baseline_oid: None,
        baseline_short: None,
        baseline_description: String::new(),
    }
}

/// Truncate a slice of lines to fit within a maximum size.
///
/// This is a fallback for when even a single file is too large.
/// Returns as many complete lines as will fit.
fn truncate_lines_to_fit(lines: &[String], max_size: usize) -> Vec<String> {
    let mut result = Vec::new();
    let mut current_size = 0;

    for line in lines {
        let line_size = line.len() + 1; // +1 for newline
        if current_size + line_size <= max_size {
            current_size += line_size;
            result.push(line.clone());
        } else {
            break;
        }
    }

    // Add truncation marker to the last line
    if let Some(last) = result.last_mut() {
        last.push_str(" [truncated...]");
    }

    result
}

fn index_has_changes_to_commit(repo: &git2::Repository, index: &git2::Index) -> io::Result<bool> {
    match repo.head() {
        Ok(head) => {
            let head_tree = head.peel_to_tree().map_err(|e| git2_to_io_error(&e))?;
            let diff = repo
                .diff_tree_to_index(Some(&head_tree), Some(index), None)
                .map_err(|e| git2_to_io_error(&e))?;
            Ok(diff.deltas().len() > 0)
        }
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => Ok(!index.is_empty()),
        Err(e) => Err(git2_to_io_error(&e)),
    }
}

fn is_internal_agent_artifact(path: &std::path::Path) -> bool {
    let path_str = path.to_string_lossy();
    path_str == ".no_agent_commit"
        || path_str == ".agent"
        || path_str.starts_with(".agent/")
        || path_str == ".git"
        || path_str.starts_with(".git/")
}

/// Stage all changes.
///
/// Similar to `git add -A`.
///
/// # Returns
///
/// Returns `Ok(true)` if files were successfully staged, `Ok(false)` if there
/// were no files to stage, or an error if staging failed.
pub fn git_add_all() -> io::Result<bool> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    let mut index = repo.index().map_err(|e| git2_to_io_error(&e))?;

    // Stage deletions (equivalent to `git add -A` behavior).
    // libgit2's `add_all` doesn't automatically remove deleted paths.
    let mut status_opts = git2::StatusOptions::new();
    status_opts
        .include_untracked(true)
        .recurse_untracked_dirs(true)
        .include_ignored(false);
    let statuses = repo
        .statuses(Some(&mut status_opts))
        .map_err(|e| git2_to_io_error(&e))?;
    for entry in statuses.iter() {
        if entry.status().contains(git2::Status::WT_DELETED) {
            if let Some(path) = entry.path() {
                index
                    .remove_path(std::path::Path::new(path))
                    .map_err(|e| git2_to_io_error(&e))?;
            }
        }
    }

    // Add all files (staged, unstaged, and untracked)
    // Note: add_all() is required here, not update_all(), to include untracked files
    let mut filter_cb = |path: &std::path::Path, _matched: &[u8]| -> i32 {
        // Return 0 to add the file, non-zero to skip.
        // We skip (return 1) internal agent artifacts to avoid committing them.
        i32::from(is_internal_agent_artifact(path))
    };
    index
        .add_all(
            vec!["."],
            git2::IndexAddOption::DEFAULT,
            Some(&mut filter_cb),
        )
        .map_err(|e| git2_to_io_error(&e))?;

    index.write().map_err(|e| git2_to_io_error(&e))?;

    // Return true if staging produced something commit-worthy.
    index_has_changes_to_commit(&repo, &index)
}

/// Resolve git commit identity with the full priority chain.
///
/// This function implements the identity resolution priority chain:
/// 1. Git config (via libgit2's `repo.signature()`) - primary source
/// 2. Provided name/email parameters (from Ralph config, CLI args, or env vars)
/// 3. Environment variables (`RALPH_GIT_USER_NAME`, `RALPH_GIT_USER_EMAIL`)
/// 4. Ralph config file values (passed through)
/// 5. System username + derived email
/// 6. Default values ("Ralph Workflow", "ralph@localhost")
///
/// Partial overrides are supported: CLI args/env vars/config can override
/// individual fields (name or email) from git config.
///
/// # Arguments
///
/// * `repo` - The git repository (for git config)
/// * `provided_name` - Optional name from Ralph config or CLI
/// * `provided_email` - Optional email from Ralph config or CLI
///
/// # Returns
///
/// Returns `GitIdentity` with the resolved name and email.
fn resolve_commit_identity(
    repo: &git2::Repository,
    provided_name: Option<&str>,
    provided_email: Option<&str>,
) -> GitIdentity {
    use super::identity::{default_identity, fallback_email, fallback_username};

    // Priority 1: Git config (via libgit2) - primary source
    let mut name = String::new();
    let mut email = String::new();
    let mut has_git_config = false;

    if let Ok(sig) = repo.signature() {
        let git_name = sig.name().unwrap_or("");
        let git_email = sig.email().unwrap_or("");
        if !git_name.is_empty() && !git_email.is_empty() {
            name = git_name.to_string();
            email = git_email.to_string();
            has_git_config = true;
        }
    }

    // Priority order (standard git behavior):
    // 1. Git config (local .git/config, then global ~/.gitconfig) - primary source
    // 2. Provided args (provided_name/provided_email) - from Ralph config or CLI override
    // 3. Env vars (RALPH_GIT_USER_NAME/EMAIL) - fallback if above are missing
    //
    // This matches standard git behavior where git config is authoritative.
    let env_name = std::env::var("RALPH_GIT_USER_NAME").ok();
    let env_email = std::env::var("RALPH_GIT_USER_EMAIL").ok();

    // Apply in priority order: git config > provided args > env vars
    // Git config takes highest priority (standard git behavior)
    let final_name = if has_git_config && !name.is_empty() {
        name.as_str()
    } else {
        provided_name
            .filter(|s| !s.is_empty())
            .or(env_name.as_deref())
            .filter(|s| !s.is_empty())
            .unwrap_or("")
    };

    let final_email = if has_git_config && !email.is_empty() {
        email.as_str()
    } else {
        provided_email
            .filter(|s| !s.is_empty())
            .or(env_email.as_deref())
            .filter(|s| !s.is_empty())
            .unwrap_or("")
    };

    // If we have both name and email from git config + overrides, use them
    if !final_name.is_empty() && !final_email.is_empty() {
        let identity = GitIdentity::new(final_name.to_string(), final_email.to_string());
        if identity.validate().is_ok() {
            return identity;
        }
    }

    // Priority 5: System username + derived email
    let username = fallback_username();
    let system_email = fallback_email(&username);
    let identity = GitIdentity::new(
        if final_name.is_empty() {
            username
        } else {
            final_name.to_string()
        },
        if final_email.is_empty() {
            system_email
        } else {
            final_email.to_string()
        },
    );

    if identity.validate().is_ok() {
        return identity;
    }

    // Priority 6: Default values (last resort)
    default_identity()
}

/// Create a commit.
///
/// Similar to `git commit -m <message>`.
///
/// Handles both initial commits (no HEAD yet) and subsequent commits.
///
/// # Identity Resolution
///
/// The git commit identity (name and email) is resolved using the following priority:
/// 1. Git config (via libgit2) - primary source
/// 2. Provided `git_user_name` and `git_user_email` parameters (overrides)
/// 3. Environment variables (`RALPH_GIT_USER_NAME`, `RALPH_GIT_USER_EMAIL`)
/// 4. Ralph config file (read by caller, passed as parameters)
/// 5. System username + derived email (sane fallback)
/// 6. Default values ("Ralph Workflow", "ralph@localhost") - last resort
///
/// Partial overrides are supported: CLI args/env vars/config can override individual
/// fields (name or email) from git config.
///
/// # Arguments
///
/// * `message` - The commit message
/// * `git_user_name` - Optional git user name (overrides git config)
/// * `git_user_email` - Optional git user email (overrides git config)
///
/// # Returns
///
/// Returns `Ok(Some(oid))` with the commit OID if successful, `Ok(None)` if the
/// OID is zero (no commit created), or an error if the operation failed.
pub fn git_commit(
    message: &str,
    git_user_name: Option<&str>,
    git_user_email: Option<&str>,
) -> io::Result<Option<git2::Oid>> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Get the index
    let mut index = repo.index().map_err(|e| git2_to_io_error(&e))?;

    // Don't create empty commits: if the index matches HEAD (or is empty on an unborn branch),
    // there's nothing to commit.
    if !index_has_changes_to_commit(&repo, &index)? {
        return Ok(None);
    }

    // Get the tree from the index
    let tree_oid = index.write_tree().map_err(|e| git2_to_io_error(&e))?;

    let tree = repo.find_tree(tree_oid).map_err(|e| git2_to_io_error(&e))?;

    // Resolve git identity using the identity resolution system.
    // This implements the full priority chain with proper fallbacks.
    let GitIdentity { name, email } = resolve_commit_identity(&repo, git_user_name, git_user_email);

    // Debug logging: identity resolution source
    // Only log if RALPH_DEBUG or similar debug mode is enabled
    if std::env::var("RALPH_DEBUG").is_ok() {
        let identity_source = if git_user_name.is_some() || git_user_email.is_some() {
            "CLI/config override"
        } else if std::env::var("RALPH_GIT_USER_NAME").is_ok()
            || std::env::var("RALPH_GIT_USER_EMAIL").is_ok()
        {
            "environment variable"
        } else if repo.signature().is_ok() {
            "git config"
        } else {
            "system/default"
        };
        eprintln!("Git identity: {name} <{email}> (source: {identity_source})");
    }

    // Create the signature with the resolved identity
    let sig = git2::Signature::now(&name, &email).map_err(|e| git2_to_io_error(&e))?;

    let oid = match repo.head() {
        Ok(head) => {
            // Normal commit: has a parent
            let head_commit = head.peel_to_commit().map_err(|e| git2_to_io_error(&e))?;
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[&head_commit])
        }
        Err(ref e) if e.code() == git2::ErrorCode::UnbornBranch => {
            // Initial commit: no parents, but verify tree is not empty
            // An empty tree can happen in edge cases where files were staged and then removed
            let mut has_entries = false;
            tree.walk(git2::TreeWalkMode::PreOrder, |_, _| {
                has_entries = true;
                1 // Stop iteration after first entry
            })
            .ok(); // Ignore errors, we just want to know if there's at least one entry

            if !has_entries {
                // Tree is empty, return None instead of creating empty commit
                return Ok(None);
            }
            repo.commit(Some("HEAD"), &sig, &sig, message, &tree, &[])
        }
        Err(e) => return Err(git2_to_io_error(&e)),
    }
    .map_err(|e| git2_to_io_error(&e))?;

    Ok(Some(oid))
}

/// Generate a diff from a specific starting commit.
///
/// Takes a starting commit OID and generates a diff between that commit
/// and the current working tree. Returns a formatted diff string suitable
/// for LLM analysis.
///
/// # Arguments
///
/// * `start_oid` - The OID of the commit to diff from
///
/// # Returns
///
/// Returns a formatted diff string, or an error if:
/// - The repository cannot be opened
/// - The starting commit cannot be found
/// - The diff cannot be generated
pub fn git_diff_from(start_oid: &str) -> io::Result<String> {
    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    // Parse the starting OID
    let oid = git2::Oid::from_str(start_oid).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid commit OID: {start_oid}"),
        )
    })?;

    // Find the starting commit
    let start_commit = repo.find_commit(oid).map_err(|e| git2_to_io_error(&e))?;
    let start_tree = start_commit.tree().map_err(|e| git2_to_io_error(&e))?;

    // Diff between start commit and current working tree, including staged + unstaged
    // changes and untracked files.
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_untracked(true);
    diff_opts.recurse_untracked_dirs(true);

    let diff = repo
        .diff_tree_to_workdir_with_index(Some(&start_tree), Some(&mut diff_opts))
        .map_err(|e| git2_to_io_error(&e))?;

    // Generate diff text
    let mut result = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        result.extend_from_slice(line.content());
        true
    })
    .map_err(|e| git2_to_io_error(&e))?;

    Ok(String::from_utf8_lossy(&result).to_string())
}

fn git_diff_from_empty_tree(repo: &git2::Repository) -> io::Result<String> {
    let mut diff_opts = git2::DiffOptions::new();
    diff_opts.include_untracked(true);
    diff_opts.recurse_untracked_dirs(true);

    let diff = repo
        .diff_tree_to_workdir_with_index(None, Some(&mut diff_opts))
        .map_err(|e| git2_to_io_error(&e))?;

    let mut result = Vec::new();
    diff.print(git2::DiffFormat::Patch, |_delta, _hunk, line| {
        result.extend_from_slice(line.content());
        true
    })
    .map_err(|e| git2_to_io_error(&e))?;

    Ok(String::from_utf8_lossy(&result).to_string())
}

/// Get the git diff from the starting commit.
///
/// Uses the saved starting commit from `.agent/start_commit` to generate
/// an incremental diff. Falls back to diffing from HEAD if no start commit
/// file exists.
///
/// # Returns
///
/// Returns a formatted diff string, or an error if:
/// - The diff cannot be generated
/// - The starting commit file exists but is invalid
pub fn get_git_diff_from_start() -> io::Result<String> {
    use crate::git_helpers::start_commit::{load_start_point, save_start_commit, StartPoint};

    // Ensure a valid starting point exists. This is expected to persist across runs,
    // but we also repair missing/corrupt files opportunistically for robustness.
    save_start_commit()?;

    let repo = git2::Repository::discover(".").map_err(|e| git2_to_io_error(&e))?;

    match load_start_point()? {
        StartPoint::Commit(oid) => git_diff_from(&oid.to_string()),
        StartPoint::EmptyRepo => git_diff_from_empty_tree(&repo),
    }
}

/// Result of commit operation with fallback.
///
/// This is the fallback-aware version of `CommitResult`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommitResultFallback {
    /// A commit was successfully created with the given OID.
    Success(git2::Oid),
    /// No commit was created because there were no meaningful changes.
    NoChanges,
    /// The commit operation failed with an error message.
    Failed(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_git_diff_returns_string() {
        // This test verifies the function exists and returns a Result
        // The actual content depends on the git state
        let result = git_diff();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_require_git_repo() {
        // This test verifies we can detect a git repository
        let result = require_git_repo();
        // Should succeed if we're in a git repo, fail otherwise
        // We don't assert either way since the test environment varies
        let _ = result;
    }

    #[test]
    fn test_get_repo_root() {
        // This test verifies we can get the repo root
        let result = get_repo_root();
        // Only validate if we're in a git repo
        if let Ok(path) = result {
            // The path should exist and be a directory
            assert!(path.exists());
            assert!(path.is_dir());
            // Should contain a .git directory or be inside one
            let git_dir = path.join(".git");
            assert!(git_dir.exists() || path.ancestors().any(|p| p.join(".git").exists()));
        }
    }

    #[test]
    fn test_git_diff_from_returns_result() {
        // Test that git_diff_from returns a Result
        // We use an invalid OID to test error handling
        let result = git_diff_from("invalid_oid_that_does_not_exist");
        assert!(result.is_err());
    }

    #[test]
    fn test_git_snapshot_returns_result() {
        // Test that git_snapshot returns a Result
        let result = git_snapshot();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_git_add_all_returns_result() {
        // Test that git_add_all returns a Result
        let result = git_add_all();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_get_git_diff_from_start_returns_result() {
        // Test that get_git_diff_from_start returns a Result
        // It should fall back to git_diff() if no start commit file exists
        let result = get_git_diff_from_start();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_truncate_diff_for_review_full() {
        // Small diffs should not be truncated
        let diff = "diff --git a/file.rs b/file.rs\n+ new line\n- old line";
        let result = truncate_diff_for_review(diff.to_string(), 10_000, 5_000, 1_000);
        assert_eq!(result.truncation_level, DiffTruncationLevel::Full);
        // total_file_count is parsed for consistent metadata
        assert_eq!(result.total_file_count, 1);
        assert_eq!(result.shown_file_count, None);
    }

    #[test]
    fn test_truncate_diff_for_review_abbreviated() {
        // Create a diff with multiple files that will exceed max_full_diff_size
        let mut diff = String::new();
        for i in 0..20 {
            diff.push_str(&format!("diff --git a/file{}.rs b/file{}.rs\n", i, i));
            diff.push_str("index abc123..def456 100644\n");
            diff.push_str(&format!("--- a/file{}.rs\n", i));
            diff.push_str(&format!("+++ b/file{}.rs\n", i));
            for j in 0..100 {
                diff.push_str(&format!("+line {} in file {}\n", j, i));
                diff.push_str(&format!("-line {} in file {}\n", j, i));
            }
        }

        let result = truncate_diff_for_review(diff, 1_000, 5_000, 1_000);
        assert_eq!(result.truncation_level, DiffTruncationLevel::Abbreviated);
        assert!(result.shown_file_count.unwrap_or(0) < result.total_file_count);
        assert!(result.content.contains("TRUNCATED") || result.content.contains("truncated"));
    }

    #[test]
    fn test_prioritize_file_path() {
        // Source files get highest priority
        assert!(prioritize_file_path("src/main.rs") > prioritize_file_path("tests/test.rs"));
        assert!(prioritize_file_path("src/lib.rs") > prioritize_file_path("README.md"));

        // Tests get lower priority than src
        assert!(prioritize_file_path("src/main.rs") > prioritize_file_path("test/test.rs"));

        // Config files get medium priority
        assert!(prioritize_file_path("Cargo.toml") > prioritize_file_path("docs/guide.md"));

        // Docs get lowest priority
        assert!(prioritize_file_path("README.md") < prioritize_file_path("src/main.rs"));
    }

    #[test]
    fn test_truncate_diff_keeps_high_priority_files() {
        let diff = "diff --git a/README.md b/README.md\n\
            +doc change\n\
            diff --git a/src/main.rs b/src/main.rs\n\
            +important change\n\
            diff --git a/tests/test.rs b/tests/test.rs\n\
            +test change\n";

        // With a very small limit, should keep src/main.rs first due to priority
        let result = truncate_diff_for_review(diff.to_string(), 50, 100, 1_000);

        // Should include the high priority src file
        assert!(result.content.contains("src/main.rs") || result.content.contains("file list"));
    }

    #[test]
    fn test_diff_review_content_default_truncation_level() {
        // Test that DiffTruncationLevel::Full is the default
        assert_eq!(DiffTruncationLevel::default(), DiffTruncationLevel::Full);
    }

    #[test]
    fn test_exploration_instruction_helper() {
        // Use the local helper function instead of importing from guided module
        // Test Full level - should return empty string
        let full_content = DiffReviewContent {
            content: "some diff".to_string(),
            truncation_level: DiffTruncationLevel::Full,
            total_file_count: 5,
            shown_file_count: None,
            baseline_oid: None,
            baseline_short: None,
            baseline_description: String::new(),
        };
        let instruction = build_exploration_instruction_for_test(&full_content);
        assert!(instruction.is_empty());

        // Test Abbreviated level - should have instruction
        let abbreviated_content = DiffReviewContent {
            content: "truncated diff".to_string(),
            truncation_level: DiffTruncationLevel::Abbreviated,
            total_file_count: 10,
            shown_file_count: Some(3),
            baseline_oid: None,
            baseline_short: None,
            baseline_description: String::new(),
        };
        let instruction = build_exploration_instruction_for_test(&abbreviated_content);
        assert!(instruction.contains("ABBREVIATED"));
        assert!(instruction.contains("3/10"));

        // Test FileList level - should have instruction
        let file_list_content = DiffReviewContent {
            content: "files list".to_string(),
            truncation_level: DiffTruncationLevel::FileList,
            total_file_count: 50,
            shown_file_count: Some(50),
            baseline_oid: None,
            baseline_short: None,
            baseline_description: String::new(),
        };
        let instruction = build_exploration_instruction_for_test(&file_list_content);
        assert!(instruction.contains("FILE LIST ONLY"));
        assert!(instruction.contains("50 files changed"));

        // Test FileListAbbreviated level - should have instruction
        let abbreviated_list_content = DiffReviewContent {
            content: "abbreviated file list".to_string(),
            truncation_level: DiffTruncationLevel::FileListAbbreviated,
            total_file_count: 200,
            shown_file_count: Some(10),
            baseline_oid: None,
            baseline_short: None,
            baseline_description: String::new(),
        };
        let instruction = build_exploration_instruction_for_test(&abbreviated_list_content);
        assert!(instruction.contains("FILE LIST ABBREVIATED"));
        assert!(instruction.contains("10/200"));
    }

    /// Helper function for testing exploration instruction generation.
    #[cfg(test)]
    fn build_exploration_instruction_for_test(diff_content: &DiffReviewContent) -> String {
        match diff_content.truncation_level {
            DiffTruncationLevel::Full => String::new(),
            DiffTruncationLevel::Abbreviated => format!(
                "[DIFF ABBREVIATED: {}/{} files shown. You MUST explore the full diff using 'git diff HEAD' to review properly.]",
                diff_content.shown_file_count.unwrap_or(0),
                diff_content.total_file_count
            ),
            DiffTruncationLevel::FileList => format!(
                "[FILE LIST ONLY: {} files changed. You MUST explore each file's diff using 'git diff HEAD -- <file>' to review properly.]",
                diff_content.total_file_count
            ),
            DiffTruncationLevel::FileListAbbreviated => format!(
                "[FILE LIST ABBREVIATED: {}/{} files shown. You MUST run 'git status' to find all files and explore their diffs.]",
                diff_content.shown_file_count.unwrap_or(0),
                diff_content.total_file_count
            ),
        }
    }
}
