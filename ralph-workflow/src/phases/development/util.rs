//! Utility functions for development phase.
//!
//! Helper functions for verification, fast checks, and result types.

use std::process::Command;

use crate::phases::context::PhaseContext;

/// Result of the development phase.
pub struct DevelopmentResult {
    /// Whether any errors occurred during the phase.
    pub had_errors: bool,
}

/// Run fast check command.
pub fn run_fast_check(
    ctx: &PhaseContext<'_>,
    fast_cmd: &str,
    iteration: u32,
) -> anyhow::Result<()> {
    let argv = crate::common::split_command(fast_cmd)
        .map_err(|e| anyhow::anyhow!("FAST_CHECK_CMD parse error (iteration {iteration}): {e}"))?;
    if argv.is_empty() {
        ctx.logger
            .warn("FAST_CHECK_CMD is empty; skipping fast check");
        return Ok(());
    }

    let display_cmd = crate::common::format_argv_for_log(&argv);
    ctx.logger.info(&format!(
        "Running fast check: {}{}{}",
        ctx.colors.dim(),
        display_cmd,
        ctx.colors.reset()
    ));

    let Some((program, cmd_args)) = argv.split_first() else {
        ctx.logger
            .warn("FAST_CHECK_CMD is empty after parsing; skipping fast check");
        return Ok(());
    };
    let status = Command::new(program).args(cmd_args).status()?;

    if status.success() {
        ctx.logger.success("Fast check passed");
    } else {
        ctx.logger.warn("Fast check had issues (non-blocking)");
    }

    Ok(())
}
