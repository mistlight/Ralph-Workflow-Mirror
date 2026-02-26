// Pipeline Event Loop Execution
//
// This module contains the core pipeline execution logic using the reducer-based event loop.
//
// Architecture:
//
// The pipeline follows the reducer pattern:
// State → Orchestrator → Effect → Handler → Event → Reducer → State
//
// Execution Flow:
//
// 1. Resume Handling: Check for existing checkpoint and offer interactive resume
// 2. State Initialization: Create or restore pipeline state from checkpoint
// 3. Context Setup: Configure interrupt handlers, git helpers, monitoring
// 4. Event Loop: Run the reducer event loop until completion
// 5. Finalization: Write completion checkpoint, cleanup, restore PROMPT.md
//
// Checkpoint and Resume:
//
// - Fresh run: Creates new `RunContext` with UUID, initializes state
// - Resume: Restores state from checkpoint, applies config overrides, restores env vars
// - Completion: Saves final checkpoint with Complete phase for idempotent resume
//
// Event Loop Result Handling:
//
// The event loop returns `EventLoopResult`:
// - `completed=true`: Normal completion (Complete or Interrupted phase)
// - `completed=false`: Abnormal exit (bug in event loop or reducer)
//
// When `completed=false`, we write a defensive completion marker to ensure
// external orchestrators can detect termination.

/// Runs the pipeline with the default `MainEffectHandler`.
///
/// This is the production entry point - it creates a `MainEffectHandler` internally.
///
/// # Architecture
///
/// This function orchestrates the full pipeline execution:
/// 1. Resume handling (interactive prompt or --resume flag)
/// 2. State initialization (new or from checkpoint)
/// 3. Context setup (git helpers, PROMPT.md monitoring, interrupt handlers)
/// 4. Event loop execution via reducer pattern
/// 5. Finalization (checkpoint save, cleanup)
///
/// # Resume Flow
///
/// Resume can happen two ways:
/// - **Interactive**: If checkpoint exists without --resume, prompts user
/// - **Automatic**: If --resume flag is set, loads checkpoint directly
///
/// Configuration and environment variables are restored from the checkpoint.
///
/// # State Initialization
///
/// State is initialized with config-derived limits:
/// - `developer_iters`: Developer iteration budget
/// - `reviewer_reviews`: Reviewer pass budget
/// - `continuation_limits`: Per-phase continuation budgets
///
/// When resuming, progress is restored but budgets remain config-driven.
///
/// # Event Loop
///
/// The event loop runs until:
/// - Pipeline reaches Complete phase
/// - Pipeline is interrupted (Ctrl+C or non-terminating failure)
/// - Event loop hits iteration limit (bug protection)
///
/// # Errors
///
/// Returns error if:
/// - Resume validation fails
/// - State initialization fails
/// - Event loop execution fails
/// - Finalization operations fail
pub(super) fn run_pipeline_with_default_handler(ctx: &PipelineContext) -> anyhow::Result<()> {
    use crate::app::event_loop::EventLoopConfig;
    use crate::cloud::{CloudReporter, HeartbeatGuard, HttpCloudReporter, NoopCloudReporter};
    use crate::reducer::MainEffectHandler;
    use crate::reducer::PipelineState;
    use std::sync::Arc;
    use std::time::Duration;

    // First, offer interactive resume if checkpoint exists without --resume flag
    let resume_result = offer_resume_if_checkpoint_exists(
        &ctx.args,
        &ctx.config,
        &ctx.registry,
        &ctx.logger,
        &ctx.developer_agent,
        &ctx.reviewer_agent,
        &*ctx.workspace,
    );

    // If interactive resume didn't happen, check for --resume flag
    let resume_result = match resume_result {
        Some(result) => Some(result),
        None => handle_resume_with_validation(
            &ctx.args,
            &ctx.config,
            &ctx.registry,
            &ctx.logger,
            &ctx.developer_display,
            &ctx.reviewer_display,
            &*ctx.workspace,
        )?,
    };

    let resume_checkpoint = resume_result.map(|r| r.checkpoint);

    // Create run context - either new or from checkpoint
    let run_context = if let Some(ref checkpoint) = resume_checkpoint {
        use crate::checkpoint::RunContext;
        RunContext::from_checkpoint(checkpoint)
    } else {
        use crate::checkpoint::RunContext;
        RunContext::new()
    };

    // Apply checkpoint configuration restoration if resuming
    let mut config = if let Some(ref checkpoint) = resume_checkpoint {
        use crate::checkpoint::apply_checkpoint_to_config;
        let mut restored_config = ctx.config.clone();
        apply_checkpoint_to_config(&mut restored_config, checkpoint);
        ctx.logger.info("Restored configuration from checkpoint:");
        if checkpoint.cli_args.developer_iters > 0 {
            ctx.logger.info(&format!(
                "  Developer iterations: {} (from checkpoint)",
                checkpoint.cli_args.developer_iters
            ));
        }
        if checkpoint.cli_args.reviewer_reviews > 0 {
            ctx.logger.info(&format!(
                "  Reviewer passes: {} (from checkpoint)",
                checkpoint.cli_args.reviewer_reviews
            ));
        }
        restored_config
    } else {
        ctx.config.clone()
    };

    // Restore environment variables from checkpoint if resuming
    if let Some(ref checkpoint) = resume_checkpoint {
        use crate::checkpoint::restore::restore_environment_from_checkpoint;
        let restored_count = restore_environment_from_checkpoint(checkpoint);
        if restored_count > 0 {
            ctx.logger.info(&format!(
                "  Restored {restored_count} environment variable(s) from checkpoint"
            ));
        }
    }

    // Cloud mode git defaults must be resolved from repo reality.
    // In particular, push/PR head branches must be explicit branch names.
    if config.cloud_config.enabled {
        resolve_cloud_git_defaults(&mut config, ctx)?;
        // Fail-fast if config is still invalid after resolving defaults.
        config
            .cloud_config
            .validate()
            .map_err(|e| anyhow::anyhow!("Cloud config validation failed: {e}"))?;
    }

    // Set up git helpers and agent phase
    // Use workspace-aware marker operations; then attempt hook install / wrapper.
    // This is best-effort: failures must not terminate the pipeline.
    let mut git_helpers = crate::git_helpers::GitHelpers::new();

    // Marker cleanup/creation are filesystem concerns; use Workspace so tests can run
    // with MemoryWorkspace, and production stays consistent.
    if let Err(err) =
        crate::git_helpers::cleanup_orphaned_marker_with_workspace(&*ctx.workspace, &ctx.logger)
    {
        ctx.logger
            .warn(&format!("Failed to cleanup orphaned marker: {err}"));
    }

    // Restore PROMPT.md permissions if left read-only by a prior crashed run.
    // This handles the SIGKILL case where neither the RAII guard nor the reducer
    // could run cleanup. Best-effort: only warn on failure since this is expected
    // recovery behavior after a crash (success is silent).
    if let Some(warning) = crate::files::make_prompt_writable_with_workspace(&*ctx.workspace) {
        ctx.logger
            .warn(&format!("PROMPT.md permission restore on startup: {warning}"));
    }

    if let Err(err) = crate::git_helpers::create_marker_with_workspace(&*ctx.workspace) {
        ctx.logger
            .warn(&format!("Failed to create agent phase marker: {err}"));
    }

    // Hook install / wrapper require a real repo; treat as best-effort.
    // IMPORTANT: do not call std::fs-based orphan marker cleanup here, because we just
    // created the marker via workspace. The wrapper cleanup would consider it "orphaned"
    // and remove it immediately, defeating the safety mechanism.

    // Restore git hooks if left in Ralph-managed state by a prior crashed run.
    //
    // IMPORTANT: Avoid noisy startup warnings in normal repos that do not have
    // Ralph-managed hooks installed. We only attempt an uninstall if a Ralph
    // marker is present in a known hook file.
    //
    // This handles the SIGKILL case where neither the RAII guard nor the reducer
    // could run cleanup. Best-effort: only warn on failure.
    let hooks_dir = crate::git_helpers::get_hooks_dir_in_repo(&ctx.repo_root);
    let ralph_hook_detected = hooks_dir.ok().is_some_and(|dir| {
        ["pre-commit", "pre-push"].into_iter().any(|name| {
            crate::files::file_contains_marker(
                &dir.join(name),
                crate::git_helpers::HOOK_MARKER,
            )
            .unwrap_or(false)
        })
    });

    if ralph_hook_detected {
        if let Err(err) = crate::git_helpers::uninstall_hooks_in_repo(&ctx.repo_root, &ctx.logger)
        {
            ctx.logger
                .warn(&format!("Startup hook cleanup warning: {err}"));
        }
    }

    if let Err(err) = crate::git_helpers::start_agent_phase(&mut git_helpers) {
        ctx.logger
            .warn(&format!("Failed to start agent phase: {err}"));
    }

    let mut agent_phase_guard =
        AgentPhaseGuard::new(&mut git_helpers, &ctx.logger, &*ctx.workspace);

    // Print welcome banner and validate PROMPT.md
    print_welcome_banner(ctx.colors, &ctx.developer_display, &ctx.reviewer_display);
    print_pipeline_info_with_config(ctx, &config);
    validate_prompt_and_setup_backup(ctx)?;

    // Set up PROMPT.md monitoring
    let mut prompt_monitor = setup_prompt_monitor(ctx);

    // Detect project stack and review guidelines
    let (_project_stack, review_guidelines) =
        detect_project_stack(&config, &ctx.repo_root, &ctx.logger, ctx.colors);

    print_review_guidelines(ctx, review_guidelines.as_ref());
    println!();

    // Initialize cloud reporter if cloud mode is enabled
    let cloud_reporter: Arc<dyn CloudReporter> = if config.cloud_config.enabled {
        Arc::new(HttpCloudReporter::new(config.cloud_config.clone()))
    } else {
        Arc::new(NoopCloudReporter)
    };

    // Start heartbeat if cloud mode enabled
    let _heartbeat_guard = if config.cloud_config.enabled {
        Some(HeartbeatGuard::start(
            Arc::clone(&cloud_reporter),
            Duration::from_secs(u64::from(config.cloud_config.heartbeat_interval_secs)),
        ))
    } else {
        None
    };

    // Create phase context and save starting commit
    let mut timer = Timer::new();
    let mut phase_ctx = create_phase_context_with_config(
        ctx,
        &config,
        &mut timer,
        review_guidelines.as_ref(),
        &run_context,
        resume_checkpoint.as_ref(),
        cloud_reporter.as_ref(),
    );
    save_start_commit_or_warn(ctx);

    // Set up interrupt context for checkpoint saving on Ctrl+C
    // This must be done after phase_ctx is created
    let initial_phase = if let Some(ref checkpoint) = resume_checkpoint {
        checkpoint.phase
    } else {
        PipelinePhase::Planning
    };
    setup_interrupt_context_for_pipeline(
        initial_phase,
        config.developer_iters,
        config.reviewer_reviews,
        &phase_ctx.execution_history,
        &phase_ctx.prompt_history,
        &run_context,
        std::sync::Arc::clone(&ctx.workspace),
    );

    // Ensure interrupt context is cleared on completion
    let _interrupt_guard = defer_clear_interrupt_context();

    // Determine if we should run rebase based on current args only.
    let should_run_rebase = ctx.args.rebase_flags.with_rebase;

    // Update interrupt context before entering the reducer event loop.
    update_interrupt_context_from_phase(
        &phase_ctx,
        initial_phase,
        config.developer_iters,
        config.reviewer_reviews,
        &run_context,
        std::sync::Arc::clone(&ctx.workspace),
    );

    // ============================================
    // RUN PIPELINE PHASES VIA REDUCER EVENT LOOP
    // ============================================

    // Initialize pipeline state
    let mut initial_state = if let Some(ref checkpoint) = resume_checkpoint {
        // Restore progress from checkpoint, but keep budgets/limits config-driven.
        // Initialize a config-aware base state first, then overlay checkpoint progress.
        let mut base_state = crate::app::event_loop::create_initial_state_with_config(&phase_ctx);
        let migrated = PipelineState::from_checkpoint_with_execution_history_limit(
            checkpoint.clone(),
            phase_ctx.config.execution_history_limit,
        );

        crate::app::event_loop::overlay_checkpoint_progress_onto_base_state(
            &mut base_state,
            migrated,
            phase_ctx.config.execution_history_limit,
        );

        base_state
    } else {
        // Create new initial state with config-derived continuation limits.
        crate::app::event_loop::create_initial_state_with_config(&phase_ctx)
    };

    if should_run_rebase {
        if matches!(
            initial_state.rebase,
            crate::reducer::state::RebaseState::NotStarted
        ) {
            let default_branch =
                crate::git_helpers::get_default_branch().unwrap_or_else(|_| "main".to_string());
            initial_state.rebase = crate::reducer::state::RebaseState::InProgress {
                original_head: "HEAD".to_string(),
                target_branch: default_branch,
            };
        }
    } else if matches!(
        initial_state.rebase,
        crate::reducer::state::RebaseState::NotStarted
    ) {
        initial_state.rebase = crate::reducer::state::RebaseState::Skipped;
    }

    // Configure event loop
    let event_loop_config = EventLoopConfig {
        max_iterations: event_loop::MAX_EVENT_LOOP_ITERATIONS,
    };

    // Create effect handler and run event loop.
    let loop_result = {
        use crate::app::event_loop::run_event_loop_with_handler;
        let mut handler = MainEffectHandler::new(initial_state.clone());
        let phase_ctx_ref = &mut phase_ctx;
        run_event_loop_with_handler(
            phase_ctx_ref,
            Some(initial_state),
            event_loop_config,
            &mut handler,
        )
    };

    // Handle event loop result
    let loop_result = loop_result?;
    if loop_result.completed {
        match loop_result.final_phase {
            crate::reducer::event::PipelinePhase::Complete => {
                ctx.logger
                    .success("Pipeline completed successfully via reducer event loop");
            }
            crate::reducer::event::PipelinePhase::Interrupted => {
                ctx.logger
                    .info("Pipeline completed with Interrupted phase (failure handled)");
                ctx.logger.info(
                    "Completion marker was written during failure handling. \
                     External orchestration can detect termination via .agent/tmp/completion_marker"
                );
            }
            _ => {
                ctx.logger
                    .success("Pipeline completed via reducer event loop");
            }
        }
        ctx.logger.info(&format!(
            "Total events processed: {}",
            loop_result.events_processed
        ));
    } else {
        ctx.logger
            .error("⚠️  EXCEPTIONAL: Pipeline exited without normal completion");
        ctx.logger.warn(&format!(
            "This indicates a bug in the event loop or reducer. \
             Expected final phase: Complete or Interrupted+checkpoint. \
             Actual: completed=false, final_phase={:?}, events_processed={}",
            loop_result.final_phase, loop_result.events_processed
        ));

        // If we exited from AwaitingDevFix without completing, this is the specific bug
        // we're trying to fix - log it explicitly with state details
        if matches!(
            loop_result.final_phase,
            crate::reducer::event::PipelinePhase::AwaitingDevFix
        ) {
            ctx.logger.error(
                "BUG DETECTED: Event loop exited from AwaitingDevFix without completing dev-fix flow. \
                 This should transition to Interrupted and save checkpoint. \
                 Check: Was TriggerDevFixFlow executed? Was completion marker written? \
                 See .agent/tmp/event_loop_trace.jsonl for execution trace."
            );
        }

        // DEFENSIVE: Emit completion marker for orchestration
        // This ensures external systems can detect termination even if the
        // event loop exited unexpectedly before SaveCheckpoint was processed.
        write_defensive_completion_marker(&*ctx.workspace, &ctx.logger, loop_result.final_phase);
    }

    // If SIGINT was requested while the reducer event loop was active, we must
    // exit with code 130 (SIGINT convention) after reducer-driven cleanup.
    //
    // We intentionally check this AFTER the event loop completes because:
    // - The SIGINT handler only sets a flag when the event loop is active
    // - The event loop translates it into reducer state (interrupted_by_user=true)
    // - We still want checkpoint/permission/hook cleanup to run normally
    //
    // IMPORTANT: A SIGINT may arrive very late (after the event loop has already
    // passed its per-iteration interrupt check). In that case, the reducer will not
    // observe the request, so `interrupted_by_user` may remain false. We still
    // treat this run as user-interrupted for exit-code purposes.
    let pending_sigint_request = crate::interrupt::take_user_interrupt_request();
    let exit_after_cleanup_due_to_sigint =
        loop_result.final_state.interrupted_by_user || pending_sigint_request;

    // Save Complete checkpoint before clearing (for idempotent resume)
    if config.features.checkpoint_enabled
        && should_write_complete_checkpoint(loop_result.final_phase)
    {
        let builder = CheckpointBuilder::new()
            .phase(
                PipelinePhase::Complete,
                config.developer_iters,
                config.developer_iters,
            )
            .reviewer_pass(config.reviewer_reviews, config.reviewer_reviews)
            .capture_from_context(
                &config,
                &ctx.registry,
                &ctx.developer_agent,
                &ctx.reviewer_agent,
                &ctx.logger,
                &run_context,
            )
            .with_executor_from_context(std::sync::Arc::clone(&ctx.executor));

        let builder = builder
            .with_execution_history(phase_ctx.execution_history.clone())
            .with_prompt_history(phase_ctx.clone_prompt_history())
            .with_log_run_id(ctx.run_log_context.run_id().to_string());

        if let Some(checkpoint) = builder.build_with_workspace(&*ctx.workspace) {
            let mut checkpoint = checkpoint;
            if loop_result.final_state.cloud_config.enabled {
                checkpoint.cloud_state = Some(
                    crate::checkpoint::state::CloudCheckpointState::from_pipeline_state(
                        &loop_result.final_state,
                    ),
                );
            }
            let _ = save_checkpoint_with_workspace(&*ctx.workspace, &checkpoint);
        }
    }

    // Cloud completion reporting - notify orchestrator of final result
    // This is done after checkpoint saving to ensure all state is persisted first
    if config.cloud_config.enabled {
        let result_payload = build_cloud_completion_payload(&loop_result, &timer);

        if let Err(e) = cloud_reporter.report_completion(&result_payload) {
            let error = crate::cloud::redaction::redact_secrets(&e.to_string());
            if !config.cloud_config.graceful_degradation {
                return Err(anyhow::anyhow!("Cloud completion report failed: {error}"));
            }
            ctx.logger
                .warn(&format!("Cloud completion report failed: {error}"));
        }
    }

    // Post-pipeline operations
    check_prompt_restoration(ctx, &mut prompt_monitor, "event loop");
    update_status_with_workspace(&*ctx.workspace, "In progress.", config.isolation_mode)?;

    // Finalization
    //
    // IMPORTANT: If the user hit Ctrl+C (either observed by the reducer or late),
    // avoid running finalization. Finalization includes user-visible output and may
    // clear checkpoints; a Ctrl+C should behave like an abort after cleanup.
    //
    // Cleanup is still guaranteed by AgentPhaseGuard::drop() and reducer effects.
    if !exit_after_cleanup_due_to_sigint {
        finalize_pipeline(
            &mut agent_phase_guard,
            crate::app::finalization::FinalizeContext {
                logger: &ctx.logger,
                colors: ctx.colors,
                config: &config,
                timer: &timer,
                workspace: &*ctx.workspace,
            },
            &loop_result.final_state,
            prompt_monitor,
        );
    }

    if exit_after_cleanup_due_to_sigint {
        // Exit after cleanup so SIGINT has the conventional exit code.
        // IMPORTANT: do NOT call `process::exit(130)` here; it would bypass RAII
        // cleanup (AgentPhaseGuard::drop). Instead, request the exit code and let
        // `main()` perform the exit after stack unwinding.
        crate::interrupt::request_exit_130_after_run();
    }

    Ok(())
}

fn build_cloud_completion_payload(
    loop_result: &crate::app::event_loop::EventLoopResult,
    timer: &Timer,
) -> crate::cloud::types::PipelineResult {
    let success = loop_result.completed
        && matches!(
            loop_result.final_phase,
            crate::reducer::event::PipelinePhase::Complete
        );

    crate::cloud::types::PipelineResult {
        success,
        commit_sha: loop_result.final_state.last_pushed_commit.clone().or_else(
            || match &loop_result.final_state.commit {
                crate::reducer::state::CommitState::Committed { hash } => Some(hash.clone()),
                _ => None,
            },
        ),
        pr_url: loop_result.final_state.pr_url.clone(),
        push_count: loop_result.final_state.push_count,
        last_pushed_commit: loop_result.final_state.last_pushed_commit.clone(),
        unpushed_commits: loop_result.final_state.unpushed_commits.clone(),
        last_push_error: loop_result.final_state.last_push_error.clone(),
        iterations_used: loop_result.final_state.metrics.dev_iterations_completed,
        review_passes_used: loop_result.final_state.metrics.review_passes_completed,
        issues_found: loop_result.final_state.review_issues_found,
        duration_secs: timer.elapsed().as_secs(),
        error_message: if matches!(
            loop_result.final_phase,
            crate::reducer::event::PipelinePhase::Interrupted
        ) {
            Some("Pipeline interrupted".to_string())
        } else {
            None
        },
    }
}

#[cfg(test)]
mod cloud_completion_payload_tests {
    use super::build_cloud_completion_payload;

    #[test]
    fn completion_payload_reports_completed_iteration_and_review_counts_from_metrics() {
        let mut state = crate::reducer::PipelineState::initial(10, 5);
        state.metrics.dev_iterations_completed = 3;
        state.metrics.review_passes_completed = 2;
        state.iteration = 4;
        state.reviewer_pass = 3;

        let loop_result = crate::app::event_loop::EventLoopResult {
            completed: true,
            events_processed: 0,
            final_phase: crate::reducer::event::PipelinePhase::Complete,
            final_state: state,
        };

        let timer = crate::pipeline::Timer::new();
        let payload = build_cloud_completion_payload(&loop_result, &timer);

        assert_eq!(
            payload.iterations_used, 3,
            "iterations_used should report completed dev iterations (metrics)"
        );
        assert_eq!(
            payload.review_passes_used, 2,
            "review_passes_used should report completed review passes (metrics)"
        );
    }
}

fn resolve_cloud_git_defaults(
    config: &mut crate::config::Config,
    ctx: &PipelineContext,
) -> anyhow::Result<()> {
    // Default push branch to the current branch name (safe, non-secret).
    if config.cloud_config.git_remote.push_branch.is_none() {
        let output = ctx.executor.execute(
            "git",
            &["rev-parse", "--abbrev-ref", "HEAD"],
            &[],
            Some(&ctx.repo_root),
        )?;
        if !output.status.success() {
            let stderr = crate::cloud::redaction::redact_secrets(&output.stderr);
            return Err(anyhow::anyhow!(
                "Failed to detect current branch for cloud push (git rev-parse). stderr: {stderr}"
            ));
        }

        let branch = output.stdout.trim();
        if branch.is_empty() || branch == "HEAD" {
            return Err(anyhow::anyhow!(
                "Cloud mode requires a branch name for pushing/PRs. Current ref is detached (HEAD). Set RALPH_GIT_PUSH_BRANCH explicitly."
            ));
        }
        config.cloud_config.git_remote.push_branch = Some(branch.to_string());
    }

    // Defensive: reject literal HEAD even if explicitly set.
    if config
        .cloud_config
        .git_remote
        .push_branch
        .as_deref()
        .is_some_and(|b| b.trim() == "HEAD")
    {
        return Err(anyhow::anyhow!(
            "RALPH_GIT_PUSH_BRANCH must be a branch name (not literal 'HEAD')"
        ));
    }

    Ok(())
}
