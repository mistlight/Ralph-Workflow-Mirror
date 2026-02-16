//! Main event loop driver implementing the orchestrate-handle-reduce cycle.
//!
//! This module contains the core iteration logic that repeatedly:
//! 1. Determines the next effect from current state (orchestration)
//! 2. Executes the effect through the handler (side effects)
//! 3. Applies the resulting event through the reducer (pure state transition)
//! 4. Repeats until terminal state or max iterations
//!
//! ## Event Loop Architecture
//!
//! ```text
//! State → determine_next_effect → Effect → execute → Event → reduce → Next State
//!         (pure, from orchestrator)       (impure)          (pure)
//! ```
//!
//! The loop maintains strict separation between pure reducer logic and impure
//! effect handlers, with all state transitions driven by events.

use crate::logging::EventLoopLogger;
use crate::phases::PhaseContext;
use crate::reducer::event::{PipelineEvent, PipelinePhase};
use crate::reducer::{determine_next_effect, reduce, EffectHandler, PipelineState};
use anyhow::Result;
use std::time::Instant;

use super::config::{create_initial_state_with_config, EventLoopConfig, EventLoopResult};
use super::error_handling::{
    execute_effect_guarded, handle_panic, handle_unrecoverable_error, ErrorRecoveryContext,
    GuardedEffectResult,
};
use super::iteration::{should_exit_after_effect, should_exit_before_effect};
use super::recovery::{
    handle_forced_checkpoint_after_completion, handle_max_iterations_in_awaiting_dev_fix,
    RecoveryResult,
};
use super::trace::{
    build_trace_entry, dump_event_loop_trace, EventTraceBuffer, DEFAULT_EVENT_LOOP_TRACE_CAPACITY,
};
use super::StatefulHandler;

fn safe_cloud_error_string(e: &crate::cloud::types::CloudError) -> String {
    crate::cloud::redaction::redact_secrets(&e.to_string())
}

/// Run the main event loop with the given handler and configuration.
///
/// This function implements the reducer-based event loop cycle, orchestrating
/// pure state transitions with impure effect execution while maintaining panic
/// recovery and defensive completion guarantees.
///
/// # Arguments
///
/// * `ctx` - Phase context for effect handlers
/// * `initial_state` - Optional initial state (if None, creates a new state)
/// * `config` - Event loop configuration
/// * `handler` - Effect handler implementing side effects
///
/// # Returns
///
/// Returns the event loop result containing completion status and final state.
pub(super) fn run_event_loop_driver<'ctx, H>(
    ctx: &mut PhaseContext<'_>,
    initial_state: Option<PipelineState>,
    config: EventLoopConfig,
    handler: &mut H,
) -> Result<EventLoopResult>
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    let mut state = initial_state.unwrap_or_else(|| create_initial_state_with_config(ctx));

    handler.update_state(state.clone());
    let mut events_processed = 0;
    let mut trace = EventTraceBuffer::new(DEFAULT_EVENT_LOOP_TRACE_CAPACITY);

    // Create event loop logger, continuing from existing log if present (resume case)
    let event_loop_log_path = ctx.run_log_context.event_loop_log();
    let mut event_loop_logger =
        match EventLoopLogger::from_existing_log(ctx.workspace, &event_loop_log_path) {
            Ok(logger) => logger,
            Err(e) => {
                // If reading existing log fails, log a warning and start fresh
                ctx.logger.warn(&format!(
                    "Failed to read existing event loop log, starting fresh: {}",
                    e
                ));
                EventLoopLogger::new()
            }
        };

    ctx.logger.info("Starting reducer-based event loop");

    while events_processed < config.max_iterations {
        // Check if we're already in a terminal state before executing any effect.
        // This handles the case where the initial state is already complete
        // (e.g., resuming from an Interrupted checkpoint).
        //
        // Special case: If we just transitioned to Interrupted from AwaitingDevFix
        // without a checkpoint, allow one more iteration to execute SaveCheckpoint.
        //
        // CRITICAL: If we're in AwaitingDevFix and haven't executed TriggerDevFixFlow yet,
        // allow at least one iteration to execute it, even if approaching max iterations.
        // This ensures completion marker is ALWAYS written and dev-fix agent is ALWAYS
        // dispatched before the event loop can exit.
        if should_exit_before_effect(&state) {
            ctx.logger.info(&format!(
                "Event loop: state already complete (phase: {:?}, checkpoint_saved_count: {})",
                state.phase, state.checkpoint_saved_count
            ));
            break;
        }

        let effect = determine_next_effect(&state);
        let effect_str = format!("{effect:?}");

        // Execute returns EffectResult with both PipelineEvent and UIEvents.
        // Catch panics here so we can still dump a best-effort trace.
        let start_time = Instant::now();
        let result = match execute_effect_guarded(handler, effect, ctx, &state) {
            GuardedEffectResult::Ok(result) => *result,
            GuardedEffectResult::Unrecoverable(err) => {
                // Non-terminating-by-default requirement:
                // Even "unrecoverable" handler errors must route through reducer-visible
                // remediation (AwaitingDevFix) so TriggerDevFixFlow can write the completion
                // marker and dispatch dev-fix.
                let mut recovery_ctx = ErrorRecoveryContext {
                    ctx,
                    trace: &trace,
                    state: &state,
                    effect_str: &effect_str,
                    start_time,
                    handler,
                    event_loop_logger: &mut event_loop_logger,
                };
                state = handle_unrecoverable_error(&mut recovery_ctx, &err);
                events_processed += 1;
                continue;
            }
            GuardedEffectResult::Panic => {
                // Handler panics are internal failures and must not terminate the run.
                // Route through AwaitingDevFix so TriggerDevFixFlow writes the completion marker
                // and dispatches dev-fix.
                let mut recovery_ctx = ErrorRecoveryContext {
                    ctx,
                    trace: &trace,
                    state: &state,
                    effect_str: &effect_str,
                    start_time,
                    handler,
                    event_loop_logger: &mut event_loop_logger,
                };
                state = handle_panic(&mut recovery_ctx, events_processed);
                events_processed += 1;
                continue;
            }
        };

        let crate::reducer::effect::EffectResult {
            event,
            additional_events,
            ui_events,
        } = result;

        // Display UI events (does not affect state)
        for ui_event in &ui_events {
            ctx.logger
                .info(&crate::rendering::render_ui_event(ui_event));
        }

        let event_str = format!("{:?}", event);
        let duration_ms = start_time.elapsed().as_millis() as u64;

        // Apply pipeline event to state (reducer remains pure)
        let new_state = reduce(state, event.clone());

        // Log to event loop log (best-effort, does not affect correctness)
        log_effect_execution(
            ctx,
            &mut event_loop_logger,
            &new_state,
            &effect_str,
            &event_str,
            &additional_events,
            duration_ms,
        );

        trace.push(build_trace_entry(
            events_processed,
            &new_state,
            &effect_str,
            &event_str,
        ));
        handler.update_state(new_state.clone());
        state = new_state;
        events_processed += 1;

        // Apply additional pipeline events in order.
        for additional_event in additional_events {
            let event_str = format!("{:?}", additional_event);
            let additional_state = reduce(state, additional_event);
            trace.push(build_trace_entry(
                events_processed,
                &additional_state,
                &effect_str,
                &event_str,
            ));
            handler.update_state(additional_state.clone());
            state = additional_state;
            events_processed += 1;
        }

        // Update loop detection counters AFTER all events have been processed.
        // This is critical: additional events can change phase, agent chain, etc.,
        // and loop detection must consider the final state after all reductions.
        let current_fingerprint = crate::reducer::compute_effect_fingerprint(&state);
        state = PipelineState {
            continuation: state
                .continuation
                .update_loop_detection_counters(current_fingerprint),
            ..state
        };
        handler.update_state(state.clone());

        // Cloud progress reporting - ONLY when enabled.
        // Report based on the post-reduction state so phase/iteration values do not lag.
        if let Some(reporter) = ctx.cloud_reporter {
            for ui_event in &ui_events {
                if let Some(update) =
                    ui_event_to_progress_update(ui_event, &state, ctx.cloud_config)
                {
                    if let Err(e) = reporter.report_progress(&update) {
                        let error = safe_cloud_error_string(&e);
                        if !ctx.cloud_config.graceful_degradation {
                            return Err(anyhow::anyhow!("Cloud progress report failed: {}", error));
                        }
                        ctx.logger
                            .warn(&format!("Cloud progress report failed: {}", error));
                    }
                }
            }
        }

        // Check completion AFTER effect execution and state update.
        // This ensures that transitions to terminal phases (e.g., Interrupted)
        // have a chance to save their checkpoint before the loop exits.
        if should_exit_after_effect(&state) {
            ctx.logger.info(&format!(
                "Event loop: state became complete (phase: {:?}, checkpoint_saved_count: {})",
                state.phase, state.checkpoint_saved_count
            ));

            // DEFENSIVE: If we're in Interrupted from AwaitingDevFix without a checkpoint,
            // warn that SaveCheckpoint should execute next
            if matches!(state.phase, PipelinePhase::Interrupted)
                && matches!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix))
                && state.checkpoint_saved_count == 0
            {
                ctx.logger.warn(
                    "Interrupted phase reached from AwaitingDevFix without checkpoint saved. \
                     SaveCheckpoint effect should execute on next iteration.",
                );
            }

            break;
        }
    }

    // Handle edge cases when max iterations is reached
    let mut forced_completion = false;
    let mut recovery_failed = false;
    let mut trace_already_dumped = false;

    if events_processed >= config.max_iterations {
        // Try to force checkpoint execution if needed
        let checkpoint_result = handle_forced_checkpoint_after_completion(
            ctx,
            handler,
            state.clone(),
            events_processed,
            &mut trace,
        );
        match checkpoint_result {
            RecoveryResult::Success(new_state, new_events_processed, dumped) => {
                state = new_state;
                events_processed = new_events_processed;
                trace_already_dumped = trace_already_dumped || dumped;
            }
            RecoveryResult::FailedUnrecoverable(new_state, new_events_processed, dumped) => {
                state = new_state;
                events_processed = new_events_processed;
                recovery_failed = true;
                trace_already_dumped = trace_already_dumped || dumped;
            }
            RecoveryResult::NotNeeded => {
                // No checkpoint needed, continue with existing state
            }
        }

        // If not complete yet and checkpoint didn't fail, try defensive recovery in AwaitingDevFix
        if !state.is_complete() && !recovery_failed {
            let dev_fix_result = handle_max_iterations_in_awaiting_dev_fix(
                ctx,
                handler,
                state.clone(),
                events_processed,
                &mut trace,
            );
            match dev_fix_result {
                RecoveryResult::Success(new_state, new_events_processed, dumped) => {
                    state = new_state;
                    events_processed = new_events_processed;
                    forced_completion = true;
                    trace_already_dumped = trace_already_dumped || dumped;
                }
                RecoveryResult::FailedUnrecoverable(new_state, new_events_processed, dumped) => {
                    state = new_state;
                    events_processed = new_events_processed;
                    recovery_failed = true;
                    trace_already_dumped = trace_already_dumped || dumped;
                }
                RecoveryResult::NotNeeded => {
                    // Not in AwaitingDevFix, no recovery needed
                }
            }
        }

        // Dump trace if we hit max iterations (but only if not already dumped)
        if !trace_already_dumped {
            let dumped = dump_event_loop_trace(ctx, &trace, &state, "max_iterations");
            if dumped {
                let trace_path = ctx.run_log_context.event_loop_trace();
                ctx.logger.warn(&format!(
                    "Event loop reached max iterations ({}) without completion (trace: {})",
                    config.max_iterations,
                    trace_path.display()
                ));
            } else {
                ctx.logger.warn(&format!(
                    "Event loop reached max iterations ({}) without completion",
                    config.max_iterations
                ));
            }
        }

        if !forced_completion && !state.is_complete() {
            ctx.logger.error(&format!(
                "Event loop exiting: reason=max_iterations, phase={:?}, checkpoint_saved_count={}, events_processed={}",
                state.phase, state.checkpoint_saved_count, events_processed
            ));
        }
    }

    // Determine if the loop completed successfully.
    // If recovery failed, report incomplete even if state.is_complete() returns true.
    let completed = state.is_complete() && !recovery_failed;

    if !completed {
        ctx.logger.warn(&format!(
            "Event loop exiting without completion: phase={:?}, checkpoint_saved_count={}, \
             previous_phase={:?}, events_processed={}, recovery_failed={}",
            state.phase,
            state.checkpoint_saved_count,
            state.previous_phase,
            events_processed,
            recovery_failed
        ));
        ctx.logger.info(&format!(
            "Final state: agent_chain.retry_cycle={}, agent_chain.current_role={:?}",
            state.agent_chain.retry_cycle, state.agent_chain.current_role
        ));
    }

    Ok(EventLoopResult {
        completed,
        events_processed,
        final_phase: state.phase,
        final_state: state.clone(),
    })
}

/// Log effect execution to the event loop log.
///
/// This is a best-effort operation - failures are logged but do not affect
/// pipeline execution since event loop logging is for observability only.
pub(super) fn log_effect_execution(
    ctx: &mut PhaseContext<'_>,
    event_loop_logger: &mut EventLoopLogger,
    state: &PipelineState,
    effect_str: &str,
    event_str: &str,
    additional_events: &[PipelineEvent],
    duration_ms: u64,
) {
    let extra_events: Vec<String> = additional_events
        .iter()
        .map(|e| format!("{:?}", e))
        .collect();

    let context_pairs: Vec<(&str, String)> = vec![
        ("iteration", state.iteration.to_string()),
        ("reviewer_pass", state.reviewer_pass.to_string()),
    ];
    let context_refs: Vec<(&str, &str)> = context_pairs
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();

    if let Err(e) = event_loop_logger.log_effect(crate::logging::LogEffectParams {
        workspace: ctx.workspace,
        log_path: &ctx.run_log_context.event_loop_log(),
        phase: state.phase,
        effect: effect_str,
        primary_event: event_str,
        extra_events: &extra_events,
        duration_ms,
        context: &context_refs,
    }) {
        ctx.logger
            .warn(&format!("Failed to write to event loop log: {}", e));
    }
}

/// Convert a UI event to a progress update for cloud reporting.
///
/// Returns None for events that don't warrant cloud progress updates.
fn ui_event_to_progress_update(
    ui_event: &crate::reducer::ui_event::UIEvent,
    state: &PipelineState,
    cloud_config: &crate::config::CloudConfig,
) -> Option<crate::cloud::types::ProgressUpdate> {
    use crate::cloud::types::{ProgressEventType, ProgressUpdate};
    use crate::reducer::ui_event::UIEvent;

    let _run_id = cloud_config.run_id.clone()?;

    fn nonzero(v: u32) -> Option<u32> {
        if v == 0 {
            None
        } else {
            Some(v)
        }
    }

    fn one_based(current_zero_based: u32, total: u32) -> Option<u32> {
        nonzero(total).map(|t| (current_zero_based.saturating_add(1)).min(t))
    }

    let mut iteration = one_based(state.iteration, state.total_iterations);
    let mut total_iterations = nonzero(state.total_iterations);
    let mut review_pass = one_based(state.reviewer_pass, state.total_reviewer_passes);
    let mut total_review_passes = nonzero(state.total_reviewer_passes);
    let mut previous_phase = state.previous_phase.as_ref().map(|p| format!("{:?}", p));

    let (message, event_type) = match ui_event {
        UIEvent::PhaseTransition { from, to } => {
            let from_str = from.as_ref().map(|p| format!("{:?}", p));
            let to_str = format!("{:?}", to);
            previous_phase = from_str.clone();
            let message = format!(
                "Phase transition: {} -> {}",
                from_str.as_deref().unwrap_or("None"),
                to_str
            );
            (
                message,
                ProgressEventType::PhaseTransition {
                    from: from_str,
                    to: to_str,
                },
            )
        }
        UIEvent::IterationProgress { current, total } => {
            let message = format!("Development iteration {}/{}", current, total);
            iteration = Some(*current);
            total_iterations = Some(*total);
            (
                message,
                ProgressEventType::IterationProgress {
                    current: *current,
                    total: *total,
                },
            )
        }
        UIEvent::ReviewProgress { pass, total } => {
            let message = format!("Review pass {}/{}", pass, total);
            review_pass = Some(*pass);
            total_review_passes = Some(*total);
            (
                message,
                ProgressEventType::ReviewProgress {
                    pass: *pass,
                    total: *total,
                },
            )
        }
        UIEvent::AgentActivity {
            agent,
            message: activity_msg,
        } => {
            // Report agent activity for progress tracking
            let message = format!("Agent {}: {}", agent, activity_msg);
            (
                message,
                ProgressEventType::AgentInvoked {
                    role: "Agent".to_string(),
                    agent: agent.clone(),
                },
            )
        }
        UIEvent::PushCompleted {
            remote,
            branch,
            commit_sha,
        } => {
            let short = &commit_sha[..7.min(commit_sha.len())];
            let message = format!("Push completed: {short} -> {remote}/{branch}");
            (
                message,
                ProgressEventType::PushCompleted {
                    remote: remote.clone(),
                    branch: branch.clone(),
                },
            )
        }
        UIEvent::PushFailed {
            remote,
            branch,
            error,
        } => {
            let error = crate::cloud::redaction::redact_secrets(error);
            let message = format!("Push failed: {remote}/{branch}: {error}");
            (
                message,
                ProgressEventType::PushFailed {
                    remote: remote.clone(),
                    branch: branch.clone(),
                    error,
                },
            )
        }
        UIEvent::PullRequestCreated { url, number } => {
            let message = if *number > 0 {
                format!("PR created #{number}: {url}")
            } else {
                format!("PR created: {url}")
            };
            (
                message,
                ProgressEventType::PullRequestCreated {
                    url: url.clone(),
                    number: *number,
                },
            )
        }
        UIEvent::PullRequestFailed { error } => {
            let error = crate::cloud::redaction::redact_secrets(error);
            let message = format!("PR creation failed: {error}");
            (message, ProgressEventType::PullRequestFailed { error })
        }
        // Don't report XmlOutput events to cloud
        UIEvent::XmlOutput { .. } => return None,
    };

    Some(ProgressUpdate {
        timestamp: chrono::Utc::now().to_rfc3339(),
        phase: format!("{:?}", state.phase),
        previous_phase,
        iteration,
        total_iterations,
        review_pass,
        total_review_passes,
        message,
        event_type,
    })
}

#[cfg(test)]
mod progress_mapping_tests {
    use super::ui_event_to_progress_update;
    use crate::config::types::{CloudConfig, GitAuthMethod, GitRemoteConfig};
    use crate::reducer::event::PipelinePhase;
    use crate::reducer::state::PipelineState;
    use crate::reducer::ui_event::UIEvent;

    fn cloud_config_for_test() -> CloudConfig {
        CloudConfig {
            enabled: true,
            api_url: Some("https://api.example.com".to_string()),
            api_token: Some("secret".to_string()),
            run_id: Some("run_1".to_string()),
            heartbeat_interval_secs: 30,
            graceful_degradation: true,
            git_remote: GitRemoteConfig {
                auth_method: GitAuthMethod::SshKey { key_path: None },
                push_branch: Some("main".to_string()),
                create_pr: false,
                pr_title_template: None,
                pr_body_template: None,
                pr_base_branch: None,
                force_push: false,
                remote_name: "origin".to_string(),
            },
        }
    }

    #[test]
    fn iteration_progress_maps_to_iteration_progress_event_type() {
        let cloud = cloud_config_for_test();
        let mut state = PipelineState::initial(10, 0);
        state.phase = PipelinePhase::Development;
        state.iteration = 99;

        let ui = UIEvent::IterationProgress {
            current: 2,
            total: 5,
        };
        let update = ui_event_to_progress_update(&ui, &state, &cloud).expect("update");

        assert_eq!(update.iteration, Some(2));
        assert_eq!(update.total_iterations, Some(5));

        match update.event_type {
            crate::cloud::types::ProgressEventType::IterationProgress { current, total } => {
                assert_eq!(current, 2);
                assert_eq!(total, 5);
            }
            other => panic!("unexpected event type: {other:?}"),
        }
    }

    #[test]
    fn review_progress_maps_to_review_progress_event_type() {
        let cloud = cloud_config_for_test();
        let mut state = PipelineState::initial(10, 0);
        state.phase = PipelinePhase::Review;
        state.reviewer_pass = 99;

        let ui = UIEvent::ReviewProgress { pass: 1, total: 3 };
        let update = ui_event_to_progress_update(&ui, &state, &cloud).expect("update");

        assert_eq!(update.review_pass, Some(1));
        assert_eq!(update.total_review_passes, Some(3));

        match update.event_type {
            crate::cloud::types::ProgressEventType::ReviewProgress { pass, total } => {
                assert_eq!(pass, 1);
                assert_eq!(total, 3);
            }
            other => panic!("unexpected event type: {other:?}"),
        }
    }

    #[test]
    fn push_failed_maps_to_push_failed_event_type() {
        let cloud = cloud_config_for_test();
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::CommitMessage;

        let ui = UIEvent::PushFailed {
            remote: "origin".to_string(),
            branch: "main".to_string(),
            error: "Bearer SECRET".to_string(),
        };
        let update = ui_event_to_progress_update(&ui, &state, &cloud).expect("update");

        match update.event_type {
            crate::cloud::types::ProgressEventType::PushFailed {
                remote,
                branch,
                error,
            } => {
                assert_eq!(remote, "origin");
                assert_eq!(branch, "main");
                assert!(!error.contains("SECRET"), "error must be redacted: {error}");
            }
            other => panic!("unexpected event type: {other:?}"),
        }
    }

    #[test]
    fn phase_transition_uses_one_based_iteration_and_review_pass() {
        let cloud = cloud_config_for_test();
        let mut state = PipelineState::initial(5, 3);
        state.phase = PipelinePhase::Planning;
        state.iteration = 0;
        state.total_iterations = 5;
        state.reviewer_pass = 0;
        state.total_reviewer_passes = 3;

        let ui = UIEvent::PhaseTransition {
            from: None,
            to: PipelinePhase::Development,
        };
        let update = ui_event_to_progress_update(&ui, &state, &cloud).expect("update");

        assert_eq!(update.iteration, Some(1));
        assert_eq!(update.total_iterations, Some(5));
        assert_eq!(update.review_pass, Some(1));
        assert_eq!(update.total_review_passes, Some(3));
    }
}

#[cfg(test)]
mod cloud_progress_error_redaction_tests {
    use super::safe_cloud_error_string;

    #[test]
    fn cloud_progress_errors_are_redacted_and_truncated_for_logs_and_errors() {
        let e = crate::cloud::types::CloudError::HttpError(
            401,
            "Bearer SECRET_TOKEN and https://user:pass@example.com?access_token=abc".to_string(),
        );
        let out = safe_cloud_error_string(&e);

        assert!(!out.contains("SECRET_TOKEN"), "should redact tokens: {out}");
        assert!(
            !out.contains("user:pass"),
            "should redact url userinfo: {out}"
        );
        assert!(
            out.contains("<redacted>"),
            "should include redaction marker: {out}"
        );
    }
}
