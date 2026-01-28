//! Tests for development phase events (iterations, plan generation).

use super::*;
use crate::reducer::state::{ContinuationState, DevelopmentStatus};

#[test]
fn test_development_phase_started_sets_development_phase() {
    let state = create_test_state();
    let new_state = reduce(state, PipelineEvent::development_phase_started());

    assert_eq!(new_state.phase, PipelinePhase::Development);
}

#[test]
fn test_development_iteration_started_sets_iteration() {
    let state = create_test_state();
    let new_state = reduce(state, PipelineEvent::development_iteration_started(3));

    assert_eq!(new_state.iteration, 3);
}

#[test]
fn test_development_iteration_started_resets_agent_chain() {
    let base_state = create_test_state();
    // Setup agent chain with multiple agents, models, and retry_cycle
    let mut agent_chain = base_state.agent_chain.with_agents(
        vec!["agent1".to_string(), "agent2".to_string()],
        vec![
            vec!["model1".to_string(), "model2".to_string()],
            vec!["model3".to_string()],
        ],
        crate::agents::AgentRole::Developer,
    );
    agent_chain = agent_chain.switch_to_next_agent(); // Move to agent index 1
    agent_chain.retry_cycle = 5; // Manually set retry_cycle to verify preservation

    let state = PipelineState {
        agent_chain,
        ..base_state
    };

    // Verify we're at agent 1 with retry_cycle = 5
    assert_eq!(state.agent_chain.current_agent_index, 1);
    assert_eq!(state.agent_chain.retry_cycle, 5);

    let new_state = reduce(state, PipelineEvent::development_iteration_started(1));

    // Iteration should be set
    assert_eq!(new_state.iteration, 1);

    // Agent chain should be reset (indices to 0, but retry_cycle preserved)
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
    assert_eq!(new_state.agent_chain.retry_cycle, 5); // Preserved, not reset
}

#[test]
fn test_development_iteration_completed_increments_iteration() {
    // After dev iteration completes, go to CommitMessage (don't increment yet)
    // Iteration increments after commit is created
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 2,
        total_iterations: 5,
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_completed(2, true),
    );

    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    assert_eq!(new_state.iteration, 2); // Don't increment yet

    // After commit, increment and go to Planning
    let new_state = reduce(
        new_state,
        PipelineEvent::commit_created("abc123".to_string(), "test".to_string()),
    );

    assert_eq!(new_state.iteration, 3); // NOW increment
    assert_eq!(new_state.phase, PipelinePhase::Planning);
}

#[test]
fn test_development_iteration_completed_does_not_transition_when_output_invalid() {
    let continuation = ContinuationState::new().trigger_continuation(
        DevelopmentStatus::Partial,
        "partial work".to_string(),
        Some(vec!["src/lib.rs".to_string()]),
        Some("do more".to_string()),
    );
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 0,
        total_iterations: 5,
        continuation: continuation.clone(),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_completed(0, false),
    );

    assert_eq!(new_state.phase, PipelinePhase::Development);
    assert_eq!(new_state.continuation, continuation);
}

#[test]
fn test_development_iteration_completed_stays_in_development_when_more_iterations() {
    // Dev iteration complete -> CommitMessage -> Planning (next iteration)
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 2,
        total_iterations: 5,
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_completed(2, true),
    );

    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    assert_eq!(new_state.iteration, 2);

    // After commit, go to Planning for next iteration
    let new_state = reduce(
        new_state,
        PipelineEvent::commit_created("abc".to_string(), "test".to_string()),
    );

    assert_eq!(new_state.phase, PipelinePhase::Planning);
    assert_eq!(new_state.iteration, 3);
}

#[test]
fn test_development_iteration_completed_transitions_to_review_when_done() {
    // Last dev iteration -> CommitMessage -> Review (all iterations done)
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 2,
        total_iterations: 3,
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_completed(2, true),
    );

    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);

    // After commit, go to Review (all dev iterations done)
    let new_state = reduce(
        new_state,
        PipelineEvent::commit_created("abc".to_string(), "test".to_string()),
    );

    assert_eq!(new_state.phase, PipelinePhase::Review);
    assert_eq!(new_state.iteration, 3);
}

#[test]
fn test_development_iteration_continuation_succeeded_transitions_to_commit_message() {
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 2,
        total_iterations: 5,
        continuation: ContinuationState::new().trigger_continuation(
            DevelopmentStatus::Partial,
            "partial work".to_string(),
            None,
            Some("finish it".to_string()),
        ),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_continuation_succeeded(2, 1),
    );

    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    assert_eq!(new_state.previous_phase, Some(PipelinePhase::Development));
    assert!(matches!(new_state.commit, CommitState::NotStarted));
    assert_eq!(new_state.continuation, ContinuationState::new());
}

#[test]
fn test_development_iteration_completed_with_large_iteration_number() {
    // Large iteration numbers - but not at the edge to avoid overflow
    // Use MAX-2 iteration with MAX total so we can still have one more
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: u32::MAX - 2,
        total_iterations: u32::MAX,
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_completed(u32::MAX - 2, true),
    );

    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);

    // After commit, go to Planning with next iteration (MAX-1)
    let new_state = reduce(
        new_state,
        PipelineEvent::commit_created("abc".to_string(), "test".to_string()),
    );

    assert_eq!(new_state.iteration, u32::MAX - 1);
    assert_eq!(new_state.phase, PipelinePhase::Planning);
}

#[test]
fn test_development_phase_completed_transitions_to_review() {
    let state = create_state_in_phase(PipelinePhase::Development);
    let new_state = reduce(state, PipelineEvent::development_phase_completed());

    assert_eq!(new_state.phase, PipelinePhase::Review);
}

#[test]
fn test_development_iteration_completed_with_zero_total_iterations() {
    // Edge case: iteration 0 with total 0 -> CommitMessage -> Review
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 0,
        total_iterations: 0,
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_completed(0, true),
    );

    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);

    // After commit, go to Review
    let new_state = reduce(
        new_state,
        PipelineEvent::commit_created("abc".to_string(), "test".to_string()),
    );

    assert_eq!(new_state.phase, PipelinePhase::Review);
    assert_eq!(new_state.iteration, 1);
}

#[test]
fn test_development_iteration_started_with_max_u32() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_started(u32::MAX),
    );

    assert_eq!(new_state.iteration, u32::MAX);
}

#[test]
fn test_development_iteration_with_commit_cycle() {
    // Test the full cycle: Planning -> Dev -> CommitMessage -> Planning (next iter)
    let mut state = PipelineState::initial(3, 0); // 3 dev iterations, 0 reviews

    // Start at Planning phase, iteration 0
    assert_eq!(state.phase, PipelinePhase::Planning);
    assert_eq!(state.iteration, 0);

    // After plan generated, transition to Development
    state = reduce(state, PipelineEvent::plan_generation_completed(0, true));
    assert_eq!(state.phase, PipelinePhase::Development);

    // After dev iteration completes, go to CommitMessage
    state = reduce(
        state,
        PipelineEvent::development_iteration_completed(0, true),
    );
    assert_eq!(state.phase, PipelinePhase::CommitMessage);
    assert_eq!(state.iteration, 0); // Don't increment yet!

    // After commit created, go back to Planning for next iteration
    state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "test".to_string()),
    );
    assert_eq!(state.phase, PipelinePhase::Planning);
    assert_eq!(state.iteration, 1); // NOW increment!

    // Repeat for iteration 1
    state = reduce(state, PipelineEvent::plan_generation_completed(1, true));
    assert_eq!(state.phase, PipelinePhase::Development);

    state = reduce(
        state,
        PipelineEvent::development_iteration_completed(1, true),
    );
    assert_eq!(state.phase, PipelinePhase::CommitMessage);

    state = reduce(
        state,
        PipelineEvent::commit_created("def456".to_string(), "test2".to_string()),
    );
    assert_eq!(state.phase, PipelinePhase::Planning);
    assert_eq!(state.iteration, 2);
}
