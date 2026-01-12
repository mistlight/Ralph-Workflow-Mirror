//! Review phase execution.
//!
//! This module handles the review and fix phase of the Ralph pipeline. It runs
//! a configurable number of review-fix cycles, where each cycle:
//! 1. Reviews the code and creates ISSUES.md
//! 2. Fixes the issues found
//! 3. Cleans up ISSUES.md (in isolation mode)

use crate::agents::AgentRole;
use crate::config::ReviewDepth;
use crate::guidelines::ReviewGuidelines;
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{
    prompt_comprehensive_review, prompt_detailed_review_without_guidelines, prompt_for_agent,
    prompt_incremental_review, prompt_security_focused_review, Action, ContextLevel, Role,
};
use crate::review_metrics::ReviewMetrics;
use crate::utils::{
    clean_context_for_reviewer, delete_issues_file_for_isolation, print_progress, save_checkpoint,
    update_status, PipelineCheckpoint, PipelinePhase,
};

use super::context::PhaseContext;

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

    // Review-Fix iterations
    for j in start_pass..=ctx.config.reviewer_reviews {
        // Save checkpoint at start of each iteration
        if ctx.config.checkpoint_enabled {
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

        // REVIEW PASS
        update_status("Reviewing code", ctx.config.isolation_mode)?;
        let (review_label, review_prompt) =
            build_review_prompt(ctx, reviewer_context, ctx.review_guidelines);

        let _ = {
            let mut runtime = PipelineRuntime {
                timer: ctx.timer,
                logger: ctx.logger,
                colors: ctx.colors,
                config: ctx.config,
            };
            run_with_fallback(
                AgentRole::Reviewer,
                &format!("{} #{}", review_label, j),
                &review_prompt,
                &format!(".agent/logs/reviewer_review_{}", j),
                &mut runtime,
                ctx.registry,
                ctx.reviewer_agent,
            )
        };
        ctx.stats.reviewer_runs_completed += 1;

        // EARLY EXIT CHECK: If review found no issues, stop
        if let Ok(metrics) = ReviewMetrics::from_issues_file() {
            if metrics.no_issues_declared && metrics.total_issues == 0 {
                ctx.logger.success(&format!(
                    "No issues found after cycle {} - stopping early",
                    j
                ));
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
                &format!("fix #{}", j),
                &fix_prompt,
                &format!(".agent/logs/reviewer_fix_{}", j),
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
    }

    Ok(ReviewResult {
        completed_early: false,
    })
}

/// Generate commit message using the reviewer agent.
///
/// # Arguments
///
/// * `ctx` - The phase context containing shared state
///
/// # Returns
///
/// Returns `Ok(())` on success, or an error if the generation fails.
pub fn generate_commit_message(ctx: &mut PhaseContext<'_>) -> anyhow::Result<()> {
    let reviewer_context = ContextLevel::from(ctx.config.reviewer_context);

    // Save checkpoint at start of commit message phase (if enabled)
    if ctx.config.checkpoint_enabled {
        let _ = save_checkpoint(&PipelineCheckpoint::new(
            PipelinePhase::CommitMessage,
            ctx.config.developer_iters,
            ctx.config.developer_iters,
            ctx.config.reviewer_reviews,
            ctx.config.reviewer_reviews,
            ctx.developer_agent,
            ctx.reviewer_agent,
        ));
    }

    ctx.logger.subheader("Generating Commit Message");
    update_status("Generating commit message", ctx.config.isolation_mode)?;

    let commit_msg_prompt = prompt_for_agent(
        Role::Reviewer,
        Action::GenerateCommitMessage,
        reviewer_context,
        None,
        None,
        None, // No guidelines needed for commit message generation
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
            "generate commit msg",
            &commit_msg_prompt,
            ".agent/logs/commit_message",
            &mut runtime,
            ctx.registry,
            ctx.reviewer_agent,
        )
    };
    ctx.stats.reviewer_runs_completed += 1;

    Ok(())
}

/// Build the review prompt based on configuration.
fn build_review_prompt(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    guidelines: Option<&ReviewGuidelines>,
) -> (String, String) {
    match ctx.config.review_depth {
        ReviewDepth::Security => {
            if let Some(g) = guidelines {
                ctx.logger
                    .info("Using security-focused review with language-specific checks");
                (
                    "review (security)".to_string(),
                    prompt_security_focused_review(reviewer_context, g),
                )
            } else {
                ctx.logger.info("Using security-focused review");
                (
                    "review (security)".to_string(),
                    prompt_security_focused_review(reviewer_context, &ReviewGuidelines::default()),
                )
            }
        }
        ReviewDepth::Incremental => {
            ctx.logger
                .info("Using incremental review (changed files only)");
            (
                "review (incremental)".to_string(),
                prompt_incremental_review(reviewer_context),
            )
        }
        ReviewDepth::Comprehensive => {
            if let Some(g) = guidelines {
                ctx.logger
                    .info("Using comprehensive review with language-specific checks");
                (
                    "review (comprehensive)".to_string(),
                    prompt_comprehensive_review(reviewer_context, g),
                )
            } else {
                ctx.logger.info("Using comprehensive review");
                (
                    "review (comprehensive)".to_string(),
                    prompt_comprehensive_review(reviewer_context, &ReviewGuidelines::default()),
                )
            }
        }
        ReviewDepth::Standard => {
            if let Some(g) = guidelines {
                ctx.logger
                    .info("Using standard review with language-specific checks");
                (
                    "review (standard)".to_string(),
                    prompt_for_agent(
                        Role::Reviewer,
                        Action::Review,
                        reviewer_context,
                        None,
                        None,
                        Some(g),
                    ),
                )
            } else {
                ctx.logger
                    .info("Using detailed review without stack-specific checks");
                (
                    "review (standard)".to_string(),
                    prompt_detailed_review_without_guidelines(reviewer_context),
                )
            }
        }
    }
}
