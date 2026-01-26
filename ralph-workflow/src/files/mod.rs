//! File management utilities for Ralph's agent files.
//!
//! This module manages the `.agent/` directory structure and files:
//! - PLAN.md, ISSUES.md, STATUS.md, NOTES.md lifecycle
//! - commit-message.txt management
//! - PROMPT.md validation
//! - Isolation mode file cleanup
//! - Result extraction from agent JSON logs
//! - File integrity verification and checksums
//! - Error recovery and state repair
//! - Real-time file system monitoring for PROMPT.md protection
//!
//! # Module Organization
//!
//! The files module is organized by domain concern:
//!
//! - [`io`] - File I/O operations (agent files, recovery, backup, context)
//! - [`protection`] - File protection and integrity (validation, integrity, monitoring)
//! - [`llm_output_extraction`] - LLM output extraction (commit message, JSON extraction)
//! - [`result_extraction`] - Plan and issue extraction from logs
//!
//! # Isolation Mode
//!
//! By default, Ralph operates in isolation mode where STATUS.md, NOTES.md,
//! and ISSUES.md are not persisted between runs. This prevents context
//! contamination from previous runs.
//!
//! # Orchestrator-Controlled File I/O
//!
//! The orchestrator is the sole entity responsible for writing output files.
//! Agent JSON output is extracted and written by the orchestrator, ensuring
//! consistent file handling regardless of agent behavior.

// Domain-driven submodules
pub mod io;
pub mod protection;

// Extraction modules (already domain-organized)
pub mod llm_output_extraction;
pub mod result_extraction;

// Re-exports from new domain structure for backward compatibility
pub use io::{
    clean_context_for_reviewer, clean_context_for_reviewer_with_workspace, cleanup_generated_files,
    cleanup_generated_files_with_workspace, create_prompt_backup,
    create_prompt_backup_with_workspace, delete_commit_message_file,
    delete_commit_message_file_with_workspace, delete_issues_file_for_isolation,
    delete_issues_file_for_isolation_with_workspace, delete_plan_file,
    delete_plan_file_with_workspace, ensure_files, file_contains_marker,
    file_contains_marker_with_workspace, make_prompt_read_only,
    make_prompt_read_only_with_workspace, make_prompt_writable,
    make_prompt_writable_with_workspace, read_commit_message_file,
    read_commit_message_file_with_workspace, reset_context_for_isolation,
    setup_xsd_schemas_with_workspace, update_status, update_status_with_workspace,
    verify_file_not_corrupted_with_workspace, write_commit_message_file,
    write_commit_message_file_with_workspace, write_file_atomic_with_workspace,
};

pub use protection::{
    restore_prompt_if_needed, validate_prompt_md, validate_prompt_md_with_workspace,
};
pub use result_extraction::extract_issues;
#[cfg(any(test, feature = "test-utils"))]
pub use result_extraction::extract_plan;
