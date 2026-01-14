//! Project stack detection and review guidelines generation.
//!
//! This module handles automatic detection of the project technology stack
//! and generation of language-specific review guidelines.

use crate::config::Config;
use crate::guidelines::ReviewGuidelines;
use crate::language_detector::ProjectStack;
use crate::logger::Colors;
use crate::logger::Logger;

/// Detects project stack and generates review guidelines.
pub fn detect_project_stack(
    config: &Config,
    repo_root: &std::path::Path,
    logger: &Logger,
    colors: Colors,
) -> (Option<ProjectStack>, Option<ReviewGuidelines>) {
    if !config.auto_detect_stack {
        return (None, None);
    }

    match crate::language_detector::detect_stack(repo_root) {
        Ok(stack) => {
            logger.info(&format!(
                "Detected stack: {}{}{}",
                colors.cyan(),
                stack.summary(),
                colors.reset()
            ));
            let guidelines = ReviewGuidelines::for_stack(&stack);
            (Some(stack), Some(guidelines))
        }
        Err(e) => {
            logger.warn(&format!("Could not detect project stack: {e}"));
            (None, None)
        }
    }
}
