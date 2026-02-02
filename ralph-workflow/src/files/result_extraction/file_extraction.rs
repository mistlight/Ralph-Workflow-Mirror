//! File path extraction from ISSUES content.
//!
//! This module provides utilities to extract file paths from ISSUES markdown content.
//! The fix agent uses this to identify which files it may modify without needing
//! to explore the repository.

include!("file_extraction/extraction.rs");
include!("file_extraction/tests.rs");
