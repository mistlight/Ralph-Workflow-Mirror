//! File I/O operations for Ralph's agent files.
//!
//! This module handles basic file input/output operations:
//! - Agent directory management (.agent/)
//! - Commit message file operations
//! - Context cleanup and file operations
//! - Error recovery for file operations
//! - File integrity and atomic writes
//! - PROMPT.md backup management
//!
//! # Submodules
//!
//! - [`integrity`] - File integrity and atomic writes
//! - [`recovery`] - Error recovery and state repair
//! - [`context`] - Context file management (STATUS.md, NOTES.md, ISSUES.md)
//! - [`agent_files`] - Agent file operations (`ensure_files`, commit message, etc.)
//! - [`backup`] - PROMPT.md backup and read-only protection
//!
//! # File Operations
//!
//! All file operations should go through the `Workspace` trait for testability.
//! See `crate::workspace` for the `Workspace` trait and `MemoryWorkspace` for testing.

pub(in crate::files) mod integrity;
pub(in crate::files) mod recovery;

pub mod agent_files;
pub mod backup;
pub mod context;

// Re-exports for backward compatibility
pub use agent_files::{
    cleanup_generated_files, delete_commit_message_file, delete_plan_file, ensure_files,
    file_contains_marker, read_commit_message_file, write_commit_message_file,
};

pub use integrity::check_and_cleanup_xml_before_retry;

pub use backup::{
    create_prompt_backup, create_prompt_backup_with_workspace, make_prompt_read_only,
    make_prompt_read_only_with_workspace, make_prompt_writable,
    make_prompt_writable_with_workspace,
};

pub use context::{
    clean_context_for_reviewer, clean_context_for_reviewer_with_workspace,
    delete_issues_file_for_isolation, delete_issues_file_for_isolation_with_workspace,
    reset_context_for_isolation, update_status, update_status_with_workspace,
};
