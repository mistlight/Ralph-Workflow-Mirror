// DO NOT CHANGE THESE CLIPPY SETTINGS, YOU MUST REFACTOR INSTEAD, EVEN IF IT TAKES YOU 100 YEARS
#![deny(
    warnings,
    unsafe_code,
    clippy::all,
    clippy::pedantic,
    clippy::nursery,
    clippy::cargo
)]
//! Ralph: PROMPT-driven agent loop for git repos
//!
//! Runs:
//! - Developer agent: iterative progress against PROMPT.md
//! - Reviewer agent: review → fix → review passes
//! - Optional fast/full checks
//! - Final `git add -A` + `git commit -m <msg>`

use clap::Parser;
use ralph_workflow::app;
use ralph_workflow::cli::Args;
use ralph_workflow::interrupt;
use ralph_workflow::RealProcessExecutor;

fn main() -> anyhow::Result<()> {
    // Set up Ctrl+C handler for graceful checkpoint save on interrupt
    interrupt::setup_interrupt_handler();

    // Create real process executor for production use
    let executor = std::sync::Arc::new(RealProcessExecutor::new());
    let result = app::run(Args::parse(), executor);

    // If the pipeline requested a SIGINT exit code, exit after cleanup has completed.
    if interrupt::take_exit_130_after_run() {
        std::process::exit(130);
    }

    result
}
