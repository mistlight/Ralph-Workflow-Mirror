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
//! - [`validation`] - Pre-flight and post-flight validation checks

/// Maximum continuation attempts for fix passes to prevent infinite loops.
///
/// This is a safety limit for the outer loop that continues while
/// status != "all_issues_addressed". The fix agent should complete
/// well before reaching this limit under normal circumstances.
const MAX_CONTINUATION_ATTEMPTS: usize = 100;

use crate::agents::AgentRole;
use crate::checkpoint::restore::ResumeContext;
use crate::checkpoint::{save_checkpoint_with_workspace, CheckpointBuilder, PipelinePhase};
use crate::files::extract_issues;
use crate::files::llm_output_extraction::xsd_validation::XsdValidationError;
use crate::files::llm_output_extraction::{
    archive_xml_file_with_workspace, extract_fix_result_xml, extract_issues_xml,
    extract_xml_with_file_fallback_with_workspace, try_extract_from_file_with_workspace,
    validate_fix_result_xml, validate_issues_xml, xml_paths, IssuesElements,
};
use crate::files::result_extraction::extract_file_paths_from_issues;
use crate::files::{
    clean_context_for_reviewer_with_workspace, delete_issues_file_for_isolation_with_workspace,
    update_status_with_workspace,
};
use crate::git_helpers::{
    get_baseline_summary, git_snapshot, update_review_baseline, CommitResultFallback,
};
use crate::logger::{print_progress, Logger};
use crate::phases::commit::commit_with_generated_message;
use crate::phases::get_primary_commit_agent;
use crate::pipeline::{run_xsd_retry_with_session, PipelineRuntime, XsdRetryConfig};
use crate::prompts::{
    get_stored_or_generate_prompt, prompt_fix_xml_with_context, prompt_fix_xsd_retry_with_context,
    prompt_review_xml_with_references, prompt_review_xsd_retry_with_context, ContextLevel,
    PromptContentBuilder,
};
use crate::reducer::state::AgentChainState;
use crate::workspace::Workspace;
use std::path::Path;
use std::time::Duration;

mod validation;
pub use validation::{
    post_flight_review_check, pre_flight_review_check, PostflightResult, PreflightResult,
};

use super::context::PhaseContext;

use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};

use std::time::Instant;

/// Result of the review phase.
pub struct ReviewResult {
    /// Whether the review completed early due to no issues found.
    pub completed_early: bool,
}

fn is_auth_failure_error(err: &anyhow::Error) -> bool {
    let msg = err.to_string().to_lowercase();
    msg.contains("authentication error")
        || msg.contains("auth/credential")
        || msg.contains("unauthorized")
        || msg.contains("credential")
        || msg.contains("api key")
}

fn build_agent_chain_state(
    fallback_config: &crate::agents::fallback::FallbackConfig,
    role: AgentRole,
    primary_agent: &str,
) -> AgentChainState {
    let mut agents: Vec<String> = fallback_config.get_fallbacks(role).to_vec();

    if !agents.iter().any(|agent| agent == primary_agent) {
        agents.insert(0, primary_agent.to_string());
    }

    let mut seen = std::collections::HashSet::new();
    agents.retain(|agent| seen.insert(agent.clone()));

    if agents.is_empty() {
        agents.push(primary_agent.to_string());
    }

    let models_per_agent = agents.iter().map(|_| Vec::new()).collect();
    let mut chain = AgentChainState::initial()
        .with_agents(agents, models_per_agent, role)
        .with_max_cycles(fallback_config.max_cycles);

    if let Some(index) = chain.agents.iter().position(|agent| agent == primary_agent) {
        chain.current_agent_index = index;
    }

    chain
}

fn advance_agent_chain_on_auth_failure(
    chain: &mut AgentChainState,
    fallback_config: &crate::agents::fallback::FallbackConfig,
) -> anyhow::Result<Option<u64>> {
    let next = chain.switch_to_next_agent();
    if next.is_exhausted() || next.current_agent().is_none() {
        anyhow::bail!("Agent fallback chain exhausted after authentication failures");
    }

    let backoff_delay = if next.retry_cycle > chain.retry_cycle {
        Some(fallback_config.calculate_backoff(next.retry_cycle))
    } else {
        None
    };

    *chain = next;
    Ok(backoff_delay)
}

fn current_agent_from_chain(chain: &AgentChainState) -> anyhow::Result<&str> {
    chain
        .current_agent()
        .map(|agent| agent.as_str())
        .ok_or_else(|| anyhow::anyhow!("No available agent in fallback chain"))
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
/// * `resume_context` - Optional resume context for resumed sessions
///
/// # Returns
///
/// Returns `Ok(ReviewResult)` on success, or an error if a critical failure occurs.
pub fn run_review_phase(
    ctx: &mut PhaseContext<'_>,
    start_pass: u32,
    resume_context: Option<&ResumeContext>,
) -> anyhow::Result<ReviewResult> {
    let reviewer_context = ContextLevel::from(ctx.config.reviewer_context);
    let fallback_config = ctx.registry.fallback_config();

    // Skip if no review cycles configured
    if ctx.config.reviewer_reviews == 0 {
        ctx.logger
            .info("Skipping review phase (reviewer_reviews=0)");
        return Ok(ReviewResult {
            completed_early: false,
        });
    }

    // Clean context for reviewer if using minimal context (only if review is enabled)
    if reviewer_context == ContextLevel::Minimal {
        clean_context_for_reviewer_with_workspace(
            ctx.workspace,
            ctx.logger,
            ctx.config.isolation_mode,
        )?;
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
        let resuming_into_review = resume_context.is_some() && j == start_pass;
        // Save checkpoint at start of each iteration
        if ctx.config.features.checkpoint_enabled {
            let builder = CheckpointBuilder::new()
                .phase(
                    PipelinePhase::Review,
                    ctx.config.developer_iters,
                    ctx.config.developer_iters,
                )
                .reviewer_pass(j, ctx.config.reviewer_reviews)
                .capture_from_context(
                    ctx.config,
                    ctx.registry,
                    ctx.developer_agent,
                    ctx.reviewer_agent,
                    ctx.logger,
                    &ctx.run_context,
                )
                .with_executor_from_context(std::sync::Arc::clone(&ctx.executor_arc))
                .with_execution_history(ctx.execution_history.clone())
                .with_prompt_history(ctx.clone_prompt_history());

            if let Some(checkpoint) = builder.build() {
                let _ = save_checkpoint_with_workspace(ctx.workspace, &checkpoint);
            }
        }

        // Record this pass as completed
        ctx.record_reviewer_pass();

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
            ctx.workspace,
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
        update_status_with_workspace(ctx.workspace, "Reviewing code", ctx.config.isolation_mode)?;

        let review_label = "review";

        // Use prompt replay if available, otherwise build new review prompt
        let review_prompt_key = format!("review_{}", j);
        let (review_prompt, was_replayed) =
            get_stored_or_generate_prompt(&review_prompt_key, &ctx.prompt_history, || {
                let plan_content = ctx
                    .workspace
                    .read(Path::new(".agent/PLAN.md"))
                    .unwrap_or_default();

                let (changes_content, baseline_oid_for_prompts) =
                    match crate::git_helpers::get_git_diff_for_review_with_workspace(ctx.workspace)
                    {
                        Ok((diff, baseline_oid)) => (diff, baseline_oid),
                        Err(e) => {
                            ctx.logger.warn(&format!(
                                "Failed to get baseline diff for review prompt: {e}"
                            ));
                            (String::new(), String::new())
                        }
                    };

                let refs = PromptContentBuilder::new(ctx.workspace)
                    .with_plan(plan_content)
                    .with_diff(changes_content, &baseline_oid_for_prompts)
                    .build();

                prompt_review_xml_with_references(ctx.template_context, &refs)
            });

        // Capture the review prompt for checkpoint/resume (only if newly generated)
        if !review_prompt.is_empty() {
            if !was_replayed {
                ctx.capture_prompt(&review_prompt_key, &review_prompt);
            } else {
                ctx.logger.info(&format!(
                    "Using stored prompt from checkpoint for determinism: {}",
                    review_prompt_key
                ));
            }
        }

        // Check if the review prompt is empty (e.g., due to diff retrieval failure)
        // If so, skip the review and fix passes but still check for git changes
        if review_prompt.is_empty() {
            ctx.logger
                .warn(&format!("Skipping review cycle {j} due to: {review_label}"));
            skipped_cycles += 1;

            // Check for external git changes and commit if found
            prev_snap = handle_skipped_cycle(ctx, j, &prev_snap)?;
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

        let mut agent_chain =
            build_agent_chain_state(fallback_config, AgentRole::Reviewer, ctx.reviewer_agent);
        let mut active_agent = current_agent_from_chain(&agent_chain)?;

        // Run review pass with auth-aware fallback
        let review_result = loop {
            let result = run_review_pass(ctx, j, review_label, &review_prompt, Some(active_agent))?;

            if result.auth_failure {
                ctx.logger.warn(&format!(
                    "Auth failure during review with '{}', switching agent",
                    active_agent
                ));
                let backoff_delay =
                    advance_agent_chain_on_auth_failure(&mut agent_chain, fallback_config)?;
                active_agent = current_agent_from_chain(&agent_chain)?;
                if let Some(delay_ms) = backoff_delay.filter(|d| *d > 0) {
                    ctx.logger.info(&format!(
                        "Backoff before retrying with '{}': {}ms",
                        active_agent, delay_ms
                    ));
                    ctx.registry
                        .retry_timer()
                        .sleep(Duration::from_millis(delay_ms));
                }
                continue;
            }

            break result;
        };

        // Check for early exit (no issues found)
        if review_result.early_exit {
            return Ok(ReviewResult {
                completed_early: true,
            });
        }

        // Run fix pass with auth-aware fallback
        loop {
            match run_fix_pass(
                ctx,
                j,
                reviewer_context,
                if resuming_into_review {
                    resume_context
                } else {
                    None
                },
                Some(active_agent),
            ) {
                Ok(()) => break,
                Err(err) if is_auth_failure_error(&err) => {
                    ctx.logger.warn(&format!(
                        "Auth failure during fix with '{}', switching agent",
                        active_agent
                    ));
                    let backoff_delay =
                        advance_agent_chain_on_auth_failure(&mut agent_chain, fallback_config)?;
                    active_agent = current_agent_from_chain(&agent_chain)?;
                    if let Some(delay_ms) = backoff_delay.filter(|d| *d > 0) {
                        ctx.logger.info(&format!(
                            "Backoff before retrying with '{}': {}ms",
                            active_agent, delay_ms
                        ));
                        ctx.registry
                            .retry_timer()
                            .sleep(Duration::from_millis(delay_ms));
                    }
                }
                Err(err) => return Err(err),
            }
        }

        // UPDATE REVIEW BASELINE: Move baseline forward after fixes
        // This ensures the next review cycle sees only new changes
        if let Err(e) = update_review_baseline() {
            ctx.logger.warn(&format!(
                "Failed to update review baseline: {e}. Next review may see old changes."
            ));
        }

        // Check for changes and create commit if modified
        prev_snap = handle_post_fix_commit(ctx, j, &prev_snap)?;

        // Save checkpoint after review-fix cycle completes (if enabled)
        // This checkpoint captures the completed cycle so resume won't re-run it
        if ctx.config.features.checkpoint_enabled {
            let next_pass = j + 1;
            let builder = CheckpointBuilder::new()
                .phase(
                    PipelinePhase::Review,
                    ctx.config.developer_iters,
                    ctx.config.developer_iters,
                )
                .reviewer_pass(next_pass, ctx.config.reviewer_reviews)
                .capture_from_context(
                    ctx.config,
                    ctx.registry,
                    ctx.developer_agent,
                    ctx.reviewer_agent,
                    ctx.logger,
                    &ctx.run_context,
                )
                .with_executor_from_context(std::sync::Arc::clone(&ctx.executor_arc))
                .with_execution_history(ctx.execution_history.clone())
                .with_prompt_history(ctx.clone_prompt_history());

            if let Some(checkpoint) = builder.build() {
                let _ = save_checkpoint_with_workspace(ctx.workspace, &checkpoint);
            }
        }
    }

    // Provide feedback if any review cycles were skipped
    log_skipped_cycles_feedback(ctx, skipped_cycles);

    Ok(ReviewResult {
        completed_early: false,
    })
}

/// Handle a skipped review cycle by checking for external git changes.
fn handle_skipped_cycle(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    prev_snap: &str,
) -> anyhow::Result<String> {
    let start_time = Instant::now();

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

            // Check if diff is empty before requesting commit message generation
            if diff.trim().is_empty() {
                ctx.logger
                    .info("Skipping commit (no meaningful changes in diff)");

                let duration = start_time.elapsed().as_secs();
                let step = ExecutionStep::new(
                    "Review",
                    iteration,
                    "commit",
                    StepOutcome::skipped("No meaningful changes to commit".to_string()),
                )
                .with_duration(duration);
                ctx.execution_history.add_step(step);
            } else {
                // Get git identity from config
                let git_name = ctx.config.git_user_name.as_deref();
                let git_email = ctx.config.git_user_email.as_deref();

                match commit_with_generated_message(&diff, &agent, git_name, git_email, ctx) {
                    CommitResultFallback::Success(oid) => {
                        ctx.logger
                            .success(&format!("Commit created successfully: {oid}"));
                        ctx.stats.commits_created += 1;

                        let duration = start_time.elapsed().as_secs();
                        let step = ExecutionStep::new(
                            "Review",
                            iteration,
                            "commit",
                            StepOutcome::success(Some(oid.to_string()), vec![]),
                        )
                        .with_agent(&agent)
                        .with_duration(duration);
                        ctx.execution_history.add_step(step);
                    }
                    CommitResultFallback::NoChanges => {
                        ctx.logger.info("No commit created (no meaningful changes)");

                        let duration = start_time.elapsed().as_secs();
                        let step = ExecutionStep::new(
                            "Review",
                            iteration,
                            "commit",
                            StepOutcome::skipped("No meaningful changes to commit".to_string()),
                        )
                        .with_duration(duration);
                        ctx.execution_history.add_step(step);
                    }
                    CommitResultFallback::Failed(err) => {
                        ctx.logger.error(&format!("Failed to create commit: {err}"));

                        let duration = start_time.elapsed().as_secs();
                        let step = ExecutionStep::new(
                            "Review",
                            iteration,
                            "commit",
                            StepOutcome::failure(err.to_string(), false),
                        )
                        .with_duration(duration);
                        ctx.execution_history.add_step(step);

                        return Err(anyhow::anyhow!(err));
                    }
                }
            }
        } else {
            ctx.logger.warn("Unable to get commit agent for commit");

            let duration = start_time.elapsed().as_secs();
            let step = ExecutionStep::new(
                "Review",
                iteration,
                "commit",
                StepOutcome::failure("No commit agent available".to_string(), true),
            )
            .with_duration(duration);
            ctx.execution_history.add_step(step);
        }
    }
    Ok(snap)
}

/// Result of running a review pass.
#[derive(Debug)]
pub struct ReviewPassResult {
    /// Whether the review found no issues and should exit early.
    pub early_exit: bool,
    /// Whether an authentication/credential error was detected.
    /// When true, the caller should trigger agent fallback instead of retrying.
    pub auth_failure: bool,
}

/// Result of parsing review output.
#[derive(Debug)]
enum ParseResult {
    /// Successfully parsed with issues found
    IssuesFound { issues: Vec<String> },
    /// Successfully parsed with explicit "no issues" declaration
    NoIssuesExplicit,
    /// Failed to parse - includes error description for re-prompting
    ParseFailed(String),
}

/// Log prefix-based file search results.
fn log_prefix_search_results(
    logger: &Logger,
    workspace: &dyn Workspace,
    parent: &Path,
    prefix: &str,
) {
    use crate::files::result_extraction::file_finder::{
        find_log_files_with_prefix, find_subdirs_with_prefix,
    };

    logger.info(&format!("Debug: Parent directory: {}", parent.display()));
    logger.info(&format!("Debug: Log prefix: '{prefix}'"));

    // Check for prefix-based log files (PRIMARY mode)
    let prefix_files_result: std::io::Result<Vec<std::path::PathBuf>> =
        find_log_files_with_prefix(workspace, parent, prefix);
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
        find_subdirs_with_prefix(workspace, parent, prefix);
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
fn log_directory_details(logger: &Logger, workspace: &dyn Workspace, log_dir_path: &Path) {
    // Count log files in the directory
    match workspace.read_dir(log_dir_path) {
        Ok(entries) => {
            let file_count = entries.len();
            logger.info(&format!(
                "Debug: Log directory exists with {file_count} file(s)"
            ));
            // List files for diagnosis
            for entry in &entries {
                logger.info(&format!("Debug:   - {}", entry.path().display()));
            }
        }
        Err(e) => {
            logger.info(&format!("Debug: Error reading log directory: {e}"));
        }
    }

    // Try to read first log file content for diagnosis
    if let Ok(entries) = workspace.read_dir(log_dir_path) {
        if let Some(first_entry) = entries.first() {
            logger.info(&format!(
                "Debug: Reading first file for diagnosis: {}",
                first_entry.path().display()
            ));
            match workspace.read(first_entry.path()) {
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
fn log_extraction_diagnostics(logger: &Logger, workspace: &dyn Workspace, log_dir: &str) {
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
        log_prefix_search_results(logger, workspace, parent, prefix);
    }

    // Check if log path exists as directory
    if workspace.exists(log_dir_path) {
        if workspace.is_dir(log_dir_path) {
            log_directory_details(logger, workspace, log_dir_path);
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
///
/// This function implements a nested loop structure similar to fix:
/// - **Outer loop (continuation)**: Not used for review (single pass)
/// - **Inner loop (XSD retry)**: Retry XSD validation with error feedback (max 100)
pub fn run_review_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    review_label: &str,
    _review_prompt: &str, // Unused - we build XML prompt internally
    _agent: Option<&str>,
) -> anyhow::Result<ReviewPassResult> {
    let active_agent = _agent.unwrap_or(ctx.reviewer_agent);
    let issues_path = Path::new(".agent/ISSUES.md");
    let max_xsd_retries = crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS as usize;

    // Read PROMPT.md, PLAN.md for context
    let prompt_content = ctx
        .workspace
        .read(Path::new("PROMPT.md"))
        .unwrap_or_default();
    let plan_content = ctx
        .workspace
        .read(Path::new(".agent/PLAN.md"))
        .unwrap_or_default();

    // Get the diff for review context.
    // IMPORTANT: This must be the diff from the review baseline (or start_commit for the first
    // cycle) to the current state on disk. It may or may not correspond to the last commit.
    let (changes_content, baseline_oid_for_prompts) =
        match crate::git_helpers::get_git_diff_for_review_with_workspace(ctx.workspace) {
            Ok((diff, baseline_oid)) => (diff, baseline_oid),
            Err(e) => {
                ctx.logger
                    .warn(&format!("Failed to get baseline diff for review: {e}"));
                (String::new(), String::new())
            }
        };

    // Session info for potential session continuation on XSD retries
    let mut session_info: Option<crate::pipeline::session::SessionInfo> = None;

    // Track previous log directory for reading errors and output on retries
    let mut prev_log_dir: Option<String> = None;

    // Inner loop: XSD validation retry with error feedback
    for retry_num in 0..max_xsd_retries {
        let is_retry = retry_num > 0;
        let log_dir = format!(".agent/logs/reviewer_review_{j}_attempt_{retry_num}");

        // Before each retry, check if the XML file is writable and clean up if locked
        if is_retry {
            use crate::files::io::check_and_cleanup_xml_before_retry_with_workspace;
            use std::path::Path;
            let xml_path = Path::new(crate::files::llm_output_extraction::xml_paths::ISSUES_XML);
            let _ = check_and_cleanup_xml_before_retry_with_workspace(
                ctx.workspace,
                xml_path,
                ctx.logger,
            );
        }

        // For initial attempt, use XML prompt
        // For retries, use XSD retry prompt with error feedback
        let review_prompt_xml = if !is_retry {
            // First attempt - use initial XML prompt
            let prompt_key = format!("review_{}", j);
            let (prompt, was_replayed) =
                get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                    let refs = PromptContentBuilder::new(ctx.workspace)
                        .with_plan(plan_content.clone())
                        .with_diff(changes_content.clone(), &baseline_oid_for_prompts)
                        .build();

                    prompt_review_xml_with_references(ctx.template_context, &refs)
                });

            if !was_replayed {
                ctx.capture_prompt(&prompt_key, &prompt);
            } else {
                ctx.logger.info(&format!(
                    "Using stored prompt from checkpoint for determinism: {}",
                    prompt_key
                ));
            }

            prompt
        } else {
            // XSD retry - use retry prompt with error feedback
            ctx.logger.info(&format!(
                "  In-session retry {}/{} for XSD validation",
                retry_num,
                max_xsd_retries - 1
            ));

            // Read from PREVIOUS attempt's directory (the one that just failed)
            // prev_log_dir should be Some because is_retry means retry_num > 0
            let prev_dir = prev_log_dir.as_ref().ok_or_else(|| {
                anyhow::anyhow!(
                    "Internal error: prev_log_dir missing on retry (iteration {})",
                    retry_num
                )
            })?;
            let last_output = read_last_review_output(Path::new(prev_dir), ctx.workspace);

            // Get XSD error from previous iteration
            let xsd_error = get_last_xsd_error(ctx, Path::new(prev_dir));

            if let Some(ref error) = xsd_error {
                ctx.logger.info(&format!("  XSD error: {}", error));
            }

            prompt_review_xsd_retry_with_context(
                ctx.template_context,
                &prompt_content,
                &plan_content,
                &changes_content,
                xsd_error.as_deref().unwrap_or("Unknown error"),
                &last_output,
                ctx.workspace,
            )
        };

        // Log the review prompt details for debugging (when verbose)
        if ctx.config.verbosity.is_debug() && !is_retry {
            ctx.logger.info(&format!(
                "Review prompt length: {} characters",
                review_prompt_xml.len()
            ));
        }

        let attempt_start = Instant::now();

        // Run the agent with session continuation for XSD retries
        let xsd_result = {
            let mut runtime = PipelineRuntime {
                timer: ctx.timer,
                logger: ctx.logger,
                colors: ctx.colors,
                config: ctx.config,
                executor: ctx.executor,
                executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
                workspace: ctx.workspace,
            };

            // Output validator: checks if reviewer produced valid output
            // Priority: 1) File-based XML at .agent/tmp/issues.xml
            //           2) JSON result events in log files
            let validate_output: crate::pipeline::OutputValidator =
                |ws: &dyn crate::workspace::Workspace,
                 log_dir_path: &Path,
                 _logger: &crate::logger::Logger|
                 -> std::io::Result<bool> {
                    use crate::files::llm_output_extraction::{has_valid_xml_output, xml_paths};

                    // First, check if XML file was written directly (file-based mode)
                    if has_valid_xml_output(ws, Path::new(xml_paths::ISSUES_XML)) {
                        return Ok(true); // Valid XML file exists
                    }

                    // Fall back to JSON log extraction (legacy/streaming mode)
                    use crate::files::result_extraction::extract_last_result;
                    match extract_last_result(ws, log_dir_path) {
                        Ok(Some(_)) => Ok(true), // Valid JSON output exists
                        Ok(None) => Ok(false),   // No valid output found
                        Err(_) => Ok(true), // On error, assume success (let extraction handle validation)
                    }
                };

            let base_label = format!(
                "{review_label} #{j}{}",
                if is_retry {
                    format!(" (retry {retry_num})")
                } else {
                    String::new()
                }
            );

            let mut xsd_retry_config = XsdRetryConfig {
                role: AgentRole::Reviewer,
                base_label: &base_label,
                prompt: &review_prompt_xml,
                logfile_prefix: &log_dir,
                runtime: &mut runtime,
                registry: ctx.registry,
                primary_agent: active_agent,
                session_info: session_info.as_ref(),
                retry_num,
                output_validator: Some(validate_output),
                workspace: ctx.workspace,
            };
            run_xsd_retry_with_session(&mut xsd_retry_config)?
        };
        ctx.stats.reviewer_runs_completed += 1;

        // Check for auth error FIRST - if detected, signal for agent fallback
        // This breaks out of the XSD retry loop immediately
        if xsd_result.auth_error_detected {
            ctx.logger
                .warn("  Auth/credential error detected during review, signaling agent fallback");
            return Ok(ReviewPassResult {
                early_exit: false,
                auth_failure: true,
            });
        }

        // Extract session info for potential retry (only if we don't have it yet)
        // IMPORTANT: Always extract from attempt 0's log directory, as that's where the
        // initial session was created. Subsequent retries use continuation with the same session.
        if session_info.is_none() {
            let first_attempt_log_dir = format!(".agent/logs/reviewer_review_{j}_attempt_0");
            let log_dir_path = Path::new(&first_attempt_log_dir);
            if let Some(agent_config) = ctx.registry.resolve_config(active_agent) {
                session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
                    log_dir_path,
                    agent_config.json_parser,
                    Some(active_agent),
                    ctx.workspace,
                );
            }
        }

        let attempt_duration = attempt_start.elapsed().as_secs();

        // Extract and validate the review output using XML extraction
        let parse_result = extract_and_validate_review_output_xml(ctx, &log_dir, issues_path)?;

        match parse_result {
            ParseResult::IssuesFound { issues } => {
                // POST-FLIGHT VALIDATION: Check review output after agent completes
                handle_postflight_validation(ctx, j);

                ctx.logger
                    .success(&format!("Issues extracted: {} total", issues.len()));

                // Note: XML display is handled via UIEvent::XmlOutput in the effect handler

                let step = ExecutionStep::new(
                    "Review",
                    j,
                    "review",
                    StepOutcome::success(
                        Some(format!("{} issues found", issues.len())),
                        vec![".agent/ISSUES.md".to_string()],
                    ),
                )
                .with_agent(active_agent)
                .with_duration(attempt_duration);
                ctx.execution_history.add_step(step);

                return Ok(ReviewPassResult {
                    early_exit: false,
                    auth_failure: false,
                });
            }
            ParseResult::NoIssuesExplicit => {
                ctx.logger
                    .success(&format!("No issues found after cycle {j} - stopping early"));
                // Clean up ISSUES.md before early exit in isolation mode
                if ctx.config.isolation_mode {
                    delete_issues_file_for_isolation_with_workspace(ctx.workspace, ctx.logger)?;
                }

                let step = ExecutionStep::new(
                    "Review",
                    j,
                    "review",
                    StepOutcome::success(Some("No issues found".to_string()), vec![]),
                )
                .with_agent(active_agent)
                .with_duration(attempt_duration);
                ctx.execution_history.add_step(step);

                return Ok(ReviewPassResult {
                    early_exit: true,
                    auth_failure: false,
                });
            }
            ParseResult::ParseFailed(error_description) => {
                let step = ExecutionStep::new(
                    "Review",
                    j,
                    "review",
                    StepOutcome::failure(
                        format!("XSD validation failed: {error_description}"),
                        true,
                    ),
                )
                .with_agent(active_agent)
                .with_duration(attempt_duration);
                ctx.execution_history.add_step(step);

                // Store XSD error for next retry
                store_xsd_error_for_retry(ctx, Path::new(&log_dir), &error_description);

                // Last retry failed - write marker and continue
                if retry_num >= max_xsd_retries - 1 {
                    ctx.logger.error(&format!(
                        "Failed to get valid XML review output after {} XSD retries. Last error: {}",
                        max_xsd_retries, error_description
                    ));
                    // Write a marker file indicating the failure
                    let failure_marker = format!(
                        "# Review Output XSD Validation Failure\n\n\
                        The reviewer agent's output failed XSD validation after {} attempts.\n\n\
                        Last validation error: {}\n\n\
                        This does NOT mean there are no issues - it means the XML format was invalid.\n\n\
                        Please check the logs in .agent/logs/ for the raw reviewer output.\n",
                        max_xsd_retries, error_description
                    );
                    ctx.workspace.write(issues_path, &failure_marker)?;
                    // Continue with fix pass anyway - the fix agent will see the failure message
                    return Ok(ReviewPassResult {
                        early_exit: false,
                        auth_failure: false,
                    });
                }

                ctx.logger.warn(&format!(
                    "Review XSD validation failed (attempt {}/{}): {}",
                    retry_num + 1,
                    max_xsd_retries,
                    error_description
                ));
                // Continue to next retry with XSD error feedback
            }
        }

        // Update previous log directory for next iteration
        // This allows the next retry to read from this attempt's directory
        prev_log_dir = Some(log_dir);
    }

    // Should not reach here, but handle the case
    Ok(ReviewPassResult {
        early_exit: false,
        auth_failure: false,
    })
}

/// Extract review output using XML extraction and validate with XSD.
///
/// Returns a `ParseResult` indicating whether the output was successfully parsed,
/// explicitly declared no issues, or failed to parse (with an error description).
///
/// # Extraction Priority
///
/// 1. File-based XML at `.agent/tmp/issues.xml` (preferred for agents that write XML directly)
/// 2. JSON result events in log files
/// 3. Legacy ISSUES.md file (fallback)
fn extract_and_validate_review_output_xml(
    ctx: &mut PhaseContext<'_>,
    log_dir: &str,
    issues_path: &Path,
) -> anyhow::Result<ParseResult> {
    // Priority 1: Check for file-based XML at .agent/tmp/issues.xml
    // This is the preferred path for agents that write XML directly (e.g., opencode parser)
    if let Some(xml_content) =
        try_extract_from_file_with_workspace(ctx.workspace, Path::new(xml_paths::ISSUES_XML))
    {
        ctx.logger
            .info("Found XML in .agent/tmp/issues.xml (file-based mode)");
        return validate_and_process_issues_xml(ctx, &xml_content, issues_path);
    }

    // Priority 2: Try JSON log extraction
    let extraction = extract_issues(ctx.workspace, Path::new(log_dir))?;

    let raw_content = if let Some(content) = extraction.raw_content {
        content
    } else {
        // JSON extraction failed - check for legacy agent-written ISSUES.md
        if ctx.config.verbosity.is_debug() {
            ctx.logger
                .info("No JSON result event found in reviewer logs");
            log_extraction_diagnostics(ctx.logger, ctx.workspace, log_dir);
        }

        // Priority 3: Check for legacy agent-written ISSUES.md
        if ctx.workspace.exists(issues_path) {
            if let Ok(content) = ctx.workspace.read(issues_path) {
                if !content.trim().is_empty() {
                    ctx.logger
                        .info("Using agent-written ISSUES.md (legacy mode)");
                    content
                } else {
                    return Ok(ParseResult::ParseFailed(
                        "Agent wrote an empty ISSUES.md file. Expected XML output with <ralph-issues> tags."
                            .to_string(),
                    ));
                }
            } else {
                return Ok(ParseResult::ParseFailed(
                    "No review output captured. The agent may have failed to produce any output."
                        .to_string(),
                ));
            }
        } else {
            // All three sources failed
            return Ok(ParseResult::ParseFailed(
                "No review output captured. Agent did not write to .agent/tmp/issues.xml, \
                 no JSON result found in logs, and no ISSUES.md file exists."
                    .to_string(),
            ));
        }
    };

    // Extract XML from raw content (handles embedded XML in text)
    let xml_content = match extract_issues_xml(&raw_content) {
        Some(xml) => xml,
        None => {
            // No XML found - assume entire output is XML and validate to get specific error
            ctx.logger
                .warn("No XML tags found in output, assuming entire output is XML for validation");

            // Try to validate the raw content as XML to get specific error message
            match validate_issues_xml(&raw_content) {
                Ok(_) => {
                    // Unexpectedly valid - might be a bug in extraction, but accept it
                    ctx.logger.info(
                        "Raw content validated as XML despite no tags found (extraction bug?)",
                    );
                    raw_content
                }
                Err(e) => {
                    // Return the specific XSD error
                    return Ok(ParseResult::ParseFailed(format!(
                        "XSD validation failed: {}",
                        e.format_for_ai_retry()
                    )));
                }
            }
        }
    };

    validate_and_process_issues_xml(ctx, &xml_content, issues_path)
}

/// Helper to validate XML and process the result for issues extraction.
fn validate_and_process_issues_xml(
    ctx: &mut PhaseContext<'_>,
    xml_content: &str,
    issues_path: &Path,
) -> anyhow::Result<ParseResult> {
    // Validate the extracted XML against XSD
    let validated: Result<IssuesElements, XsdValidationError> = validate_issues_xml(xml_content);

    match validated {
        Ok(elements) => {
            // Write the validated XML to ISSUES.md
            ctx.workspace.write(issues_path, xml_content)?;

            // Archive the XML file for debugging (moves to .xml.processed)
            archive_xml_file_with_workspace(ctx.workspace, Path::new(xml_paths::ISSUES_XML));

            if elements.no_issues_found.is_some() {
                return Ok(ParseResult::NoIssuesExplicit);
            }

            if !elements.issues.is_empty() {
                return Ok(ParseResult::IssuesFound {
                    issues: elements.issues,
                });
            }

            Ok(ParseResult::ParseFailed(
                "XML validated but contains no issues or no-issues-found element.".to_string(),
            ))
        }
        Err(xsd_error) => {
            // Return the specific XSD error for retry
            Ok(ParseResult::ParseFailed(xsd_error.format_for_ai_retry()))
        }
    }
}

/// Read the last review output from logs.
///
/// The `log_prefix` is a path prefix (not a directory) like `.agent/logs/reviewer_1`.
/// Actual log files are named `{prefix}_{agent}_{model}.log`, e.g.:
/// `.agent/logs/reviewer_1_ccs-glm_0.log`
fn read_last_review_output(
    log_prefix: &Path,
    workspace: &dyn crate::workspace::Workspace,
) -> String {
    read_last_output_from_prefix(log_prefix, workspace)
}

/// Get the last XSD error from the log directory for retry feedback.
fn get_last_xsd_error(ctx: &PhaseContext<'_>, log_dir: &Path) -> Option<String> {
    let error_file = log_dir.join("xsd_error.txt");
    if let Ok(content) = ctx.workspace.read(&error_file) {
        if !content.trim().is_empty() {
            return Some(content);
        }
    }
    None
}

/// Store XSD error for the next retry attempt.
fn store_xsd_error_for_retry(ctx: &PhaseContext<'_>, log_dir: &Path, error: &str) {
    let error_file = log_dir.join("xsd_error.txt");
    let _ = ctx.workspace.write(&error_file, error);
}

/// Handle post-flight validation after a review pass.
fn handle_postflight_validation(ctx: &PhaseContext<'_>, j: u32) {
    let postflight_result = post_flight_review_check(ctx.workspace, ctx.logger, j);
    match postflight_result {
        PostflightResult::Valid => {
            // ISSUES.md found and valid, continue
        }
        PostflightResult::Missing(msg) => {
            ctx.logger.warn(&format!(
                "Post-flight check: {msg}. Proceeding with fix pass anyway."
            ));
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

/// Read the last fix output from logs.
///
/// The `log_prefix` is a path prefix (not a directory) like `.agent/logs/fix_1_1`.
/// Actual log files are named `{prefix}_{agent}_{model}.log`, e.g.:
/// `.agent/logs/fix_1_1_ccs-glm_0.log`
fn read_last_fix_output(log_prefix: &Path, workspace: &dyn crate::workspace::Workspace) -> String {
    read_last_output_from_prefix(log_prefix, workspace)
}

/// Read the most recent log file matching a prefix pattern.
///
/// This is a shared helper for reading log output. Truncation of large prompts
/// is handled centrally in `build_agent_command` to prevent E2BIG errors.
fn read_last_output_from_prefix(
    log_prefix: &Path,
    workspace: &dyn crate::workspace::Workspace,
) -> String {
    crate::pipeline::logfile::read_most_recent_logfile(log_prefix, workspace)
}

/// Format XSD error for display (for fix result).
fn format_xsd_error_for_fix(error: &XsdValidationError) -> String {
    format!(
        "{} - expected: {}, found: {}",
        error.element_path, error.expected, error.found
    )
}

/// Run the fix pass for a single cycle.
///
/// This function implements a nested loop structure similar to development:
/// - **Outer loop (continuation)**: Continue while status != "all_issues_addressed" (max 100)
/// - **Inner loop (XSD retry)**: Retry XSD validation with error feedback (max 100)
pub fn run_fix_pass(
    ctx: &mut PhaseContext<'_>,
    j: u32,
    _reviewer_context: ContextLevel,
    _resume_context: Option<&ResumeContext>,
    _agent: Option<&str>,
) -> anyhow::Result<()> {
    let active_agent = _agent.unwrap_or(ctx.reviewer_agent);
    let fix_start_time = Instant::now();

    update_status_with_workspace(ctx.workspace, "Applying fixes", ctx.config.isolation_mode)?;

    // Read PROMPT.md, PLAN.md, and ISSUES.md for context
    let prompt_content = ctx
        .workspace
        .read(Path::new("PROMPT.md"))
        .unwrap_or_default();
    let plan_content = ctx
        .workspace
        .read(Path::new(".agent/PLAN.md"))
        .unwrap_or_default();
    let issues_content = ctx
        .workspace
        .read(Path::new(".agent/ISSUES.md"))
        .unwrap_or_default();

    // Extract file paths from issues for the fix prompt
    let files_to_modify = extract_file_paths_from_issues(&issues_content);

    let log_dir = format!(".agent/logs/reviewer_fix_{j}");

    let max_xsd_retries = crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS as usize;
    let max_continuations = MAX_CONTINUATION_ATTEMPTS;
    let mut _had_any_error = false; // Tracked for potential future use

    // Session info for potential session continuation on XSD retries
    let mut session_info: Option<crate::pipeline::session::SessionInfo> = None;

    // Outer loop: Continue until agent returns status="all_issues_addressed" or "no_issues_found"
    'continuation: for continuation_num in 0..max_continuations {
        let is_continuation = continuation_num > 0;
        if is_continuation {
            ctx.logger.info(&format!(
                "Fix continuation {} of {} (status was not complete)",
                continuation_num, max_continuations
            ));
        }

        let mut xsd_error: Option<String> = None;

        // Inner loop: XSD validation retry with error feedback
        for retry_num in 0..max_xsd_retries {
            let is_retry = retry_num > 0;
            let total_attempts = continuation_num * max_xsd_retries + retry_num + 1;

            // Before each retry, check if the XML file is writable and clean up if locked
            if is_retry {
                use crate::files::io::check_and_cleanup_xml_before_retry_with_workspace;
                use std::path::Path;
                let xml_path =
                    Path::new(crate::files::llm_output_extraction::xml_paths::FIX_RESULT_XML);
                let _ = check_and_cleanup_xml_before_retry_with_workspace(
                    ctx.workspace,
                    xml_path,
                    ctx.logger,
                );
            }

            // For initial attempt, use XML prompt
            // For retries, use XSD retry prompt with error feedback
            let fix_prompt = if !is_retry && !is_continuation {
                // First attempt ever - use initial XML prompt
                let prompt_key = format!("fix_{}", j);
                let (prompt, was_replayed) =
                    get_stored_or_generate_prompt(&prompt_key, &ctx.prompt_history, || {
                        prompt_fix_xml_with_context(
                            ctx.template_context,
                            &prompt_content,
                            &plan_content,
                            &issues_content,
                            &files_to_modify,
                        )
                    });

                if !was_replayed {
                    ctx.capture_prompt(&prompt_key, &prompt);
                } else {
                    ctx.logger.info(&format!(
                        "Using stored prompt from checkpoint for determinism: {}",
                        prompt_key
                    ));
                }

                prompt
            } else if !is_continuation {
                // XSD retry only (no continuation yet)
                ctx.logger.info(&format!(
                    "  In-session retry {}/{} for XSD validation (total attempt: {})",
                    retry_num,
                    max_xsd_retries - 1,
                    total_attempts
                ));
                if let Some(ref error) = xsd_error {
                    ctx.logger.info(&format!("  XSD error: {}", error));
                }

                let last_output = read_last_fix_output(Path::new(&log_dir), ctx.workspace);

                prompt_fix_xsd_retry_with_context(
                    ctx.template_context,
                    &issues_content,
                    xsd_error.as_deref().unwrap_or("Unknown error"),
                    &last_output,
                    ctx.workspace,
                )
            } else if !is_retry {
                // Continuation only (first XSD attempt after continuation)
                ctx.logger.info(&format!(
                    "  Continuation attempt {} (XSD validation attempt {}/{})",
                    total_attempts, 1, max_xsd_retries
                ));

                prompt_fix_xml_with_context(
                    ctx.template_context,
                    &prompt_content,
                    &plan_content,
                    &issues_content,
                    &files_to_modify,
                )
            } else {
                // Both continuation and XSD retry
                ctx.logger.info(&format!(
                    "  Continuation retry {}/{} for XSD validation (total attempt: {})",
                    retry_num,
                    max_xsd_retries - 1,
                    total_attempts
                ));
                if let Some(ref error) = xsd_error {
                    ctx.logger.info(&format!("  XSD error: {}", error));
                }

                let last_output = read_last_fix_output(Path::new(&log_dir), ctx.workspace);

                prompt_fix_xsd_retry_with_context(
                    ctx.template_context,
                    &issues_content,
                    xsd_error.as_deref().unwrap_or("Unknown error"),
                    &last_output,
                    ctx.workspace,
                )
            };

            // Log the fix prompt details for debugging (when verbose)
            if ctx.config.verbosity.is_debug() && !is_continuation && !is_retry {
                ctx.logger.info(&format!(
                    "Fix prompt length: {} characters",
                    fix_prompt.len()
                ));
            }

            // Run the agent with session continuation for XSD retries
            let xsd_result = {
                let mut runtime = PipelineRuntime {
                    timer: ctx.timer,
                    logger: ctx.logger,
                    colors: ctx.colors,
                    config: ctx.config,
                    executor: ctx.executor,
                    executor_arc: std::sync::Arc::clone(&ctx.executor_arc),
                    workspace: ctx.workspace,
                };

                // Output validator: checks if fixer produced valid output
                // Priority: 1) File-based XML at .agent/tmp/fix_result.xml
                //           2) JSON result events in log files
                let validate_output: crate::pipeline::OutputValidator =
                    |ws: &dyn crate::workspace::Workspace,
                     log_dir_path: &Path,
                     _logger: &crate::logger::Logger|
                     -> std::io::Result<bool> {
                        use crate::files::llm_output_extraction::{
                            has_valid_xml_output, xml_paths,
                        };

                        // First, check if XML file was written directly (file-based mode)
                        if has_valid_xml_output(ws, Path::new(xml_paths::FIX_RESULT_XML)) {
                            return Ok(true); // Valid XML file exists
                        }

                        // Fall back to JSON log extraction (legacy/streaming mode)
                        use crate::files::result_extraction::extract_last_result;
                        match extract_last_result(ws, log_dir_path) {
                            Ok(Some(_)) => Ok(true), // Valid JSON output exists
                            Ok(None) => Ok(false),   // No valid output found
                            Err(_) => Ok(true), // On error, assume success (let extraction handle validation)
                        }
                    };

                let base_label = format!(
                    "fix #{}{}",
                    j,
                    if is_continuation {
                        format!(" (continuation {})", continuation_num)
                    } else {
                        String::new()
                    }
                );

                let mut xsd_retry_config = XsdRetryConfig {
                    role: AgentRole::Reviewer,
                    base_label: &base_label,
                    prompt: &fix_prompt,
                    logfile_prefix: &log_dir,
                    runtime: &mut runtime,
                    registry: ctx.registry,
                    primary_agent: active_agent,
                    session_info: session_info.as_ref(),
                    retry_num,
                    output_validator: Some(validate_output),
                    workspace: ctx.workspace,
                };
                run_xsd_retry_with_session(&mut xsd_retry_config)?
            };

            ctx.stats.reviewer_runs_completed += 1;

            // Check for auth error FIRST - if detected, bail with an error that signals agent fallback
            if xsd_result.auth_error_detected {
                ctx.logger
                    .warn("  Auth/credential error detected during fix, signaling agent fallback");
                anyhow::bail!("Authentication error during fix - agent fallback required");
            }

            // Extract session info for potential retry (only if we don't have it yet)
            let log_dir_path = Path::new(&log_dir);
            if session_info.is_none() {
                if let Some(agent_config) = ctx.registry.resolve_config(active_agent) {
                    session_info = crate::pipeline::session::extract_session_info_from_log_prefix(
                        log_dir_path,
                        agent_config.json_parser,
                        Some(active_agent),
                        ctx.workspace,
                    );
                }
            }

            // Track if any agent run had an error
            if xsd_result.exit_code != 0 {
                _had_any_error = true;
            }
            let fix_content = read_last_fix_output(log_dir_path, ctx.workspace);

            // Try file-based extraction first - allows agents to write XML to .agent/tmp/fix_result.xml
            let xml_to_validate = extract_xml_with_file_fallback_with_workspace(
                ctx.workspace,
                Path::new(xml_paths::FIX_RESULT_XML),
                &fix_content,
                extract_fix_result_xml,
            )
            .unwrap_or_else(|| {
                // No XML found anywhere - assume entire log content is XML for validation
                // This allows us to get specific XSD errors to send back to the agent
                fix_content.clone()
            });

            // Try to validate against XSD
            match validate_fix_result_xml(&xml_to_validate) {
                Ok(result_elements) => {
                    // XSD validation passed - archive the file for debugging (moves to .xml.processed)
                    // Note: XML display is handled via UIEvent::XmlOutput in the effect handler
                    archive_xml_file_with_workspace(
                        ctx.workspace,
                        Path::new(xml_paths::FIX_RESULT_XML),
                    );

                    if is_retry {
                        ctx.logger
                            .success(&format!("Fix validated after {} retries", retry_num));
                    } else {
                        ctx.logger
                            .success("Fix status extracted and validated (XML)");
                    }

                    // Check the status to determine if we should continue
                    if result_elements.is_complete() || result_elements.is_no_issues() {
                        // Status is "all_issues_addressed" or "no_issues_found" - we're done
                        let duration = fix_start_time.elapsed().as_secs();
                        let step = ExecutionStep::new(
                            "Review",
                            j,
                            "fix",
                            StepOutcome::success(result_elements.summary, vec![]),
                        )
                        .with_agent(active_agent)
                        .with_duration(duration);
                        ctx.execution_history.add_step(step);

                        return Ok(());
                    } else if result_elements.has_remaining_issues() {
                        // Status is "issues_remain" - continue the outer loop
                        ctx.logger
                            .info("Status is 'issues_remain' - continuing with same fix pass");
                        continue 'continuation;
                    }
                }
                Err(xsd_err) => {
                    // XSD validation failed - check if we can retry
                    let error_msg = format_xsd_error_for_fix(&xsd_err);
                    ctx.logger
                        .warn(&format!("  XSD validation failed: {}", error_msg));

                    if retry_num < max_xsd_retries - 1 {
                        // Store error for next retry attempt
                        xsd_error = Some(error_msg);
                        // Continue to next XSD retry iteration
                        continue;
                    } else {
                        ctx.logger
                            .warn("  No more in-session XSD retries remaining");
                        // Fall through to return what we have
                        break 'continuation;
                    }
                }
            }
        }

        // If we've exhausted XSD retries, break the continuation loop
        ctx.logger
            .warn("XSD retry loop exhausted - stopping continuation");
        break;
    }

    // If we get here, we exhausted the continuation limit
    let duration = fix_start_time.elapsed().as_secs();
    let step = ExecutionStep::new(
        "Review",
        j,
        "fix",
        StepOutcome::failure(
            format!(
                "Continuation stopped after {} attempts",
                max_continuations * max_xsd_retries
            ),
            true,
        ),
    )
    .with_agent(active_agent)
    .with_duration(duration);
    ctx.execution_history.add_step(step);

    Ok(())
}

/// Handle post-fix commit creation.
fn handle_post_fix_commit(
    ctx: &mut PhaseContext<'_>,
    iteration: u32,
    prev_snap: &str,
) -> anyhow::Result<String> {
    let start_time = Instant::now();

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

            // Check if diff is empty before requesting commit message generation
            if diff.trim().is_empty() {
                ctx.logger
                    .info("Skipping commit (no meaningful changes in diff)");

                let duration = start_time.elapsed().as_secs();
                let step = ExecutionStep::new(
                    "Review",
                    iteration,
                    "commit",
                    StepOutcome::skipped("No meaningful changes to commit".to_string()),
                )
                .with_duration(duration);
                ctx.execution_history.add_step(step);
            } else {
                // Get git identity from config
                let git_name = ctx.config.git_user_name.as_deref();
                let git_email = ctx.config.git_user_email.as_deref();

                match commit_with_generated_message(&diff, &agent, git_name, git_email, ctx) {
                    CommitResultFallback::Success(oid) => {
                        ctx.logger
                            .success(&format!("Commit created successfully: {oid}"));
                        ctx.stats.commits_created += 1;

                        let duration = start_time.elapsed().as_secs();
                        let step = ExecutionStep::new(
                            "Review",
                            iteration,
                            "commit",
                            StepOutcome::success(Some(oid.to_string()), vec![]),
                        )
                        .with_agent(&agent)
                        .with_duration(duration);
                        ctx.execution_history.add_step(step);
                    }
                    CommitResultFallback::NoChanges => {
                        ctx.logger.info("No commit created (no meaningful changes)");

                        let duration = start_time.elapsed().as_secs();
                        let step = ExecutionStep::new(
                            "Review",
                            iteration,
                            "commit",
                            StepOutcome::skipped("No meaningful changes to commit".to_string()),
                        )
                        .with_duration(duration);
                        ctx.execution_history.add_step(step);
                    }
                    CommitResultFallback::Failed(err) => {
                        ctx.logger.error(&format!(
                            "Failed to create commit (git operation failed): {err}"
                        ));

                        let duration = start_time.elapsed().as_secs();
                        let step = ExecutionStep::new(
                            "Review",
                            iteration,
                            "commit",
                            StepOutcome::failure(err.to_string(), false),
                        )
                        .with_duration(duration);
                        ctx.execution_history.add_step(step);

                        return Err(anyhow::anyhow!(err));
                    }
                }
            }
        } else {
            ctx.logger
                .warn("Unable to get commit agent chain for commit");

            let duration = start_time.elapsed().as_secs();
            let step = ExecutionStep::new(
                "Review",
                iteration,
                "commit",
                StepOutcome::failure("No commit agent available".to_string(), true),
            )
            .with_duration(duration);
            ctx.execution_history.add_step(step);
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_is_auth_failure_error_detects_auth_failure() {
        let err = anyhow::anyhow!("Authentication error during fix - agent fallback required");
        assert!(is_auth_failure_error(&err));

        let other = anyhow::anyhow!("some other error");
        assert!(!is_auth_failure_error(&other));
    }

    #[test]
    fn test_advance_agent_chain_on_auth_failure_advances_and_exhausts() {
        let fallback_config = crate::agents::fallback::FallbackConfig {
            reviewer: vec!["agent-a".to_string(), "agent-b".to_string()],
            ..crate::agents::fallback::FallbackConfig::default()
        };

        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["agent-a".to_string(), "agent-b".to_string()],
                vec![Vec::new(), Vec::new()],
                AgentRole::Reviewer,
            )
            .with_max_cycles(1);

        let backoff = advance_agent_chain_on_auth_failure(&mut chain, &fallback_config).unwrap();
        assert_eq!(backoff, None);
        assert_eq!(chain.current_agent().map(String::as_str), Some("agent-b"));

        let exhausted = advance_agent_chain_on_auth_failure(&mut chain, &fallback_config);
        assert!(exhausted.is_err());
        assert_eq!(chain.current_agent().map(String::as_str), Some("agent-b"));
    }

    #[test]
    fn test_advance_agent_chain_on_auth_failure_applies_backoff_on_cycle() {
        let fallback_config = crate::agents::fallback::FallbackConfig {
            reviewer: vec!["solo-agent".to_string()],
            retry_delay_ms: 1000,
            backoff_multiplier: 2.0,
            max_backoff_ms: 60000,
            max_cycles: 2,
            ..crate::agents::fallback::FallbackConfig::default()
        };

        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["solo-agent".to_string()],
                vec![Vec::new()],
                AgentRole::Reviewer,
            )
            .with_max_cycles(2);

        let backoff = advance_agent_chain_on_auth_failure(&mut chain, &fallback_config).unwrap();
        assert_eq!(backoff, Some(2000));
    }
}
