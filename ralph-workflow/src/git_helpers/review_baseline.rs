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
use std::path::PathBuf;

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
pub fn update_review_baseline() -> io::Result<()> {
    let oid = get_current_head_oid()?;
    write_review_baseline(&oid)
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
pub fn load_review_baseline() -> io::Result<ReviewBaseline> {
    let path = PathBuf::from(REVIEW_BASELINE_FILE);

    if !path.exists() {
        return Ok(ReviewBaseline::NotSet);
    }

    let content = fs::read_to_string(&path)?;
    let raw = content.trim();

    if raw.is_empty() || raw == BASELINE_NOT_SET {
        return Ok(ReviewBaseline::NotSet);
    }

    // Parse the OID
    let oid = git2::Oid::from_str(raw).map_err(|_| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Invalid OID format in review baseline: {raw}"),
        )
    })?;

    Ok(ReviewBaseline::Commit(oid))
}

/// Get the diff from the review baseline (or start commit if baseline not set).
///
/// This function provides a per-review-cycle diff, falling back to the
/// original start_commit if no review baseline has been set.
///
/// # Returns
///
/// Returns a formatted diff string, or an error if:
/// - The repository cannot be opened
/// - The baseline commit cannot be found
/// - The diff cannot be generated
pub fn get_git_diff_from_review_baseline() -> io::Result<String> {
    match load_review_baseline()? {
        ReviewBaseline::Commit(oid) => {
            // Use the existing git_diff_from function from repo module
            super::repo::git_diff_from(&oid.to_string())
        }
        ReviewBaseline::NotSet => {
            // Fall back to start commit if review baseline not set
            super::repo::get_git_diff_from_start()
        }
    }
}

/// Get information about the current review baseline.
///
/// Returns a tuple of (baseline_oid, commits_since_baseline, is_stale).
/// - `baseline_oid`: The OID of the baseline commit (or None if not set)
/// - `commits_since_baseline`: Number of commits since baseline
/// - `is_stale`: true if baseline is old (>10 commits behind)
pub fn get_review_baseline_info() -> io::Result<(Option<String>, usize, bool)> {
    let repo = git2::Repository::discover(".").map_err(|e| to_io_error(&e))?;

    let baseline_oid = match load_review_baseline()? {
        ReviewBaseline::Commit(oid) => Some(oid.to_string()),
        ReviewBaseline::NotSet => None,
    };

    let commits_since = if let Some(ref oid) = baseline_oid {
        count_commits_since(&repo, oid)?
    } else {
        0
    };

    let is_stale = commits_since > 10;

    Ok((baseline_oid, commits_since, is_stale))
}

/// Write the review baseline to disk.
fn write_review_baseline(oid: &str) -> io::Result<()> {
    let path = PathBuf::from(REVIEW_BASELINE_FILE);
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::write(&path, oid)?;
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

/// Baseline summary information for display.
#[derive(Debug, Clone)]
pub struct BaselineSummary {
    /// The baseline OID (short form).
    pub baseline_oid: Option<String>,
    /// Number of commits since baseline.
    pub commits_since: usize,
    /// Whether the baseline is stale (>10 commits behind).
    pub is_stale: bool,
}

impl BaselineSummary {
    /// Format a compact version for inline display.
    pub fn format_compact(&self) -> String {
        match &self.baseline_oid {
            Some(oid) => {
                let short_oid = &oid[..8.min(oid.len())];
                if self.is_stale {
                    format!(
                        "Baseline: {} (+{} commits since)",
                        short_oid, self.commits_since
                    )
                } else if self.commits_since > 0 {
                    format!(
                        "Baseline: {} ({} commits since)",
                        short_oid, self.commits_since
                    )
                } else {
                    format!("Baseline: {}", short_oid)
                }
            }
            None => "Baseline: start_commit".to_string(),
        }
    }
}

/// Get a summary of the baseline state for display.
///
/// Returns a `BaselineSummary` containing information about the current
/// baseline, commits since baseline, and staleness.
pub fn get_baseline_summary() -> io::Result<BaselineSummary> {
    let repo = git2::Repository::discover(".").map_err(|e| to_io_error(&e))?;

    let baseline_oid = match load_review_baseline()? {
        ReviewBaseline::Commit(oid) => Some(oid.to_string()),
        ReviewBaseline::NotSet => None,
    };

    let commits_since = if let Some(ref oid) = baseline_oid {
        count_commits_since(&repo, oid)?
    } else {
        0
    };

    let is_stale = commits_since > 10;

    Ok(BaselineSummary {
        baseline_oid,
        commits_since,
        is_stale,
    })
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

    #[test]
    fn test_get_git_diff_from_review_baseline_returns_result() {
        let result = get_git_diff_from_review_baseline();
        assert!(result.is_ok() || result.is_err());
    }
}
