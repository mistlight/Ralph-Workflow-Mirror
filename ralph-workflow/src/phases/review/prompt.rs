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
    prompt_comprehensive_review_with_diff, prompt_detailed_review_without_guidelines_with_diff,
    prompt_incremental_review_with_diff, prompt_reviewer_review_with_guidelines_and_diff,
    prompt_security_focused_review_with_diff, prompt_universal_review_with_diff, ContextLevel,
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
    let diff_result = get_and_validate_diff(ctx);

    let use_universal = should_use_universal_prompt(
        ctx.reviewer_agent,
        ctx.config.reviewer_model.as_deref(),
        ctx.config.features.force_universal_prompt,
    );

    if use_universal {
        return build_universal_prompt(ctx, reviewer_context, diff_result);
    }

    match ctx.config.review_depth {
        ReviewDepth::Security => {
            build_security_prompt(ctx, reviewer_context, guidelines, diff_result)
        }
        ReviewDepth::Incremental => build_incremental_prompt(ctx, reviewer_context, diff_result),
        ReviewDepth::Comprehensive => {
            build_comprehensive_prompt(ctx, reviewer_context, guidelines, diff_result)
        }
        ReviewDepth::Standard => {
            build_standard_prompt(ctx, reviewer_context, guidelines, diff_result)
        }
    }
}

/// Build the universal/simplified review prompt.
fn build_universal_prompt(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    diff_result: Result<Option<String>, ()>,
) -> (String, String) {
    let reason = if ctx.config.features.force_universal_prompt {
        "forced via config/env"
    } else {
        "better compatibility"
    };
    ctx.logger.info(&format!(
        "Using universal/simplified review prompt for agent '{}' ({reason}')",
        ctx.reviewer_agent
    ));
    match diff_result {
        Ok(Some(diff)) => (
            "review (universal)".to_string(),
            prompt_universal_review_with_diff(reviewer_context, &diff),
        ),
        Ok(None) => ("review (universal - skipped)".to_string(), String::new()),
        Err(()) => ("review (universal - error)".to_string(), String::new()),
    }
}

/// Build the security-focused review prompt.
fn build_security_prompt(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    guidelines: Option<&ReviewGuidelines>,
    diff_result: Result<Option<String>, ()>,
) -> (String, String) {
    ctx.logger.info(if guidelines.is_some() {
        "Using security-focused review with language-specific checks"
    } else {
        "Using security-focused review"
    });
    match diff_result {
        Ok(Some(diff)) => {
            let guidelines_ref = guidelines.unwrap_or_else(|| {
                static DEFAULT_GUIDELINES: std::sync::OnceLock<ReviewGuidelines> =
                    std::sync::OnceLock::new();
                DEFAULT_GUIDELINES.get_or_init(ReviewGuidelines::default)
            });
            (
                "review (security)".to_string(),
                prompt_security_focused_review_with_diff(reviewer_context, guidelines_ref, &diff),
            )
        }
        Ok(None) => ("review (security - skipped)".to_string(), String::new()),
        Err(()) => ("review (security - error)".to_string(), String::new()),
    }
}

/// Build the incremental review prompt.
fn build_incremental_prompt(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    diff_result: Result<Option<String>, ()>,
) -> (String, String) {
    match diff_result {
        Ok(Some(diff)) => {
            ctx.logger
                .info("Using incremental review (changed files only)");
            (
                "review (incremental)".to_string(),
                prompt_incremental_review_with_diff(reviewer_context, &diff),
            )
        }
        Ok(None) => ("review (incremental - skipped)".to_string(), String::new()),
        Err(()) => ("review (incremental - error)".to_string(), String::new()),
    }
}

/// Build the comprehensive review prompt.
fn build_comprehensive_prompt(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    guidelines: Option<&ReviewGuidelines>,
    diff_result: Result<Option<String>, ()>,
) -> (String, String) {
    ctx.logger.info(if guidelines.is_some() {
        "Using comprehensive review with language-specific checks"
    } else {
        "Using comprehensive review"
    });
    match diff_result {
        Ok(Some(diff)) => {
            let guidelines_ref = guidelines.unwrap_or_else(|| {
                static DEFAULT_GUIDELINES: std::sync::OnceLock<ReviewGuidelines> =
                    std::sync::OnceLock::new();
                DEFAULT_GUIDELINES.get_or_init(ReviewGuidelines::default)
            });
            (
                "review (comprehensive)".to_string(),
                prompt_comprehensive_review_with_diff(reviewer_context, guidelines_ref, &diff),
            )
        }
        Ok(None) => (
            "review (comprehensive - skipped)".to_string(),
            String::new(),
        ),
        Err(()) => ("review (comprehensive - error)".to_string(), String::new()),
    }
}

/// Build the standard review prompt.
fn build_standard_prompt(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    guidelines: Option<&ReviewGuidelines>,
    diff_result: Result<Option<String>, ()>,
) -> (String, String) {
    match diff_result {
        Ok(Some(diff)) => {
            ctx.logger.info(if guidelines.is_some() {
                "Using standard review with language-specific checks"
            } else {
                "Using detailed review without stack-specific checks"
            });
            guidelines.map_or_else(
                || {
                    (
                        "review (standard)".to_string(),
                        prompt_detailed_review_without_guidelines_with_diff(
                            reviewer_context,
                            &diff,
                        ),
                    )
                },
                |g| {
                    (
                        "review (standard)".to_string(),
                        prompt_reviewer_review_with_guidelines_and_diff(reviewer_context, g, &diff),
                    )
                },
            )
        }
        Ok(None) => ("review (standard - skipped)".to_string(), String::new()),
        Err(()) => ("review (standard - error)".to_string(), String::new()),
    }
}

/// Check if the given agent/model combination is a problematic prompt target.
///
/// Certain AI agents have known compatibility issues with complex structured prompts.
/// This function detects those agents for which alternative handling may be needed.
fn is_problematic_prompt_target(agent: &str, model_flag: Option<&str>) -> bool {
    is_glm_like_agent(agent) || model_flag.is_some_and(is_glm_like_agent)
}

/// Fetch and validate the git diff from the starting commit.
///
/// Returns:
/// - `Ok(Some(diff))` - Successfully retrieved and validated diff
/// - `Ok(None)` - No diff found (no changes since start commit)
/// - `Err(..)` - Failed to retrieve diff (git error)
fn get_and_validate_diff(ctx: &PhaseContext<'_>) -> Result<Option<String>, ()> {
    match get_git_diff_from_start() {
        Ok(d) if !d.trim().is_empty() => {
            let original_size = d.len();
            let (truncated_diff, was_truncated) = crate::git_helpers::validate_and_truncate_diff(d);
            if was_truncated {
                ctx.logger.warn(&format!(
                    "Review diff truncated from {} to {} bytes for LLM processing",
                    original_size,
                    truncated_diff.len()
                ));
            }
            if ctx.config.verbosity.is_debug() {
                ctx.logger.info(&format!(
                    "Diff size for review: {} bytes",
                    truncated_diff.len()
                ));
                // Log a preview of the diff content for verification
                if truncated_diff.len() > 500 {
                    ctx.logger
                        .info(&format!("Diff preview:\n{}...", &truncated_diff[..500]));
                } else {
                    ctx.logger.info(&format!("Diff preview:\n{truncated_diff}"));
                }
            }
            Ok(Some(truncated_diff))
        }
        Ok(_) => {
            ctx.logger
                .warn("No diff found from starting commit; review will be skipped for this cycle");
            Ok(None)
        }
        Err(e) => {
            ctx.logger.error(&format!(
                "Failed to get diff from starting commit: {e}; skipping review cycle"
            ));
            ctx.logger.info(
                "This may indicate a git repository issue. The review cycle will be skipped.",
            );
            Err(())
        }
    }
}
