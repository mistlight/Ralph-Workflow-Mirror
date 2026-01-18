//! Pipeline finalization and cleanup.
//!
//! This module handles the final phase of the pipeline including cleanup,
//! final summary, and checkpoint clearing.

use crate::banner::{print_final_summary, PipelineSummary};
use crate::checkpoint::clear_checkpoint;
use crate::config::Config;
use crate::files::make_prompt_writable;
use crate::files::protection::monitoring::PromptMonitor;
use crate::logger::Colors;
use crate::logger::Logger;
use crate::pipeline::Timer;
use crate::pipeline::{AgentPhaseGuard, Stats};

/// Finalizes the pipeline: cleans up and prints summary.
///
/// Commits now happen per-iteration during development and per-cycle during review,
/// so this function only handles cleanup and final summary.
pub fn finalize_pipeline(
    agent_phase_guard: &mut AgentPhaseGuard,
    logger: &Logger,
    colors: Colors,
    config: &Config,
    timer: &Timer,
    stats: &Stats,
    prompt_monitor: Option<PromptMonitor>,
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
        total_time: timer.elapsed_formatted(),
        dev_runs_completed: stats.developer_runs_completed as usize,
        dev_runs_total: config.developer_iters as usize,
        review_runs: stats.reviewer_runs_completed as usize,
        changes_detected: stats.changes_detected as usize,
        isolation_mode: config.isolation_mode,
        verbose: config.verbosity.is_verbose(),
        review_summary: None,
    };
    print_final_summary(colors, &summary, logger);

    if config.features.checkpoint_enabled {
        if let Err(err) = clear_checkpoint() {
            logger.warn(&format!("Failed to clear checkpoint: {err}"));
        }
    }

    // Restore PROMPT.md write permissions so users can edit it normally
    // This is important to ensure users can edit PROMPT.md after pipeline completion
    if let Some(warning) = make_prompt_writable() {
        // Make this visible even if stdout is redirected or logger filtering is enabled.
        eprintln!("{warning}");
        eprintln!("If PROMPT.md is still read-only, run: chmod u+w PROMPT.md");
        logger.warn(&warning);
    }

    agent_phase_guard.disarm();
}
