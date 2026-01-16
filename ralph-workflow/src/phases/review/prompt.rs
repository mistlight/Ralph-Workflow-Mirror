//! Review prompt building logic.
//!
//! This module handles the construction of review prompts based on:
//! - Agent type and model (for compatibility detection)
//! - Review depth configuration
//! - Custom review guidelines

use std::fs;

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

/// Read PROMPT.md and .agent/PLAN.md files for review context.
///
/// Returns (`prompt_content`, `plan_content`). If files don't exist or can't be read,
/// returns empty strings to allow templates to render without errors.
fn read_prompt_and_plan() -> (String, String) {
    let prompt = fs::read_to_string("PROMPT.md").unwrap_or_default();
    let plan = fs::read_to_string(".agent/PLAN.md").unwrap_or_default();
    (prompt, plan)
}

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
    let (prompt_content, plan_content) = read_prompt_and_plan();
    match diff_result {
        Ok(Some(diff)) => {
            let prompt = prompt_universal_review_with_diff(
                reviewer_context,
                &diff,
                &prompt_content,
                &plan_content,
            );
            log_prompt_debug_info(ctx, &prompt, "universal");
            ("review (universal)".to_string(), prompt)
        }
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
    let (prompt_content, plan_content) = read_prompt_and_plan();
    match diff_result {
        Ok(Some(diff)) => {
            let guidelines_ref = guidelines.unwrap_or_else(|| {
                static DEFAULT_GUIDELINES: std::sync::OnceLock<ReviewGuidelines> =
                    std::sync::OnceLock::new();
                DEFAULT_GUIDELINES.get_or_init(ReviewGuidelines::default)
            });
            let prompt = prompt_security_focused_review_with_diff(
                reviewer_context,
                guidelines_ref,
                &diff,
                &prompt_content,
                &plan_content,
            );
            log_prompt_debug_info(ctx, &prompt, "security");
            ("review (security)".to_string(), prompt)
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
    let (prompt_content, plan_content) = read_prompt_and_plan();
    match diff_result {
        Ok(Some(diff)) => {
            ctx.logger
                .info("Using incremental review (changed files only)");
            let prompt = prompt_incremental_review_with_diff(
                reviewer_context,
                &diff,
                &prompt_content,
                &plan_content,
            );
            log_prompt_debug_info(ctx, &prompt, "incremental");
            ("review (incremental)".to_string(), prompt)
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
    let (prompt_content, plan_content) = read_prompt_and_plan();
    match diff_result {
        Ok(Some(diff)) => {
            let guidelines_ref = guidelines.unwrap_or_else(|| {
                static DEFAULT_GUIDELINES: std::sync::OnceLock<ReviewGuidelines> =
                    std::sync::OnceLock::new();
                DEFAULT_GUIDELINES.get_or_init(ReviewGuidelines::default)
            });
            let prompt = prompt_comprehensive_review_with_diff(
                reviewer_context,
                guidelines_ref,
                &diff,
                &prompt_content,
                &plan_content,
            );
            log_prompt_debug_info(ctx, &prompt, "comprehensive");
            ("review (comprehensive)".to_string(), prompt)
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
    let (prompt_content, plan_content) = read_prompt_and_plan();
    match diff_result {
        Ok(Some(diff)) => {
            ctx.logger.info(if guidelines.is_some() {
                "Using standard review with language-specific checks"
            } else {
                "Using detailed review without stack-specific checks"
            });
            guidelines.map_or_else(
                || {
                    let prompt = prompt_detailed_review_without_guidelines_with_diff(
                        reviewer_context,
                        &diff,
                        &prompt_content,
                        &plan_content,
                    );
                    log_prompt_debug_info(ctx, &prompt, "standard (detailed)");
                    ("review (standard)".to_string(), prompt)
                },
                |g| {
                    let prompt = prompt_reviewer_review_with_guidelines_and_diff(
                        reviewer_context,
                        g,
                        &diff,
                        &prompt_content,
                        &plan_content,
                    );
                    log_prompt_debug_info(ctx, &prompt, "standard (guided)");
                    ("review (standard)".to_string(), prompt)
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

/// Log debug information about the prompt being sent to the reviewer agent.
fn log_prompt_debug_info(ctx: &PhaseContext<'_>, prompt: &str, prompt_type: &str) {
    if ctx.config.verbosity.is_debug() {
        ctx.logger.info(&format!(
            "Review prompt type: {prompt_type}, size: {} bytes",
            prompt.len()
        ));
        // Log a preview of the prompt content for verification
        if prompt.len() > 500 {
            ctx.logger
                .info(&format!("Prompt preview:\n{}...", &prompt[..500]));
        } else {
            ctx.logger.info(&format!("Prompt preview:\n{prompt}"));
        }
        // Verify diff was included (check for actual content, not placeholder)
        let has_diff_placeholder = prompt.contains("{{DIFF}}");
        let has_actual_diff = prompt.contains("DIFF TO REVIEW")
            && prompt
                .lines()
                .any(|l| l.starts_with('+') || l.starts_with('-') || l.starts_with("diff --git"));
        ctx.logger.info(&format!(
            "Diff inclusion check - placeholder present: {has_diff_placeholder}, actual diff content: {has_actual_diff}"
        ));
    }
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guidelines::ReviewGuidelines;

    /// Test that all prompt builder functions explicitly include the diff content.
    #[test]
    fn test_all_prompts_include_diff_content() {
        let sample_diff = "+ new line\n- old line";

        // Test guided prompts (with guidelines)
        let guidelines = ReviewGuidelines::default();
        let prompt_with_guidelines = prompt_reviewer_review_with_guidelines_and_diff(
            ContextLevel::Minimal,
            &guidelines,
            sample_diff,
            "",
            "",
        );
        assert!(
            prompt_with_guidelines.contains(sample_diff),
            "Standard review prompt should include diff"
        );
        assert!(
            prompt_with_guidelines.contains("CRITICAL CONSTRAINTS"),
            "Prompt should have constraints"
        );
        assert!(
            prompt_with_guidelines.contains("MUST NOT run git commands"),
            "Prompt should explicitly forbid running git commands"
        );

        let comprehensive_prompt = prompt_comprehensive_review_with_diff(
            ContextLevel::Normal,
            &guidelines,
            sample_diff,
            "",
            "",
        );
        assert!(
            comprehensive_prompt.contains(sample_diff),
            "Comprehensive prompt should include diff"
        );

        let security_prompt = prompt_security_focused_review_with_diff(
            ContextLevel::Minimal,
            &guidelines,
            sample_diff,
            "",
            "",
        );
        assert!(
            security_prompt.contains(sample_diff),
            "Security prompt should include diff"
        );

        // Test unguided prompts (without guidelines)
        let detailed_prompt = prompt_detailed_review_without_guidelines_with_diff(
            ContextLevel::Normal,
            sample_diff,
            "",
            "",
        );
        assert!(
            detailed_prompt.contains(sample_diff),
            "Detailed prompt should include diff"
        );

        let incremental_prompt =
            prompt_incremental_review_with_diff(ContextLevel::Minimal, sample_diff, "", "");
        assert!(
            incremental_prompt.contains(sample_diff),
            "Incremental prompt should include diff"
        );

        let universal_prompt =
            prompt_universal_review_with_diff(ContextLevel::Normal, sample_diff, "", "");
        assert!(
            universal_prompt.contains(sample_diff),
            "Universal prompt should include diff"
        );
    }

    /// Test that prompts explicitly constrain agents from exploring the codebase.
    #[test]
    fn test_prompts_constrain_agent_from_exploring() {
        let sample_diff = "+ new line";

        let prompts_to_check = vec![
            prompt_reviewer_review_with_guidelines_and_diff(
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                sample_diff,
                "",
                "",
            ),
            prompt_comprehensive_review_with_diff(
                ContextLevel::Normal,
                &ReviewGuidelines::default(),
                sample_diff,
                "",
                "",
            ),
            prompt_security_focused_review_with_diff(
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                sample_diff,
                "",
                "",
            ),
            prompt_detailed_review_without_guidelines_with_diff(
                ContextLevel::Normal,
                sample_diff,
                "",
                "",
            ),
            prompt_incremental_review_with_diff(ContextLevel::Minimal, sample_diff, "", ""),
            prompt_universal_review_with_diff(ContextLevel::Normal, sample_diff, "", ""),
        ];

        let forbidden_patterns = [
            ("DO NOT run", "explicitly forbids running"),
            ("MUST NOT", "uses strong constraint language"),
            ("read other files", "forbids reading other files"),
            ("explore the", "forbids exploration"),
        ];

        for prompt in prompts_to_check {
            // Each prompt should have at least some constraint language
            let has_constraints = forbidden_patterns
                .iter()
                .any(|(pattern, _)| prompt.contains(pattern));
            assert!(
                has_constraints,
                "Prompt should contain constraint language. Prompt: {}",
                &prompt[..prompt.len().min(200)]
            );
        }
    }

    /// Test that prompts forbid running specific commands.
    #[test]
    fn test_prompts_forbid_specific_commands() {
        let sample_diff = "+ new line";

        let universal_prompt =
            prompt_universal_review_with_diff(ContextLevel::Minimal, sample_diff, "", "");
        // Universal prompt should explicitly forbid running commands like ls, find, git, cat
        assert!(
            universal_prompt.contains("ls")
                || universal_prompt.contains("git")
                || universal_prompt.contains("cat"),
            "Universal prompt should explicitly forbid running common commands"
        );
    }

    /// Test `should_use_universal_prompt` detection.
    #[test]
    fn test_should_use_universal_prompt() {
        // Should return true for GLM agents
        assert!(should_use_universal_prompt("glmtok", None, false));
        assert!(should_use_universal_prompt("zhipuai", None, false));

        // Should return true when forced
        assert!(should_use_universal_prompt("claude", None, true));

        // Should return false for non-GLM agents when not forced
        assert!(!should_use_universal_prompt("claude", None, false));
        assert!(!should_use_universal_prompt("openai", None, false));
    }

    /// Test that model flag is checked for GLM agents.
    #[test]
    fn test_should_use_universal_with_model_flag() {
        // Should detect GLM via model flag
        assert!(should_use_universal_prompt("openai", Some("glm-4"), false));

        // Should not trigger for non-GLM models
        assert!(!should_use_universal_prompt("openai", Some("gpt-4"), false));
    }

    /// Test that prompts contain the "CLOSED BOOK REVIEW" constraint phrase.
    #[test]
    fn test_prompts_contain_closed_book_constraint() {
        let sample_diff = "+ new line";

        let prompts_to_check = vec![
            prompt_reviewer_review_with_guidelines_and_diff(
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                sample_diff,
                "",
                "",
            ),
            prompt_comprehensive_review_with_diff(
                ContextLevel::Normal,
                &ReviewGuidelines::default(),
                sample_diff,
                "",
                "",
            ),
            prompt_security_focused_review_with_diff(
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                sample_diff,
                "",
                "",
            ),
            prompt_detailed_review_without_guidelines_with_diff(
                ContextLevel::Normal,
                sample_diff,
                "",
                "",
            ),
            prompt_incremental_review_with_diff(ContextLevel::Minimal, sample_diff, "", ""),
            prompt_universal_review_with_diff(ContextLevel::Normal, sample_diff, "", ""),
        ];

        for prompt in prompts_to_check {
            assert!(
                prompt.contains("CLOSED BOOK REVIEW"),
                "Prompt should contain 'CLOSED BOOK REVIEW' constraint. Prompt: {}",
                &prompt[..prompt.len().min(200)]
            );
            assert!(
                prompt.contains("NO ACCESS TO REPOSITORY"),
                "Prompt should contain 'NO ACCESS TO REPOSITORY' constraint. Prompt: {}",
                &prompt[..prompt.len().min(200)]
            );
        }
    }

    /// Test that actual diff content is substituted (not the {{DIFF}} placeholder).
    #[test]
    fn test_actual_diff_content_is_substituted_not_placeholder() {
        let sample_diff = "+ new line\n- old line";

        let prompts_to_check = vec![
            prompt_reviewer_review_with_guidelines_and_diff(
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                sample_diff,
                "",
                "",
            ),
            prompt_comprehensive_review_with_diff(
                ContextLevel::Normal,
                &ReviewGuidelines::default(),
                sample_diff,
                "",
                "",
            ),
            prompt_security_focused_review_with_diff(
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                sample_diff,
                "",
                "",
            ),
            prompt_detailed_review_without_guidelines_with_diff(
                ContextLevel::Normal,
                sample_diff,
                "",
                "",
            ),
            prompt_incremental_review_with_diff(ContextLevel::Minimal, sample_diff, "", ""),
            prompt_universal_review_with_diff(ContextLevel::Normal, sample_diff, "", ""),
        ];

        for prompt in prompts_to_check {
            // Prompt should contain the actual diff content
            assert!(
                prompt.contains(sample_diff),
                "Prompt should contain actual diff content. Prompt: {}",
                &prompt[..prompt.len().min(500)]
            );
            // Prompt should NOT contain the template placeholder
            assert!(
                !prompt.contains("{{DIFF}}"),
                "Prompt should not contain {{DIFF}} placeholder. Prompt: {}",
                &prompt[..prompt.len().min(500)]
            );
        }
    }
}
