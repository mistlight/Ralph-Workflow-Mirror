//! File path extraction helpers for review output.
//!
//! This module provides utilities for extracting file paths from ISSUES content.
//! Legacy log-based extraction has been removed; all pipeline outputs are read
//! from explicit XML files managed by the reducer effects.

mod file_extraction;

pub use file_extraction::extract_file_paths_from_issues;
