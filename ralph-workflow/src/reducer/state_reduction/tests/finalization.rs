// Finalization flow tests.
//
// Tests for finalizing started, prompt permissions restored, and
// the complete finalization orchestration integration.

use super::*;

#[test]
fn test_reduce_finalizing_started() {
    let state = PipelineState {
        phase: PipelinePhase::FinalValidation,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::finalizing_started());
    assert_eq!(new_state.phase, PipelinePhase::Finalizing);
}

#[test]
fn test_reduce_prompt_permissions_restored() {
    let state = PipelineState {
        phase: PipelinePhase::Finalizing,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::prompt_permissions_restored());
    assert_eq!(new_state.phase, PipelinePhase::Complete);
}

#[test]
fn test_reduce_finalization_full_flow() {
    let mut state = PipelineState {
        phase: PipelinePhase::FinalValidation,
        ..create_test_state()
    };

    // FinalValidation -> Finalizing
    state = reduce(state, PipelineEvent::finalizing_started());
    assert_eq!(state.phase, PipelinePhase::Finalizing);

    // Finalizing -> Complete
    state = reduce(state, PipelineEvent::prompt_permissions_restored());
    assert_eq!(state.phase, PipelinePhase::Complete);
}

/// Test the complete finalization flow from FinalValidation through effects.
///
/// This tests the orchestration + reduction path:
/// 1. FinalValidation phase -> ValidateFinalState effect
/// 2. ValidateFinalState effect -> FinalizingStarted event
/// 3. FinalizingStarted event -> Finalizing phase
/// 4. Finalizing phase -> RestorePromptPermissions effect
/// 5. RestorePromptPermissions effect -> PromptPermissionsRestored event
/// 6. PromptPermissionsRestored event -> Complete phase
#[test]
fn test_finalization_orchestration_integration() {
    use crate::reducer::mock_effect_handler::MockEffectHandler;
    use crate::reducer::orchestration::determine_next_effect;

    // Start in FinalValidation
    let mut initial_state = PipelineState::initial(5, 2);
    initial_state.phase = PipelinePhase::FinalValidation;
    // Simulate mid-pipeline (permissions already locked at startup)
    initial_state.prompt_permissions.locked = true;
    initial_state.prompt_permissions.restore_needed = true;

    let mut handler = MockEffectHandler::new(initial_state.clone());

    // Step 1: Determine effect for FinalValidation
    let effect1 = determine_next_effect(&initial_state);
    assert!(
        matches!(effect1, crate::reducer::effect::Effect::ValidateFinalState),
        "FinalValidation should emit ValidateFinalState effect"
    );

    // Step 2: Execute effect, get event
    let result1 = handler.execute_mock(effect1);
    assert!(
        matches!(result1.event, PipelineEvent::FinalizingStarted),
        "ValidateFinalState should return FinalizingStarted"
    );

    // Step 3: Reduce state with event
    let state2 = reduce(initial_state, result1.event);
    assert_eq!(state2.phase, PipelinePhase::Finalizing);
    assert!(!state2.is_complete(), "Finalizing should not be complete");

    // Step 4: Determine effect for Finalizing
    let effect2 = determine_next_effect(&state2);
    assert!(
        matches!(
            effect2,
            crate::reducer::effect::Effect::RestorePromptPermissions
        ),
        "Finalizing should emit RestorePromptPermissions effect"
    );

    // Step 5: Execute effect, get event
    let result2 = handler.execute_mock(effect2);
    assert!(
        matches!(result2.event, PipelineEvent::PromptPermissionsRestored),
        "RestorePromptPermissions should return PromptPermissionsRestored"
    );

    // Step 6: Reduce state with event
    let final_state = reduce(state2, result2.event);
    assert_eq!(final_state.phase, PipelinePhase::Complete);
    assert!(final_state.is_complete(), "Complete should be complete");

    // Verify effects were captured
    let effects = handler.captured_effects();
    assert_eq!(effects.len(), 2);
    assert!(matches!(
        effects[0],
        crate::reducer::effect::Effect::ValidateFinalState
    ));
    assert!(matches!(
        effects[1],
        crate::reducer::effect::Effect::RestorePromptPermissions
    ));
}
