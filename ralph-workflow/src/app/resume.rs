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
    if !args.resume {
        return None;
    }

    match load_checkpoint() {
        Ok(Some(checkpoint)) => {
            logger.header("RESUME: Loading Checkpoint", |c| c.yellow());
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
            logger.warn(&format!(
                "Failed to load checkpoint (starting fresh): {}",
                e
            ));
            None
        }
    }
}

/// Helper to get phase rank for resume logic.
pub fn phase_rank(p: PipelinePhase) -> u8 {
    match p {
        PipelinePhase::Planning => 0,
        PipelinePhase::Development => 1,
        PipelinePhase::Review => 2,
        PipelinePhase::Fix => 3,
        PipelinePhase::ReviewAgain => 4,
        PipelinePhase::CommitMessage => 5,
        PipelinePhase::FinalValidation => 6,
        PipelinePhase::Complete => 7,
    }
}

/// Determines if a phase should run based on resume checkpoint.
pub fn should_run_from(
    phase: PipelinePhase,
    resume_checkpoint: Option<&PipelineCheckpoint>,
) -> bool {
    match resume_checkpoint {
        None => true,
        Some(checkpoint) => phase_rank(phase) >= phase_rank(checkpoint.phase),
    }
}
