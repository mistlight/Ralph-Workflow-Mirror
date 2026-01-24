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

mod agents;
mod app;
mod banner;
mod checkpoint;
mod cli;
mod common;
mod config;
mod diagnostics;
mod files;
mod git_helpers;
mod guidelines;
mod interrupt;
mod json_parser;
mod language_detector;
mod logger;
mod phases;
mod pipeline;
mod platform;
mod prompts;
mod reducer;
mod review_metrics;
mod templates;

use crate::cli::Args;
use clap::Parser;

fn main() -> anyhow::Result<()> {
    // Set up Ctrl+C handler for graceful checkpoint save on interrupt
    crate::interrupt::setup_interrupt_handler();

    // Create real process executor for production use
    let executor = crate::executor::RealProcessExecutor::new();
    app::run(Args::parse(), &executor)
}
