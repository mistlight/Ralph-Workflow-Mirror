//! Resume functionality for pipeline checkpoints.
//!
//! This module handles the --resume flag and checkpoint loading logic,
//! including validation and state restoration.

use crate::agents::AgentRegistry;
use crate::checkpoint::file_state::FileSystemState;
use crate::checkpoint::{load_checkpoint, validate_checkpoint, PipelineCheckpoint, PipelinePhase};
use crate::config::Config;
use crate::git_helpers::rebase_in_progress;
use crate::logger::Logger;

/// Result of handling resume, containing the checkpoint.
pub struct ResumeResult {
    /// The loaded checkpoint.
    pub checkpoint: PipelineCheckpoint,
}

/// Result of file system validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ValidationOutcome {
    /// Validation passed, safe to resume
    Passed,
    /// Validation failed, cannot resume
    Failed(String),
}

/// Handles the --resume flag and loads checkpoint if applicable.
///
/// This function loads and validates the checkpoint, providing detailed
/// feedback about what state is being restored and any configuration changes.
///
/// # Arguments
///
/// * `args` - CLI arguments
/// * `config` - Current configuration (for validation comparison)
/// * `registry` - Agent registry (for agent validation)
/// * `logger` - Logger for output
/// * `developer_agent` - Current developer agent name
/// * `reviewer_agent` - Current reviewer agent name
///
/// # Returns
///
/// `Some(ResumeResult)` if a valid checkpoint was found and loaded,
/// `None` if no checkpoint exists or --resume was not specified.
pub fn handle_resume_with_validation(
    args: &crate::cli::Args,
    config: &Config,
    registry: &AgentRegistry,
    logger: &Logger,
    developer_agent: &str,
    reviewer_agent: &str,
) -> Option<ResumeResult> {
    if !args.recovery.resume {
        return None;
    }

    match load_checkpoint() {
        Ok(Some(checkpoint)) => {
            logger.header("RESUME: Loading Checkpoint", crate::logger::Colors::yellow);
            display_checkpoint_summary(&checkpoint, logger);

            // Validate checkpoint
            let validation = validate_checkpoint(&checkpoint, config, registry);

            // Display validation results
            for warning in &validation.warnings {
                logger.warn(warning);
            }
            for error in &validation.errors {
                logger.error(error);
            }

            if !validation.is_valid {
                logger.error("Checkpoint validation failed. Cannot resume.");
                logger.info(
                    "Delete .agent/checkpoint.json and start fresh, or fix the issues above.",
                );
                return None;
            }

            // Verify agents match (additional agent-specific warnings)
            if checkpoint.developer_agent != developer_agent {
                logger.warn(&format!(
                    "Developer agent changed: {} -> {}",
                    checkpoint.developer_agent, developer_agent
                ));
            }
            if checkpoint.reviewer_agent != reviewer_agent {
                logger.warn(&format!(
                    "Reviewer agent changed: {} -> {}",
                    checkpoint.reviewer_agent, reviewer_agent
                ));
            }

            // Check for in-progress git rebase
            check_rebase_state_on_resume(&checkpoint, logger);

            // Perform file system state validation
            let validation_outcome = if let Some(file_system_state) = &checkpoint.file_system_state
            {
                validate_file_system_state(
                    file_system_state,
                    logger,
                    args.recovery.recovery_strategy.into(),
                )
            } else {
                ValidationOutcome::Passed
            };

            if matches!(validation_outcome, ValidationOutcome::Failed(_)) {
                return None;
            }

            Some(ResumeResult { checkpoint })
        }
        Ok(None) => {
            logger.warn("No checkpoint found. Starting fresh pipeline...");
            None
        }
        Err(e) => {
            logger.warn(&format!("Failed to load checkpoint (starting fresh): {e}"));
            None
        }
    }
}

/// Validate file system state when resuming.
///
/// This function validates that the current file system state matches
/// the state captured in the checkpoint. This is part of the hardened
/// resume feature that ensures idempotent recovery.
///
/// Returns a `ValidationOutcome` indicating whether validation passed
/// or failed with a reason.
fn validate_file_system_state(
    file_system_state: &FileSystemState,
    logger: &Logger,
    strategy: crate::checkpoint::recovery::RecoveryStrategy,
) -> ValidationOutcome {
    let errors = file_system_state.validate();

    if errors.is_empty() {
        logger.info("File system state validation passed.");
        return ValidationOutcome::Passed;
    }

    logger.warn("File system state validation detected changes:");

    for error in &errors {
        logger.warn(&format!("  - {}", error));
        logger.info(&format!("    Suggestion: {}", error.recovery_suggestion()));
    }

    // Handle based on the recovery strategy
    match strategy {
        crate::checkpoint::recovery::RecoveryStrategy::Fail => {
            logger.error("File system state validation failed (strategy: fail).");
            logger.info("Use --recovery-strategy=auto to attempt automatic recovery.");
            logger.info("Use --recovery-strategy=force to proceed anyway (not recommended).");
            ValidationOutcome::Failed(
                "File system state changed - see errors above or use --recovery-strategy=force to proceed anyway".to_string()
            )
        }
        crate::checkpoint::recovery::RecoveryStrategy::Force => {
            logger.warn("Proceeding with resume despite file changes (strategy: force).");
            logger.info("Note: Pipeline behavior may be unpredictable.");
            ValidationOutcome::Passed
        }
        crate::checkpoint::recovery::RecoveryStrategy::Auto => {
            // Auto recovery - for now, we just warn but continue
            // In the future, this could attempt automatic fixes
            logger.warn("Automatic recovery not fully implemented for file system changes.");
            logger.warn("Proceeding with resume despite file changes (strategy: auto).");
            logger.info("Note: Pipeline behavior may be unpredictable.");
            ValidationOutcome::Passed
        }
    }
}

/// Check for in-progress git rebase when resuming.
///
/// This function detects if a git rebase is in progress and provides
/// appropriate guidance to the user.
fn check_rebase_state_on_resume(checkpoint: &PipelineCheckpoint, logger: &Logger) {
    // Only check for rebase if we're resuming from a rebase-related phase
    let is_rebase_phase = matches!(
        checkpoint.phase,
        PipelinePhase::PreRebase
            | PipelinePhase::PreRebaseConflict
            | PipelinePhase::PostRebase
            | PipelinePhase::PostRebaseConflict
    );

    if !is_rebase_phase {
        return;
    }

    match rebase_in_progress() {
        Ok(true) => {
            logger.warn("A git rebase is currently in progress.");
            logger.info("The checkpoint indicates you were in a rebase phase.");
            logger.info("Options:");
            logger.info("  - Continue: Let ralph complete the rebase process");
            logger.info("  - Abort manually: Run 'git rebase --abort' then use --resume");
        }
        Ok(false) => {
            // No rebase in progress - this is expected if rebase completed
            // but checkpoint wasn't cleared (e.g., pipeline was interrupted)
            logger.info("No git rebase is currently in progress.");
        }
        Err(e) => {
            logger.warn(&format!("Could not check rebase state: {e}"));
        }
    }
}

/// Display a summary of the checkpoint being loaded.
fn display_checkpoint_summary(checkpoint: &PipelineCheckpoint, logger: &Logger) {
    logger.info(&format!("Resuming from: {}", checkpoint.description()));
    logger.info(&format!("Checkpoint saved at: {}", checkpoint.timestamp));
    logger.info(&format!("Checkpoint version: {}", checkpoint.version));

    // Show run ID and resume count
    logger.info(&format!("Run ID: {}", checkpoint.run_id));
    if checkpoint.resume_count > 0 {
        logger.info(&format!(
            "Resume count: {} (this is resume #{} of this session)",
            checkpoint.resume_count,
            checkpoint.resume_count + 1
        ));
    }
    if let Some(ref parent_id) = checkpoint.parent_run_id {
        logger.info(&format!("Parent run ID: {}", parent_id));
    }

    // Show actual execution counts vs configured counts
    logger.info(&format!(
        "Development: {} iteration(s) configured, {} completed",
        checkpoint.total_iterations, checkpoint.actual_developer_runs
    ));
    logger.info(&format!(
        "Review: {} pass(es) configured, {} completed",
        checkpoint.total_reviewer_passes, checkpoint.actual_reviewer_runs
    ));

    // Show iteration progress
    if checkpoint.total_iterations > 0 {
        logger.info(&format!(
            "Current position: iteration {}/{}",
            checkpoint.iteration, checkpoint.total_iterations
        ));
    }
    if checkpoint.total_reviewer_passes > 0 {
        logger.info(&format!(
            "Current position: pass {}/{}",
            checkpoint.reviewer_pass, checkpoint.total_reviewer_passes
        ));
    }

    // Show CLI args if available
    let cli = &checkpoint.cli_args;
    if cli.developer_iters > 0 || cli.reviewer_reviews > 0 {
        logger.info(&format!(
            "Original config: -D {} -R {}",
            cli.developer_iters, cli.reviewer_reviews
        ));
    }

    // Show agent configs
    logger.info(&format!("Developer agent: {}", checkpoint.developer_agent));
    logger.info(&format!("Reviewer agent: {}", checkpoint.reviewer_agent));

    // Show model overrides if present
    if let Some(ref model) = checkpoint.developer_agent_config.model_override {
        logger.info(&format!("Developer model override: {}", model));
    }
    if let Some(ref model) = checkpoint.reviewer_agent_config.model_override {
        logger.info(&format!("Reviewer model override: {}", model));
    }

    // Show provider overrides if present
    if let Some(ref provider) = checkpoint.developer_agent_config.provider_override {
        logger.info(&format!("Developer provider: {}", provider));
    }
    if let Some(ref provider) = checkpoint.reviewer_agent_config.provider_override {
        logger.info(&format!("Reviewer provider: {}", provider));
    }

    // Show rebase state if applicable
    match &checkpoint.rebase_state {
        crate::checkpoint::RebaseState::PreRebaseInProgress { upstream_branch } => {
            logger.warn(&format!("Pre-rebase in progress to: {}", upstream_branch));
        }
        crate::checkpoint::RebaseState::HasConflicts { files } => {
            logger.warn(&format!("Rebase has conflicts in {} files", files.len()));
            for file in files.iter().take(3) {
                logger.warn(&format!("  - {}", file));
            }
            if files.len() > 3 {
                logger.warn(&format!("  ... and {} more", files.len() - 3));
            }
        }
        _ => {}
    }
}

/// Helper to get phase rank for resume logic.
///
/// Lower ranks = earlier in pipeline. Used to determine which phases
/// should run when resuming from a checkpoint.
pub const fn phase_rank(p: PipelinePhase) -> u8 {
    match p {
        PipelinePhase::Planning => 0,
        PipelinePhase::PreRebase => 1,
        PipelinePhase::PreRebaseConflict => 2,
        PipelinePhase::Development => 3,
        PipelinePhase::Review => 4,
        PipelinePhase::Fix => 5,
        PipelinePhase::ReviewAgain => 6,
        PipelinePhase::PostRebase => 7,
        PipelinePhase::PostRebaseConflict => 8,
        PipelinePhase::CommitMessage => 9,
        PipelinePhase::FinalValidation => 10,
        PipelinePhase::Complete => 11,
    }
}

/// Determines if a phase should run based on resume checkpoint.
pub const fn should_run_from(
    phase: PipelinePhase,
    resume_checkpoint: Option<&PipelineCheckpoint>,
) -> bool {
    match resume_checkpoint {
        None => true,
        Some(checkpoint) => phase_rank(phase) >= phase_rank(checkpoint.phase),
    }
}
