//! File I/O operations for Ralph's agent files.
//!
//! This module handles basic file input/output operations:
//! - Agent directory management (.agent/)
//! - Commit message file operations
//! - Context cleanup and file operations
//! - Error recovery for file operations
//!
//! # Submodules
//!
//! - [`agent_files`](super::agent_files) - Core agent file operations
//! - [`recovery`](super::recovery) - Error recovery and state repair

pub use super::agent_files::{
    clean_context_for_reviewer, cleanup_generated_files, create_prompt_backup,
    delete_commit_message_file, delete_issues_file_for_isolation, delete_plan_file, ensure_files,
    file_contains_marker, make_prompt_read_only, read_commit_message_file,
    reset_context_for_isolation, update_status, write_commit_message_file,
};
