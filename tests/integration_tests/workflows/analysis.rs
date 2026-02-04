//! Integration tests for independent result analysis.
//!
//! These tests verify that the analysis agent is invoked after EVERY
//! development iteration to produce an objective assessment based on git diff
//! vs PLAN.md.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow the integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.

use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::{DevelopmentEvent, PipelineEvent, PipelinePhase};
use ralph_workflow::reducer::orchestration::determine_next_effect;
use ralph_workflow::reducer::state::PipelineState;
use ralph_workflow::reducer::state_reduction::reduce;

use crate::test_timeout::with_default_timeout;

/// Test that AnalysisAgentInvoked event type exists and can be constructed.
///
/// This basic test verifies:
/// 1. The AnalysisAgentInvoked event variant exists
/// 2. It can be constructed with an iteration number
#[test]
fn test_analysis_agent_invoked_event_exists() {
    with_default_timeout(|| {
        // Verify the event type can be constructed
        let event =
            PipelineEvent::Development(DevelopmentEvent::AnalysisAgentInvoked { iteration: 0 });

        // Verify it's the correct variant
        match event {
            PipelineEvent::Development(DevelopmentEvent::AnalysisAgentInvoked { iteration }) => {
                assert_eq!(iteration, 0);
            }
            _ => panic!("Expected AnalysisAgentInvoked event"),
        }
    });
}

/// Test that InvokeAnalysisAgent effect type exists and can be constructed.
///
/// This test verifies:
/// 1. The InvokeAnalysisAgent effect variant exists
/// 2. It can be constructed with an iteration number
#[test]
fn test_invoke_analysis_agent_effect_exists() {
    with_default_timeout(|| {
        // Verify the effect type can be constructed
        let effect = Effect::InvokeAnalysisAgent { iteration: 0 };

        // Verify it's the correct variant
        match effect {
            Effect::InvokeAnalysisAgent { iteration } => {
                assert_eq!(iteration, 0);
            }
            _ => panic!("Expected InvokeAnalysisAgent effect"),
        }
    });
}

/// Test that analysis agent is invoked after the first iteration when multiple iterations exist.
///
/// This test verifies that analysis runs after EVERY development iteration,
/// not just the final one.
#[test]
fn test_analysis_runs_after_first_iteration_when_multiple_iterations() {
    with_default_timeout(|| {
        use ralph_workflow::agents::AgentRole;

        // Given: Pipeline with 3 total iterations, first iteration just completed
        let mut state = PipelineState::initial(3, 2);
        state.phase = PipelinePhase::Development;
        state.iteration = 0; // First iteration

        // Mark all prerequisite development steps as complete for iteration 0
        state.development_context_prepared_iteration = Some(0);
        state.development_prompt_prepared_iteration = Some(0);
        state.development_xml_cleaned_iteration = Some(0);
        state.development_agent_invoked_iteration = Some(0);

        // Set up agent chain (required for orchestration)
        state.agent_chain = state.agent_chain.with_agents(
            vec!["claude".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // When: Determining next effect
        let effect = determine_next_effect(&state);

        // Then: Should invoke analysis agent for iteration 0
        assert!(
            matches!(effect, Effect::InvokeAnalysisAgent { iteration: 0 }),
            "Analysis agent should run after first iteration, got {:?}",
            effect
        );
    });
}

/// Test that analysis agent is invoked after EVERY iteration.
///
/// Verifies the core requirement: analysis must run after each development
/// iteration, regardless of iteration count.
#[test]
fn test_analysis_runs_after_every_iteration() {
    with_default_timeout(|| {
        use ralph_workflow::agents::AgentRole;

        // Test across multiple iterations
        for iter in 0..3 {
            // Given: Pipeline with 3 iterations, current iteration just completed
            let mut state = PipelineState::initial(3, 2);
            state.phase = PipelinePhase::Development;
            state.iteration = iter;

            // Mark all prerequisite development steps as complete for this iteration
            state.development_context_prepared_iteration = Some(iter);
            state.development_prompt_prepared_iteration = Some(iter);
            state.development_xml_cleaned_iteration = Some(iter);
            state.development_agent_invoked_iteration = Some(iter);

            // Set up agent chain (required for orchestration)
            state.agent_chain = state.agent_chain.with_agents(
                vec!["claude".to_string()],
                vec![vec![]],
                AgentRole::Developer,
            );

            // When: Determining next effect after dev agent completes
            let effect = determine_next_effect(&state);

            // Then: Should invoke analysis agent for this iteration
            assert!(
                matches!(effect, Effect::InvokeAnalysisAgent { iteration: i } if i == iter),
                "Analysis should run after iteration {}, got {:?}",
                iter,
                effect
            );
        }
    });
}

/// Test that analysis agent does NOT run before development agent completes.
///
/// Verifies the sequencing: dev agent must complete before analysis agent runs.
#[test]
fn test_analysis_does_not_run_before_dev_agent_completes() {
    with_default_timeout(|| {
        // Given: Pipeline where development agent has NOT completed yet
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Development;
        state.iteration = 0;
        state.development_agent_invoked_iteration = None; // Dev agent not invoked yet

        // When: Determining next effect
        let effect = determine_next_effect(&state);

        // Then: Should NOT be InvokeAnalysisAgent
        assert!(
            !matches!(effect, Effect::InvokeAnalysisAgent { .. }),
            "Analysis should not run before dev agent completes, got {:?}",
            effect
        );
    });
}

/// Test that analysis agent does NOT run twice for the same iteration.
///
/// Verifies idempotency: once analysis runs for an iteration, it doesn't run again.
#[test]
fn test_analysis_does_not_run_twice_for_same_iteration() {
    with_default_timeout(|| {
        // Given: Pipeline where both dev and analysis agents have completed for iteration 0
        let mut state = PipelineState::initial(2, 0);
        state.phase = PipelinePhase::Development;
        state.iteration = 0;
        state.development_agent_invoked_iteration = Some(0);
        state.analysis_agent_invoked_iteration = Some(0); // Analysis already ran

        // When: Determining next effect
        let effect = determine_next_effect(&state);

        // Then: Should NOT be InvokeAnalysisAgent (should move to ExtractDevelopmentXml)
        assert!(
            !matches!(effect, Effect::InvokeAnalysisAgent { .. }),
            "Analysis should not run twice for iteration 0, got {:?}",
            effect
        );
    });
}

/// Test that AnalysisAgentInvoked event updates state correctly.
///
/// Verifies that the reducer properly records when analysis agent is invoked.
#[test]
fn test_analysis_agent_invoked_event_updates_state() {
    with_default_timeout(|| {
        // Given: State where analysis should be recorded for iteration 1
        let mut state = PipelineState::initial(3, 0);
        state.phase = PipelinePhase::Development;
        state.iteration = 1;
        state.development_agent_invoked_iteration = Some(1);

        // When: Processing AnalysisAgentInvoked event
        let event =
            PipelineEvent::Development(DevelopmentEvent::AnalysisAgentInvoked { iteration: 1 });
        let new_state = reduce(state, event);

        // Then: State should record that analysis was invoked for iteration 1
        assert_eq!(
            new_state.analysis_agent_invoked_iteration,
            Some(1),
            "State should record analysis agent invocation for iteration 1"
        );
    });
}

/// Test that analysis does NOT increment the iteration counter.
///
/// CRITICAL: This verifies the core constraint that -D N means exactly N
/// planning cycles, regardless of analysis or continuation.
///
/// Only the commit phase (via compute_post_commit_transition) should
/// increment the iteration counter. Analysis is verification only, NOT
/// a development iteration.
#[test]
fn test_analysis_does_not_increment_iteration_counter() {
    with_default_timeout(|| {
        // Given: State at iteration 1 before analysis
        let mut state = PipelineState::initial(3, 0);
        state.phase = PipelinePhase::Development;
        state.iteration = 1;
        state.development_agent_invoked_iteration = Some(1);

        // When: Processing AnalysisAgentInvoked event
        let event =
            PipelineEvent::Development(DevelopmentEvent::AnalysisAgentInvoked { iteration: 1 });
        let new_state = reduce(state, event);

        // Then: Iteration counter should remain unchanged
        assert_eq!(
            new_state.iteration, 1,
            "Analysis must NOT increment iteration counter"
        );

        // And: Only analysis_agent_invoked_iteration should be updated
        assert_eq!(
            new_state.analysis_agent_invoked_iteration,
            Some(1),
            "Should record analysis invocation"
        );
    });
}
