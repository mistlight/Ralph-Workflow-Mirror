#![deny(unsafe_code)]
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
mod colors;
mod config;
mod files;
mod git_helpers;
mod guidelines;
mod json_parser;
mod language_detector;
mod logger;
mod output;
mod phases;
mod pipeline;
mod platform;
mod prompts;
mod review_metrics;
#[cfg(test)]
mod test_utils;
mod timer;
mod utils;

use crate::cli::Args;
use crate::git_helpers::cleanup_agent_phase_silent;
use clap::Parser;

fn main() -> anyhow::Result<()> {
    // Set up Ctrl+C handler for cleanup on unexpected exit
    ctrlc::set_handler(move || {
        eprintln!("\n✋ Interrupted! Cleaning up generated files...");
        cleanup_agent_phase_silent();
        std::process::exit(130); // Standard exit code for SIGINT
    })
    .ok(); // Ignore errors if handler can't be set (e.g., nested handlers)

    app::run(Args::parse())
}
