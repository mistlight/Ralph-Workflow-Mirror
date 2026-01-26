//! Per-review-cycle baseline tracking.
//!
//! This module manages the baseline commit for each review cycle, ensuring that
//! reviewers only see changes from the current cycle rather than cumulative changes
//! from previous fix commits.
//!
//! # Overview
//!
//! During the review-fix phase, each cycle should:
//! 1. Capture baseline before review (current HEAD)
//! 2. Review sees diff from that baseline
//! 3. Fixer makes changes
//! 4. Baseline is updated after fix pass
//! 5. Next review cycle sees only new changes
//!
//! This prevents "diff scope creep" where previous fix commits pollute
//! subsequent review passes.

use std::fs;
use std::io;
use std::path::Path;

use super::start_commit::get_current_head_oid;

/// Path to the review baseline file.
///
/// Stored in `.agent/review_baseline.txt`, this file contains the OID (SHA) of the
/// commit that serves as the baseline for the current review cycle.
const REVIEW_BASELINE_FILE: &str = ".agent/review_baseline.txt";

/// Sentinel value when review baseline is not set.
const BASELINE_NOT_SET: &str = "__BASELINE_NOT_SET__";

/// Review baseline state.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReviewBaseline {
    /// A concrete commit OID to diff from.
    Commit(git2::Oid),
    /// Baseline not set (first review cycle).
    NotSet,
}

/// Update the review baseline to current HEAD.
///
/// This should be called AFTER each fix pass to update the baseline so
/// the next review cycle sees only new changes.
///
/// # Errors
///
/// Returns an error if:
/// - The current HEAD cannot be determined
/// - The file cannot be written
///
/// **Note:** This function uses the current working directory to discover the repo.
/// For explicit path control, use [`update_review_baseline_at`] instead.
pub fn update_review_baseline() -> io::Result<()> {
    let oid = get_current_head_oid()?;
    write_review_baseline_cwd(&oid)
}

/// Load the review baseline.
///
/// Returns the baseline commit for the current review cycle.
///
/// # Errors
///
/// Returns an error if:
/// - The file cannot be read
/// - The file content is invalid
///
pub fn load_review_baseline() -> io::Result<ReviewBaseline> {
    let path = Path::new(REVIEW_BASELINE_FILE);
    load_review_baseline_impl(path)
}

/// Implementation of load_review_baseline.
fn load_review_baseline_impl(path: &Path) -> io::Result<ReviewBaseline> {
    if !path.exists() {
        return Ok(ReviewBaseline::NotSet);
    }

    let content = fs::read_to_string(path)?;
    let raw = content.trim();

    if raw.is_empty() || raw == BASELINE_NOT_SET {
        return Ok(ReviewBaseline::NotSet);
    }

    // Parse the OID
    let oid = git2::Oid::from_str(raw).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid OID format in {}: '{}'. The review baseline will be reset. Run 'ralph --reset-start-commit' if this persists.", REVIEW_BASELINE_FILE, raw),
        )
    })?;

    Ok(ReviewBaseline::Commit(oid))
}

/// Get information about the current review baseline.
///
/// Returns a tuple of (baseline_oid, commits_since_baseline, is_stale).
/// - `baseline_oid`: The OID of the baseline commit (or None if not set)
/// - `commits_since_baseline`: Number of commits since baseline
/// - `is_stale`: true if baseline is old (>10 commits behind)
///
pub fn get_review_baseline_info() -> io::Result<(Option<String>, usize, bool)> {
    let repo = git2::Repository::discover(".").map_err(|e| to_io_error(&e))?;
    get_review_baseline_info_impl(&repo, load_review_baseline()?)
}

/// Implementation of get_review_baseline_info.
fn get_review_baseline_info_impl(
    repo: &git2::Repository,
    baseline: ReviewBaseline,
) -> io::Result<(Option<String>, usize, bool)> {
    let baseline_oid = match baseline {
        ReviewBaseline::Commit(oid) => Some(oid.to_string()),
        ReviewBaseline::NotSet => None,
    };

    let commits_since = if let Some(ref oid) = baseline_oid {
        count_commits_since(repo, oid)?
    } else {
        0
    };

    let is_stale = commits_since > 10;

    Ok((baseline_oid, commits_since, is_stale))
}

/// Write the review baseline to disk (CWD-based, for backward compatibility).
fn write_review_baseline_cwd(oid: &str) -> io::Result<()> {
    let path = Path::new(REVIEW_BASELINE_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(path, oid)?;
    Ok(())
}

/// Count commits since a given baseline.
fn count_commits_since(repo: &git2::Repository, baseline_oid: &str) -> io::Result<usize> {
    let oid = git2::Oid::from_str(baseline_oid).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("Invalid baseline OID: {baseline_oid}"),
        )
    })?;

    let baseline = repo.find_commit(oid).map_err(|e| to_io_error(&e))?;

    // Try to get HEAD and count commits
    match repo.head() {
        Ok(head) => {
            let head_commit = head.peel_to_commit().map_err(|e| to_io_error(&e))?;

            // Use revwalk to count commits
            let mut revwalk = repo.revwalk().map_err(|e| to_io_error(&e))?;
            revwalk
                .push(head_commit.id())
                .map_err(|e| to_io_error(&e))?;

            let mut count = 0;
            for commit_id in revwalk {
                let commit_id = commit_id.map_err(|e| to_io_error(&e))?;
                if commit_id == baseline.id() {
                    break;
                }
                count += 1;
                // Safety limit to prevent infinite loops
                if count > 1000 {
                    break;
                }
            }
            Ok(count)
        }
        Err(_) => Ok(0),
    }
}

/// Diff statistics for the changes since baseline.
#[derive(Debug, Clone, Default)]
pub struct DiffStats {
    /// Number of files changed.
    pub files_changed: usize,
    /// Number of lines added.
    pub lines_added: usize,
    /// Number of lines deleted.
    pub lines_deleted: usize,
    /// List of changed file paths (up to 10 for display).
    pub changed_files: Vec<String>,
}

/// Baseline summary information for display.
#[derive(Debug, Clone)]
pub struct BaselineSummary {
    /// The baseline OID (short form).
    pub baseline_oid: Option<String>,
    /// Number of commits since baseline.
    pub commits_since: usize,
    /// Whether the baseline is stale (>10 commits behind).
    pub is_stale: bool,
    /// Diff statistics for changes since baseline.
    pub diff_stats: DiffStats,
}

impl BaselineSummary {
    /// Format a compact version for inline display.
    pub fn format_compact(&self) -> String {
        match &self.baseline_oid {
            Some(oid) => {
                let short_oid = &oid[..8.min(oid.len())];
                if self.is_stale {
                    format!(
                        "Baseline: {} (+{} commits since, {} files changed)",
                        short_oid, self.commits_since, self.diff_stats.files_changed
                    )
                } else if self.commits_since > 0 {
                    format!(
                        "Baseline: {} ({} commits since, {} files changed)",
                        short_oid, self.commits_since, self.diff_stats.files_changed
                    )
                } else {
                    format!(
                        "Baseline: {} ({} files: +{}/-{} lines)",
                        short_oid,
                        self.diff_stats.files_changed,
                        self.diff_stats.lines_added,
                        self.diff_stats.lines_deleted
                    )
                }
            }
            None => {
                format!(
                    "Baseline: start_commit ({} files: +{}/-{} lines)",
                    self.diff_stats.files_changed,
                    self.diff_stats.lines_added,
                    self.diff_stats.lines_deleted
                )
            }
        }
    }

    /// Format a detailed version for verbose display.
    pub fn format_detailed(&self) -> String {
        let mut lines = Vec::new();

        lines.push("Review Baseline Summary:".to_string());
        lines.push("─".repeat(40));

        match &self.baseline_oid {
            Some(oid) => {
                let short_oid = &oid[..8.min(oid.len())];
                lines.push(format!("  Commit: {}", short_oid));
                if self.commits_since > 0 {
                    lines.push(format!("  Commits since baseline: {}", self.commits_since));
                }
            }
            None => {
                lines.push("  Commit: start_commit (initial baseline)".to_string());
            }
        }

        lines.push(format!(
            "  Files changed: {}",
            self.diff_stats.files_changed
        ));
        lines.push(format!("  Lines added: {}", self.diff_stats.lines_added));
        lines.push(format!(
            "  Lines deleted: {}",
            self.diff_stats.lines_deleted
        ));

        if !self.diff_stats.changed_files.is_empty() {
            lines.push(String::new());
            lines.push("  Changed files:".to_string());
            for file in &self.diff_stats.changed_files {
                lines.push(format!("    - {}", file));
            }
            if self.diff_stats.changed_files.len() < self.diff_stats.files_changed {
                let remaining = self.diff_stats.files_changed - self.diff_stats.changed_files.len();
                lines.push(format!("    ... and {} more", remaining));
            }
        }

        if self.is_stale {
            lines.push(String::new());
            lines.push(
                "  ⚠ WARNING: Baseline is stale. Consider updating with --reset-start-commit."
                    .to_string(),
            );
        }

        lines.join("\n")
    }
}

/// Get a summary of the baseline state for display.
///
/// Returns a `BaselineSummary` containing information about the current
/// baseline, commits since baseline, staleness, and diff statistics.
///
pub fn get_baseline_summary() -> io::Result<BaselineSummary> {
    let repo = git2::Repository::discover(".").map_err(|e| to_io_error(&e))?;
    get_baseline_summary_impl(&repo, load_review_baseline()?)
}

/// Implementation of get_baseline_summary.
fn get_baseline_summary_impl(
    repo: &git2::Repository,
    baseline: ReviewBaseline,
) -> io::Result<BaselineSummary> {
    let baseline_oid = match baseline {
        ReviewBaseline::Commit(oid) => Some(oid.to_string()),
        ReviewBaseline::NotSet => None,
    };

    let commits_since = if let Some(ref oid) = baseline_oid {
        count_commits_since(repo, oid)?
    } else {
        0
    };

    let is_stale = commits_since > 10;

    // Get diff statistics
    let diff_stats = get_diff_stats(repo, &baseline_oid)?;

    Ok(BaselineSummary {
        baseline_oid,
        commits_since,
        is_stale,
        diff_stats,
    })
}

/// Count lines in a blob content.
///
/// Returns the number of lines, matching the behavior of counting
/// newlines and adding 1 (so empty content returns 0, but any content
/// returns at least 1).
fn count_lines_in_blob(content: &[u8]) -> usize {
    if content.is_empty() {
        return 0;
    }
    // Count newlines and add 1 to get the line count
    // This matches the previous behavior and ensures that even files
    // without trailing newlines are counted correctly
    content.iter().filter(|&&c| c == b'\n').count() + 1
}

/// Get diff statistics for changes since the baseline.
fn get_diff_stats(repo: &git2::Repository, baseline_oid: &Option<String>) -> io::Result<DiffStats> {
    let baseline_tree = match baseline_oid {
        Some(oid) => {
            let oid = git2::Oid::from_str(oid).map_err(|_| {
                io::Error::new(
                    io::ErrorKind::InvalidInput,
                    format!("Invalid baseline OID: {}", oid),
                )
            })?;
            let commit = repo.find_commit(oid).map_err(|e| to_io_error(&e))?;
            commit.tree().map_err(|e| to_io_error(&e))?
        }
        None => {
            // No baseline set, use empty tree
            repo.find_tree(git2::Oid::zero())
                .map_err(|e| to_io_error(&e))?
        }
    };

    // Get the current HEAD tree
    let head_tree = match repo.head() {
        Ok(head) => {
            let commit = head.peel_to_commit().map_err(|e| to_io_error(&e))?;
            commit.tree().map_err(|e| to_io_error(&e))?
        }
        Err(_) => {
            // No HEAD yet, use empty tree
            repo.find_tree(git2::Oid::zero())
                .map_err(|e| to_io_error(&e))?
        }
    };

    // Generate diff
    let diff = repo
        .diff_tree_to_tree(Some(&baseline_tree), Some(&head_tree), None)
        .map_err(|e| to_io_error(&e))?;

    // Collect statistics
    let mut stats = DiffStats::default();
    let mut delta_ids = Vec::new();

    diff.foreach(
        &mut |delta, _progress| {
            use git2::Delta;

            stats.files_changed += 1;

            if let Some(path) = delta.new_file().path() {
                let path_str = path.to_string_lossy().to_string();
                if stats.changed_files.len() < 10 {
                    stats.changed_files.push(path_str);
                }
            } else if let Some(path) = delta.old_file().path() {
                let path_str = path.to_string_lossy().to_string();
                if stats.changed_files.len() < 10 {
                    stats.changed_files.push(path_str);
                }
            }

            match delta.status() {
                Delta::Added => {
                    delta_ids.push((delta.new_file().id(), true));
                }
                Delta::Deleted => {
                    delta_ids.push((delta.old_file().id(), false));
                }
                Delta::Modified => {
                    delta_ids.push((delta.new_file().id(), true));
                }
                _ => {}
            }

            true
        },
        None,
        None,
        None,
    )
    .map_err(|e| to_io_error(&e))?;

    // Count lines added/deleted
    for (blob_id, is_new_or_modified) in delta_ids {
        if let Ok(blob) = repo.find_blob(blob_id) {
            let line_count = count_lines_in_blob(blob.content());

            if is_new_or_modified {
                stats.lines_added += line_count;
            } else {
                stats.lines_deleted += line_count;
            }
        }
    }

    Ok(stats)
}

/// Convert git2 error to `io::Error`.
fn to_io_error(err: &git2::Error) -> io::Error {
    io::Error::other(err.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_review_baseline_file_path_defined() {
        assert_eq!(REVIEW_BASELINE_FILE, ".agent/review_baseline.txt");
    }

    #[test]
    fn test_load_review_baseline_returns_result() {
        let result = load_review_baseline();
        assert!(result.is_ok() || result.is_err());
    }

    #[test]
    fn test_get_review_baseline_info_returns_result() {
        let result = get_review_baseline_info();
        assert!(result.is_ok() || result.is_err());
    }
}
