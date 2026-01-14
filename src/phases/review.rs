//! Review phase execution.
//!
//! This module handles the review and fix phase of the Ralph pipeline. It runs
//! a configurable number of review-fix cycles, where each cycle:
//! 1. Reviews the code and creates ISSUES.md
//! 2. Fixes the issues found
//! 3. Cleans up ISSUES.md (in isolation mode)

use crate::agents::{is_glm_like_agent, AgentRole};
use crate::config::ReviewDepth;
use crate::files::extract_issues;
use crate::git_helpers::{commit_with_auto_message_result, get_git_diff_from_start, git_snapshot};
use crate::guidelines::ReviewGuidelines;
use crate::pipeline::{run_with_fallback, PipelineRuntime};
use crate::prompts::{
    prompt_comprehensive_review, prompt_detailed_review_without_guidelines, prompt_for_agent,
    prompt_incremental_review_with_diff, prompt_security_focused_review, prompt_universal_review,
    Action, ContextLevel, Role,
};
use crate::review_metrics::ReviewMetrics;
use crate::utils::{
    clean_context_for_reviewer, delete_issues_file_for_isolation, print_progress, save_checkpoint,
    update_status, PipelineCheckpoint, PipelinePhase,
};

use super::context::PhaseContext;
use std::fs;
use std::path::Path;

/// Result of the review phase.
pub struct ReviewResult {
    /// Whether the review completed early due to no issues found.
    pub completed_early: bool,
}

/// Result of pre-flight validation
#[derive(Debug)]
enum PreflightResult {
    /// All checks passed
    Ok,
    /// Warning issued but can proceed
    Warning(String),
    /// Critical error that should halt execution
    Error(String),
}

/// Result of post-flight validation
#[derive(Debug)]
enum PostflightResult {
    /// ISSUES.md found and valid
    Valid,
    /// ISSUES.md missing or empty
    Missing(String),
    /// ISSUES.md has unexpected format
    Malformed(String),
}

fn is_problematic_prompt_target(agent: &str, model_flag: Option<&str>) -> bool {
    is_glm_like_agent(agent) || model_flag.is_some_and(is_glm_like_agent)
}

/// Check if an agent is GLM-based (for validation purposes).
/// NOTE: This function is deprecated. Use `is_glm_like_agent` instead.
fn is_glm_agent(agent: &str) -> bool {
    is_glm_like_agent(agent)
}

/// Run pre-flight validation checks before starting a review pass.
///
/// These checks verify that the environment is suitable for running
/// the review agent and help diagnose issues early.
fn pre_flight_review_check(
    logger: &crate::logger::Logger,
    cycle: u32,
    reviewer_agent: &str,
    reviewer_model: Option<&str>,
) -> PreflightResult {
    let agent_dir = Path::new(".agent");
    let issues_path = Path::new(".agent/ISSUES.md");

    // Check 0: Agent compatibility warning (non-blocking)
    let is_problematic_reviewer = is_problematic_prompt_target(reviewer_agent, reviewer_model);

    if is_problematic_reviewer {
        logger.warn(&format!(
            "Note: Reviewer may have compatibility issues with review tasks. (agent='{}', model={})",
            reviewer_agent,
            reviewer_model.unwrap_or("none")
        ));
        logger.info("If review fails, consider these workarounds:");
        logger.info("  1. Use Claude/Codex as reviewer: ralph --reviewer-agent codex");
        logger.info("  2. Try generic parser: ralph --reviewer-json-parser generic");
        logger.info("  3. Skip review: RALPH_REVIEWER_REVIEWS=0 ralph");
        // Continue anyway - don't block execution
    }

    // Check 0.1: GLM-specific command validation (diagnostic only)
    if is_glm_agent(reviewer_agent) {
        // Log diagnostic info about GLM agent configuration
        logger.info(&format!(
            "GLM agent detected: '{}'. Command will include '-p' flag for non-interactive mode.",
            reviewer_agent
        ));
        logger.info("Tip: Use --verbosity debug to see the full command being executed");
    }

    // Check 0.5: Check for existing ISSUES.md from previous failed run
    if issues_path.exists() {
        match fs::metadata(issues_path) {
            Ok(metadata) if metadata.len() > 0 => {
                logger.warn(&format!(
                    "ISSUES.md already exists from a previous run (size: {} bytes).",
                    metadata.len()
                ));
                logger
                    .info("The review agent will overwrite this file. If the previous run failed,");
                logger.info("consider checking the old ISSUES.md for clues about what went wrong.");
            }
            Ok(_) => {
                // Empty ISSUES.md - warn but continue
                logger.warn("Found empty ISSUES.md from previous run. Will be overwritten.");
            }
            Err(e) => {
                logger.warn(&format!("Cannot check ISSUES.md metadata: {}", e));
            }
        }
    }

    // Check 1: Verify .agent directory is writable
    if !agent_dir.exists() {
        // Try to create it
        if let Err(e) = fs::create_dir_all(agent_dir) {
            return PreflightResult::Error(format!(
                "Cannot create .agent directory: {}. Check directory permissions.",
                e
            ));
        }
    }

    // Test write by touching a temp file
    let test_file = agent_dir.join(format!(".write_test_{}", cycle));
    match fs::write(&test_file, b"test") {
        Ok(_) => {
            let _ = fs::remove_file(&test_file);
        }
        Err(e) => {
            return PreflightResult::Error(format!(
                ".agent directory is not writable: {}. Check file permissions.",
                e
            ));
        }
    }

    // Check 2: Verify available disk space (at least 10MB free)
    if let Ok(_metadata) = fs::metadata(agent_dir) {
        // We can't easily get disk space on all platforms, so we'll
        // just log a reminder if the directory seems unusually large
        if let Ok(mut entries) = fs::read_dir(agent_dir) {
            let entry_count = entries.by_ref().count();
            if entry_count > 1000 {
                logger.warn(&format!(
                    ".agent directory has {} files. Consider cleaning up old logs.",
                    entry_count
                ));
                return PreflightResult::Warning(
                    "Large .agent directory detected. Review may be slow.".to_string(),
                );
            }
        }
    }

    PreflightResult::Ok
}

/// Run post-flight validation after a review pass completes.
///
/// These checks verify that the review agent produced expected output.
fn post_flight_review_check(logger: &crate::logger::Logger, cycle: u32) -> PostflightResult {
    let issues_path = Path::new(".agent/ISSUES.md");

    // Check 1: Verify ISSUES.md exists
    if !issues_path.exists() {
        logger.warn(&format!(
            "Review cycle {} completed but ISSUES.md was not created. \
             The agent may have failed or used a different output format.",
            cycle
        ));
        logger.info("Possible causes:");
        logger.info("  - Agent failed to write the file (permission/execution error)");
        logger.info("  - Agent used a different output filename or format");
        logger.info("  - Agent was interrupted during execution");
        return PostflightResult::Missing(
            "ISSUES.md not found after review. Agent may have failed.".to_string(),
        );
    }

    // Check 2: Verify ISSUES.md is not empty and log its size
    let file_size = match fs::metadata(issues_path) {
        Ok(metadata) if metadata.len() == 0 => {
            logger.warn(&format!(
                "Review cycle {} created an empty ISSUES.md.",
                cycle
            ));
            logger.info("Possible causes:");
            logger.info("  - Agent reviewed but found no issues (should write 'No issues found.')");
            logger.info("  - Agent failed during file write");
            logger.info("  - Agent doesn't understand the expected output format");
            return PostflightResult::Missing("ISSUES.md is empty".to_string());
        }
        Ok(metadata) => {
            // Log the file size for debugging
            logger.info(&format!("ISSUES.md created ({} bytes)", metadata.len()));
            metadata.len()
        }
        Err(e) => {
            logger.warn(&format!("Cannot read ISSUES.md metadata: {}", e));
            return PostflightResult::Missing(format!("Cannot read ISSUES.md: {}", e));
        }
    };

    // Check 3: Verify ISSUES.md has valid structure
    match ReviewMetrics::from_issues_file() {
        Ok(metrics) => {
            // Check if metrics indicate reasonable content
            if metrics.total_issues == 0 && !metrics.no_issues_declared {
                // Partial recovery: file has content but no parseable issues
                logger.warn(&format!(
                    "Review cycle {} produced ISSUES.md ({} bytes) but no parseable issues detected.",
                    cycle, file_size
                ));
                logger.info("Content may be in unexpected format. The fix pass may still work.");
                logger.info(
                    "Consider checking .agent/ISSUES.md manually to see what the agent wrote.",
                );
                return PostflightResult::Malformed(
                    "ISSUES.md exists but no issues detected. Check format.".to_string(),
                );
            }

            // Log a summary of what was found
            if metrics.total_issues > 0 {
                logger.info(&format!(
                    "Review found {} issues ({} critical, {} high, {} medium, {} low)",
                    metrics.total_issues,
                    metrics.critical_issues,
                    metrics.high_issues,
                    metrics.medium_issues,
                    metrics.low_issues
                ));
            } else if metrics.no_issues_declared {
                logger.info("Review declared no issues found.");
            }

            PostflightResult::Valid
        }
        Err(e) => {
            // Partial recovery: attempt to show what content we can
            logger.warn(&format!("Failed to parse ISSUES.md: {}", e));
            logger.info(&format!(
                "ISSUES.md has {} bytes but failed to parse.",
                file_size
            ));
            logger.info("The file may be malformed or in an unexpected format.");
            logger.info(
                "Attempting partial recovery: fix pass will proceed but may have limited success.",
            );

            // Try to read first few lines to give user a hint
            if let Ok(content) = fs::read_to_string(issues_path) {
                let preview: String = content.lines().take(5).collect::<Vec<_>>().join("\n");
                if !preview.is_empty() {
                    logger.info("ISSUES.md preview (first 5 lines):");
                    for line in preview.lines() {
                        logger.info(&format!("  {}", line));
                    }
                }
            }

            PostflightResult::Malformed(format!("Failed to parse ISSUES.md: {}", e))
        }
    }
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
                ctx.logger
                    .error(&format!("Pre-flight check failed: {}", msg));
                return Err(anyhow::anyhow!(
                    "Review pre-flight validation failed: {}",
                    msg
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
            ctx.logger.warn(&format!(
                "Skipping review cycle {} due to: {}",
                j, review_label
            ));
            skipped_cycles += 1;

            // Even though review/fix are skipped, we still check for external git changes.
            // This ensures that manual edits or external tool changes are committed,
            // maintaining the invariant that every iteration with changes gets a commit.
            // The check here is independent of whether review ran - it's about detecting
            // any modifications to the working directory since the last snapshot.
            let snap = git_snapshot()?;
            if snap != prev_snap {
                ctx.logger.success("Repository modified (external changes detected)");
                ctx.stats.changes_detected += 1;

                let agent_cmd = ctx
                    .config
                    .developer_cmd
                    .clone()
                    .or_else(|| ctx.registry.developer_cmd(ctx.developer_agent));
                if let Some(agent_cmd) = agent_cmd {
                    ctx.logger
                        .info("Creating commit with auto-generated message...");

                    // Get git identity from config
                    let git_name = ctx.config.git_user_name.as_deref();
                    let git_email = ctx.config.git_user_email.as_deref();

                    match commit_with_auto_message_result(&agent_cmd, git_name, git_email) {
                        crate::git_helpers::CommitResult::Success(oid) => {
                            ctx.logger.success(&format!("Commit created successfully: {}", oid));
                            ctx.stats.commits_created += 1;
                        }
                        crate::git_helpers::CommitResult::NoChanges => {
                            ctx.logger.info("No commit created (no meaningful changes)");
                        }
                        crate::git_helpers::CommitResult::Failed(err) => {
                            ctx.logger
                                .error(&format!("Failed to create commit: {}", err));
                            return Err(anyhow::anyhow!(err));
                        }
                    }
                } else {
                    ctx.logger
                        .warn("Unable to get developer agent command for commit");
                }
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
        let log_dir = format!(".agent/logs/reviewer_review_{}", j);

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
                ctx.logger.success("Issues extracted from agent output (JSON)");
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
                .map(|s| !s.trim().is_empty())
                .unwrap_or(false);

            if agent_wrote_file {
                ctx.logger.info("Using agent-written ISSUES.md (legacy mode)");
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
                    "Post-flight check: {}. Proceeding with fix pass anyway.",
                    msg
                ));
                // If using a problematic agent, suggest alternatives
                if should_use_universal_prompt(
                    ctx.reviewer_agent,
                    ctx.config.reviewer_model.as_deref(),
                    ctx.config.force_universal_prompt,
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
                    "Post-flight check: {}. The fix pass may not work correctly.",
                    msg
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

        // Check for changes and create commit if modified
        let snap = git_snapshot()?;
        if snap != prev_snap {
            ctx.logger.success("Repository modified during fix pass");
            ctx.stats.changes_detected += 1;

            // Create a commit with auto-generated message
            // This is done by the orchestrator, not the agent
            // Note: commit_with_auto_message has fallback behavior if LLM fails
            let agent_cmd = ctx
                .config
                .developer_cmd
                .clone()
                .or_else(|| ctx.registry.developer_cmd(ctx.developer_agent));
            if let Some(agent_cmd) = agent_cmd {
                ctx.logger
                    .info("Creating commit with auto-generated message...");

                // Get git identity from config
                let git_name = ctx.config.git_user_name.as_deref();
                let git_email = ctx.config.git_user_email.as_deref();

                match commit_with_auto_message_result(&agent_cmd, git_name, git_email) {
                    crate::git_helpers::CommitResult::Success(oid) => {
                        ctx.logger.success(&format!("Commit created successfully: {}", oid));
                        ctx.stats.commits_created += 1;
                    }
                    crate::git_helpers::CommitResult::NoChanges => {
                        // No meaningful changes to commit
                        ctx.logger.info("No commit created (no meaningful changes)");
                    }
                    crate::git_helpers::CommitResult::Failed(err) => {
                        // Actual git operation failed - this is critical
                        // The commit_with_auto_message function handles LLM failures internally
                        // So this error indicates a real git problem
                        ctx.logger
                            .error(&format!("Failed to create commit (git operation failed): {}", err));
                        // Don't continue - this is a real error that needs attention
                        return Err(anyhow::anyhow!(err));
                    }
                }
            } else {
                ctx.logger
                    .warn("Unable to get developer agent command for commit");
            }
        }
        prev_snap = snap;
    }

    // Provide feedback if any review cycles were skipped
    if skipped_cycles > 0 {
        let total_cycles = ctx.config.reviewer_reviews;
        ctx.logger.warn(&format!(
            "{} of {} review cycle(s) were skipped due to diff retrieval failures.",
            skipped_cycles, total_cycles
        ));
        ctx.logger.info(
            "This may indicate a git repository issue or that no changes have been made yet.",
        );
        if skipped_cycles == total_cycles {
            ctx.logger.warn("No review cycles were completed. Consider checking your git repository state.");
        }
    }

    Ok(ReviewResult {
        completed_early: false,
    })
}

// Note: try_extract_issues_from_log function was removed.
// Issues extraction is now handled by the centralized result_extraction module.
// The orchestrator always writes ISSUES.md using extract_issues().

// Note: generate_commit_message function was removed.
// Commit messages are now generated inline by the orchestrator using
// commit_with_auto_message() in git_helpers/repo.rs.

/// Check if the reviewer agent should use the universal/simplified prompt.
///
/// Some AI agents have known compatibility issues with complex structured prompts.
/// This function detects those agents and returns true if the universal prompt
/// should be used instead.
///
/// The universal prompt can also be forced via the `RALPH_REVIEWER_UNIVERSAL_PROMPT`
/// environment variable or the `force_universal_prompt` config setting.
fn should_use_universal_prompt(agent: &str, model_flag: Option<&str>, force: bool) -> bool {
    // If explicitly forced via config/env, always use universal prompt
    if force {
        return true;
    }

    // Detect GLM, ZhipuAI, and other known-problematic agents, including cases
    // where the model is selected via provider/model flags.
    is_problematic_prompt_target(agent, model_flag)
}

/// Build the review prompt based on configuration and agent type.
fn build_review_prompt(
    ctx: &PhaseContext<'_>,
    reviewer_context: ContextLevel,
    guidelines: Option<&ReviewGuidelines>,
) -> (String, String) {
    // Check if we should use the universal prompt for this agent
    let use_universal = should_use_universal_prompt(
        ctx.reviewer_agent,
        ctx.config.reviewer_model.as_deref(),
        ctx.config.force_universal_prompt,
    );

    if use_universal {
        let reason = if ctx.config.force_universal_prompt {
            "forced via config/env"
        } else {
            "better compatibility"
        };
        ctx.logger.info(&format!(
            "Using universal/simplified review prompt for agent '{}' ({})",
            ctx.reviewer_agent, reason
        ));
        return (
            "review (universal)".to_string(),
            prompt_universal_review(reviewer_context),
        );
    }

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

            // Get the diff from the starting commit to pass directly to the reviewer
            // This keeps agents isolated from git operations
            let diff = match get_git_diff_from_start() {
                Ok(d) if !d.trim().is_empty() => {
                    let original_size = d.len();
                    // For reviewer, use truncation for very large diffs (not chunking)
                    // Chunking is only for commit message generation where we need to combine results
                    let (truncated_diff, was_truncated) = crate::git_helpers::validate_and_truncate_diff(d);
                    if was_truncated {
                        ctx.logger.warn(&format!(
                            "Review diff truncated from {} to {} bytes for LLM processing",
                            original_size,
                            truncated_diff.len()
                        ));
                    }
                    truncated_diff
                }
                Ok(_) => {
                    ctx.logger
                        .warn("No diff found from starting commit; review will be skipped for this cycle");
                    // Return empty prompt to signal no review should be done
                    return (
                        "review (incremental - skipped)".to_string(),
                        String::new(),
                    );
                }
                Err(e) => {
                    // Diff retrieval failed - this is a more serious issue
                    // Return an error result to signal the caller should skip this cycle
                    ctx.logger.error(&format!(
                        "Failed to get diff from starting commit: {}; skipping review cycle",
                        e
                    ));
                    ctx.logger.info(
                        "This may indicate a git repository issue. The review cycle will be skipped.",
                    );
                    // Return empty prompt to signal no review should be done
                    return (
                        "review (incremental - error)".to_string(),
                        String::new(),
                    );
                }
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
