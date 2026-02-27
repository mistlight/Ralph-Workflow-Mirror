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
use crate::reducer::event::{ErrorEvent, PipelineEvent, PipelinePhase, PromptInputEvent};
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
struct LoopRuntime {
    state: PipelineState,
    events_processed: usize,
    trace: EventTraceBuffer,
    event_loop_logger: EventLoopLogger,
}

enum IterationResult {
    Continue,
    Break,
}

enum EffectExecutionOutcome {
    Continue,
    EffectResult(Box<crate::reducer::effect::EffectResult>),
}

struct MaxIterationRecovery {
    recovery_failed: bool,
}

fn create_event_loop_logger(ctx: &PhaseContext<'_>) -> EventLoopLogger {
    let event_loop_log_path = ctx.run_log_context.event_loop_log();
    match EventLoopLogger::from_existing_log(ctx.workspace, &event_loop_log_path) {
        Ok(logger) => logger,
        Err(e) => {
            ctx.logger.warn(&format!(
                "Failed to read existing event loop log, starting fresh: {e}"
            ));
            EventLoopLogger::new()
        }
    }
}

fn handle_user_interrupt<'ctx, H>(
    ctx: &PhaseContext<'_>,
    handler: &mut H,
    runtime: &mut LoopRuntime,
) -> bool
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    if !crate::interrupt::take_user_interrupt_request() {
        return false;
    }

    let effect_str = "Signal(SIGINT)".to_string();
    let interrupt_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
        phase: runtime.state.phase,
        error: ErrorEvent::UserInterruptRequested,
    });
    let event_str = format!("{interrupt_event:?}");
    let start_time = Instant::now();
    let new_state = reduce(runtime.state.clone(), interrupt_event);
    let duration_ms = u64::try_from(start_time.elapsed().as_millis()).unwrap_or(u64::MAX);

    log_effect_execution(
        ctx,
        &mut runtime.event_loop_logger,
        &new_state,
        &effect_str,
        &event_str,
        &[],
        duration_ms,
    );

    runtime.trace.push(build_trace_entry(
        runtime.events_processed,
        &new_state,
        &effect_str,
        &event_str,
    ));
    handler.update_state(new_state.clone());
    runtime.state = new_state;
    runtime.events_processed += 1;
    true
}

fn report_cloud_progress(
    ctx: &PhaseContext<'_>,
    state: &PipelineState,
    ui_events: &[crate::reducer::ui_event::UIEvent],
) -> Result<()> {
    if let Some(reporter) = ctx.cloud_reporter {
        for ui_event in ui_events {
            if let Some(update) = ui_event_to_progress_update(ui_event, state, ctx.cloud) {
                if let Err(e) = reporter.report_progress(&update) {
                    let error = safe_cloud_error_string(&e);
                    if !ctx.cloud.graceful_degradation {
                        return Err(anyhow::anyhow!("Cloud progress report failed: {error}"));
                    }
                    ctx.logger
                        .warn(&format!("Cloud progress report failed: {error}"));
                }
            }
        }
    }

    Ok(())
}

fn execute_effect_with_recovery<'ctx, H>(
    ctx: &mut PhaseContext<'_>,
    handler: &mut H,
    runtime: &mut LoopRuntime,
    effect_str: &str,
    start_time: Instant,
    effect: crate::reducer::effect::Effect,
) -> EffectExecutionOutcome
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    match execute_effect_guarded(handler, effect, ctx, &runtime.state) {
        GuardedEffectResult::Ok(result) => EffectExecutionOutcome::EffectResult(result),
        GuardedEffectResult::Unrecoverable(err) => {
            let mut recovery_ctx = ErrorRecoveryContext {
                ctx,
                trace: &runtime.trace,
                state: &runtime.state,
                effect_str,
                start_time,
                handler,
                event_loop_logger: &mut runtime.event_loop_logger,
            };
            runtime.state = handle_unrecoverable_error(&mut recovery_ctx, &err);
            runtime.events_processed += 1;
            EffectExecutionOutcome::Continue
        }
        GuardedEffectResult::Panic => {
            let mut recovery_ctx = ErrorRecoveryContext {
                ctx,
                trace: &runtime.trace,
                state: &runtime.state,
                effect_str,
                start_time,
                handler,
                event_loop_logger: &mut runtime.event_loop_logger,
            };
            runtime.state = handle_panic(&mut recovery_ctx, runtime.events_processed);
            runtime.events_processed += 1;
            EffectExecutionOutcome::Continue
        }
    }
}

fn process_primary_event<'ctx, H>(
    ctx: &PhaseContext<'_>,
    handler: &mut H,
    runtime: &mut LoopRuntime,
    effect_str: &str,
    event: PipelineEvent,
    additional_events: &[PipelineEvent],
    duration_ms: u64,
) where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    let event_str = format!("{event:?}");
    let new_state = reduce(runtime.state.clone(), event);

    log_effect_execution(
        ctx,
        &mut runtime.event_loop_logger,
        &new_state,
        effect_str,
        &event_str,
        additional_events,
        duration_ms,
    );

    runtime.trace.push(build_trace_entry(
        runtime.events_processed,
        &new_state,
        effect_str,
        &event_str,
    ));
    handler.update_state(new_state.clone());
    runtime.state = new_state;
    runtime.events_processed += 1;
}

fn process_additional_events<'ctx, H>(
    handler: &mut H,
    runtime: &mut LoopRuntime,
    effect_str: &str,
    additional_events: Vec<PipelineEvent>,
) where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    for additional_event in additional_events {
        let event_str = format!("{additional_event:?}");
        let additional_state = reduce(runtime.state.clone(), additional_event);
        runtime.trace.push(build_trace_entry(
            runtime.events_processed,
            &additional_state,
            effect_str,
            &event_str,
        ));
        handler.update_state(additional_state.clone());
        runtime.state = additional_state;
        runtime.events_processed += 1;
    }
}

fn update_loop_detection_state<'ctx, H>(handler: &mut H, runtime: &mut LoopRuntime)
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    let current_fingerprint = crate::reducer::compute_effect_fingerprint(&runtime.state);
    let continuation = runtime
        .state
        .continuation
        .clone()
        .update_loop_detection_counters(current_fingerprint);
    runtime.state = PipelineState {
        continuation,
        ..runtime.state.clone()
    };
    handler.update_state(runtime.state.clone());
}

fn log_completion_transition_if_needed(ctx: &PhaseContext<'_>, state: &PipelineState) -> bool {
    if !should_exit_after_effect(state) {
        return false;
    }

    ctx.logger.info(&format!(
        "Event loop: state became complete (phase: {:?}, checkpoint_saved_count: {})",
        state.phase, state.checkpoint_saved_count
    ));

    if matches!(state.phase, PipelinePhase::Interrupted)
        && matches!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix))
        && state.checkpoint_saved_count == 0
    {
        ctx.logger.warn(
            "Interrupted phase reached from AwaitingDevFix without checkpoint saved. \
             SaveCheckpoint effect should execute on next iteration.",
        );
    }

    true
}

fn execute_single_iteration<'ctx, H>(
    ctx: &mut PhaseContext<'_>,
    handler: &mut H,
    runtime: &mut LoopRuntime,
) -> Result<IterationResult>
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    if should_exit_before_effect(&runtime.state) {
        ctx.logger.info(&format!(
            "Event loop: state already complete (phase: {:?}, checkpoint_saved_count: {})",
            runtime.state.phase, runtime.state.checkpoint_saved_count
        ));
        return Ok(IterationResult::Break);
    }

    if handle_user_interrupt(ctx, handler, runtime) {
        return Ok(IterationResult::Continue);
    }

    let effect = determine_next_effect(&runtime.state);
    let effect_str = format!("{effect:?}");
    let start_time = Instant::now();

    let result = match execute_effect_with_recovery(
        ctx,
        handler,
        runtime,
        &effect_str,
        start_time,
        effect,
    ) {
        EffectExecutionOutcome::Continue => return Ok(IterationResult::Continue),
        EffectExecutionOutcome::EffectResult(result) => *result,
    };

    let crate::reducer::effect::EffectResult {
        event,
        additional_events,
        ui_events,
    } = result;

    for ui_event in &ui_events {
        ctx.logger
            .info(&crate::rendering::render_ui_event(ui_event));
    }

    let duration_ms = u64::try_from(start_time.elapsed().as_millis()).unwrap_or(u64::MAX);
    process_primary_event(
        ctx,
        handler,
        runtime,
        &effect_str,
        event,
        &additional_events,
        duration_ms,
    );
    process_additional_events(handler, runtime, &effect_str, additional_events);
    update_loop_detection_state(handler, runtime);
    report_cloud_progress(ctx, &runtime.state, &ui_events)?;

    if log_completion_transition_if_needed(ctx, &runtime.state) {
        return Ok(IterationResult::Break);
    }

    Ok(IterationResult::Continue)
}

fn apply_recovery_result(
    recovery: RecoveryResult,
    runtime: &mut LoopRuntime,
    trace_already_dumped: &mut bool,
) -> bool {
    match recovery {
        RecoveryResult::Success(new_state, new_events_processed, dumped) => {
            runtime.state = new_state;
            runtime.events_processed = new_events_processed;
            *trace_already_dumped = *trace_already_dumped || dumped;
            false
        }
        RecoveryResult::FailedUnrecoverable(new_state, new_events_processed, dumped) => {
            runtime.state = new_state;
            runtime.events_processed = new_events_processed;
            *trace_already_dumped = *trace_already_dumped || dumped;
            true
        }
        RecoveryResult::NotNeeded => false,
    }
}

fn handle_max_iteration_recovery<'ctx, H>(
    ctx: &mut PhaseContext<'_>,
    handler: &mut H,
    config: EventLoopConfig,
    runtime: &mut LoopRuntime,
) -> MaxIterationRecovery
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    let mut forced_completion = false;
    let mut recovery_failed = false;
    let mut trace_already_dumped = false;

    if runtime.events_processed < config.max_iterations {
        return MaxIterationRecovery { recovery_failed };
    }

    let checkpoint_result = handle_forced_checkpoint_after_completion(
        ctx,
        handler,
        runtime.state.clone(),
        runtime.events_processed,
        &mut runtime.trace,
    );
    recovery_failed = apply_recovery_result(checkpoint_result, runtime, &mut trace_already_dumped);

    if !runtime.state.is_complete() && !recovery_failed {
        let dev_fix_result = handle_max_iterations_in_awaiting_dev_fix(
            ctx,
            handler,
            runtime.state.clone(),
            runtime.events_processed,
            &mut runtime.trace,
        );
        recovery_failed = apply_recovery_result(dev_fix_result, runtime, &mut trace_already_dumped);
        forced_completion = !recovery_failed;
    }

    if !trace_already_dumped {
        let dumped = dump_event_loop_trace(ctx, &runtime.trace, &runtime.state, "max_iterations");
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

    if !forced_completion && !runtime.state.is_complete() {
        ctx.logger.error(&format!(
            "Event loop exiting: reason=max_iterations, phase={:?}, checkpoint_saved_count={}, events_processed={}",
            runtime.state.phase, runtime.state.checkpoint_saved_count, runtime.events_processed
        ));
    }

    MaxIterationRecovery { recovery_failed }
}

pub(super) fn run_event_loop_driver<'ctx, H>(
    ctx: &mut PhaseContext<'_>,
    initial_state: Option<PipelineState>,
    config: EventLoopConfig,
    handler: &mut H,
) -> Result<EventLoopResult>
where
    H: EffectHandler<'ctx> + StatefulHandler,
{
    let mut runtime = LoopRuntime {
        state: initial_state.unwrap_or_else(|| create_initial_state_with_config(ctx)),
        events_processed: 0,
        trace: EventTraceBuffer::new(DEFAULT_EVENT_LOOP_TRACE_CAPACITY),
        event_loop_logger: create_event_loop_logger(ctx),
    };

    handler.update_state(runtime.state.clone());
    ctx.logger.info("Starting reducer-based event loop");

    let _event_loop_guard = crate::interrupt::event_loop_active_guard();

    while runtime.events_processed < config.max_iterations {
        match execute_single_iteration(ctx, handler, &mut runtime)? {
            IterationResult::Continue => {}
            IterationResult::Break => break,
        }
    }

    let recovery = handle_max_iteration_recovery(ctx, handler, config, &mut runtime);
    let completed = runtime.state.is_complete() && !recovery.recovery_failed;

    if !completed {
        ctx.logger.warn(&format!(
            "Event loop exiting without completion: phase={:?}, checkpoint_saved_count={}, \
             previous_phase={:?}, events_processed={}, recovery_failed={}",
            runtime.state.phase,
            runtime.state.checkpoint_saved_count,
            runtime.state.previous_phase,
            runtime.events_processed,
            recovery.recovery_failed
        ));
        ctx.logger.info(&format!(
            "Final state: agent_chain.retry_cycle={}, agent_chain.current_role={:?}",
            runtime.state.agent_chain.retry_cycle, runtime.state.agent_chain.current_role
        ));
    }

    Ok(EventLoopResult {
        completed,
        events_processed: runtime.events_processed,
        final_phase: runtime.state.phase,
        final_state: runtime.state.clone(),
    })
}

/// Log effect execution to the event loop log.
///
/// This is a best-effort operation - failures are logged but do not affect
/// pipeline execution since event loop logging is for observability only.
pub(super) fn log_effect_execution(
    ctx: &PhaseContext<'_>,
    event_loop_logger: &mut EventLoopLogger,
    state: &PipelineState,
    effect_str: &str,
    event_str: &str,
    additional_events: &[PipelineEvent],
    duration_ms: u64,
) {
    let extra_events: Vec<String> = additional_events.iter().map(|e| format!("{e:?}")).collect();

    let context_pairs: Vec<(&str, String)> = vec![
        ("iteration", state.iteration.to_string()),
        ("reviewer_pass", state.reviewer_pass.to_string()),
    ];
    let context_refs: Vec<(&str, &str)> = context_pairs
        .iter()
        .map(|(k, v)| (*k, v.as_str()))
        .collect();

    if let Err(e) = event_loop_logger.log_effect(&crate::logging::LogEffectParams {
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
            .warn(&format!("Failed to write to event loop log: {e}"));
    }
}

#[derive(Debug)]
struct ProgressContextFields {
    iteration: Option<u32>,
    total_iterations: Option<u32>,
    review_pass: Option<u32>,
    total_review_passes: Option<u32>,
    previous_phase: Option<String>,
}

impl ProgressContextFields {
    fn from_state(state: &PipelineState) -> Self {
        Self {
            iteration: one_based(state.iteration, state.total_iterations),
            total_iterations: nonzero(state.total_iterations),
            review_pass: one_based(state.reviewer_pass, state.total_reviewer_passes),
            total_review_passes: nonzero(state.total_reviewer_passes),
            previous_phase: state.previous_phase.as_ref().map(|p| format!("{p:?}")),
        }
    }
}

const fn nonzero(v: u32) -> Option<u32> {
    if v == 0 {
        None
    } else {
        Some(v)
    }
}

fn one_based(current_zero_based: u32, total: u32) -> Option<u32> {
    nonzero(total).map(|t| (current_zero_based.saturating_add(1)).min(t))
}

fn phase_transition_progress(
    from: Option<PipelinePhase>,
    to: PipelinePhase,
    fields: &mut ProgressContextFields,
) -> (String, crate::cloud::types::ProgressEventType) {
    let from_str = from.map(|p| format!("{p:?}"));
    let to_str = format!("{to:?}");
    fields.previous_phase.clone_from(&from_str);
    let message = format!(
        "Phase transition: {} -> {}",
        from_str.as_deref().unwrap_or("None"),
        to_str
    );
    (
        message,
        crate::cloud::types::ProgressEventType::PhaseTransition {
            from: from_str,
            to: to_str,
        },
    )
}

fn iteration_progress(
    current: u32,
    total: u32,
    fields: &mut ProgressContextFields,
) -> (String, crate::cloud::types::ProgressEventType) {
    fields.iteration = Some(current);
    fields.total_iterations = Some(total);
    (
        format!("Development iteration {current}/{total}"),
        crate::cloud::types::ProgressEventType::IterationProgress { current, total },
    )
}

fn review_progress(
    pass: u32,
    total: u32,
    fields: &mut ProgressContextFields,
) -> (String, crate::cloud::types::ProgressEventType) {
    fields.review_pass = Some(pass);
    fields.total_review_passes = Some(total);
    (
        format!("Review pass {pass}/{total}"),
        crate::cloud::types::ProgressEventType::ReviewProgress { pass, total },
    )
}

fn agent_activity_progress(agent: &str) -> (String, crate::cloud::types::ProgressEventType) {
    (
        format!("Agent {agent}: activity"),
        crate::cloud::types::ProgressEventType::AgentInvoked {
            role: "Agent".to_string(),
            agent: agent.to_string(),
        },
    )
}

fn push_completed_progress(
    remote: &str,
    branch: &str,
    commit_sha: &str,
) -> (String, crate::cloud::types::ProgressEventType) {
    let short = &commit_sha[..7.min(commit_sha.len())];
    (
        format!("Push completed: {short} -> {remote}/{branch}"),
        crate::cloud::types::ProgressEventType::PushCompleted {
            remote: remote.to_string(),
            branch: branch.to_string(),
        },
    )
}

fn push_failed_progress(
    remote: &str,
    branch: &str,
    error: &str,
) -> (String, crate::cloud::types::ProgressEventType) {
    let error = crate::cloud::redaction::redact_secrets(error);
    (
        format!("Push failed: {remote}/{branch}: {error}"),
        crate::cloud::types::ProgressEventType::PushFailed {
            remote: remote.to_string(),
            branch: branch.to_string(),
            error,
        },
    )
}

fn pull_request_created_progress(
    url: &str,
    number: u32,
) -> (String, crate::cloud::types::ProgressEventType) {
    let message = if number > 0 {
        format!("PR created #{number}: {url}")
    } else {
        format!("PR created: {url}")
    };
    (
        message,
        crate::cloud::types::ProgressEventType::PullRequestCreated {
            url: url.to_string(),
            number,
        },
    )
}

fn pull_request_failed_progress(error: &str) -> (String, crate::cloud::types::ProgressEventType) {
    let error = crate::cloud::redaction::redact_secrets(error);
    (
        format!("PR creation failed: {error}"),
        crate::cloud::types::ProgressEventType::PullRequestFailed { error },
    )
}

fn map_ui_event_to_progress(
    ui_event: &crate::reducer::ui_event::UIEvent,
    fields: &mut ProgressContextFields,
) -> Option<(String, crate::cloud::types::ProgressEventType)> {
    use crate::reducer::ui_event::UIEvent;

    match ui_event {
        UIEvent::PhaseTransition { from, to } => {
            Some(phase_transition_progress(*from, *to, fields))
        }
        UIEvent::IterationProgress { current, total } => {
            Some(iteration_progress(*current, *total, fields))
        }
        UIEvent::ReviewProgress { pass, total } => Some(review_progress(*pass, *total, fields)),
        UIEvent::AgentActivity {
            agent,
            message: _activity_msg,
        } => Some(agent_activity_progress(agent)),
        UIEvent::PushCompleted {
            remote,
            branch,
            commit_sha,
        } => Some(push_completed_progress(remote, branch, commit_sha)),
        UIEvent::PushFailed {
            remote,
            branch,
            error,
        } => Some(push_failed_progress(remote, branch, error)),
        UIEvent::PullRequestCreated { url, number } => {
            Some(pull_request_created_progress(url, *number))
        }
        UIEvent::PullRequestFailed { error } => Some(pull_request_failed_progress(error)),
        UIEvent::XmlOutput { .. } => None,
    }
}

/// Convert a UI event to a progress update for cloud reporting.
///
/// Returns None for events that don't warrant cloud progress updates.
fn ui_event_to_progress_update(
    ui_event: &crate::reducer::ui_event::UIEvent,
    state: &PipelineState,
    cloud: &crate::config::CloudConfig,
) -> Option<crate::cloud::types::ProgressUpdate> {
    use crate::cloud::types::ProgressUpdate;

    let _run_id = cloud.run_id.clone()?;
    let mut fields = ProgressContextFields::from_state(state);
    let (message, event_type) = map_ui_event_to_progress(ui_event, &mut fields)?;

    Some(ProgressUpdate {
        timestamp: chrono::Utc::now().to_rfc3339(),
        phase: format!("{:?}", state.phase),
        previous_phase: fields.previous_phase,
        iteration: fields.iteration,
        total_iterations: fields.total_iterations,
        review_pass: fields.review_pass,
        total_review_passes: fields.total_review_passes,
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

    fn cloud_for_test() -> CloudConfig {
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
        let cloud = cloud_for_test();
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
        let cloud = cloud_for_test();
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
        let cloud = cloud_for_test();
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
        let cloud = cloud_for_test();
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

    #[test]
    fn agent_activity_is_not_forwarded_verbatim_to_cloud_progress() {
        let cloud = cloud_for_test();
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Development;

        let ui = UIEvent::AgentActivity {
            agent: "dev-agent".to_string(),
            message: "token=SECRET_VALUE and /home/user/.ssh/id_rsa".to_string(),
        };
        let update = ui_event_to_progress_update(&ui, &state, &cloud).expect("update");

        assert!(
            update.message.contains("dev-agent"),
            "should still identify which agent produced activity"
        );
        assert!(
            !update.message.contains("SECRET_VALUE"),
            "must not forward raw activity text containing secrets"
        );
        assert!(
            !update.message.contains("id_rsa"),
            "must not forward sensitive paths from activity messages"
        );
    }

    #[test]
    fn mapping_returns_none_when_run_id_missing() {
        let mut cloud = cloud_for_test();
        cloud.run_id = None;

        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Planning;

        let ui = UIEvent::IterationProgress {
            current: 1,
            total: 1,
        };
        let update = ui_event_to_progress_update(&ui, &state, &cloud);

        assert!(update.is_none(), "run_id is required for cloud progress");
    }

    #[test]
    fn phase_transition_uses_event_from_for_previous_phase() {
        let cloud = cloud_for_test();
        let mut state = PipelineState::initial(2, 1);
        state.phase = PipelinePhase::Development;
        state.previous_phase = Some(PipelinePhase::Planning);

        let ui = UIEvent::PhaseTransition {
            from: Some(PipelinePhase::Review),
            to: PipelinePhase::CommitMessage,
        };
        let update = ui_event_to_progress_update(&ui, &state, &cloud).expect("update");

        assert_eq!(update.previous_phase.as_deref(), Some("Review"));
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
