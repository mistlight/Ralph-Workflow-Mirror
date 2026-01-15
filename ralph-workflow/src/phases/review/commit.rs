//! Commit handling during review phase.
//!
//! This module contains the logic for creating commits when changes are detected
//! during review cycles.

use crate::git_helpers::{git_diff, CommitResultFallback};
use crate::phases::commit::commit_with_generated_message;
use crate::phases::context::PhaseContext;
use crate::phases::get_primary_commit_agent;

/// Handle commit creation when repository is modified during review.
///
/// This function is called both after normal review-fix cycles and when
/// external changes are detected during skipped cycles.
pub fn handle_review_commit(ctx: &mut PhaseContext<'_>) -> Result<(), anyhow::Error> {
    // Get the primary commit agent
    let commit_agent = get_primary_commit_agent(ctx);
    if let Some(agent) = commit_agent {
        ctx.logger.info(&format!(
            "Creating commit with auto-generated message (agent: {agent})..."
        ));

        // Get the diff for commit message generation
        let diff = match git_diff() {
            Ok(d) => d,
            Err(e) => {
                ctx.logger
                    .error(&format!("Failed to get diff for commit: {e}"));
                return Err(anyhow::anyhow!(e));
            }
        };

        // Get git identity from config
        let git_name = ctx.config.git_user_name.as_deref();
        let git_email = ctx.config.git_user_email.as_deref();

        match commit_with_generated_message(&diff, &agent, git_name, git_email, ctx) {
            CommitResultFallback::Success(oid) => {
                ctx.logger
                    .success(&format!("Commit created successfully: {oid}"));
                ctx.stats.commits_created += 1;
            }
            CommitResultFallback::NoChanges => {
                // No meaningful changes to commit
                ctx.logger.info("No commit created (no meaningful changes)");
            }
            CommitResultFallback::Failed(err) => {
                // Actual git operation failed - this is critical
                ctx.logger.error(&format!(
                    "Failed to create commit (git operation failed): {err}"
                ));
                // Don't continue - this is a real error that needs attention
                return Err(anyhow::anyhow!(err));
            }
        }
    } else {
        ctx.logger.warn("Unable to get commit agent for commit");
    }

    Ok(())
}
