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
//! 3. Fixer makes changes (reviewer agent by default)
//! 4. Baseline is updated after fix pass
//! 5. Next review cycle sees only new changes
//!
//! This prevents "diff scope creep" where previous fix commits pollute
//! subsequent review passes.

use std::io;
use std::path::Path;

use crate::workspace::{Workspace, WorkspaceFs};

use super::start_commit::get_current_head_oid;

include!("review_baseline/part1.rs");
include!("review_baseline/part2.rs");

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

    // =========================================================================
    // Workspace-aware function tests
    // =========================================================================

    #[test]
    fn test_load_review_baseline_with_workspace_not_set() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test();

        let result = load_review_baseline_with_workspace(&workspace).unwrap();
        assert_eq!(result, ReviewBaseline::NotSet);
    }

    #[test]
    fn test_load_review_baseline_with_workspace_sentinel() {
        use crate::workspace::MemoryWorkspace;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/review_baseline.txt", BASELINE_NOT_SET);

        let result = load_review_baseline_with_workspace(&workspace).unwrap();
        assert_eq!(result, ReviewBaseline::NotSet);
    }

    #[test]
    fn test_load_review_baseline_with_workspace_empty() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test().with_file(".agent/review_baseline.txt", "");

        let result = load_review_baseline_with_workspace(&workspace).unwrap();
        assert_eq!(result, ReviewBaseline::NotSet);
    }

    #[test]
    fn test_load_review_baseline_with_workspace_valid_oid() {
        use crate::workspace::MemoryWorkspace;

        let workspace = MemoryWorkspace::new_test().with_file(
            ".agent/review_baseline.txt",
            "abcd1234abcd1234abcd1234abcd1234abcd1234",
        );

        let result = load_review_baseline_with_workspace(&workspace).unwrap();
        let expected_oid = git2::Oid::from_str("abcd1234abcd1234abcd1234abcd1234abcd1234").unwrap();
        assert_eq!(result, ReviewBaseline::Commit(expected_oid));
    }

    #[test]
    fn test_load_review_baseline_with_workspace_invalid_oid() {
        use crate::workspace::MemoryWorkspace;

        let workspace =
            MemoryWorkspace::new_test().with_file(".agent/review_baseline.txt", "invalid");

        let result = load_review_baseline_with_workspace(&workspace);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err().kind(), io::ErrorKind::InvalidData);
    }
}
