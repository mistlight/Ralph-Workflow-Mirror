//! Tests for completion marker emission during failure handling.
//!
//! Verifies completion marker semantics for failure handling.
//!
//! Completion markers are written only when the pipeline is actually terminating
//! (Effect::EmitCompletionMarkerAndTerminate), not when entering recovery.

use super::common::Fixture;
use crate::common::with_locked_prompt_permissions;
use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::AgentRole;
use ralph_workflow::app::event_loop::{run_event_loop_with_handler, EventLoopConfig};
use ralph_workflow::reducer::determine_next_effect;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::{ErrorEvent, PipelineEvent, PipelinePhase, PromptInputEvent};
use ralph_workflow::reducer::handler::MainEffectHandler;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::state::{AgentChainState, PipelineState};
use ralph_workflow::reducer::state_reduction::reduce;
use ralph_workflow::reducer::EffectHandler;
use std::path::Path;

#[test]
fn test_agent_chain_exhausted_emits_completion_marker() {
    with_default_timeout(|| {
        // Given: Initial pipeline state
        let state = with_locked_prompt_permissions(PipelineState::initial(1, 1));
        assert_eq!(state.phase, PipelinePhase::Planning);

        // When: AgentChainExhausted error occurs
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: state.phase,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 3,
            },
        });

        let new_state = reduce(state, error_event);

        // Then: State transitions to AwaitingDevFix
        assert_eq!(new_state.phase, PipelinePhase::AwaitingDevFix);
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Planning));

        // When: Orchestration determines next effect
        let effect = determine_next_effect(&new_state);

        // Then: Effect should be TriggerDevFixFlow
        assert!(
            matches!(effect, Effect::TriggerDevFixFlow { .. }),
            "Expected TriggerDevFixFlow, got {:?}",
            effect
        );

        // TriggerDevFixFlow must NOT write the completion marker.
        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();
        let mut handler = MockEffectHandler::new(new_state.clone());

        let _result = handler
            .execute(effect, &mut ctx)
            .expect("TriggerDevFixFlow should not error");

        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            !fixture.workspace.exists(marker_path),
            "Completion marker must not be written during recovery entry"
        );
    });
}

#[test]
fn test_failed_status_dispatches_dev_fix_agent_without_emitting_completion_marker() {
    with_default_timeout(|| {
        let mut fixture = Fixture::new();
        fixture
            .workspace
            .write(Path::new("PROMPT.md"), "Fix pipeline failure")
            .expect("PROMPT.md should be writable");
        fixture
            .workspace
            .write(
                Path::new(".agent/PLAN.md"),
                "1. Diagnose failure\n2. Fix root cause",
            )
            .expect("PLAN.md should be writable");

        let mut ctx = fixture.ctx();

        let mut state = PipelineState {
            phase: PipelinePhase::AwaitingDevFix,
            previous_phase: Some(PipelinePhase::Development),
            ..with_locked_prompt_permissions(PipelineState::initial(1, 1))
        };
        state.agent_chain = AgentChainState::initial().with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        let mut handler = MainEffectHandler::new(state.clone());

        // Execute a single TriggerDevFixFlow effect (do not run full event loop).
        // The recovery loop is non-terminating by default, so driving the full loop
        // here is both slow and unnecessary.
        let result = handler
            .execute(
                Effect::TriggerDevFixFlow {
                    failed_phase: PipelinePhase::Development,
                    failed_role: AgentRole::Developer,
                    retry_cycle: 0,
                },
                &mut ctx,
            )
            .expect("TriggerDevFixFlow should succeed");

        assert!(
            !fixture.executor.agent_calls().is_empty(),
            "Dev-fix agent should be dispatched on failure"
        );

        // TriggerDevFixFlow must NOT emit completion marker.
        assert!(
            !fixture
                .workspace
                .exists(Path::new(".agent/tmp/completion_marker")),
            "Completion marker must not be written during recovery"
        );

        assert!(
            result.additional_events.iter().any(|e| {
                matches!(
                    e,
                    PipelineEvent::AwaitingDevFix(
                        ralph_workflow::reducer::event::AwaitingDevFixEvent::DevFixCompleted { .. }
                    )
                )
            }),
            "TriggerDevFixFlow should emit DevFixCompleted so recovery can advance"
        );
    });
}

#[test]
fn test_completion_marker_written_before_interrupted_transition() {
    with_default_timeout(|| {
        // Completion marker must be written by EmitCompletionMarkerAndTerminate
        // BEFORE the reducer transitions to Interrupted.

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let mut state = with_locked_prompt_permissions(PipelineState::initial(1, 0));
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Development);

        let mut handler = MainEffectHandler::new(state.clone());
        let result = handler
            .execute(
                Effect::EmitCompletionMarkerAndTerminate {
                    is_failure: true,
                    reason: Some("test termination".to_string()),
                },
                &mut ctx,
            )
            .expect("EmitCompletionMarkerAndTerminate should succeed");

        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            fixture.workspace.exists(marker_path),
            "Completion marker should be written before Interrupted transition"
        );

        let new_state = reduce(state, result.event);
        assert_eq!(
            new_state.phase,
            PipelinePhase::Interrupted,
            "Reducer should transition to Interrupted after CompletionMarkerEmitted"
        );
    });
}

#[test]
fn test_failure_completion_full_event_loop_with_logging() {
    with_default_timeout(|| {
        // This test verifies the termination path only:
        // when recovery is exhausted, orchestration derives EmitCompletionMarkerAndTerminate,
        // the handler writes the marker, and the reducer transitions to Interrupted.

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let mut state = with_locked_prompt_permissions(PipelineState::initial(2, 1));
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Development);
        state.failed_phase_for_recovery = Some(PipelinePhase::Development);
        state.dev_fix_attempt_count = 13;
        state.recovery_escalation_level = 4;
        state.dev_fix_triggered = true;

        let mut handler = MockEffectHandler::new(state.clone());
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should not error");

        // Verify completion marker was written
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            fixture.workspace.exists(marker_path),
            "Completion marker should be written during termination"
        );

        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Should read completion marker");
        assert!(
            marker_content.starts_with("failure"),
            "Completion marker should indicate failure, got: {}",
            marker_content
        );

        // CRITICAL: Event loop should complete successfully
        assert!(
            result.completed,
            "Event loop MUST complete after failure handling. \
             If this fails, the 'Pipeline exited without completion marker' bug has occurred. \
             Check event loop logs for: phase, checkpoint_saved_count, exit reason."
        );

        assert_eq!(
            result.final_phase,
            PipelinePhase::Interrupted,
            "Termination should end in Interrupted"
        );
    });
}

#[test]
fn test_event_loop_does_not_exit_prematurely_on_agent_exhaustion() {
    with_default_timeout(|| {
        // This test verifies the recovery loop behavior after AgentChainExhausted.
        // With the recovery loop fix, the pipeline will:
        // 1. Transition to AwaitingDevFix
        // 2. Execute TriggerDevFixFlow
        // 3. Execute RecoveryAttempted (transition back to failed phase)
        // 4. Retry the work (will fail again with mock handler)
        // 5. Repeat up to 12 times
        // 6. Eventually emit completion marker and transition to Interrupted

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        // Start with state that has ALREADY completed 13 recovery attempts
        // This simulates exhaustion and ensures immediate termination for the test
        let mut state = PipelineState::initial(1, 1);
        state.phase = PipelinePhase::AwaitingDevFix;
        state.previous_phase = Some(PipelinePhase::Planning);
        state.failed_phase_for_recovery = Some(PipelinePhase::Planning);
        state.dev_fix_attempt_count = 13; // Already exhausted (>12 triggers termination)
        state.recovery_escalation_level = 4;
        state.dev_fix_triggered = true; // Dev-fix already ran

        let mut handler = MockEffectHandler::new(state.clone());
        let config = EventLoopConfig {
            max_iterations: 20, // Reduced since we expect quick termination at exhaustion
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should not error");

        // CRITICAL: Event loop MUST report completion
        assert!(
            result.completed,
            "BUG: Event loop exited without completion marker. \
             After recovery exhaustion (12+ attempts), should emit completion marker. \
             final_phase={:?}, events_processed={}, checkpoint_saved_count={}",
            result.final_phase, result.events_processed, handler.state.checkpoint_saved_count
        );

        // Verify we reached Interrupted phase with checkpoint saved
        assert_eq!(
            result.final_phase,
            PipelinePhase::Interrupted,
            "Should transition to Interrupted after exhausting recovery attempts"
        );

        // Verify EmitCompletionMarkerAndTerminate effect was executed
        // (MockEffectHandler captures effects but doesn't write actual files)
        assert!(
            handler.was_effect_executed(|e| matches!(
                e,
                Effect::EmitCompletionMarkerAndTerminate {
                    is_failure: true,
                    ..
                }
            )),
            "EmitCompletionMarkerAndTerminate effect should be executed after recovery exhaustion"
        );
    });
}

#[test]
fn test_awaiting_dev_fix_executes_trigger_before_max_iterations() {
    with_default_timeout(|| {
        // This test verifies the fix for the bug where the event loop could exit
        // from AwaitingDevFix without executing TriggerDevFixFlow when approaching
        // max iterations.

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        // Create a state that transitions to AwaitingDevFix after several iterations
        let mut state = PipelineState::initial(1, 1);

        // Simulate AgentChainExhausted error
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Planning,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Planning,
                cycle: 3,
            },
        });

        state = reduce(state, error_event);
        assert_eq!(state.phase, PipelinePhase::AwaitingDevFix);
        assert!(
            !state.dev_fix_triggered,
            "dev_fix_triggered should start false"
        );

        // Set a low max_iterations to simulate approaching the limit
        // With the bug, the loop would exit here without executing TriggerDevFixFlow
        // With the fix, TriggerDevFixFlow should execute and recovery should be attempted
        let mut handler = MockEffectHandler::new(state.clone());
        let config = EventLoopConfig { max_iterations: 10 };

        let _result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should run until max_iterations");

        // With recovery enabled, the pipeline will attempt recovery and may hit max_iterations
        // before completing. The important thing is that TriggerDevFixFlow DID execute.

        // Verify dev_fix_triggered flag was set (TriggerDevFixFlow executed)
        assert!(
            handler.state.dev_fix_triggered,
            "dev_fix_triggered flag should be set after TriggerDevFixFlow executes"
        );

        // Verify recovery was attempted (transitioned back from AwaitingDevFix)
        assert!(
            handler.state.previous_phase == Some(PipelinePhase::AwaitingDevFix)
                || handler.state.phase == PipelinePhase::AwaitingDevFix
                || handler.state.recovery_escalation_level > 0,
            "Pipeline should have attempted recovery (previous_phase={:?}, phase={:?}, recovery_level={})",
            handler.state.previous_phase,
            handler.state.phase,
            handler.state.recovery_escalation_level
        );
    });
}

#[test]
fn test_regression_pipeline_exits_without_completion_marker_on_dev_iter_2_failure() {
    with_default_timeout(|| {
        // Regression test for: "Pipeline exited without completion marker"
        // Scenario: Development Iteration 2 fails, Status: Failed, pipeline should
        // continue via dev-fix flow (or commit if budget exhausted), not exit early.

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        // Start at Development iteration 2 (simulating the bug report scenario)
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.iteration = 2;

        // Simulate AgentChainExhausted during Development
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Development,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Development,
                cycle: 3,
            },
        });

        let awaiting_fix_state = reduce(state, error_event);
        assert_eq!(
            awaiting_fix_state.phase,
            PipelinePhase::AwaitingDevFix,
            "AgentChainExhausted should transition to AwaitingDevFix"
        );

        let mut handler = MockEffectHandler::new(awaiting_fix_state.clone());
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result =
            run_event_loop_with_handler(&mut ctx, Some(awaiting_fix_state), config, &mut handler)
                .expect("Event loop should not error");

        // CRITICAL: Pipeline must complete, not exit early
        assert!(
            result.completed,
            "REGRESSION: Pipeline exited without completion. \
             This is the original bug. \
             Status: Failed should trigger dev-fix flow, not immediate exit. \
             final_phase={:?}, events_processed={}",
            result.final_phase, result.events_processed
        );

        // Completion marker must NOT be written during recovery.
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            !fixture.workspace.exists(marker_path),
            "Completion marker must not be written during recovery"
        );

        // With the mock handler, the pipeline recovers and completes.
        assert_eq!(result.final_phase, PipelinePhase::Complete);
    });
}
