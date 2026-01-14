//! Pipeline finalization and cleanup.
//!
//! This module handles the final phase of the pipeline including cleanup,
//! final summary, and checkpoint clearing.

use crate::banner::print_final_summary;
use crate::checkpoint::clear_checkpoint;
use crate::colors::Colors;
use crate::config::Config;
use crate::files::monitoring::PromptMonitor;
use crate::logger::Logger;
use crate::pipeline::{AgentPhaseGuard, Stats};
use crate::timer::Timer;

/// Finalizes the pipeline: cleans up and prints summary.
///
/// Commits now happen per-iteration during development and per-cycle during review,
/// so this function only handles cleanup and final summary.
pub fn finalize_pipeline(
    agent_phase_guard: &mut AgentPhaseGuard,
    logger: &Logger,
    colors: &Colors,
    config: &Config,
    timer: &Timer,
    stats: &Stats,
    prompt_monitor: Option<PromptMonitor>,
) -> anyhow::Result<()> {
    // Stop the PROMPT.md monitor if it was started
    if let Some(monitor) = prompt_monitor {
        monitor.stop();
    }

    // End agent phase and clean up
    crate::git_helpers::end_agent_phase()?;
    crate::git_helpers::disable_git_wrapper(agent_phase_guard.git_helpers);
    if let Err(err) = crate::git_helpers::uninstall_hooks(logger) {
        logger.warn(&format!("Failed to uninstall Ralph hooks: {}", err));
    }

    // Note: Individual commits were created per-iteration during development
    // and per-cycle during review. The final commit phase has been removed.

    // Final summary
    print_final_summary(colors, config, timer, stats, logger);

    if config.checkpoint_enabled {
        if let Err(err) = clear_checkpoint() {
            logger.warn(&format!("Failed to clear checkpoint: {}", err));
        }
    }

    agent_phase_guard.disarm();
    Ok(())
}
