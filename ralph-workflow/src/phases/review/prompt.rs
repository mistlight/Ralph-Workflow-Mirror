//! Review prompt building logic.
//!
//! This module handles the construction of review prompts based on:
//! - Agent type and model (for compatibility detection)
//! - Review depth configuration
//! - Custom review guidelines

use crate::agents::is_glm_like_agent;
use crate::config::ReviewDepth;
use crate::git_helpers::get_git_diff_from_start;
use crate::guidelines::ReviewGuidelines;
use crate::prompts::{
    prompt_comprehensive_review, prompt_detailed_review_without_guidelines, prompt_for_agent,
    prompt_incremental_review_with_diff, prompt_security_focused_review, prompt_universal_review,
    Action, ContextLevel, Role,
};

use super::super::context::PhaseContext;

/// Check if the reviewer agent should use the universal/simplified prompt.
///
/// Some AI agents have known compatibility issues with complex structured prompts.
/// This function detects those agents and returns true if the universal prompt
/// should be used instead.
///
/// The universal prompt can also be forced via the `RALPH_REVIEWER_UNIVERSAL_PROMPT`
/// environment variable or the `force_universal_prompt` config setting.
pub fn should_use_universal_prompt(agent: &str, model_flag: Option<&str>, force: bool) -> bool {
    // If explicitly forced via config/env, always use universal prompt
    if force {
        return true;
    }

    // Detect GLM, ZhipuAI, and other known-problematic agents, including cases
    // where the model is selected via provider/model flags.
    is_problematic_prompt_target(agent, model_flag)
}

/// Build the review prompt based on configuration and agent type.
pub fn build_review_prompt(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    guidelines: Option<&ReviewGuidelines>,
) -> (String, String) {
    // Check if we should use the universal prompt for this agent
    let use_universal = should_use_universal_prompt(
        ctx.reviewer_agent,
        ctx.config.reviewer_model.as_deref(),
        ctx.config.features.force_universal_prompt,
    );

    if use_universal {
        return build_universal_prompt(ctx, reviewer_context);
    }

    build_review_by_depth(ctx, reviewer_context, guidelines)
}

fn build_universal_prompt(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
) -> (String, String) {
    let reason = if ctx.config.features.force_universal_prompt {
        "forced via config/env"
    } else {
        "better compatibility"
    };
    ctx.logger.info(&format!(
        "Using universal/simplified review prompt for agent '{}' ({})",
        ctx.reviewer_agent, reason
    ));
    (
        "review (universal)".to_string(),
        prompt_universal_review(reviewer_context),
    )
}

fn build_review_by_depth(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    guidelines: Option<&ReviewGuidelines>,
) -> (String, String) {
    match ctx.config.review_depth {
        ReviewDepth::Security => build_security_review(ctx, reviewer_context, guidelines),
        ReviewDepth::Incremental => build_incremental_review(ctx, reviewer_context),
        ReviewDepth::Comprehensive => build_comprehensive_review(ctx, reviewer_context, guidelines),
        ReviewDepth::Standard => build_standard_review(ctx, reviewer_context, guidelines),
    }
}

fn build_security_review(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    guidelines: Option<&ReviewGuidelines>,
) -> (String, String) {
    ctx.logger.info(if guidelines.is_some() {
        "Using security-focused review with language-specific checks"
    } else {
        "Using security-focused review"
    });
    let prompt = guidelines.map_or_else(
        || {
            let default = ReviewGuidelines::default();
            prompt_security_focused_review(reviewer_context, &default)
        },
        |g| prompt_security_focused_review(reviewer_context, g),
    );
    ("review (security)".to_string(), prompt)
}

fn build_incremental_review(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
) -> (String, String) {
    ctx.logger
        .info("Using incremental review (changed files only)");

    // Get the diff from the starting commit to pass directly to the reviewer
    // This keeps agents isolated from git operations
    let Some(diff) = get_incremental_diff(ctx) else {
        return ("review (incremental - skipped)".to_string(), String::new());
    };

    if ctx.config.verbosity.is_debug() {
        ctx.logger
            .info(&format!("Diff size for review: {} bytes", diff.len()));
    }

    (
        "review (incremental)".to_string(),
        prompt_incremental_review_with_diff(reviewer_context, &diff),
    )
}

fn get_incremental_diff(ctx: &PhaseContext<'_>) -> Option<String> {
    match get_git_diff_from_start() {
        Ok(d) if !d.trim().is_empty() => {
            let original_size = d.len();
            // For reviewer, use truncation for very large diffs (not chunking)
            // The limit is 1MB which provides substantial context for review
            // Chunking is only for commit message generation where we need to combine results
            let (truncated_diff, was_truncated) = crate::git_helpers::validate_and_truncate_diff(d);
            if was_truncated {
                ctx.logger.warn(&format!(
                    "Review diff truncated from {} to {} bytes for LLM processing",
                    original_size,
                    truncated_diff.len()
                ));
            }
            Some(truncated_diff)
        }
        Ok(_) => {
            ctx.logger
                .warn("No diff found from starting commit; review will be skipped for this cycle");
            None
        }
        Err(e) => {
            // Diff retrieval failed - this is a more serious issue
            // Return an error result to signal the caller should skip this cycle
            ctx.logger.error(&format!(
                "Failed to get diff from starting commit: {e}; skipping review cycle"
            ));
            ctx.logger.info(
                "This may indicate a git repository issue. The review cycle will be skipped.",
            );
            None
        }
    }
}

fn build_comprehensive_review(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    guidelines: Option<&ReviewGuidelines>,
) -> (String, String) {
    guidelines.map_or_else(
        || {
            ctx.logger.info("Using comprehensive review");
            (
                "review (comprehensive)".to_string(),
                prompt_comprehensive_review(reviewer_context, &ReviewGuidelines::default()),
            )
        },
        |g| {
            ctx.logger
                .info("Using comprehensive review with language-specific checks");
            (
                "review (comprehensive)".to_string(),
                prompt_comprehensive_review(reviewer_context, g),
            )
        },
    )
}

fn build_standard_review(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    guidelines: Option<&ReviewGuidelines>,
) -> (String, String) {
    guidelines.map_or_else(
        || {
            ctx.logger
                .info("Using detailed review without stack-specific checks");
            (
                "review (standard)".to_string(),
                prompt_detailed_review_without_guidelines(reviewer_context),
            )
        },
        |g| {
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
                    None,
                ),
            )
        },
    )
}

/// Check if the given agent/model combination is a problematic prompt target.
///
/// Certain AI agents have known compatibility issues with complex structured prompts.
/// This function detects those agents for which alternative handling may be needed.
fn is_problematic_prompt_target(agent: &str, model_flag: Option<&str>) -> bool {
    is_glm_like_agent(agent) || model_flag.is_some_and(is_glm_like_agent)
}
