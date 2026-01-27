//! PROMPT.md integrity utilities.
//!
//! This module provides utilities for ensuring PROMPT.md integrity during pipeline execution.

use crate::workspace::Workspace;

/// Periodically restore PROMPT.md if it was deleted by an agent.
///
/// This is a defense-in-depth measure to ensure PROMPT.md is always available
/// even if an agent accidentally deletes it during pipeline execution.
///
/// Uses the workspace abstraction for file operations, enabling testing with
/// `MemoryWorkspace`.
///
/// # Parameters
/// - `workspace`: The workspace for file operations
/// - `logger`: The logger to use for output
/// - `phase`: The phase name (e.g., "development", "review") for logging
/// - `iteration`: The iteration/cycle number for logging
pub fn ensure_prompt_integrity(
    workspace: &dyn Workspace,
    logger: &crate::logger::Logger,
    phase: &str,
    iteration: u32,
) {
    // Use validate_prompt_md_with_workspace which handles restore internally
    let result = crate::files::validate_prompt_md_with_workspace(workspace, false, false);

    // Check the result for warnings about restoration
    for warning in &result.warnings {
        if warning.contains("restored from") {
            logger.warn("[PROMPT_INTEGRITY] PROMPT.md was missing or empty and has been restored from backup");
            logger.warn(&format!(
                "[PROMPT_INTEGRITY] Deletion detected during {phase} phase (iteration {iteration})"
            ));
            logger.warn("[PROMPT_INTEGRITY] Possible cause: Agent used 'rm' or file write tools on PROMPT.md");
            logger.success(
                &warning.replace("PROMPT.md was missing and was automatically ", "PROMPT.md "),
            );
            return;
        }
    }

    // Check for errors (no backup available)
    for error in &result.errors {
        if error.contains("not found") || error.contains("missing") || error.contains("empty") {
            logger.error(&format!(
                "[PROMPT_INTEGRITY] Failed to restore PROMPT.md: {error}"
            ));
            logger.error(&format!(
                "[PROMPT_INTEGRITY] Error occurred during {phase} phase (iteration {iteration})"
            ));
            logger.error("Pipeline may not function correctly without PROMPT.md");
            return;
        }
    }

    // File exists with content - no action needed
}
