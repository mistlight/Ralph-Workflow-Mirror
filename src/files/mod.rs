//! File management utilities for Ralph's agent files.
//!
//! This module manages the `.agent/` directory structure and files:
//! - PLAN.md, ISSUES.md, STATUS.md, NOTES.md lifecycle
//! - commit-message.txt management
//! - PROMPT.md validation
//! - Isolation mode file cleanup
//!
//! # Isolation Mode
//!
//! By default, Ralph operates in isolation mode where STATUS.md, NOTES.md,
//! and ISSUES.md are not persisted between runs. This prevents context
//! contamination from previous runs.

mod agent_files;
mod validation;

pub use agent_files::{
    clean_context_for_reviewer, cleanup_generated_files, delete_commit_message_file,
    delete_issues_file_for_isolation, delete_plan_file, ensure_files, file_contains_marker,
    read_commit_message_file, reset_context_for_isolation, update_status, GENERATED_FILES,
};
pub use validation::{validate_prompt_md, PromptValidationResult};
