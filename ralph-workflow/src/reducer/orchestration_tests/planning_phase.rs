// Planning phase orchestration tests.
//
// Tests for planning phase: agent chain initialization, prompt preparation,
// XSD retry mode, and transition to development phase.

use super::*;

#[test]
fn test_planning_initializes_agent_chain_when_empty() {
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
fn test_planning_prepares_prompt_when_agents_ready() {
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
fn test_planning_prompt_uses_xsd_retry_mode_when_pending() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        context_cleaned: true,
        iteration: 0,
        total_iterations: 1,
        continuation: PipelineState::initial(1, 1)
            .continuation
            .trigger_xsd_retry(),
        agent_chain: PipelineState::initial(1, 1).agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        ),
        ..create_test_state()
    };

    let effect = determine_next_effect(&state);

    assert!(matches!(
        effect,
        Effect::PreparePlanningPrompt {
            iteration: 0,
            prompt_mode: PromptMode::XsdRetry
        }
    ));
}

#[test]
fn test_planning_emits_prepare_prompt_effect() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        context_cleaned: true,
        iteration: 0,
        total_iterations: 5,
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
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
fn test_planning_transitions_to_development_after_completion() {
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

    // Plan generation completes
    state = reduce(state, PipelineEvent::plan_generation_completed(1, true));

    assert_eq!(
        state.phase,
        PipelinePhase::Development,
        "Phase should transition to Development after PlanGenerationCompleted"
    );

    // Orchestration should now return PrepareDevelopmentContext, not planning effects
    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::PrepareDevelopmentContext { .. }),
        "Expected PrepareDevelopmentContext, got {:?}",
        effect
    );
}
