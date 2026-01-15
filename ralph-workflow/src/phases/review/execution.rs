//! Main review phase execution logic.
//!
//! This module contains the core loop for the review phase, handling
//! each review-fix cycle.

use crate::agents::AgentRole;
use crate::checkpoint::{save_checkpoint, PipelineCheckpoint, PipelinePhase};
use crate::files::extract_issues;
use crate::files::{clean_context_for_reviewer, delete_issues_file_for_isolation, update_status};
use crate::git_helpers::git_snapshot;
use crate::logger::print_progress;
use crate::phases::context::PhaseContext;
use crate::phases::integrity::ensure_prompt_integrity;
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{prompt_for_agent, Action, ContextLevel, Role};
use crate::review_metrics::ReviewMetrics;

use super::commit::handle_review_commit;
use super::prompt::{build_review_prompt, should_use_universal_prompt};
use super::validation::{
    post_flight_review_check, pre_flight_review_check, PostflightResult, PreflightResult,
};

use std::fs;
use std::path::Path;

/// Result of the review phase.
pub struct ReviewResult {
    /// Whether the review completed early due to no issues found.
    pub completed_early: bool,
}

/// Run the review and fix phase.
///
/// This phase runs `reviewer_reviews` review-fix cycles. Each cycle:
/// 1. Runs a code review (creates ISSUES.md)
/// 2. Fixes the identified issues
/// 3. Cleans up ISSUES.md in isolation mode
///
/// The phase may exit early if a review finds no issues.
///
/// # Arguments
///
/// * `ctx` - The phase context containing shared state
/// * `start_pass` - The review pass to start from (for resume support)
///
/// # Returns
///
/// Returns `Ok(ReviewResult)` on success, or an error if a critical failure occurs.
#[allow(clippy::too_many_lines)]
pub fn run_review_phase(
    ctx: &mut PhaseContext<'_>,
    start_pass: u32,
) -> anyhow::Result<ReviewResult> {
    let reviewer_context = ContextLevel::from(ctx.config.reviewer_context);

    // Clean context for reviewer if using minimal context
    if reviewer_context == ContextLevel::Minimal {
        clean_context_for_reviewer(ctx.logger, ctx.config.isolation_mode)?;
    }

    // Skip if no review cycles configured
    if ctx.config.reviewer_reviews == 0 {
        ctx.logger
            .info("Skipping review phase (reviewer_reviews=0)");
        return Ok(ReviewResult {
            completed_early: false,
        });
    }

    ctx.logger.info(&format!(
        "Running {}{}{} review → fix cycles ({})",
        ctx.colors.bold(),
        ctx.config.reviewer_reviews,
        ctx.colors.reset(),
        ctx.reviewer_agent
    ));

    // Track git snapshots for detecting changes during review
    let mut prev_snap = git_snapshot()?;
    // Track how many review cycles were skipped due to diff retrieval failures
    let mut skipped_cycles = 0;

    // Review-Fix iterations
    for j in start_pass..=ctx.config.reviewer_reviews {
        // Save checkpoint at start of each iteration
        if ctx.config.features.checkpoint_enabled {
            let _ = save_checkpoint(&PipelineCheckpoint::new(
                PipelinePhase::Review,
                ctx.config.developer_iters,
                ctx.config.developer_iters,
                j,
                ctx.config.reviewer_reviews,
                ctx.developer_agent,
                ctx.reviewer_agent,
            ));
        }

        ctx.logger.subheader(&format!(
            "Review-Fix Cycle {} of {}",
            j, ctx.config.reviewer_reviews
        ));
        print_progress(j, ctx.config.reviewer_reviews, "Review-Fix cycles");

        // PRE-FLIGHT VALIDATION: Check environment before running review
        match pre_flight_review_check(
            ctx.logger,
            j,
            ctx.reviewer_agent,
            ctx.config.reviewer_model.as_deref(),
        ) {
            PreflightResult::Ok => {
                // All checks passed, proceed
            }
            PreflightResult::Warning(msg) => {
                ctx.logger.warn(&msg);
                // Continue anyway
            }
            PreflightResult::Error(msg) => {
                ctx.logger.error(&format!("Pre-flight check failed: {msg}"));
                return Err(anyhow::anyhow!(
                    "Review pre-flight validation failed: {msg}"
                ));
            }
        }

        // REVIEW PASS
        update_status("Reviewing code", ctx.config.isolation_mode)?;
        let (review_label, review_prompt) =
            build_review_prompt(ctx, reviewer_context, ctx.review_guidelines);

        // Check if the review prompt is empty (e.g., due to diff retrieval failure)
        // If so, skip the review and fix passes but still check for git changes
        if review_prompt.is_empty() {
            ctx.logger
                .warn(&format!("Skipping review cycle {j} due to: {review_label}"));
            skipped_cycles += 1;

            // Even though review/fix are skipped, we still check for external git changes.
            // This ensures that manual edits or external tool changes are committed,
            // maintaining the invariant that every iteration with changes gets a commit.
            // The check here is independent of whether review ran - it's about detecting
            // any modifications to the working directory since the last snapshot.
            let snap = git_snapshot()?;
            if snap != prev_snap {
                ctx.logger
                    .success("Repository modified (external changes detected)");
                ctx.stats.changes_detected += 1;
                handle_review_commit(ctx)?;
            }
            prev_snap = snap;
            continue;
        }

        // Log the specific review prompt variant for debugging (when verbose)
        if ctx.config.verbosity.is_debug() {
            ctx.logger.info(&format!(
                "Review prompt variant: '{}' for agent '{}'",
                review_label, ctx.reviewer_agent
            ));
            ctx.logger.info(&format!(
                "Review prompt length: {} characters",
                review_prompt.len()
            ));
        }

        let issues_path = Path::new(".agent/ISSUES.md");
        let log_dir = format!(".agent/logs/reviewer_review_{j}");

        let _ = {
            let mut runtime = PipelineRuntime {
                timer: ctx.timer,
                logger: ctx.logger,
                colors: ctx.colors,
                config: ctx.config,
            };
            run_with_fallback(
                AgentRole::Reviewer,
                &format!("{review_label} #{j}"),
                &review_prompt,
                &log_dir,
                &mut runtime,
                ctx.registry,
                ctx.reviewer_agent,
            )
        };
        ctx.stats.reviewer_runs_completed += 1;

        // ORCHESTRATOR-CONTROLLED FILE I/O:
        // Prefer extraction from JSON log (orchestrator write), but fall back to
        // agent-written file if extraction fails (legacy/test compatibility).

        // Ensure .agent directory exists
        if let Some(parent) = issues_path.parent() {
            fs::create_dir_all(parent)?;
        }

        let extraction = extract_issues(Path::new(&log_dir))?;

        if let Some(content) = &extraction.raw_content {
            // Extraction succeeded - orchestrator writes the file
            fs::write(issues_path, content)?;

            if extraction.is_valid {
                ctx.logger
                    .success("Issues extracted from agent output (JSON)");
            } else {
                ctx.logger.warn(&format!(
                    "Issues written but validation failed: {}",
                    extraction.validation_warning.clone().unwrap_or_default()
                ));
            }
        } else {
            // JSON extraction failed - log for debugging
            ctx.logger
                .info("No JSON result event found in reviewer logs");

            // Check if agent wrote the file directly (legacy fallback)
            let agent_wrote_file = issues_path
                .exists()
                .then(|| fs::read_to_string(issues_path).ok())
                .flatten()
                .is_some_and(|s| !s.trim().is_empty());

            if agent_wrote_file {
                ctx.logger
                    .info("Using agent-written ISSUES.md (legacy mode)");
            } else {
                // No content from extraction or agent - write "no issues" marker
                // This is not an error for review (unlike planning) since having no issues is valid
                let no_issues_marker = "# Issues\n\nNo issues identified by reviewer.\n";
                fs::write(issues_path, no_issues_marker)?;
                ctx.logger
                    .info("No issues content found in agent output - assuming no issues");
            }
        }

        // POST-FLIGHT VALIDATION: Check review output after agent completes
        let postflight_result = post_flight_review_check(ctx.logger, j);
        match postflight_result {
            PostflightResult::Valid => {
                // ISSUES.md found and valid, continue
            }
            PostflightResult::Missing(msg) => {
                ctx.logger.warn(&format!(
                    "Post-flight check: {msg}. Proceeding with fix pass anyway."
                ));
                // If using a problematic agent, suggest alternatives
                if should_use_universal_prompt(
                    ctx.reviewer_agent,
                    ctx.config.reviewer_model.as_deref(),
                    ctx.config.features.force_universal_prompt,
                ) {
                    ctx.logger.info(&format!(
                        "{}Tip:{} Review with this agent may be unreliable. Consider:",
                        ctx.colors.bold(),
                        ctx.colors.reset()
                    ));
                    ctx.logger
                        .info("  1. Use Claude/Codex as reviewer: ralph --reviewer-agent codex");
                    ctx.logger
                        .info("  2. Try generic parser: ralph --reviewer-json-parser generic");
                    ctx.logger
                        .info("  3. Skip review: RALPH_REVIEWER_REVIEWS=0 ralph");
                }
                // Continue to fix pass - the agent may still have useful context
            }
            PostflightResult::Malformed(msg) => {
                ctx.logger.warn(&format!(
                    "Post-flight check: {msg}. The fix pass may not work correctly."
                ));
                // Suggest trying with generic parser as fallback
                ctx.logger.info(&format!(
                    "{}Tip:{} Try with generic parser: {}RALPH_REVIEWER_JSON_PARSER=generic ralph{}",
                    ctx.colors.bold(),
                    ctx.colors.reset(),
                    ctx.colors.bold(),
                    ctx.colors.reset()
                ));
                // Continue but warn that fix may be affected
            }
        }

        // EARLY EXIT CHECK: If review found no issues, stop
        // Orchestrator always writes ISSUES.md, so we check its content
        if let Ok(metrics) = ReviewMetrics::from_issues_file() {
            if metrics.no_issues_declared && metrics.total_issues == 0 {
                ctx.logger
                    .success(&format!("No issues found after cycle {j} - stopping early"));
                // Clean up ISSUES.md before early exit in isolation mode
                if ctx.config.isolation_mode {
                    delete_issues_file_for_isolation(ctx.logger)?;
                }
                return Ok(ReviewResult {
                    completed_early: true,
                });
            }
        }

        // FIX PASS
        update_status("Applying fixes", ctx.config.isolation_mode)?;
        let fix_prompt = prompt_for_agent(
            Role::Reviewer,
            Action::Fix,
            reviewer_context,
            None,
            None,
            None,
            None,
        );

        let _ = {
            let mut runtime = PipelineRuntime {
                timer: ctx.timer,
                logger: ctx.logger,
                colors: ctx.colors,
                config: ctx.config,
            };
            run_with_fallback(
                AgentRole::Reviewer,
                &format!("fix #{j}"),
                &fix_prompt,
                &format!(".agent/logs/reviewer_fix_{j}"),
                &mut runtime,
                ctx.registry,
                ctx.reviewer_agent,
            )
        };
        ctx.stats.reviewer_runs_completed += 1;

        // Clean up ISSUES.md after each fix cycle in isolation mode
        if ctx.config.isolation_mode {
            delete_issues_file_for_isolation(ctx.logger)?;
        }

        // Periodic restoration check - ensure PROMPT.md still exists
        // This catches agent deletions and restores from backup
        ensure_prompt_integrity(ctx.logger, "review", j);

        // Check for changes and create commit if modified
        let snap = git_snapshot()?;
        if snap != prev_snap {
            ctx.logger.success("Repository modified during fix pass");
            ctx.stats.changes_detected += 1;
            handle_review_commit(ctx)?;
        }
        prev_snap = snap;
    }

    // Provide feedback if any review cycles were skipped
    if skipped_cycles > 0 {
        let total_cycles = ctx.config.reviewer_reviews;
        ctx.logger.warn(&format!(
            "{skipped_cycles} of {total_cycles} review cycle(s) were skipped due to diff retrieval failures."
        ));
        ctx.logger.info(
            "This may indicate a git repository issue or that no changes have been made yet.",
        );
        if skipped_cycles == total_cycles {
            ctx.logger.warn(
                "No review cycles were completed. Consider checking your git repository state.",
            );
        }
    }

    Ok(ReviewResult {
        completed_early: false,
    })
}
