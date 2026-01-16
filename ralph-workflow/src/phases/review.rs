//! Review phase execution.
//!
//! This module handles the review and fix phase of the Ralph pipeline. It runs
//! a configurable number of review-fix cycles, where each cycle:
//! 1. Reviews the code and creates ISSUES.md
//! 2. Fixes the issues found
//! 3. Cleans up ISSUES.md (in isolation mode)
//!
//! # Module Structure
//!
//! - [`prompt`] - Review prompt building logic
//! - [`validation`] - Pre-flight and post-flight validation checks

use crate::agents::AgentRole;
use crate::checkpoint::{save_checkpoint, PipelineCheckpoint, PipelinePhase};
use crate::files::extract_issues;
use crate::files::{clean_context_for_reviewer, delete_issues_file_for_isolation, update_status};
use crate::git_helpers::{git_snapshot, CommitResultFallback};
use crate::logger::print_progress;
use crate::phases::commit::commit_with_generated_message;
use crate::phases::get_primary_commit_agent;
use crate::phases::integrity::ensure_prompt_integrity;
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{prompt_for_agent, Action, ContextLevel, PromptConfig, Role};
use crate::review_metrics::ReviewMetrics;

mod prompt;
pub use prompt::{build_review_prompt, should_use_universal_prompt};

mod validation;
pub use validation::{
    post_flight_review_check, pre_flight_review_check, PostflightResult, PreflightResult,
};

use super::context::PhaseContext;
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

            // Check for external git changes and commit if found
            prev_snap = handle_skipped_cycle(ctx, &prev_snap)?;
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

        // Run review pass
        let review_result = run_review_pass(ctx, j, &review_label, &review_prompt)?;

        // Check for early exit (no issues found)
        if review_result.early_exit {
            return Ok(ReviewResult {
                completed_early: true,
            });
        }

        // Run fix pass
        run_fix_pass(ctx, j, reviewer_context)?;

        // Check for changes and create commit if modified
        prev_snap = handle_post_fix_commit(ctx, &prev_snap)?;
    }

    // Provide feedback if any review cycles were skipped
    log_skipped_cycles_feedback(ctx, skipped_cycles);

    Ok(ReviewResult {
        completed_early: false,
    })
}

/// Handle a skipped review cycle by checking for external git changes.
fn handle_skipped_cycle(ctx: &mut PhaseContext<'_>, prev_snap: &str) -> anyhow::Result<String> {
    let snap = git_snapshot()?;
    if snap != prev_snap {
        ctx.logger
            .success("Repository modified (external changes detected)");
        ctx.stats.changes_detected += 1;

        // Get the primary commit agent
        let commit_agent = get_primary_commit_agent(ctx);
        if let Some(agent) = commit_agent {
            ctx.logger.info(&format!(
                "Creating commit with auto-generated message (agent: {agent})..."
            ));

            // Get the diff for commit message generation
            let diff = match crate::git_helpers::git_diff() {
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
                    ctx.logger.info("No commit created (no meaningful changes)");
                }
                CommitResultFallback::Failed(err) => {
                    ctx.logger.error(&format!("Failed to create commit: {err}"));
                    return Err(anyhow::anyhow!(err));
                }
            }
        } else {
            ctx.logger.warn("Unable to get commit agent for commit");
        }
    }
    Ok(snap)
}

/// Result of running a review pass.
struct ReviewPassResult {
    /// Whether the review found no issues and should exit early.
    early_exit: bool,
}

/// Run the review pass for a single cycle.
fn run_review_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    review_label: &str,
    review_prompt: &str,
) -> anyhow::Result<ReviewPassResult> {
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
            review_prompt,
            &log_dir,
            &mut runtime,
            ctx.registry,
            ctx.reviewer_agent,
        )
    };
    ctx.stats.reviewer_runs_completed += 1;

    // ORCHESTRATOR-CONTROLLED FILE I/O:
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
            let no_issues_marker = "# Issues\n\nNo issues identified by reviewer.\n";
            fs::write(issues_path, no_issues_marker)?;
            ctx.logger
                .info("No issues content found in agent output - assuming no issues");
        }
    }

    // POST-FLIGHT VALIDATION: Check review output after agent completes
    handle_postflight_validation(ctx, j);

    // EARLY EXIT CHECK: If review found no issues, stop
    if let Ok(metrics) = ReviewMetrics::from_issues_file() {
        if metrics.no_issues_declared && metrics.total_issues == 0 {
            ctx.logger
                .success(&format!("No issues found after cycle {j} - stopping early"));
            // Clean up ISSUES.md before early exit in isolation mode
            if ctx.config.isolation_mode {
                delete_issues_file_for_isolation(ctx.logger)?;
            }
            return Ok(ReviewPassResult { early_exit: true });
        }
    }

    Ok(ReviewPassResult { early_exit: false })
}

/// Handle post-flight validation after a review pass.
fn handle_postflight_validation(ctx: &PhaseContext<'_>, j: u32) {
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
        }
        PostflightResult::Malformed(msg) => {
            ctx.logger.warn(&format!(
                "Post-flight check: {msg}. The fix pass may not work correctly."
            ));
            ctx.logger.info(&format!(
                "{}Tip:{} Try with generic parser: {}RALPH_REVIEWER_JSON_PARSER=generic ralph{}",
                ctx.colors.bold(),
                ctx.colors.reset(),
                ctx.colors.bold(),
                ctx.colors.reset()
            ));
        }
    }
}

/// Run the fix pass for a single cycle.
fn run_fix_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    reviewer_context: ContextLevel,
) -> anyhow::Result<()> {
    update_status("Applying fixes", ctx.config.isolation_mode)?;

    // Read PROMPT.md and PLAN.md for context
    let prompt_content = fs::read_to_string("PROMPT.md").unwrap_or_default();
    let plan_content = fs::read_to_string(".agent/PLAN.md").unwrap_or_default();

    let fix_prompt = prompt_for_agent(
        Role::Reviewer,
        Action::Fix,
        reviewer_context,
        PromptConfig::new().with_prompt_and_plan(prompt_content, plan_content),
    );

    // Log the fix prompt details for debugging (when verbose)
    if ctx.config.verbosity.is_debug() {
        ctx.logger.info(&format!(
            "Fix prompt length: {} characters",
            fix_prompt.len()
        ));
        ctx.logger.info(&format!(
            "Fix prompt contains constraints: {}",
            fix_prompt.contains("MUST NOT") && fix_prompt.contains("CRITICAL CONSTRAINTS")
        ));
    }

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
    ensure_prompt_integrity(ctx.logger, "review", j);

    Ok(())
}

/// Handle post-fix commit creation.
fn handle_post_fix_commit(ctx: &mut PhaseContext<'_>, prev_snap: &str) -> anyhow::Result<String> {
    let snap = git_snapshot()?;
    if snap != prev_snap {
        ctx.logger.success("Repository modified during fix pass");
        ctx.stats.changes_detected += 1;

        // Get the primary commit agent
        let commit_agent = get_primary_commit_agent(ctx);
        if let Some(agent) = commit_agent {
            ctx.logger.info(&format!(
                "Creating commit with auto-generated message (agent: {agent})..."
            ));

            // Get the diff for commit message generation
            let diff = match crate::git_helpers::git_diff() {
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
                    ctx.logger.info("No commit created (no meaningful changes)");
                }
                CommitResultFallback::Failed(err) => {
                    ctx.logger.error(&format!(
                        "Failed to create commit (git operation failed): {err}"
                    ));
                    return Err(anyhow::anyhow!(err));
                }
            }
        } else {
            ctx.logger
                .warn("Unable to get commit agent chain for commit");
        }
    }
    Ok(snap)
}

/// Log feedback about skipped review cycles.
fn log_skipped_cycles_feedback(ctx: &PhaseContext<'_>, skipped_cycles: u32) {
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
}
