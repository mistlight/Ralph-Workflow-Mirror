//! Review prompt building logic.
//!
//! This module handles the construction of review prompts based on:
//! - Agent type and model (for compatibility detection)
//! - Review depth configuration
//! - Custom review guidelines

use std::fs;

use crate::agents::is_glm_like_agent;
use crate::checkpoint::restore::ResumeContext;
use crate::config::ReviewDepth;
use crate::git_helpers::{
    get_git_diff_from_review_baseline, get_review_baseline_info, get_start_commit_summary,
    DiffReviewContent, DiffTruncationLevel,
};
use crate::guidelines::ReviewGuidelines;
use crate::prompts::{
    generate_resume_note, prompt_comprehensive_review_with_diff_with_context,
    prompt_detailed_review_without_guidelines_with_diff_with_context,
    prompt_incremental_review_with_diff_with_context, prompt_review_xml_with_context,
    prompt_reviewer_review_with_guidelines_and_diff_with_context,
    prompt_security_focused_review_with_diff_with_context,
    prompt_universal_review_with_diff_with_context, ContextLevel,
};

#[cfg(test)]
use crate::prompts::reviewer::prompt_reviewer_review_with_guidelines_and_diff;

use super::super::context::PhaseContext;

/// Read PROMPT.md and .agent/PLAN.md files for review context.
///
/// Returns (`prompt_content`, `plan_content`). If files don't exist or can't be read,
/// returns empty strings to allow templates to render without errors.
///
/// Note: This logs warnings when expected files are missing, as this may indicate
/// a workflow issue (e.g., running review before planning phase completes).
fn read_prompt_and_plan() -> (String, String) {
    let prompt = match fs::read_to_string("PROMPT.md") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Warning: Could not read PROMPT.md: {e}. Using empty context.");
            String::new()
        }
    };
    let plan = match fs::read_to_string(".agent/PLAN.md") {
        Ok(content) => content,
        Err(e) => {
            eprintln!("Warning: Could not read .agent/PLAN.md: {e}. Using empty context.");
            String::new()
        }
    };
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
    resume_context: Option<&ResumeContext>,
) -> (String, String) {
    let diff_result = get_and_validate_diff(ctx);

    let use_universal = should_use_universal_prompt(
        ctx.reviewer_agent,
        ctx.config.reviewer_model.as_deref(),
        ctx.config.features.force_universal_prompt,
    );

    let (label, prompt) = if use_universal {
        build_universal_prompt(ctx, reviewer_context, diff_result)
    } else {
        match ctx.config.review_depth {
            ReviewDepth::Security => {
                build_security_prompt(ctx, reviewer_context, guidelines, diff_result)
            }
            ReviewDepth::Incremental => {
                build_incremental_prompt(ctx, reviewer_context, diff_result)
            }
            ReviewDepth::Comprehensive => {
                build_comprehensive_prompt(ctx, reviewer_context, guidelines, diff_result)
            }
            ReviewDepth::Standard => {
                build_standard_prompt(ctx, reviewer_context, guidelines, diff_result)
            }
        }
    };

    // Prepend resume note if this is a resumed session
    let prompt = if let Some(resume_ctx) = resume_context {
        let resume_note = generate_resume_note(resume_ctx);
        format!("{}{}", resume_note, prompt)
    } else {
        prompt
    };

    (label, prompt)
}

/// Build the XML-based review prompt for XSD retry loop.
///
/// This function builds a review prompt using XML output format with XSD validation.
/// It's used in the new XSD retry loop implementation for the review phase.
///
/// # Arguments
///
/// * `ctx` - The phase context
/// * `diff_result` - The diff content to review
///
/// # Returns
///
/// A tuple of (label, prompt_content) where label describes the prompt type and
/// prompt_content is the actual prompt to send to the agent.
#[allow(dead_code)]
pub fn build_review_xml_prompt(
    ctx: &PhaseContext<'_>,
    diff_result: Result<Option<DiffReviewContent>, ()>,
) -> (String, String) {
    let (prompt_content, plan_content) = read_prompt_and_plan();

    match diff_result {
        Ok(Some(diff_content)) => {
            // Format diff content as CHANGES for the XML template
            let changes_content = format_diff_for_xml(&diff_content);

            let review_prompt = prompt_review_xml_with_context(
                ctx.template_context,
                &prompt_content,
                &plan_content,
                &changes_content,
            );

            (
                "review (XML with XSD validation)".to_string(),
                review_prompt,
            )
        }
        Ok(None) => ("review (XML - skipped)".to_string(), String::new()),
        Err(()) => ("review (XML - error)".to_string(), String::new()),
    }
}

/// Format diff content for the XML review template.
///
/// Converts the diff content into a format suitable for the CHANGES variable
/// in the review_xml.txt template.
#[allow(dead_code)]
fn format_diff_for_xml(diff_content: &DiffReviewContent) -> String {
    let mut result = String::new();

    // Add baseline context if available
    if let Some(ref baseline_short) = diff_content.baseline_short {
        result.push_str(&format!("## Base Commit: {}\n\n", baseline_short));
    }

    // Add truncation notice if applicable
    match diff_content.truncation_level {
        DiffTruncationLevel::Full => {
            // No truncation notice needed
        }
        DiffTruncationLevel::Abbreviated => {
            result.push_str(&format!(
                "**Note:** Diff abbreviated - showing {}/{} files.\n\n",
                diff_content.shown_file_count.unwrap_or(0),
                diff_content.total_file_count
            ));
        }
        DiffTruncationLevel::FileList => {
            result
                .push_str("**Note:** Only file list shown - reviewer must explore full diff.\n\n");
        }
        DiffTruncationLevel::FileListAbbreviated => {
            result.push_str(&format!(
                "**Note:** Only {}/{} files shown - reviewer must discover all files.\n\n",
                diff_content.shown_file_count.unwrap_or(0),
                diff_content.total_file_count
            ));
        }
    }

    // Add the actual diff content
    result.push_str("```diff\n");
    result.push_str(&diff_content.content);
    result.push_str("\n```\n");

    result
}

/// Build the universal/simplified review prompt.
fn build_universal_prompt(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    diff_result: Result<Option<DiffReviewContent>, ()>,
) -> (String, String) {
    let reason = if ctx.config.features.force_universal_prompt {
        "forced via config/env"
    } else {
        "better compatibility"
    };
    ctx.logger.info(&format!(
        "Using universal/simplified review prompt for agent '{}' ({reason})",
        ctx.reviewer_agent
    ));
    let (prompt_content, plan_content) = read_prompt_and_plan();
    match diff_result {
        Ok(Some(diff_content)) => {
            let prompt = prompt_universal_review_with_diff_with_context(
                ctx.template_context,
                reviewer_context,
                &diff_content,
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
    diff_result: Result<Option<DiffReviewContent>, ()>,
) -> (String, String) {
    ctx.logger.info(if guidelines.is_some() {
        "Using security-focused review with language-specific checks"
    } else {
        "Using security-focused review"
    });
    let (prompt_content, plan_content) = read_prompt_and_plan();
    match diff_result {
        Ok(Some(diff_content)) => {
            let guidelines_ref = guidelines.unwrap_or_else(|| {
                static DEFAULT_GUIDELINES: std::sync::OnceLock<ReviewGuidelines> =
                    std::sync::OnceLock::new();
                DEFAULT_GUIDELINES.get_or_init(ReviewGuidelines::default)
            });
            let prompt = prompt_security_focused_review_with_diff_with_context(
                ctx.template_context,
                reviewer_context,
                guidelines_ref,
                &diff_content,
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
    diff_result: Result<Option<DiffReviewContent>, ()>,
) -> (String, String) {
    let (prompt_content, plan_content) = read_prompt_and_plan();
    match diff_result {
        Ok(Some(diff_content)) => {
            ctx.logger
                .info("Using incremental review (changed files only)");
            let prompt = prompt_incremental_review_with_diff_with_context(
                ctx.template_context,
                reviewer_context,
                &diff_content,
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
    diff_result: Result<Option<DiffReviewContent>, ()>,
) -> (String, String) {
    ctx.logger.info(if guidelines.is_some() {
        "Using comprehensive review with language-specific checks"
    } else {
        "Using comprehensive review"
    });
    let (prompt_content, plan_content) = read_prompt_and_plan();
    match diff_result {
        Ok(Some(diff_content)) => {
            let guidelines_ref = guidelines.unwrap_or_else(|| {
                static DEFAULT_GUIDELINES: std::sync::OnceLock<ReviewGuidelines> =
                    std::sync::OnceLock::new();
                DEFAULT_GUIDELINES.get_or_init(ReviewGuidelines::default)
            });
            let prompt = prompt_comprehensive_review_with_diff_with_context(
                ctx.template_context,
                reviewer_context,
                guidelines_ref,
                &diff_content,
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
    diff_result: Result<Option<DiffReviewContent>, ()>,
) -> (String, String) {
    let (prompt_content, plan_content) = read_prompt_and_plan();
    match diff_result {
        Ok(Some(diff_content)) => {
            ctx.logger.info(if guidelines.is_some() {
                "Using standard review with language-specific checks"
            } else {
                "Using detailed review without stack-specific checks"
            });
            guidelines.map_or_else(
                || {
                    let prompt = prompt_detailed_review_without_guidelines_with_diff_with_context(
                        ctx.template_context,
                        reviewer_context,
                        &diff_content,
                        &prompt_content,
                        &plan_content,
                    );
                    log_prompt_debug_info(ctx, &prompt, "standard (detailed)");
                    ("review (standard)".to_string(), prompt)
                },
                |g| {
                    let prompt = prompt_reviewer_review_with_guidelines_and_diff_with_context(
                        ctx.template_context,
                        reviewer_context,
                        g,
                        &diff_content,
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

/// Fetch and validate the git diff from the review baseline with progressive truncation.
///
/// Returns:
/// - `Ok(Some(content))` - Successfully retrieved diff (possibly truncated)
/// - `Ok(None)` - No diff found (no changes since baseline)
/// - `Err(..)` - Failed to retrieve diff (git error)
fn get_and_validate_diff(ctx: &PhaseContext<'_>) -> Result<Option<DiffReviewContent>, ()> {
    match get_git_diff_from_review_baseline() {
        Ok(d) if !d.trim().is_empty() => {
            let original_size = d.len();

            // Use progressive truncation with sensible defaults
            const MAX_FULL_DIFF_SIZE: usize = 100 * 1024; // 100KB
            const MAX_ABBREVIATED_SIZE: usize = 50 * 1024; // 50KB
            const MAX_FILE_LIST_SIZE: usize = 10 * 1024; // 10KB

            let mut content = crate::git_helpers::truncate_diff_for_review(
                d,
                MAX_FULL_DIFF_SIZE,
                MAX_ABBREVIATED_SIZE,
                MAX_FILE_LIST_SIZE,
            );

            // Add version context to the diff content
            if let Ok((baseline_oid, _, _)) = get_review_baseline_info() {
                if let Some(oid) = baseline_oid {
                    content.baseline_oid = Some(oid.clone());
                    content.baseline_short = Some(short_oid(&oid));
                    content.baseline_description = "review_baseline".to_string();
                } else {
                    // No review baseline set, fall back to start commit
                    if let Ok(summary) = get_start_commit_summary() {
                        if let Some(oid) = summary.start_oid {
                            content.baseline_oid = Some(oid.clone());
                            content.baseline_short = Some(short_oid(&oid));
                            content.baseline_description = "start_commit".to_string();
                        }
                    }
                }
            }

            match content.truncation_level {
                DiffTruncationLevel::Full => {
                    if ctx.config.verbosity.is_debug() {
                        ctx.logger.info(&format!(
                            "Diff size for review: {} bytes (no truncation)",
                            content.content.len()
                        ));
                    }
                }
                DiffTruncationLevel::Abbreviated => {
                    ctx.logger.warn(&format!(
                        "Review diff abbreviated: {}/{} files shown ({:.1}% of original)",
                        content.shown_file_count.unwrap_or(0),
                        content.total_file_count,
                        (content.content.len() as f64 / original_size as f64) * 100.0
                    ));
                    ctx.logger
                        .info("Reviewer agent must explore full diff independently");
                }
                DiffTruncationLevel::FileList => {
                    ctx.logger.warn(&format!(
                        "Review diff truncated to file list: {} files (reviewer must explore each)",
                        content.total_file_count
                    ));
                }
                DiffTruncationLevel::FileListAbbreviated => {
                    ctx.logger.warn(&format!(
                        "Review diff truncated: {}/{} files shown (reviewer must discover all files)",
                        content.shown_file_count.unwrap_or(0),
                        content.total_file_count
                    ));
                }
            }

            if ctx.config.verbosity.is_debug() {
                // Log a preview of the diff content for verification
                if content.content.len() > 500 {
                    ctx.logger
                        .info(&format!("Diff preview:\n{}...", &content.content[..500]));
                } else {
                    ctx.logger
                        .info(&format!("Diff preview:\n{}", content.content));
                }
            }

            Ok(Some(content))
        }
        Ok(_) => {
            ctx.logger
                .warn("No diff found from review baseline; review will be skipped for this cycle");
            Ok(None)
        }
        Err(e) => {
            ctx.logger.error(&format!(
                "Failed to get diff from review baseline: {e}; skipping review cycle"
            ));
            ctx.logger.info(
                "This may indicate a git repository issue. The review cycle will be skipped.",
            );
            Err(())
        }
    }
}

/// Convert a full OID to a short form (first 8 characters).
fn short_oid(oid: &str) -> String {
    oid.chars().take(8).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::guidelines::ReviewGuidelines;
    use crate::prompts::TemplateContext;

    /// Test that all prompt builder functions explicitly include the diff content.
    #[test]
    fn test_all_prompts_include_diff_content() {
        let sample_diff = "+ new line\n- old line";
        let diff_content = DiffReviewContent {
            content: sample_diff.to_string(),
            truncation_level: DiffTruncationLevel::Full,
            total_file_count: 1,
            shown_file_count: None,
            baseline_oid: None,
            baseline_short: None,
            baseline_description: String::new(),
        };

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
            prompt_with_guidelines.contains("MUST NOT run discovery commands")
                || prompt_with_guidelines.contains("MUST NOT run git commands"),
            "Prompt should explicitly forbid running discovery/git commands"
        );

        let template_context = TemplateContext::default();
        let comprehensive_prompt = prompt_comprehensive_review_with_diff_with_context(
            &template_context,
            ContextLevel::Normal,
            &guidelines,
            &diff_content,
            "",
            "",
        );
        assert!(
            comprehensive_prompt.contains(sample_diff),
            "Comprehensive prompt should include diff"
        );

        let security_prompt = prompt_security_focused_review_with_diff_with_context(
            &template_context,
            ContextLevel::Minimal,
            &guidelines,
            &diff_content,
            "",
            "",
        );
        assert!(
            security_prompt.contains(sample_diff),
            "Security prompt should include diff"
        );

        // Test unguided prompts (without guidelines)
        let detailed_prompt = prompt_detailed_review_without_guidelines_with_diff_with_context(
            &template_context,
            ContextLevel::Normal,
            &diff_content,
            "",
            "",
        );
        assert!(
            detailed_prompt.contains(sample_diff),
            "Detailed prompt should include diff"
        );

        let incremental_prompt = prompt_incremental_review_with_diff_with_context(
            &template_context,
            ContextLevel::Minimal,
            &diff_content,
            "",
            "",
        );
        assert!(
            incremental_prompt.contains(sample_diff),
            "Incremental prompt should include diff"
        );

        let universal_prompt = prompt_universal_review_with_diff_with_context(
            &template_context,
            ContextLevel::Normal,
            &diff_content,
            "",
            "",
        );
        assert!(
            universal_prompt.contains(sample_diff),
            "Universal prompt should include diff"
        );
    }

    /// Test that prompts explicitly constrain agents from exploring the codebase.
    #[test]
    fn test_prompts_constrain_agent_from_exploring() {
        let sample_diff = "+ new line";
        let template_context = TemplateContext::default();
        let diff_content = DiffReviewContent {
            content: sample_diff.to_string(),
            truncation_level: DiffTruncationLevel::Full,
            total_file_count: 1,
            shown_file_count: None,
            baseline_oid: None,
            baseline_short: None,
            baseline_description: String::new(),
        };

        let prompts_to_check = vec![
            prompt_reviewer_review_with_guidelines_and_diff_with_context(
                &template_context,
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                &diff_content,
                "",
                "",
            ),
            prompt_comprehensive_review_with_diff_with_context(
                &template_context,
                ContextLevel::Normal,
                &ReviewGuidelines::default(),
                &diff_content,
                "",
                "",
            ),
            prompt_security_focused_review_with_diff_with_context(
                &template_context,
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                &diff_content,
                "",
                "",
            ),
            prompt_detailed_review_without_guidelines_with_diff_with_context(
                &template_context,
                ContextLevel::Normal,
                &diff_content,
                "",
                "",
            ),
            prompt_incremental_review_with_diff_with_context(
                &template_context,
                ContextLevel::Minimal,
                &diff_content,
                "",
                "",
            ),
            prompt_universal_review_with_diff_with_context(
                &template_context,
                ContextLevel::Normal,
                &diff_content,
                "",
                "",
            ),
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
        let template_context = TemplateContext::default();
        let diff_content = DiffReviewContent {
            content: sample_diff.to_string(),
            truncation_level: DiffTruncationLevel::Full,
            total_file_count: 1,
            shown_file_count: None,
            baseline_oid: None,
            baseline_short: None,
            baseline_description: String::new(),
        };

        let universal_prompt = prompt_universal_review_with_diff_with_context(
            &template_context,
            ContextLevel::Minimal,
            &diff_content,
            "",
            "",
        );
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
        let template_context = TemplateContext::default();
        let diff_content = DiffReviewContent {
            content: sample_diff.to_string(),
            truncation_level: DiffTruncationLevel::Full,
            total_file_count: 1,
            shown_file_count: None,
            baseline_oid: None,
            baseline_short: None,
            baseline_description: String::new(),
        };

        let prompts_to_check = vec![
            prompt_reviewer_review_with_guidelines_and_diff_with_context(
                &template_context,
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                &diff_content,
                "",
                "",
            ),
            prompt_comprehensive_review_with_diff_with_context(
                &template_context,
                ContextLevel::Normal,
                &ReviewGuidelines::default(),
                &diff_content,
                "",
                "",
            ),
            prompt_security_focused_review_with_diff_with_context(
                &template_context,
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                &diff_content,
                "",
                "",
            ),
            prompt_detailed_review_without_guidelines_with_diff_with_context(
                &template_context,
                ContextLevel::Normal,
                &diff_content,
                "",
                "",
            ),
            prompt_incremental_review_with_diff_with_context(
                &template_context,
                ContextLevel::Minimal,
                &diff_content,
                "",
                "",
            ),
            prompt_universal_review_with_diff_with_context(
                &template_context,
                ContextLevel::Normal,
                &diff_content,
                "",
                "",
            ),
        ];

        for prompt in prompts_to_check {
            // Check for either the old "CLOSED BOOK REVIEW" or new "LIMITED EXPLORATION" constraint
            assert!(
                prompt.contains("CLOSED BOOK REVIEW") || prompt.contains("LIMITED EXPLORATION"),
                "Prompt should contain 'CLOSED BOOK REVIEW' or 'LIMITED EXPLORATION' constraint. Prompt: {}",
                &prompt[..prompt.len().min(200)]
            );
            assert!(
                prompt.contains("NO ACCESS TO REPOSITORY")
                    || prompt.contains("CRITICAL CONSTRAINTS"),
                "Prompt should contain access constraint. Prompt: {}",
                &prompt[..prompt.len().min(200)]
            );
        }
    }

    /// Test that actual diff content is substituted (not the {{DIFF}} placeholder).
    #[test]
    fn test_actual_diff_content_is_substituted_not_placeholder() {
        let sample_diff = "+ new line\n- old line";
        let template_context = TemplateContext::default();
        let diff_content = DiffReviewContent {
            content: sample_diff.to_string(),
            truncation_level: DiffTruncationLevel::Full,
            total_file_count: 1,
            shown_file_count: None,
            baseline_oid: None,
            baseline_short: None,
            baseline_description: String::new(),
        };

        let prompts_to_check = vec![
            prompt_reviewer_review_with_guidelines_and_diff_with_context(
                &template_context,
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                &diff_content,
                "",
                "",
            ),
            prompt_comprehensive_review_with_diff_with_context(
                &template_context,
                ContextLevel::Normal,
                &ReviewGuidelines::default(),
                &diff_content,
                "",
                "",
            ),
            prompt_security_focused_review_with_diff_with_context(
                &template_context,
                ContextLevel::Minimal,
                &ReviewGuidelines::default(),
                &diff_content,
                "",
                "",
            ),
            prompt_detailed_review_without_guidelines_with_diff_with_context(
                &template_context,
                ContextLevel::Normal,
                &diff_content,
                "",
                "",
            ),
            prompt_incremental_review_with_diff_with_context(
                &template_context,
                ContextLevel::Minimal,
                &diff_content,
                "",
                "",
            ),
            prompt_universal_review_with_diff_with_context(
                &template_context,
                ContextLevel::Normal,
                &diff_content,
                "",
                "",
            ),
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
