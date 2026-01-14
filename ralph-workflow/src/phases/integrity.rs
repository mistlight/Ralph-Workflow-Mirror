//! PROMPT.md integrity utilities.
//!
//! This module provides utilities for ensuring PROMPT.md integrity during pipeline execution.

/// Periodically restore PROMPT.md if it was deleted by an agent.
///
/// This is a defense-in-depth measure to ensure PROMPT.md is always available
/// even if an agent accidentally deletes it during pipeline execution.
///
/// # Parameters
/// - `logger`: The logger to use for output
/// - `phase`: The phase name (e.g., "development", "review") for logging
/// - `iteration`: The iteration/cycle number for logging
///
/// # Example
/// ```no_run
/// use crate::logger::Logger;
/// use crate::phases::integrity::ensure_prompt_integrity;
///
/// # let logger = Logger::new();
/// ensure_prompt_integrity(&logger, "development", 1);
/// ```
pub fn ensure_prompt_integrity(logger: &crate::logger::Logger, phase: &str, iteration: u32) {
    match crate::files::restore_prompt_if_needed() {
        Ok(true) => {
            // File exists with content, no action needed
        }
        Ok(false) => {
            logger.warn("[PROMPT_INTEGRITY] PROMPT.md was missing or empty and has been restored from backup");
            logger.warn(&format!(
                "[PROMPT_INTEGRITY] Deletion detected during {phase} phase (iteration {iteration})"
            ));
            logger.warn("[PROMPT_INTEGRITY] Possible cause: Agent used 'rm' or file write tools on PROMPT.md");
            logger.success("PROMPT.md restored from .agent/PROMPT.md.backup");
        }
        Err(e) => {
            logger.error(&format!(
                "[PROMPT_INTEGRITY] Failed to restore PROMPT.md: {e}"
            ));
            logger.error(&format!(
                "[PROMPT_INTEGRITY] Error occurred during {phase} phase (iteration {iteration})"
            ));
            logger.error("Pipeline may not function correctly without PROMPT.md");
        }
    }
}
