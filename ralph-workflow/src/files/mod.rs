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

mod agent_files;
pub mod integrity;
pub mod llm_output_extraction;
pub mod monitoring;
pub mod recovery;
pub mod result_extraction;
pub mod validation;

pub use agent_files::{
    clean_context_for_reviewer, cleanup_generated_files, create_prompt_backup,
    delete_commit_message_file, delete_issues_file_for_isolation, delete_plan_file, ensure_files,
    file_contains_marker, make_prompt_read_only, read_commit_message_file,
    reset_context_for_isolation, update_status, write_commit_message_file,
};

pub use result_extraction::{extract_issues, extract_plan, extract_plan_from_logs_text};
pub use validation::{restore_prompt_if_needed, validate_prompt_md};
