//! Review phase execution.
//!
//! This module handles the review and fix phase of the Ralph pipeline. It runs
//! a configurable number of review-fix cycles, where each cycle:
//! 1. Reviews the code and creates ISSUES.md
//! 2. Fixes the issues found
//! 3. Cleans up ISSUES.md (in isolation mode)
//!
//! # Module Structure
//!
//! - [`execution`] - Main review phase execution logic
//! - [`commit`] - Commit handling during review
//! - [`prompt`] - Review prompt building logic
//! - [`validation`] - Pre-flight and post-flight validation checks

mod commit;
mod execution;
mod prompt;
mod validation;

pub use execution::run_review_phase;
