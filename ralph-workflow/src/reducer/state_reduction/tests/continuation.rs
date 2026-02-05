// Continuation event handling tests.
//
// Tests for continuation triggered, succeeded, budget exhausted events,
// and continuation state management during development iterations.

use super::*;

#[test]
fn test_continuation_triggered_updates_state() {
    use crate::reducer::state::DevelopmentStatus;

    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_triggered(
            1,
            DevelopmentStatus::Partial,
            "Did work".to_string(),
            Some(vec!["src/main.rs".to_string()]),
            Some("Continue".to_string()),
        ),
    );

    assert!(new_state.continuation.is_continuation());
    assert_eq!(
        new_state.continuation.previous_status,
        Some(DevelopmentStatus::Partial)
    );
    assert_eq!(
        new_state.continuation.previous_summary,
        Some("Did work".to_string())
    );
    assert_eq!(
        new_state.continuation.previous_files_changed,
        Some(vec!["src/main.rs".to_string()])
    );
    assert_eq!(
        new_state.continuation.previous_next_steps,
        Some("Continue".to_string())
    );
    assert_eq!(new_state.continuation.continuation_attempt, 1);
}

#[test]
fn test_continuation_triggered_sets_iteration_from_event() {
    use crate::reducer::state::DevelopmentStatus;

    let state = PipelineState {
        iteration: 99,
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_triggered(
            2,
            DevelopmentStatus::Partial,
            "Did work".to_string(),
            None,
            None,
        ),
    );

    assert_eq!(new_state.iteration, 2);
}

#[test]
fn test_continuation_triggered_with_failed_status() {
    use crate::reducer::state::DevelopmentStatus;

    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_triggered(
            1,
            DevelopmentStatus::Failed,
            "Build failed".to_string(),
            None,
            Some("Fix errors".to_string()),
        ),
    );

    assert!(new_state.continuation.is_continuation());
    assert_eq!(
        new_state.continuation.previous_status,
        Some(DevelopmentStatus::Failed)
    );
    assert_eq!(
        new_state.continuation.previous_summary,
        Some("Build failed".to_string())
    );
    assert!(new_state.continuation.previous_files_changed.is_none());
}

#[test]
fn test_continuation_succeeded_resets_state() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = create_test_state();
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );
    assert!(state.continuation.is_continuation());

    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_succeeded(1, 2),
    );

    assert!(!new_state.continuation.is_continuation());
    assert_eq!(new_state.continuation.continuation_attempt, 0);
    assert!(new_state.continuation.previous_status.is_none());
}

#[test]
fn test_continuation_succeeded_sets_iteration_from_event() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 99,
        ..create_test_state()
    };
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );

    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_succeeded(1, 1),
    );

    assert_eq!(new_state.iteration, 1);
}

#[test]
fn test_iteration_started_resets_continuation() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = create_test_state();
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );
    assert!(state.continuation.is_continuation());

    let new_state = reduce(state, PipelineEvent::development_iteration_started(2));

    assert!(!new_state.continuation.is_continuation());
    assert_eq!(new_state.iteration, 2);
}

#[test]
fn test_iteration_completed_resets_continuation() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = create_test_state();
    state.phase = PipelinePhase::Development;
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );

    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_completed(1, true),
    );

    assert!(!new_state.continuation.is_continuation());
    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
}

#[test]
fn test_development_phase_completed_resets_continuation() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = create_test_state();
    state.phase = PipelinePhase::Development;
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );

    let new_state = reduce(state, PipelineEvent::development_phase_completed());

    assert!(!new_state.continuation.is_continuation());
    assert_eq!(new_state.phase, PipelinePhase::Review);
}

#[test]
fn test_multiple_continuation_triggers_accumulate() {
    use crate::reducer::state::DevelopmentStatus;

    let state = create_test_state();

    // First continuation
    let state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_triggered(
            1,
            DevelopmentStatus::Partial,
            "First attempt".to_string(),
            None,
            None,
        ),
    );
    assert_eq!(state.continuation.continuation_attempt, 1);

    // Second continuation
    let state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_triggered(
            1,
            DevelopmentStatus::Partial,
            "Second attempt".to_string(),
            None,
            None,
        ),
    );
    assert_eq!(state.continuation.continuation_attempt, 2);
    assert_eq!(
        state.continuation.previous_summary,
        Some("Second attempt".to_string())
    );
}

#[test]
fn test_continuation_budget_exhausted_switches_to_next_agent() {
    use crate::reducer::state::DevelopmentStatus;

    let state = create_test_state();
    assert_eq!(state.agent_chain.current_agent_index, 0);

    let new_state = reduce(
        state,
        PipelineEvent::development_continuation_budget_exhausted(0, 3, DevelopmentStatus::Partial),
    );
    assert_eq!(
        new_state.phase,
        PipelinePhase::Development,
        "Should stay in Development phase to try next agent"
    );
    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should switch to next agent when continuation budget exhausted"
    );
    assert_eq!(
        new_state.continuation.continuation_attempt, 0,
        "Should reset continuation attempt for new agent"
    );
}

#[test]
fn test_continuation_budget_exhausted_resets_continuation_state() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let mut state = create_test_state();
    state.continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "Work".to_string(),
        None,
        None,
    );
    assert!(state.continuation.is_continuation());

    let new_state = reduce(
        state,
        PipelineEvent::development_continuation_budget_exhausted(0, 3, DevelopmentStatus::Partial),
    );
    assert!(
        !new_state.continuation.is_continuation(),
        "Continuation state should be reset when switching to next agent"
    );
    assert_eq!(
        new_state.continuation.continuation_attempt, 0,
        "Continuation attempt should be reset for new agent"
    );
}

#[test]
fn test_continuation_budget_exhausted_preserves_iteration() {
    use crate::reducer::state::DevelopmentStatus;

    let mut state = create_test_state();
    state.iteration = 5;

    let new_state = reduce(
        state,
        PipelineEvent::development_continuation_budget_exhausted(5, 3, DevelopmentStatus::Failed),
    );
    assert_eq!(
        new_state.iteration, 5,
        "Should preserve the iteration number"
    );
}

#[test]
fn test_orchestration_detects_exhaustion_after_all_agents_tried() {
    use crate::agents::AgentRole;
    use crate::reducer::effect::Effect;
    use crate::reducer::orchestration::determine_next_effect;
    use crate::reducer::state::{AgentChainState, DevelopmentStatus};

    let agent_chain = AgentChainState::initial()
        .with_agents(
            vec!["agent-a".to_string(), "agent-b".to_string()],
            vec![vec!["model-1".to_string()], vec!["model-2".to_string()]],
            AgentRole::Developer,
        )
        .with_max_cycles(1); // Only 1 cycle allowed

    let mut state = PipelineState::initial(5, 3);
    state.agent_chain = agent_chain;
    state.phase = PipelinePhase::Development;

    // Exhaust continuation for agent-a
    state = reduce(
        state,
        PipelineEvent::development_continuation_budget_exhausted(0, 3, DevelopmentStatus::Failed),
    );
    assert_eq!(state.agent_chain.current_agent_index, 1); // Now on agent-b

    // Clean up context before next agent
    state = reduce(
        state,
        PipelineEvent::development_continuation_context_cleaned(),
    );

    // Exhaust continuation for agent-b (last agent) with Failed status
    // CRITICAL: Since all agents are exhausted AND last status is Failed,
    // the reducer now transitions directly to AwaitingDevFix instead of
    // just switching agents. This ensures the pipeline NEVER exits early
    // due to budget exhaustion - it always continues through dev-fix flow.
    state = reduce(
        state,
        PipelineEvent::development_continuation_budget_exhausted(0, 3, DevelopmentStatus::Failed),
    );

    // Should transition to AwaitingDevFix (new behavior for non-terminating pipeline)
    assert_eq!(
        state.phase,
        PipelinePhase::AwaitingDevFix,
        "Should transition to AwaitingDevFix when all agents exhausted with Failed status"
    );
    assert_eq!(
        state.previous_phase,
        Some(PipelinePhase::Development),
        "Should preserve previous phase"
    );
    assert!(
        !state.dev_fix_triggered,
        "dev_fix_triggered should be false so TriggerDevFixFlow executes"
    );

    // Now orchestration should trigger dev-fix flow
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::TriggerDevFixFlow { .. }),
        "Should trigger dev-fix flow when in AwaitingDevFix phase; got {:?}",
        effect
    );
}
