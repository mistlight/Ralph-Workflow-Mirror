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
use crate::git_helpers::{
    get_baseline_summary, git_snapshot, update_review_baseline, CommitResultFallback,
};
use crate::logger::{print_progress, Logger};
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

        // Display baseline information
        match get_baseline_summary() {
            Ok(summary) => {
                if ctx.config.verbosity.is_debug() {
                    // Show detailed baseline information in verbose mode
                    ctx.logger.info(&summary.format_detailed());
                } else {
                    ctx.logger.info(&summary.format_compact());
                }
                if summary.is_stale {
                    ctx.logger.warn(&format!(
                        "Baseline is stale ({} commits behind). Consider updating the baseline to focus the review on recent changes.",
                        summary.commits_since
                    ));
                }
            }
            Err(e) => {
                ctx.logger
                    .warn(&format!("Unable to retrieve baseline information: {e}"));
            }
        }

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

        // NOTE: Review baseline is NOT captured here. For the first cycle, we use
        // start_commit (via ReviewBaseline::NotSet fallback). For subsequent cycles,
        // we use the baseline that was updated after the previous fix pass.
        // This ensures the reviewer sees actual changes rather than an empty diff.

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

        // UPDATE REVIEW BASELINE: Move baseline forward after fixes
        // This ensures the next review cycle sees only new changes
        if let Err(e) = update_review_baseline() {
            ctx.logger.warn(&format!(
                "Failed to update review baseline: {e}. Next review may see old changes."
            ));
        }

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

/// Log prefix-based file search results.
fn log_prefix_search_results(logger: &Logger, parent: &Path, prefix: &str) {
    use crate::files::result_extraction::file_finder::{
        find_log_files_with_prefix, find_subdirs_with_prefix,
    };

    logger.info(&format!("Debug: Parent directory: {}", parent.display()));
    logger.info(&format!("Debug: Log prefix: '{prefix}'"));

    // Check for prefix-based log files (PRIMARY mode)
    let prefix_files_result: std::io::Result<Vec<std::path::PathBuf>> =
        find_log_files_with_prefix(parent, prefix);
    match prefix_files_result {
        Ok(files) if !files.is_empty() => {
            logger.info(&format!(
                "Debug: Found {} prefix-matched file(s)",
                files.len()
            ));
            for file in &files {
                logger.info(&format!("Debug:   - {}", file.display()));
            }
        }
        Ok(_) => {
            logger.info("Debug: No prefix-matched log files found");
        }
        Err(e) => {
            logger.info(&format!("Debug: Error searching for prefix files: {e}"));
        }
    }

    // Check for subdirectory fallback
    let subdirs_result: std::io::Result<Vec<std::path::PathBuf>> =
        find_subdirs_with_prefix(parent, prefix);
    match subdirs_result {
        Ok(subdirs) if !subdirs.is_empty() => {
            logger.info(&format!(
                "Debug: Found {} subdirectory(s) matching prefix",
                subdirs.len()
            ));
            for subdir in &subdirs {
                logger.info(&format!("Debug:   - {}", subdir.display()));
            }
        }
        Ok(_) => {
            logger.info("Debug: No matching subdirectories found");
        }
        Err(e) => {
            logger.info(&format!("Debug: Error searching for subdirs: {e}"));
        }
    }
}

/// Log directory contents and file details.
fn log_directory_details(logger: &Logger, log_dir_path: &Path) {
    // Count log files in the directory
    match std::fs::read_dir(log_dir_path) {
        Ok(entries) => {
            let files: Vec<_> = entries.filter_map(Result::ok).collect();
            let file_count = files.len();
            logger.info(&format!(
                "Debug: Log directory exists with {file_count} file(s)"
            ));
            // List files for diagnosis
            for entry in &files {
                logger.info(&format!("Debug:   - {}", entry.path().display()));
            }
        }
        Err(e) => {
            logger.info(&format!("Debug: Error reading log directory: {e}"));
        }
    }

    // Try to read first log file content for diagnosis
    if let Ok(mut entries) = std::fs::read_dir(log_dir_path) {
        if let Some(Ok(first_entry)) = entries.next() {
            logger.info(&format!(
                "Debug: Reading first file for diagnosis: {}",
                first_entry.path().display()
            ));
            match std::fs::read_to_string(first_entry.path()) {
                Ok(content) => {
                    let preview: String = content.chars().take(300).collect();
                    logger.info(&format!(
                        "Debug: First log file preview (300 chars):\n{preview}"
                    ));
                    let line_count = content.lines().count();
                    logger.info(&format!("Debug: Log file has {line_count} line(s)"));

                    // Check if file contains JSON events
                    let json_count = content
                        .lines()
                        .filter(|line| line.trim().starts_with('{'))
                        .count();
                    logger.info(&format!("Debug: Found {json_count} JSON line(s)"));

                    // Check for result events
                    let result_count = content
                        .lines()
                        .filter(|line| {
                            line.contains(r#""type":"result""#)
                                || line.contains(r#""type": "result""#)
                        })
                        .count();
                    logger.info(&format!("Debug: Found {result_count} result event line(s)"));
                }
                Err(e) => {
                    logger.info(&format!("Debug: Error reading file content: {e}"));
                }
            }
        }
    }
}

/// Log diagnostic information when JSON extraction fails.
///
/// Provides detailed debug logging about log file search strategies,
/// file contents, and why extraction might have failed.
fn log_extraction_diagnostics(logger: &Logger, log_dir: &str) {
    let log_dir_path = Path::new(log_dir);

    // Show the exact log path being searched
    logger.info(&format!("Debug: Log path searched: {log_dir}"));

    // Extract parent and prefix for prefix-mode search info
    let parent = log_dir_path.parent().unwrap_or_else(|| Path::new("."));
    let prefix = log_dir_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("");

    if !prefix.is_empty() {
        log_prefix_search_results(logger, parent, prefix);
    }

    // Check if log path exists as directory
    if log_dir_path.exists() {
        if log_dir_path.is_dir() {
            log_directory_details(logger, log_dir_path);
        } else {
            logger.info(&format!(
                "Debug: Path exists but is not a directory: {}",
                log_dir_path.display()
            ));
        }
    } else {
        logger.info(&format!("Debug: Log path does not exist: {log_dir}"));
    }
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
            let warning = extraction.validation_warning.clone().unwrap_or_default();
            ctx.logger
                .warn(&format!("Issues written but validation failed: {warning}"));
            // Debug logging: show the actual content for diagnosis
            if ctx.config.verbosity.is_debug() {
                ctx.logger.info(&format!(
                    "Extracted content (first 200 chars): {}",
                    content.chars().take(200).collect::<String>()
                ));
                ctx.logger
                    .info(&format!("Content length: {} characters", content.len()));
                ctx.logger.info("Hint: Content may not match expected issue format (checkboxes, severity markers, file paths)");
            }
        }
    } else {
        // JSON extraction failed - log for debugging
        ctx.logger
            .info("No JSON result event found in reviewer logs");

        // Debug logging: provide more diagnostic information
        if ctx.config.verbosity.is_debug() {
            log_extraction_diagnostics(ctx.logger, &log_dir);
        }

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

    // Read PROMPT.md, PLAN.md, and ISSUES.md for context
    let prompt_content = fs::read_to_string("PROMPT.md").unwrap_or_default();
    let plan_content = fs::read_to_string(".agent/PLAN.md").unwrap_or_default();
    let issues_content = fs::read_to_string(".agent/ISSUES.md").unwrap_or_default();

    let fix_prompt = prompt_for_agent(
        Role::Reviewer,
        Action::Fix,
        reviewer_context,
        ctx.template_context,
        PromptConfig::new().with_prompt_plan_and_issues(
            prompt_content,
            plan_content,
            issues_content,
        ),
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
