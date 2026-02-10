//! Tests for prompt permission orchestration.

use super::*;
use crate::reducer::state::PromptPermissionsState;

#[test]
fn test_orchestration_derives_lock_permissions_before_planning() {
    // Given: Initial state in Planning phase with permissions not locked
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        prompt_permissions: PromptPermissionsState {
            locked: false,
            restore_needed: false,
            restored: false,
            last_warning: None,
        },
        ..PipelineState::initial(1, 0)
    };

    // When: Determining next effect
    let effect = determine_next_effect(&state);

    // Then: Should derive LockPromptPermissions before planning work
    assert!(
        matches!(effect, Effect::LockPromptPermissions),
        "Expected LockPromptPermissions, got {:?}",
        effect
    );
}

#[test]
fn test_orchestration_skips_lock_when_already_locked() {
    use crate::agents::AgentRole;
    use crate::reducer::state::AgentChainState;

    // Given: State with permissions already locked
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        prompt_permissions: PromptPermissionsState {
            locked: true,
            restore_needed: true,
            restored: false,
            last_warning: None,
        },
        gitignore_entries_ensured: true, // Skip gitignore effect
        context_cleaned: true,           // Skip context cleanup effect
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string()],
            vec![vec!["model1".to_string()]],
            AgentRole::Developer,
        ),
        ..PipelineState::initial(1, 0)
    };

    // When: Determining next effect
    let effect = determine_next_effect(&state);

    // Then: Should proceed to phase-specific work, not re-lock
    // (After lock, first planning effect could be MaterializePlanningInputs or PreparePlanningPrompt)
    assert!(
        matches!(
            effect,
            Effect::MaterializePlanningInputs { .. } | Effect::PreparePlanningPrompt { .. }
        ),
        "Expected planning work, got {:?}",
        effect
    );
}

#[test]
fn test_orchestration_derives_restore_permissions_on_interrupted() {
    // Given: State in Interrupted phase with restoration pending
    let state = PipelineState {
        phase: PipelinePhase::Interrupted,
        previous_phase: Some(PipelinePhase::AwaitingDevFix),
        prompt_permissions: PromptPermissionsState {
            locked: true,
            restore_needed: true,
            restored: false,
            last_warning: None,
        },
        checkpoint_saved_count: 0,
        ..PipelineState::initial(1, 0)
    };

    // When: Determining next effect
    let effect = determine_next_effect(&state);

    // Then: Should derive RestorePromptPermissions before SaveCheckpoint
    assert!(
        matches!(effect, Effect::RestorePromptPermissions),
        "Expected RestorePromptPermissions, got {:?}",
        effect
    );
}

#[test]
fn test_orchestration_saves_checkpoint_after_restore_on_interrupted() {
    // Given: State in Interrupted with permissions already restored
    let state = PipelineState {
        phase: PipelinePhase::Interrupted,
        previous_phase: Some(PipelinePhase::AwaitingDevFix),
        prompt_permissions: PromptPermissionsState {
            locked: true,
            restore_needed: true,
            restored: true,
            last_warning: None,
        },
        checkpoint_saved_count: 0,
        ..PipelineState::initial(1, 0)
    };

    // When: Determining next effect
    let effect = determine_next_effect(&state);

    // Then: Should proceed to SaveCheckpoint now that restoration is complete
    assert!(
        matches!(effect, Effect::SaveCheckpoint { .. }),
        "Expected SaveCheckpoint, got {:?}",
        effect
    );
}

#[test]
fn test_finalizing_always_derives_restore_permissions() {
    // Given: State in Finalizing phase with restoration pending
    let state = PipelineState {
        phase: PipelinePhase::Finalizing,
        prompt_permissions: PromptPermissionsState {
            locked: true,
            restore_needed: true,
            restored: false,
            last_warning: None,
        },
        ..PipelineState::initial(1, 0)
    };

    // When: Determine next effect
    let effect = determine_next_effect(&state);

    // Then: Should always derive RestorePromptPermissions
    assert!(
        matches!(effect, Effect::RestorePromptPermissions),
        "Finalizing must derive RestorePromptPermissions, got {:?}",
        effect
    );
}
