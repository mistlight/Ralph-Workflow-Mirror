//! Resume functionality for pipeline checkpoints.
//!
//! This module handles the --resume flag and checkpoint loading logic.

use crate::checkpoint::{load_checkpoint, PipelineCheckpoint, PipelinePhase};
use crate::logger::Logger;

/// Handles the --resume flag and loads checkpoint if applicable.
pub fn handle_resume(
    args: &crate::cli::Args,
    logger: &Logger,
    developer_agent: &str,
    reviewer_agent: &str,
) -> Option<PipelineCheckpoint> {
    if !args.recovery.resume {
        return None;
    }

    match load_checkpoint() {
        Ok(Some(checkpoint)) => {
            logger.header("RESUME: Loading Checkpoint", crate::logger::Colors::yellow);
            logger.info(&format!("Resuming from: {}", checkpoint.description()));
            logger.info(&format!("Checkpoint saved at: {}", checkpoint.timestamp));

            // Verify agents match
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

            Some(checkpoint)
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

/// Helper to get phase rank for resume logic.
pub const fn phase_rank(p: PipelinePhase) -> u8 {
    match p {
        PipelinePhase::Rebase => 0,
        PipelinePhase::Planning => 1,
        PipelinePhase::Development => 2,
        PipelinePhase::Review => 3,
        PipelinePhase::Fix => 4,
        PipelinePhase::ReviewAgain => 5,
        PipelinePhase::CommitMessage => 6,
        PipelinePhase::FinalValidation => 7,
        PipelinePhase::Complete => 8,
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
