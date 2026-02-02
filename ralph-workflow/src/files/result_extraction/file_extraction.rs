//! File path extraction from ISSUES content.
//!
//! This module provides utilities to extract file paths from ISSUES markdown content.
//! The fix agent uses this to identify which files it may modify without needing
//! to explore the repository.

include!("file_extraction/part1.rs");
include!("file_extraction/part2.rs");
