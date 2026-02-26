//! Baseline display handler.
//!
//! Handles the --show-baseline CLI flag to display the current
//! start commit and review baseline state.

use std::io;

use crate::git_helpers::{get_current_head_oid, get_review_baseline_info, load_review_baseline};
use crate::git_helpers::{load_start_point, ReviewBaseline};

/// Handle the --show-baseline flag.
///
/// Displays information about the current start commit and review baseline.
///
/// # Errors
///
/// Returns error if the operation fails.
pub fn handle_show_baseline() -> io::Result<()> {
    println!("╺━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");
    println!("RALPH BASELINE STATE\n");

    // Show start commit state
    println!("Start Commit (.agent/start_commit):");
    match load_start_point() {
        Ok(crate::git_helpers::StartPoint::Commit(oid)) => {
            println!("  Commit: {oid}");
            print_commit_info(&oid.to_string());
        }
        Ok(crate::git_helpers::StartPoint::EmptyRepo) => {
            println!("  State: Empty repository (no commits yet)");
        }
        Err(e) => {
            println!("  Error: {e}");
        }
    }

    println!();

    // Show review baseline state
    println!("Review Baseline (.agent/review_baseline.txt):");
    match load_review_baseline() {
        Ok(ReviewBaseline::Commit(oid)) => {
            println!("  Commit: {oid}");
            print_commit_info(&oid.to_string());
        }
        Ok(ReviewBaseline::NotSet) => {
            println!("  State: Not set (using start commit for diff)");
        }
        Err(e) => {
            println!("  Error: {e}");
        }
    }

    println!();

    // Show baseline info (commits since baseline, stale status)
    match get_review_baseline_info() {
        Ok((baseline_oid, commits_since, is_stale)) => {
            if let Some(oid) = baseline_oid {
                println!("Baseline Analysis:");
                println!("  Commits since baseline: {commits_since}");
                if is_stale {
                    println!(
                        "  Status: STALE (>10 commits behind)\n           \
                          Consider running: ralph --reset-start-commit"
                    );
                } else {
                    println!("  Status: Current (within 10 commits)");
                }

                // Show current HEAD for comparison
                if let Ok(head) = get_current_head_oid() {
                    println!();
                    println!("Current HEAD: {head}");
                    if head != oid {
                        println!("  Difference: HEAD is {commits_since} commits ahead of baseline");
                    }
                }
            } else {
                println!("Baseline Analysis:");
                println!("  No review baseline set - using start commit");
            }
        }
        Err(e) => {
            println!("Could not analyze baseline: {e}");
        }
    }

    println!("\n╺━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━\n");

    Ok(())
}

/// Print information about a commit.
fn print_commit_info(oid: &str) {
    if let Ok(repo) = git2::Repository::discover(".") {
        if let Ok(parsed_oid) = git2::Oid::from_str(oid) {
            if let Ok(commit) = repo.find_commit(parsed_oid) {
                // Get short ID
                let short_id = commit
                    .as_object()
                    .short_id()
                    .ok()
                    .and_then(|buf| buf.as_str().map(std::string::ToString::to_string))
                    .unwrap_or_else(|| {
                        let len = 8.min(oid.len());
                        oid[..len].to_string()
                    });

                println!("  Short ID: {short_id}");

                // Get author info
                let author = commit.author();
                let name = author.name().unwrap_or("<unknown>");
                let when = author.when();
                println!("  Author: {name}");
                println!("  Time: {} seconds since epoch", when.seconds());

                // Get commit summary
                let summary = commit.summary().unwrap_or("<no message>");
                // Truncate long summaries
                let summary = if summary.len() > 60 {
                    format!("{}...", &summary[..57.min(summary.len())])
                } else {
                    summary.to_string()
                };
                println!("  Summary: {summary}");
            } else {
                println!("  Warning: Commit not found in repository");
                println!(
                    "  The OID may reference a deleted commit or be from a different repository"
                );
            }
        } else {
            println!("  Warning: Invalid OID format");
        }
    }
}
