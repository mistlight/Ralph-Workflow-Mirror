// Planning phase tests.
//
// Tests for planning phase effect determination, agent chain initialization,
// and transition to development.

use super::*;

#[test]
fn test_determine_effect_planning_phase() {
    let state = create_test_state();
    let effect = determine_next_effect(&state);
    assert!(matches!(
        effect,
        Effect::InitializeAgentChain {
            role: AgentRole::Developer
        }
    ));
}

#[test]
fn test_determine_effect_planning_with_agents() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        context_cleaned: true, // Context must be cleaned before planning
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        ),
        ..create_test_state()
    };
    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::MaterializePlanningInputs { .. }));
}

#[test]
fn test_planning_phase_emits_single_task_effect() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        context_cleaned: true,
        iteration: 0,
        total_iterations: 3,
        agent_chain: PipelineState::initial(3, 0).agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        ),
        ..create_test_state()
    };

    let effect = determine_next_effect(&state);

    assert!(
        matches!(effect, Effect::MaterializePlanningInputs { .. }),
        "Planning should emit MaterializePlanningInputs, got {:?}",
        effect
    );
}

#[test]
fn test_planning_phase_transitions_to_development_after_completion() {
    // Create state in Planning phase with agents initialized
    let mut state = PipelineState {
        phase: PipelinePhase::Planning,
        iteration: 1,
        total_iterations: 5,
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        ),
        ..create_test_state()
    };

    // Simulate plan generation completing
    state = reduce(state, PipelineEvent::plan_generation_completed(1, true));

    // After plan generation completes, phase should transition to Development
    assert_eq!(
        state.phase,
        PipelinePhase::Development,
        "Phase should transition to Development after PlanGenerationCompleted"
    );

    // Orchestration should now return PrepareDevelopmentContext
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::PrepareDevelopmentContext { .. }),
        "Expected PrepareDevelopmentContext, got {:?}",
        effect
    );
}

#[test]
fn test_initial_state_skips_planning_when_zero_developer_iters() {
    // When developer_iters=0, the initial state should skip Planning phase entirely
    let state = PipelineState::initial(0, 2);
    assert_eq!(
        state.phase,
        PipelinePhase::Review,
        "Initial phase should be Review when developer_iters=0 and reviewer_reviews>0"
    );
}

#[test]
fn test_initial_state_skips_to_commit_when_zero_iters_and_reviews() {
    // When both developer_iters=0 and reviewer_reviews=0, skip to CommitMessage
    let state = PipelineState::initial(0, 0);
    assert_eq!(
        state.phase,
        PipelinePhase::CommitMessage,
        "Initial phase should be CommitMessage when developer_iters=0 and reviewer_reviews=0"
    );
}

#[test]
fn test_initial_state_starts_planning_when_developer_iters_nonzero() {
    // When developer_iters>0, start in Planning phase as normal
    let state = PipelineState::initial(1, 0);
    assert_eq!(
        state.phase,
        PipelinePhase::Planning,
        "Initial phase should be Planning when developer_iters>0"
    );
}
