//! Rebase operations for the pipeline.
//!
//! This module contains functions for running pre-development rebase
//! and conflict resolution during the pipeline.

use crate::checkpoint::execution_history::{ExecutionStep, StepOutcome};
use crate::checkpoint::{
    save_checkpoint_with_workspace, CheckpointBuilder, PipelinePhase, RebaseState, RunContext,
};
use crate::executor::ProcessExecutor;
use crate::git_helpers::{
    abort_rebase, continue_rebase, get_conflicted_files, get_default_branch, rebase_onto,
    RebaseErrorKind, RebaseResult,
};
use crate::logger::{Colors, Logger};
use crate::phases::PhaseContext;
use crate::prompts::{get_stored_or_generate_prompt, template_context::TemplateContext};

use super::context::PipelineContext;

/// Context for conflict resolution operations.
///
/// Groups together the configuration and runtime state needed for
/// AI-assisted conflict resolution during rebase operations.
pub(crate) struct ConflictResolutionContext<'a> {
    pub config: &'a crate::config::Config,
    pub template_context: &'a TemplateContext,
    pub logger: &'a Logger,
    pub colors: Colors,
    pub executor_arc: std::sync::Arc<dyn crate::executor::ProcessExecutor>,
    pub workspace: &'a dyn crate::workspace::Workspace,
}

/// Result type for conflict resolution attempts.
///
/// Represents the different ways conflict resolution can succeed or fail.
pub(crate) enum ConflictResolutionResult {
    /// Agent provided JSON output with resolved file contents
    WithJson(String),
    /// Agent resolved conflicts by editing files directly (no JSON output)
    FileEditsOnly,
    /// Resolution failed completely
    Failed,
}

/// Run rebase to the default branch.
///
/// This function performs a rebase from the current branch to the
/// default branch (main/master). It handles all edge cases including:
/// - Already on main/master (proceeds with rebase attempt)
/// - Empty repository (returns `NoOp`)
/// - Upstream branch not found (error)
/// - Conflicts during rebase (returns `Conflicts` result)
pub fn run_rebase_to_default(
    logger: &Logger,
    colors: Colors,
    executor: &dyn ProcessExecutor,
) -> std::io::Result<RebaseResult> {
    let default_branch = get_default_branch()?;
    logger.info(&format!(
        "Rebasing onto {}{}{}",
        colors.cyan(),
        default_branch,
        colors.reset()
    ));
    rebase_onto(&default_branch, executor)
}

/// Run initial rebase before development phase.
///
/// This function is called before the development phase starts to ensure
/// the feature branch is up-to-date with the default branch.
///
/// Uses a state machine for fault tolerance and automatic recovery from
/// interruptions or failures.
pub fn run_initial_rebase(
    ctx: &PipelineContext,
    phase_ctx: &mut PhaseContext<'_>,
    run_context: &RunContext,
    executor: &dyn ProcessExecutor,
) -> anyhow::Result<()> {
    ctx.logger.header("Pre-development rebase", Colors::cyan);

    record_rebase_start(phase_ctx);
    save_pre_rebase_checkpoint(ctx, phase_ctx, run_context)?;

    match run_rebase_to_default(&ctx.logger, ctx.colors, &*ctx.executor) {
        Ok(RebaseResult::Success) => handle_rebase_success(ctx, phase_ctx, run_context),
        Ok(RebaseResult::NoOp { reason }) => {
            handle_rebase_noop(ctx, phase_ctx, run_context, &reason)
        }
        Ok(RebaseResult::Conflicts(_)) => {
            handle_rebase_conflicts(ctx, phase_ctx, run_context, executor)
        }
        Ok(RebaseResult::Failed(err)) => handle_rebase_failed(ctx, phase_ctx, err),
        Err(e) => handle_rebase_error(ctx, phase_ctx, e),
    }
}

/// Record the start of a pre-rebase operation.
fn record_rebase_start(phase_ctx: &mut PhaseContext<'_>) {
    let step = ExecutionStep::new(
        "PreRebase",
        0,
        "pre_rebase_start",
        StepOutcome::success(None, vec![]),
    );
    phase_ctx.execution_history.add_step(step);
}

/// Save checkpoint at the start of pre-rebase phase.
fn save_pre_rebase_checkpoint(
    ctx: &PipelineContext,
    phase_ctx: &PhaseContext<'_>,
    run_context: &RunContext,
) -> anyhow::Result<()> {
    if !ctx.config.features.checkpoint_enabled {
        return Ok(());
    }

    let default_branch = get_default_branch().unwrap_or_else(|_| "main".to_string());
    let builder = create_checkpoint_builder(ctx, phase_ctx, run_context, PipelinePhase::PreRebase);

    if let Some(mut checkpoint) = builder.build() {
        checkpoint.rebase_state = RebaseState::PreRebaseInProgress {
            upstream_branch: default_branch,
        };
        let _ = save_checkpoint_with_workspace(&*ctx.workspace, &checkpoint);
    }

    Ok(())
}

/// Handle successful rebase completion.
fn handle_rebase_success(
    ctx: &PipelineContext,
    phase_ctx: &mut PhaseContext<'_>,
    run_context: &RunContext,
) -> anyhow::Result<()> {
    ctx.logger.success("Rebase completed successfully");

    let step = ExecutionStep::new(
        "PreRebase",
        0,
        "pre_rebase_complete",
        StepOutcome::success(None, vec![]),
    );
    phase_ctx.execution_history.add_step(step);

    save_post_rebase_checkpoint(ctx, phase_ctx, run_context);
    Ok(())
}

/// Handle rebase that was not needed.
fn handle_rebase_noop(
    ctx: &PipelineContext,
    phase_ctx: &mut PhaseContext<'_>,
    run_context: &RunContext,
    reason: &str,
) -> anyhow::Result<()> {
    ctx.logger.info(&format!("No rebase needed: {reason}"));

    let step = ExecutionStep::new(
        "PreRebase",
        0,
        "pre_rebase_skipped",
        StepOutcome::skipped(reason.to_string()),
    );
    phase_ctx.execution_history.add_step(step);

    save_post_rebase_checkpoint(ctx, phase_ctx, run_context);
    Ok(())
}

/// Handle rebase conflicts by attempting AI resolution.
fn handle_rebase_conflicts(
    ctx: &PipelineContext,
    phase_ctx: &mut PhaseContext<'_>,
    run_context: &RunContext,
    executor: &dyn ProcessExecutor,
) -> anyhow::Result<()> {
    let conflicted_files = get_conflicted_files()?;
    if conflicted_files.is_empty() {
        ctx.logger
            .warn("Rebase reported conflicts but no conflicted files found");
        let _ = abort_rebase(executor);
        return Ok(());
    }

    record_conflict_detected(phase_ctx, conflicted_files.len());
    save_conflict_checkpoint(ctx, phase_ctx, run_context, &conflicted_files);

    ctx.logger.warn(&format!(
        "Rebase resulted in {} conflict(s), attempting AI resolution",
        conflicted_files.len()
    ));

    let resolution_ctx = ConflictResolutionContext {
        config: &ctx.config,
        template_context: &ctx.template_context,
        logger: &ctx.logger,
        colors: ctx.colors,
        executor_arc: std::sync::Arc::clone(&ctx.executor),
        workspace: &*ctx.workspace,
    };

    match try_resolve_conflicts_with_fallback(
        &conflicted_files,
        resolution_ctx,
        phase_ctx,
        "PreRebase",
        executor,
    ) {
        Ok(true) => handle_conflicts_resolved(ctx, phase_ctx, run_context, executor),
        Ok(false) => handle_resolution_failed(ctx, phase_ctx, executor),
        Err(e) => handle_resolution_error(ctx, phase_ctx, executor, e),
    }
}

/// Record that conflicts were detected during rebase.
fn record_conflict_detected(phase_ctx: &mut PhaseContext<'_>, conflict_count: usize) {
    let step = ExecutionStep::new(
        "PreRebase",
        0,
        "pre_rebase_conflict",
        StepOutcome::partial(
            "Rebase started".to_string(),
            format!("{} conflicts detected", conflict_count),
        ),
    );
    phase_ctx.execution_history.add_step(step);
}

/// Save checkpoint for conflict state.
fn save_conflict_checkpoint(
    ctx: &PipelineContext,
    phase_ctx: &PhaseContext<'_>,
    run_context: &RunContext,
    conflicted_files: &[String],
) {
    if !ctx.config.features.checkpoint_enabled {
        return;
    }

    let builder = create_checkpoint_builder(
        ctx,
        phase_ctx,
        run_context,
        PipelinePhase::PreRebaseConflict,
    );

    if let Some(mut checkpoint) = builder.build() {
        checkpoint.rebase_state = RebaseState::HasConflicts {
            files: conflicted_files.to_vec(),
        };
        let _ = save_checkpoint_with_workspace(&*ctx.workspace, &checkpoint);
    }
}

/// Handle successful conflict resolution.
fn handle_conflicts_resolved(
    ctx: &PipelineContext,
    phase_ctx: &mut PhaseContext<'_>,
    run_context: &RunContext,
    executor: &dyn ProcessExecutor,
) -> anyhow::Result<()> {
    ctx.logger
        .info("Continuing rebase after conflict resolution");

    match continue_rebase(executor) {
        Ok(()) => {
            ctx.logger
                .success("Rebase completed successfully after AI resolution");

            let step = ExecutionStep::new(
                "PreRebase",
                0,
                "pre_rebase_resolution",
                StepOutcome::success(None, vec![]),
            );
            phase_ctx.execution_history.add_step(step);

            save_post_rebase_checkpoint(ctx, phase_ctx, run_context);
            Ok(())
        }
        Err(e) => {
            ctx.logger.warn(&format!("Failed to continue rebase: {e}"));
            let _ = abort_rebase(executor);

            let step = ExecutionStep::new(
                "PreRebase",
                0,
                "pre_rebase_resolution",
                StepOutcome::partial(
                    "Conflicts resolved by AI".to_string(),
                    format!("Failed to continue rebase: {e}"),
                ),
            );
            phase_ctx.execution_history.add_step(step);
            Ok(()) // Continue anyway - conflicts were resolved
        }
    }
}

/// Handle failed AI conflict resolution.
fn handle_resolution_failed(
    ctx: &PipelineContext,
    phase_ctx: &mut PhaseContext<'_>,
    executor: &dyn ProcessExecutor,
) -> anyhow::Result<()> {
    ctx.logger
        .warn("AI conflict resolution failed, aborting rebase");
    let _ = abort_rebase(executor);

    let step = ExecutionStep::new(
        "PreRebase",
        0,
        "pre_rebase_resolution",
        StepOutcome::failure("AI conflict resolution failed".to_string(), true),
    );
    phase_ctx.execution_history.add_step(step);
    Ok(()) // Continue pipeline - don't block on rebase failure
}

/// Handle error during conflict resolution.
fn handle_resolution_error(
    ctx: &PipelineContext,
    phase_ctx: &mut PhaseContext<'_>,
    executor: &dyn ProcessExecutor,
    e: anyhow::Error,
) -> anyhow::Result<()> {
    ctx.logger.error(&format!("Conflict resolution error: {e}"));
    let _ = abort_rebase(executor);

    let step = ExecutionStep::new(
        "PreRebase",
        0,
        "pre_rebase_resolution",
        StepOutcome::failure(format!("Conflict resolution error: {e}"), true),
    );
    phase_ctx.execution_history.add_step(step);
    Ok(()) // Continue pipeline
}

/// Handle rebase failure.
fn handle_rebase_failed(
    ctx: &PipelineContext,
    phase_ctx: &mut PhaseContext<'_>,
    err: RebaseErrorKind,
) -> anyhow::Result<()> {
    ctx.logger.error(&format!("Rebase failed: {err}"));
    let _ = abort_rebase(&*ctx.executor);

    let step = ExecutionStep::new(
        "PreRebase",
        0,
        "pre_rebase_failed",
        StepOutcome::failure(format!("Rebase failed: {err}"), true),
    );
    phase_ctx.execution_history.add_step(step);
    Ok(()) // Continue pipeline despite failure
}

/// Handle rebase error.
fn handle_rebase_error(
    ctx: &PipelineContext,
    phase_ctx: &mut PhaseContext<'_>,
    e: std::io::Error,
) -> anyhow::Result<()> {
    ctx.logger
        .warn(&format!("Rebase failed, continuing without rebase: {e}"));

    let step = ExecutionStep::new(
        "PreRebase",
        0,
        "pre_rebase_error",
        StepOutcome::failure(format!("Rebase error: {e}"), true),
    );
    phase_ctx.execution_history.add_step(step);
    Ok(())
}

/// Save checkpoint after successful rebase completion.
fn save_post_rebase_checkpoint(
    ctx: &PipelineContext,
    phase_ctx: &PhaseContext<'_>,
    run_context: &RunContext,
) {
    if !ctx.config.features.checkpoint_enabled {
        return;
    }

    let builder = CheckpointBuilder::new()
        .phase(PipelinePhase::Planning, 0, ctx.config.developer_iters)
        .reviewer_pass(0, ctx.config.reviewer_reviews)
        .skip_rebase(true) // Pre-rebase is done
        .capture_from_context(
            &ctx.config,
            &ctx.registry,
            &ctx.developer_agent,
            &ctx.reviewer_agent,
            &ctx.logger,
            run_context,
        )
        .with_executor_from_context(std::sync::Arc::clone(&ctx.executor))
        .with_execution_history(phase_ctx.execution_history.clone())
        .with_prompt_history(phase_ctx.clone_prompt_history());

    if let Some(checkpoint) = builder.build() {
        let _ = save_checkpoint_with_workspace(&*ctx.workspace, &checkpoint);
    }
}

/// Create a checkpoint builder with common configuration.
fn create_checkpoint_builder(
    ctx: &PipelineContext,
    phase_ctx: &PhaseContext<'_>,
    run_context: &RunContext,
    phase: PipelinePhase,
) -> CheckpointBuilder {
    CheckpointBuilder::new()
        .phase(phase, 0, ctx.config.developer_iters)
        .reviewer_pass(0, ctx.config.reviewer_reviews)
        .capture_from_context(
            &ctx.config,
            &ctx.registry,
            &ctx.developer_agent,
            &ctx.reviewer_agent,
            &ctx.logger,
            run_context,
        )
        .with_executor_from_context(std::sync::Arc::clone(&ctx.executor))
        .with_execution_history(phase_ctx.execution_history.clone())
        .with_prompt_history(phase_ctx.clone_prompt_history())
}

/// Attempt to resolve rebase conflicts with AI fallback.
///
/// This function accepts `PhaseContext` to capture prompts and track
/// execution history for hardened resume functionality.
pub(crate) fn try_resolve_conflicts_with_fallback(
    conflicted_files: &[String],
    ctx: ConflictResolutionContext<'_>,
    phase_ctx: &mut PhaseContext<'_>,
    phase: &str,
    executor: &dyn ProcessExecutor,
) -> anyhow::Result<bool> {
    if conflicted_files.is_empty() {
        return Ok(false);
    }

    ctx.logger.info(&format!(
        "Attempting AI conflict resolution for {} file(s)",
        conflicted_files.len()
    ));

    let conflicts = collect_conflict_info_or_error(conflicted_files, ctx.logger)?;

    // Use stored_or_generate pattern for hardened resume
    let prompt_key = format!("{}_conflict_resolution", phase.to_lowercase());
    let (resolution_prompt, was_replayed) =
        get_stored_or_generate_prompt(&prompt_key, &phase_ctx.prompt_history, || {
            build_resolution_prompt(&conflicts, ctx.template_context, ctx.workspace)
        });

    // Capture the resolution prompt for deterministic resume (only if newly generated)
    if !was_replayed {
        phase_ctx.capture_prompt(&prompt_key, &resolution_prompt);
    } else {
        ctx.logger.info(&format!(
            "Using stored prompt from checkpoint for determinism: {}",
            prompt_key
        ));
    }

    match run_ai_conflict_resolution(
        &resolution_prompt,
        ctx.config,
        ctx.logger,
        ctx.colors,
        std::sync::Arc::clone(&ctx.executor_arc),
        ctx.workspace,
    ) {
        Ok(ConflictResolutionResult::WithJson(resolved_content)) => {
            handle_json_resolution(&resolved_content, ctx.logger, ctx.workspace)
        }
        Ok(ConflictResolutionResult::FileEditsOnly) => handle_file_edits_resolution(ctx.logger),
        Ok(ConflictResolutionResult::Failed) => handle_failed_resolution(ctx.logger, executor),
        Err(e) => handle_error_resolution(ctx.logger, executor, e),
    }
}

/// Handle resolution that returned JSON output.
fn handle_json_resolution(
    resolved_content: &str,
    logger: &Logger,
    workspace: &dyn crate::workspace::Workspace,
) -> anyhow::Result<bool> {
    // Attempt to parse and write files
    match parse_and_validate_resolved_files(resolved_content, logger) {
        Ok(resolved_files) => {
            write_resolved_files(&resolved_files, workspace, logger)?;
        }
        Err(_) => {
            // JSON parsing failed - this is expected and normal
            // We verify conflicts via LibGit2 state, not JSON parsing
        }
    }

    // Verify all conflicts are resolved via LibGit2 (authoritative source)
    let remaining_conflicts = get_conflicted_files()?;
    if remaining_conflicts.is_empty() {
        Ok(true)
    } else {
        logger.warn(&format!(
            "{} conflicts remain after AI resolution",
            remaining_conflicts.len()
        ));
        Ok(false)
    }
}

/// Handle resolution via direct file edits.
fn handle_file_edits_resolution(logger: &Logger) -> anyhow::Result<bool> {
    logger.info("Agent resolved conflicts via file edits (no JSON output)");

    let remaining_conflicts = get_conflicted_files()?;
    if remaining_conflicts.is_empty() {
        logger.success("All conflicts resolved via file edits");
        Ok(true)
    } else {
        logger.warn(&format!(
            "{} conflicts remain after AI resolution",
            remaining_conflicts.len()
        ));
        Ok(false)
    }
}

/// Handle failed AI resolution.
fn handle_failed_resolution(
    logger: &Logger,
    executor: &dyn ProcessExecutor,
) -> anyhow::Result<bool> {
    logger.warn("AI conflict resolution failed");
    logger.info("Attempting to continue rebase anyway...");

    match crate::git_helpers::continue_rebase(executor) {
        Ok(()) => {
            logger.info("Successfully continued rebase");
            Ok(true)
        }
        Err(rebase_err) => {
            logger.warn(&format!("Failed to continue rebase: {rebase_err}"));
            Ok(false)
        }
    }
}

/// Handle error during resolution.
fn handle_error_resolution(
    logger: &Logger,
    executor: &dyn ProcessExecutor,
    e: anyhow::Error,
) -> anyhow::Result<bool> {
    logger.warn(&format!("AI conflict resolution failed: {e}"));
    logger.info("Attempting to continue rebase anyway...");

    match crate::git_helpers::continue_rebase(executor) {
        Ok(()) => {
            logger.info("Successfully continued rebase");
            Ok(true)
        }
        Err(rebase_err) => {
            logger.warn(&format!("Failed to continue rebase: {rebase_err}"));
            Ok(false)
        }
    }
}

/// Collect conflict information from conflicted files.
fn collect_conflict_info_or_error(
    conflicted_files: &[String],
    logger: &Logger,
) -> anyhow::Result<std::collections::HashMap<String, crate::prompts::FileConflict>> {
    use crate::prompts::collect_conflict_info;

    match collect_conflict_info(conflicted_files) {
        Ok(c) => Ok(c),
        Err(e) => {
            logger.error(&format!("Failed to collect conflict info: {e}"));
            anyhow::bail!("Failed to collect conflict info");
        }
    }
}

/// Build the conflict resolution prompt from context files.
fn build_resolution_prompt(
    conflicts: &std::collections::HashMap<String, crate::prompts::FileConflict>,
    template_context: &TemplateContext,
    workspace: &dyn crate::workspace::Workspace,
) -> String {
    build_enhanced_resolution_prompt(conflicts, None::<()>, template_context, workspace)
        .unwrap_or_else(|_| String::new())
}

/// Build the conflict resolution prompt with optional branch info.
fn build_enhanced_resolution_prompt(
    conflicts: &std::collections::HashMap<String, crate::prompts::FileConflict>,
    _branch_info: Option<()>,
    template_context: &TemplateContext,
    workspace: &dyn crate::workspace::Workspace,
) -> anyhow::Result<String> {
    use std::path::Path;

    let prompt_md_content = workspace.read(Path::new("PROMPT.md")).ok();
    let plan_content = workspace.read(Path::new(".agent/PLAN.md")).ok();

    Ok(
        crate::prompts::build_conflict_resolution_prompt_with_context(
            template_context,
            conflicts,
            prompt_md_content.as_deref(),
            plan_content.as_deref(),
        ),
    )
}

/// Run AI agent to resolve conflicts with fallback mechanism.
fn run_ai_conflict_resolution(
    resolution_prompt: &str,
    config: &crate::config::Config,
    logger: &Logger,
    colors: Colors,
    executor_arc: std::sync::Arc<dyn crate::executor::ProcessExecutor>,
    workspace: &dyn crate::workspace::Workspace,
) -> anyhow::Result<ConflictResolutionResult> {
    use crate::agents::AgentRegistry;
    use crate::files::result_extraction::extract_last_result;
    use crate::pipeline::{
        run_with_fallback_and_validator, FallbackConfig, OutputValidator, PipelineRuntime,
    };
    use std::io;
    use std::path::Path;

    let log_dir = ".agent/logs/rebase_conflict_resolution";

    let registry = AgentRegistry::new()?;
    let reviewer_agent = config.reviewer_agent.as_deref().unwrap_or("codex");

    let executor_ref: &dyn crate::executor::ProcessExecutor = &*executor_arc;
    let mut runtime = PipelineRuntime {
        timer: &mut crate::pipeline::Timer::new(),
        logger,
        colors: &colors,
        config,
        executor: executor_ref,
        executor_arc: std::sync::Arc::clone(&executor_arc),
        workspace,
    };

    let validate_output: OutputValidator = |ws: &dyn crate::workspace::Workspace,
                                            log_dir_path: &Path,
                                            validation_logger: &crate::logger::Logger|
     -> io::Result<bool> {
        match extract_last_result(ws, log_dir_path) {
            Ok(Some(_)) => Ok(true),
            Ok(None) => match crate::git_helpers::get_conflicted_files() {
                Ok(conflicts) if conflicts.is_empty() => {
                    validation_logger
                        .info("Agent resolved conflicts without JSON output (file edits only)");
                    Ok(true)
                }
                Ok(conflicts) => {
                    validation_logger.warn(&format!(
                        "{} conflict(s) remain unresolved",
                        conflicts.len()
                    ));
                    Ok(false)
                }
                Err(e) => {
                    validation_logger.warn(&format!("Failed to check for conflicts: {e}"));
                    Ok(false)
                }
            },
            Err(e) => {
                validation_logger.warn(&format!("Output validation check failed: {e}"));
                Ok(false)
            }
        }
    };

    let mut fallback_config = FallbackConfig {
        role: crate::agents::AgentRole::Reviewer,
        base_label: "conflict resolution",
        prompt: resolution_prompt,
        logfile_prefix: log_dir,
        runtime: &mut runtime,
        registry: &registry,
        primary_agent: reviewer_agent,
        output_validator: Some(validate_output),
        workspace,
    };

    let exit_code = run_with_fallback_and_validator(&mut fallback_config)?;

    if exit_code != 0 {
        return Ok(ConflictResolutionResult::Failed);
    }

    let remaining_conflicts = crate::git_helpers::get_conflicted_files()?;

    if remaining_conflicts.is_empty() {
        match extract_last_result(workspace, Path::new(log_dir)) {
            Ok(Some(content)) => Ok(ConflictResolutionResult::WithJson(content)),
            _ => Ok(ConflictResolutionResult::FileEditsOnly),
        }
    } else {
        Ok(ConflictResolutionResult::Failed)
    }
}

/// Parse and validate the resolved files from AI output.
///
/// JSON parsing failures are expected and handled gracefully - LibGit2 state
/// is used for verification, not JSON output. This function only parses the
/// JSON to write resolved files if available.
fn parse_and_validate_resolved_files(
    resolved_content: &str,
    logger: &Logger,
) -> anyhow::Result<serde_json::Map<String, serde_json::Value>> {
    let json: serde_json::Value = serde_json::from_str(resolved_content).map_err(|_e| {
        // Agent did not provide JSON output - fall back to LibGit2 verification
        // This is expected and normal, not an error condition
        anyhow::anyhow!("Agent did not provide JSON output (will verify via Git state)")
    })?;

    let resolved_files = match json.get("resolved_files") {
        Some(v) if v.is_object() => v.as_object().unwrap(),
        _ => {
            logger.info("Agent output missing 'resolved_files' object");
            anyhow::bail!("Agent output missing 'resolved_files' object");
        }
    };

    if resolved_files.is_empty() {
        logger.info("No resolved files in JSON output");
        anyhow::bail!("No files were resolved by the agent");
    }

    Ok(resolved_files.clone())
}

/// Write resolved files to disk and stage them.
///
/// Uses workspace abstraction for file operations, enabling testing with
/// `MemoryWorkspace`.
fn write_resolved_files(
    resolved_files: &serde_json::Map<String, serde_json::Value>,
    workspace: &dyn crate::workspace::Workspace,
    logger: &Logger,
) -> anyhow::Result<usize> {
    use std::path::Path;

    let mut files_written = 0;
    for (path, content) in resolved_files {
        if let Some(content_str) = content.as_str() {
            workspace.write(Path::new(path), content_str).map_err(|e| {
                logger.error(&format!("Failed to write {path}: {e}"));
                anyhow::anyhow!("Failed to write {path}: {e}")
            })?;
            logger.info(&format!("Resolved and wrote: {path}"));
            files_written += 1;
            // Stage the resolved file
            if let Err(e) = crate::git_helpers::git_add_all() {
                logger.warn(&format!("Failed to stage {path}: {e}"));
            }
        }
    }

    logger.success(&format!("Successfully resolved {files_written} file(s)"));
    Ok(files_written)
}

/// Wrapper for conflict resolution without PhaseContext.
///
/// This is used for --rebase-only mode where we don't have a full pipeline context.
pub fn try_resolve_conflicts_without_phase_ctx(
    conflicted_files: &[String],
    config: &crate::config::Config,
    template_context: &TemplateContext,
    logger: &Logger,
    colors: Colors,
    executor: std::sync::Arc<dyn ProcessExecutor>,
    repo_root: &std::path::Path,
) -> anyhow::Result<bool> {
    use crate::agents::AgentRegistry;
    use crate::checkpoint::execution_history::ExecutionHistory;
    use crate::pipeline::{Stats, Timer};

    let registry = AgentRegistry::new()?;
    let mut timer = Timer::new();
    let mut stats = Stats::default();
    let workspace = crate::workspace::WorkspaceFs::new(repo_root.to_path_buf());

    let reviewer_agent = config.reviewer_agent.as_deref().unwrap_or("codex");
    let developer_agent = config.developer_agent.as_deref().unwrap_or("codex");

    let executor_arc = std::sync::Arc::clone(&executor);

    let mut phase_ctx = PhaseContext {
        config,
        registry: &registry,
        logger,
        colors: &colors,
        timer: &mut timer,
        stats: &mut stats,
        developer_agent,
        reviewer_agent,
        review_guidelines: None,
        template_context,
        run_context: RunContext::new(),
        execution_history: ExecutionHistory::new(),
        prompt_history: std::collections::HashMap::new(),
        executor: &*executor,
        executor_arc: std::sync::Arc::clone(&executor_arc),
        repo_root,
        workspace: &workspace,
    };

    let ctx = ConflictResolutionContext {
        config,
        template_context,
        logger,
        colors,
        executor_arc,
        workspace: &workspace,
    };

    try_resolve_conflicts_with_fallback(
        conflicted_files,
        ctx,
        &mut phase_ctx,
        "RebaseOnly",
        &*executor,
    )
}
