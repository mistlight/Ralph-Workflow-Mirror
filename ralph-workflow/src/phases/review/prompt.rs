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
#[allow(clippy::too_many_lines)]
pub fn build_review_prompt(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    guidelines: Option<&ReviewGuidelines>,
) -> (String, String) {
    // Fetch the diff from the starting commit to pass directly to the reviewer
    // This keeps agents isolated from git operations and ensures they only review
    // the changes made since the pipeline started.
    let diff_result = get_and_validate_diff(ctx);

    // Check if we should use the universal prompt for this agent
    let use_universal = should_use_universal_prompt(
        ctx.reviewer_agent,
        ctx.config.reviewer_model.as_deref(),
        ctx.config.features.force_universal_prompt,
    );

    if use_universal {
        let reason = if ctx.config.features.force_universal_prompt {
            "forced via config/env"
        } else {
            "better compatibility"
        };
        ctx.logger.info(&format!(
            "Using universal/simplified review prompt for agent '{}' ({})",
            ctx.reviewer_agent, reason
        ));
        return match diff_result {
            Ok(Some(diff)) => (
                "review (universal)".to_string(),
                prompt_universal_review_with_diff(reviewer_context, &diff),
            ),
            Ok(None) => ("review (universal - skipped)".to_string(), String::new()),
            Err(()) => ("review (universal - error)".to_string(), String::new()),
        };
    }

    match ctx.config.review_depth {
        ReviewDepth::Security => {
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
                        prompt_security_focused_review_with_diff(
                            reviewer_context,
                            guidelines_ref,
                            &diff,
                        ),
                    )
                }
                Ok(None) => ("review (security - skipped)".to_string(), String::new()),
                Err(()) => ("review (security - error)".to_string(), String::new()),
            }
        }
        ReviewDepth::Incremental => match diff_result {
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
        },
        ReviewDepth::Comprehensive => match diff_result {
            Ok(Some(diff)) => {
                ctx.logger.info(if guidelines.is_some() {
                    "Using comprehensive review with language-specific checks"
                } else {
                    "Using comprehensive review"
                });
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
        },
        ReviewDepth::Standard => match diff_result {
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
                            prompt_reviewer_review_with_guidelines_and_diff(
                                reviewer_context,
                                g,
                                &diff,
                            ),
                        )
                    },
                )
            }
            Ok(None) => ("review (standard - skipped)".to_string(), String::new()),
            Err(()) => ("review (standard - error)".to_string(), String::new()),
        },
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
