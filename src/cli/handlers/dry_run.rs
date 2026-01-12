//! Dry run command handler.
//!
//! This module provides validation of Ralph setup without running any agents.

use crate::config::Config;
use crate::language_detector::detect_stack_summary;
use crate::utils::{checkpoint_exists, load_checkpoint, validate_prompt_md, Logger};
use std::path::Path;

/// Handle --dry-run command.
///
/// Validates the setup without running any agents:
/// - Checks PROMPT.md exists and has required sections
/// - Validates agent configuration
/// - Reports detected project stack
/// - Shows checkpoint status if available
///
/// # Arguments
///
/// * `logger` - Logger for status output
/// * `_colors` - Color configuration (unused but kept for API consistency)
/// * `config` - The current Ralph configuration
/// * `developer_agent` - Name of the developer agent
/// * `reviewer_agent` - Name of the reviewer agent
/// * `repo_root` - Path to the repository root
///
/// # Returns
///
/// Returns `Ok(())` if validation passes, or an error if PROMPT.md validation fails.
pub fn handle_dry_run(
    logger: &Logger,
    _colors: &crate::colors::Colors,
    config: &Config,
    developer_agent: &str,
    reviewer_agent: &str,
    repo_root: &Path,
) -> anyhow::Result<()> {
    logger.header("DRY RUN: Validation", |c| c.cyan());

    // Validate PROMPT.md using the utility function
    let validation = validate_prompt_md(config.strict_validation);

    // Report errors first
    for err in &validation.errors {
        logger.error(err);
    }

    // Report warnings
    for warn in &validation.warnings {
        logger.warn(&format!("{} (recommended)", warn));
    }

    // Bail if validation failed
    if !validation.is_valid() {
        anyhow::bail!("Dry run failed: PROMPT.md validation errors");
    }

    // Report successes
    if validation.has_goal {
        logger.success("PROMPT.md has Goal section");
    }
    if validation.has_acceptance {
        logger.success("PROMPT.md has acceptance checks section");
    }
    if validation.is_perfect() {
        logger.success("PROMPT.md validation passed with no warnings");
    }

    logger.success(&format!("Developer agent: {}", developer_agent));
    logger.success(&format!("Reviewer agent: {}", reviewer_agent));
    logger.success(&format!("Developer iterations: {}", config.developer_iters));
    logger.success(&format!("Reviewer passes: {}", config.reviewer_reviews));

    // Check for checkpoint
    if checkpoint_exists() {
        logger.info("Checkpoint found - can resume with --resume");
        if let Ok(Some(cp)) = load_checkpoint() {
            logger.info(&format!("  Phase: {}", cp.phase));
            logger.info(&format!("  Progress: {}", cp.description()));
            logger.info(&format!("  Saved at: {}", cp.timestamp));
        }
    }

    // Detect stack - use the convenience function for simple display
    logger.success(&format!(
        "Detected stack: {}",
        detect_stack_summary(repo_root)
    ));

    logger.success("Dry run validation complete");
    Ok(())
}
