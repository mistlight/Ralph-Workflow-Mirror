//! File I/O operations for Ralph's agent files.
//!
//! This module handles basic file input/output operations:
//! - Agent directory management (.agent/)
//! - Commit message file operations
//! - Context cleanup and file operations
//! - Error recovery for file operations
//! - File integrity and atomic writes
//! - PROMPT.md backup management
//! - File operations trait for testable I/O (test-utils feature)
//!
//! # Submodules
//!
//! - [`integrity`] - File integrity and atomic writes
//! - [`recovery`] - Error recovery and state repair
//! - [`context`] - Context file management (STATUS.md, NOTES.md, ISSUES.md)
//! - [`agent_files`] - Agent file operations (`ensure_files`, commit message, etc.)
//! - [`backup`] - PROMPT.md backup and read-only protection
//! - [`file_ops`] - File operations trait for testable I/O (requires `test-utils` feature)

pub(in crate::files) mod integrity;
pub(in crate::files) mod recovery;

pub mod agent_files;
pub mod backup;
pub mod context;

// File operations module is only available with test-utils feature or in tests
#[cfg(any(test, feature = "test-utils"))]
pub mod file_ops;

// Re-exports for backward compatibility
pub use agent_files::{
    cleanup_generated_files, delete_commit_message_file, delete_plan_file, ensure_files,
    file_contains_marker, read_commit_message_file, write_commit_message_file,
};

pub use backup::{create_prompt_backup, make_prompt_read_only, make_prompt_writable};

pub use context::{
    clean_context_for_reviewer, delete_issues_file_for_isolation, reset_context_for_isolation,
    update_status,
};

#[cfg(any(test, feature = "test-utils"))]
pub use file_ops::{FileOps, RealFileOps};

#[cfg(any(test, feature = "test-utils"))]
pub use file_ops::{FileOperation, MockFileOps};
