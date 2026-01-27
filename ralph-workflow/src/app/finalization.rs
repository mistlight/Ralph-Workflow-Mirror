//! Pipeline finalization and cleanup.
//!
//! This module handles the final phase of the pipeline including cleanup,
//! final summary, and checkpoint clearing.
//!
//! Note: PROMPT.md permission restoration is now handled by the reducer's
//! `Effect::RestorePromptPermissions` during the `Finalizing` phase, ensuring
//! it goes through the effect system for proper testability.

use crate::banner::{print_final_summary, PipelineSummary};
use crate::checkpoint::{clear_checkpoint, clear_checkpoint_with_workspace};
use crate::config::Config;
use crate::files::protection::monitoring::PromptMonitor;
use crate::logger::Colors;
use crate::logger::Logger;
use crate::pipeline::Timer;
use crate::pipeline::{AgentPhaseGuard, Stats};
use crate::workspace::Workspace;

/// Runtime statistics collected during pipeline execution.
pub struct RuntimeStats<'a> {
    pub timer: &'a Timer,
    pub stats: &'a Stats,
}

/// Finalizes the pipeline: cleans up and prints summary.
///
/// Commits now happen per-iteration during development and per-cycle during review,
/// so this function only handles cleanup and final summary.
///
/// # Arguments
///
/// * `workspace` - Optional workspace for file operations (enables testability)
pub fn finalize_pipeline(
    agent_phase_guard: &mut AgentPhaseGuard,
    logger: &Logger,
    colors: Colors,
    config: &Config,
    runtime: RuntimeStats<'_>,
    prompt_monitor: Option<PromptMonitor>,
    workspace: Option<&dyn Workspace>,
) {
    // Stop the PROMPT.md monitor if it was started
    if let Some(monitor) = prompt_monitor {
        monitor.stop();
    }

    // End agent phase and clean up
    crate::git_helpers::end_agent_phase();
    crate::git_helpers::disable_git_wrapper(agent_phase_guard.git_helpers);
    if let Err(err) = crate::git_helpers::uninstall_hooks(logger) {
        logger.warn(&format!("Failed to uninstall Ralph hooks: {err}"));
    }

    // Note: Individual commits were created per-iteration during development
    // and per-cycle during review. The final commit phase has been removed.

    // Final summary
    let summary = PipelineSummary {
        total_time: runtime.timer.elapsed_formatted(),
        dev_runs_completed: runtime.stats.developer_runs_completed as usize,
        dev_runs_total: config.developer_iters as usize,
        review_runs: runtime.stats.reviewer_runs_completed as usize,
        changes_detected: runtime.stats.changes_detected as usize,
        isolation_mode: config.isolation_mode,
        verbose: config.verbosity.is_verbose(),
        review_summary: None,
    };
    print_final_summary(colors, &summary, logger);

    if config.features.checkpoint_enabled {
        let result = if let Some(ws) = workspace {
            clear_checkpoint_with_workspace(ws)
        } else {
            clear_checkpoint()
        };
        if let Err(err) = result {
            logger.warn(&format!("Failed to clear checkpoint: {err}"));
        }
    }

    // Note: PROMPT.md write permissions are now restored via the reducer's
    // Effect::RestorePromptPermissions during the Finalizing phase.
    // This ensures the operation goes through the effect system for testability.

    agent_phase_guard.disarm();
}
