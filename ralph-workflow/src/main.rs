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
mod container;
mod diagnostics;
mod files;
mod git_helpers;
mod guidelines;
mod json_parser;
mod language_detector;
mod logger;
mod phases;
mod pipeline;
mod platform;
mod prompts;
mod review_metrics;
mod templates;

use crate::cli::Args;
use crate::git_helpers::wrapper::cleanup_agent_phase_silent;
use clap::Parser;

fn main() -> anyhow::Result<()> {
    // Set up Ctrl+C handler for cleanup on unexpected exit
    if let Err(e) = ctrlc::set_handler(move || {
        eprintln!("\n✋ Interrupted! Cleaning up generated files...");
        cleanup_agent_phase_silent();
        std::process::exit(130); // Standard exit code for SIGINT
    }) {
        // Log a warning but don't fail - the program can still function without the handler
        eprintln!("Warning: Failed to set Ctrl+C handler: {e}");
        eprintln!("Cleanup on Ctrl+C may not work properly.");
    }

    app::run(Args::parse())
}
