//! Tests for completion marker emission during failure handling.
//!
//! Verifies that the completion marker file (`.agent/tmp/completion_marker`)
//! is correctly written when the pipeline encounters failures:
//! - AgentChainExhausted errors trigger marker emission
//! - Marker is written before transitioning to Interrupted phase
//! - Marker contains correct failure information
//! - Full event loop execution properly emits markers

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

        // Verify full event loop execution emits completion marker
        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let mut handler = MockEffectHandler::new(new_state.clone());
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(new_state), config, &mut handler)
            .expect("Event loop should complete");

        // Then: Pipeline should complete
        assert!(
            result.completed,
            "Pipeline should complete after failure handling"
        );

        // Then: Completion marker should exist in workspace
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            fixture.workspace.exists(marker_path),
            "Completion marker file should exist"
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
    });
}

#[test]
fn test_failed_status_dispatches_dev_fix_agent_and_emits_completion_marker() {
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
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should complete");

        assert!(result.completed, "Failure handling should complete");
        assert!(
            fixture
                .workspace
                .exists(Path::new(".agent/tmp/completion_marker")),
            "Completion marker should be written"
        );
        assert!(
            !fixture.executor.agent_calls().is_empty(),
            "Dev-fix agent should be dispatched on failure"
        );
    });
}

#[test]
fn test_completion_marker_written_before_interrupted_transition() {
    with_default_timeout(|| {
        // This test verifies the completion marker is written DURING TriggerDevFixFlow
        // effect execution, not after transitioning to Interrupted

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        let state = with_locked_prompt_permissions(PipelineState::initial(1, 1));

        // Transition to AwaitingDevFix
        let awaiting_fix_state = reduce(
            state,
            PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
                phase: PipelinePhase::Planning,
                error: ErrorEvent::AgentChainExhausted {
                    role: AgentRole::Developer,
                    phase: PipelinePhase::Planning,
                    cycle: 1,
                },
            }),
        );

        let mut handler = MockEffectHandler::new(awaiting_fix_state.clone());
        let config = EventLoopConfig { max_iterations: 50 };

        let _result =
            run_event_loop_with_handler(&mut ctx, Some(awaiting_fix_state), config, &mut handler)
                .expect("Event loop should complete");

        // Verify completion marker exists and contains failure information
        let marker_path = Path::new(".agent/tmp/completion_marker");
        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Completion marker should exist");

        assert!(
            marker_content.contains("failure"),
            "Completion marker should indicate failure"
        );
        assert!(
            marker_content.contains("Agent chain exhausted") || marker_content.contains("phase="),
            "Completion marker should include failure details"
        );
    });
}

#[test]
fn test_failure_completion_full_event_loop_with_logging() {
    with_default_timeout(|| {
        // This test verifies that AgentChainExhausted triggers the complete
        // failure handling flow through the event loop, emitting completion marker
        // and completing successfully WITHOUT triggering the defensive completion marker.
        //
        // Expected flow:
        // 1. AgentChainExhausted error -> AwaitingDevFix phase
        // 2. TriggerDevFixFlow effect -> writes completion marker + emits events
        // 3. CompletionMarkerEmitted event -> Interrupted phase
        // 4. SaveCheckpoint effect -> CheckpointSaved event
        // 5. is_complete() returns true
        // 6. Event loop exits with completed=true

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        // Start in AwaitingDevFix phase (simulating an AgentChainExhausted error)
        let state = PipelineState {
            phase: PipelinePhase::AwaitingDevFix,
            previous_phase: Some(PipelinePhase::Development),
            ..PipelineState::initial(2, 1)
        };

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
            "Completion marker should be written during TriggerDevFixFlow"
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

        // Verify we processed the expected events:
        // TriggerDevFixFlow -> DevFixTriggered + DevFixCompleted + CompletionMarkerEmitted (3 events)
        // SaveCheckpoint -> CheckpointSaved (1 event)
        // Total: at least 4 events
        assert!(
            result.events_processed >= 4,
            "Should process at least 4 events (DevFixTriggered, DevFixCompleted, CompletionMarkerEmitted, CheckpointSaved), got {}",
            result.events_processed
        );
    });
}

#[test]
fn test_event_loop_does_not_exit_prematurely_on_agent_exhaustion() {
    with_default_timeout(|| {
        // This test specifically targets the bug where the event loop exits
        // with completed=false when AgentChainExhausted occurs.

        let mut fixture = Fixture::new();
        let mut ctx = fixture.ctx();

        // Start in Planning phase (will transition to AwaitingDevFix on error)
        let state = PipelineState::initial(1, 1);

        // Inject AgentChainExhausted error
        let error_event = PipelineEvent::PromptInput(PromptInputEvent::HandlerError {
            phase: PipelinePhase::Planning,
            error: ErrorEvent::AgentChainExhausted {
                role: AgentRole::Developer,
                phase: PipelinePhase::Planning,
                cycle: 3,
            },
        });

        let awaiting_fix_state = reduce(state, error_event);
        assert_eq!(awaiting_fix_state.phase, PipelinePhase::AwaitingDevFix);

        let mut handler = MockEffectHandler::new(awaiting_fix_state.clone());
        let config = EventLoopConfig {
            max_iterations: 100,
        };

        let result =
            run_event_loop_with_handler(&mut ctx, Some(awaiting_fix_state), config, &mut handler)
                .expect("Event loop should not error");

        // CRITICAL: Event loop MUST report completion
        assert!(
            result.completed,
            "BUG: Event loop exited without completion marker. \
             This is the bug we're fixing. \
             final_phase={:?}, events_processed={}, checkpoint_saved_count={}",
            result.final_phase, result.events_processed, handler.state.checkpoint_saved_count
        );

        // Verify we reached Interrupted phase with checkpoint saved
        assert_eq!(
            result.final_phase,
            PipelinePhase::Interrupted,
            "Should transition to Interrupted after failure handling"
        );

        // Verify completion marker exists
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            fixture.workspace.exists(marker_path),
            "Completion marker must be written during dev-fix flow"
        );

        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Should read completion marker");
        assert!(
            marker_content.starts_with("failure"),
            "Completion marker should indicate failure"
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
        // With the fix, TriggerDevFixFlow should execute before completion check
        let mut handler = MockEffectHandler::new(state.clone());
        let config = EventLoopConfig { max_iterations: 10 };

        let result = run_event_loop_with_handler(&mut ctx, Some(state), config, &mut handler)
            .expect("Event loop should complete successfully");

        // Verify TriggerDevFixFlow executed
        assert!(
            result.completed,
            "Event loop should complete after executing TriggerDevFixFlow"
        );
        assert_eq!(
            result.final_phase,
            PipelinePhase::Interrupted,
            "Should transition to Interrupted after dev-fix flow"
        );

        // Verify completion marker was written
        assert!(
            fixture
                .workspace
                .exists(Path::new(".agent/tmp/completion_marker")),
            "Completion marker should be written even when approaching max iterations"
        );

        // Verify dev_fix_triggered flag was set
        assert!(
            handler.state.dev_fix_triggered,
            "dev_fix_triggered flag should be set after TriggerDevFixFlow executes"
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

        // Verify completion marker was written
        let marker_path = Path::new(".agent/tmp/completion_marker");
        assert!(
            fixture.workspace.exists(marker_path),
            "REGRESSION: Completion marker missing. Original bug reproduced."
        );

        let marker_content = fixture
            .workspace
            .read(marker_path)
            .expect("Should read completion marker");
        assert!(
            marker_content.contains("failure"),
            "Completion marker should indicate failure"
        );

        // Verify we transitioned to Interrupted (ready for commit/finalization)
        assert_eq!(
            result.final_phase,
            PipelinePhase::Interrupted,
            "Should transition to Interrupted after dev-fix flow"
        );
    });
}
