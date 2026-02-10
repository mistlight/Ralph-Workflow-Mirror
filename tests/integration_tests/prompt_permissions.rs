//! Integration tests for PROMPT.md permission lifecycle.

use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::PipelinePhase;
use ralph_workflow::reducer::mock_effect_handler::MockEffectHandler;
use ralph_workflow::reducer::orchestration::determine_next_effect;
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::state_reduction::reduce;

use crate::test_timeout::with_default_timeout;

#[test]
fn test_permission_lifecycle_success_path() {
    with_default_timeout(|| {
        // Given: Initial state (not locked yet)
        let initial_state = PipelineState::initial(0, 0); // 0 iters, 0 reviews -> goes to CommitMessage

        let mut handler = MockEffectHandler::new(initial_state.clone());

        // Step 1: First effect should be LockPromptPermissions
        let effect1 = determine_next_effect(&initial_state);
        assert!(
            matches!(effect1, Effect::LockPromptPermissions),
            "First effect should lock permissions, got {:?}",
            effect1
        );

        // Step 2: Execute lock, get event
        let result1 = handler.execute_mock(effect1);
        let state2 = reduce(initial_state, result1.event);
        assert!(state2.prompt_permissions.locked);
        assert!(state2.prompt_permissions.restore_needed);
        assert!(!state2.prompt_permissions.restored);

        // Step 3: Now should proceed to commit phase work (skipping lock)
        let effect3 = determine_next_effect(&state2);
        assert!(
            matches!(
                effect3,
                Effect::InitializeAgentChain { .. }
                    | Effect::EnsureGitignoreEntries
                    | Effect::PrepareCommitPrompt { .. }
            ),
            "After lock, should proceed to commit work, got {:?}",
            effect3
        );

        // Fast-forward: simulate pipeline completing through to Finalizing
        let finalizing_state = PipelineState {
            phase: PipelinePhase::Finalizing,
            prompt_permissions: state2.prompt_permissions.clone(),
            ..state2
        };

        // Step 4: Finalizing should derive RestorePromptPermissions
        let effect4 = determine_next_effect(&finalizing_state);
        assert!(
            matches!(effect4, Effect::RestorePromptPermissions),
            "Finalizing should restore permissions"
        );

        // Step 5: Execute restore, get event, reduce
        let result4 = handler.execute_mock(effect4);
        let final_state = reduce(finalizing_state, result4.event);

        // Step 6: Verify final state
        assert_eq!(final_state.phase, PipelinePhase::Complete);
        assert!(final_state.prompt_permissions.restored);
    });
}

#[test]
fn test_permission_lifecycle_failure_path() {
    with_default_timeout(|| {
        // Given: Initial state
        let initial_state = PipelineState::initial(1, 0);

        let mut handler = MockEffectHandler::new(initial_state.clone());

        // Step 1: Lock permissions at startup
        let effect1 = determine_next_effect(&initial_state);
        assert!(matches!(effect1, Effect::LockPromptPermissions));

        let result1 = handler.execute_mock(effect1);
        let state2 = reduce(initial_state, result1.event);
        assert!(state2.prompt_permissions.locked);

        // Fast-forward: simulate failure path (AwaitingDevFix → Interrupted)
        let interrupted_state = PipelineState {
            phase: PipelinePhase::Interrupted,
            previous_phase: Some(PipelinePhase::AwaitingDevFix),
            checkpoint_saved_count: 0,
            prompt_permissions: state2.prompt_permissions.clone(),
            ..state2
        };

        // Step 2: On Interrupted, should restore BEFORE saving checkpoint
        let effect2 = determine_next_effect(&interrupted_state);
        assert!(
            matches!(effect2, Effect::RestorePromptPermissions),
            "Interrupted should restore permissions before checkpoint, got {:?}",
            effect2
        );

        // Step 3: Execute restore
        let result2 = handler.execute_mock(effect2);
        let state3 = reduce(interrupted_state, result2.event);

        // Step 4: Verify phase stays Interrupted (not Complete)
        assert_eq!(state3.phase, PipelinePhase::Interrupted);
        assert!(state3.prompt_permissions.restored);

        // Step 5: Now should save checkpoint
        let effect3 = determine_next_effect(&state3);
        assert!(
            matches!(effect3, Effect::SaveCheckpoint { .. }),
            "After restore on Interrupted, should save checkpoint"
        );
    });
}

#[test]
fn test_permission_restoration_on_resume_from_interrupted() {
    with_default_timeout(|| {
        // Given: Resumed state in Interrupted with restoration pending
        // (simulates checkpoint saved after lock but before restore completed)
        let resumed_state = PipelineState {
            phase: PipelinePhase::Interrupted,
            previous_phase: Some(PipelinePhase::AwaitingDevFix),
            checkpoint_saved_count: 1, // Already saved once
            prompt_permissions: ralph_workflow::reducer::state::PromptPermissionsState {
                locked: true,
                restore_needed: true,
                restored: false,
                last_warning: None,
            },
            ..PipelineState::initial(1, 0)
        };

        let mut handler = MockEffectHandler::new(resumed_state.clone());

        // Step 1: On resume, should still derive RestorePromptPermissions
        let effect1 = determine_next_effect(&resumed_state);
        assert!(
            matches!(effect1, Effect::RestorePromptPermissions),
            "Resume should restore permissions if pending, got {:?}",
            effect1
        );

        // Step 2: Execute restore
        let result1 = handler.execute_mock(effect1);
        let final_state = reduce(resumed_state, result1.event);

        // Step 3: Verify restoration completed, phase stays Interrupted
        assert_eq!(final_state.phase, PipelinePhase::Interrupted);
        assert!(final_state.prompt_permissions.restored);
    });
}

#[test]
fn test_permission_restoration_on_user_interrupt() {
    with_default_timeout(|| {
        // Given: Pipeline interrupted by user (not AwaitingDevFix path)
        let initial_state = PipelineState::initial(2, 0);
        let mut handler = MockEffectHandler::new(initial_state.clone());

        // Step 1: Lock at startup
        let effect1 = determine_next_effect(&initial_state);
        assert!(matches!(effect1, Effect::LockPromptPermissions));
        let result1 = handler.execute_mock(effect1);
        let state2 = reduce(initial_state, result1.event);

        // Step 2: Simulate user Ctrl+C by transitioning to Interrupted
        // (normally triggered by signal handler, here we simulate it)
        let interrupted_state = PipelineState {
            phase: PipelinePhase::Interrupted,
            previous_phase: Some(PipelinePhase::Planning), // NOT AwaitingDevFix
            checkpoint_saved_count: 0,
            prompt_permissions: state2.prompt_permissions.clone(),
            ..state2
        };

        // Step 3: Should restore BEFORE checkpoint
        let effect3 = determine_next_effect(&interrupted_state);
        assert!(
            matches!(effect3, Effect::RestorePromptPermissions),
            "User interrupt should restore permissions, got {:?}",
            effect3
        );

        let result3 = handler.execute_mock(effect3);
        let state4 = reduce(interrupted_state, result3.event);

        // Step 4: Should stay Interrupted, then save checkpoint
        assert_eq!(state4.phase, PipelinePhase::Interrupted);
        assert!(state4.prompt_permissions.restored);

        let effect5 = determine_next_effect(&state4);
        assert!(matches!(effect5, Effect::SaveCheckpoint { .. }));
    });
}
