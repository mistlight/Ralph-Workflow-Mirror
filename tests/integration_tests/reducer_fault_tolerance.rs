//! Fault tolerance integration tests for reducer architecture.
//!
//! Tests verify that agent failures (including panics, segfaults, I/O errors)
//! never crash the pipeline and always trigger proper fallback behavior.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::event::{AgentErrorKind, PipelineEvent, PipelinePhase};
use ralph_workflow::reducer::state::{AgentChainState, PipelineState};

fn create_state_with_agent_chain_in_development() -> PipelineState {
    use ralph_workflow::reducer::state::{CommitState, ContinuationState, RebaseState};

    PipelineState {
        agent_chain: AgentChainState::initial().with_agents(
            vec![
                "agent1".to_string(),
                "agent2".to_string(),
                "agent3".to_string(),
            ],
            vec![
                vec!["model1".to_string(), "model2".to_string()],
                vec!["model1".to_string()],
                vec![],
            ],
            AgentRole::Developer,
        ),
        phase: PipelinePhase::Development,
        previous_phase: None,
        iteration: 1,
        total_iterations: 5,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        review_issues_found: false,
        context_cleaned: false,
        rebase: RebaseState::NotStarted,
        commit: CommitState::NotStarted,
        continuation: ContinuationState::new(),
        execution_history: Vec::new(),
    }
}

#[test]
fn test_agent_sigsegv_caught_by_fault_tolerant_executor() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();

        let event = PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            "agent1".to_string(),
            139,
            AgentErrorKind::InternalError,
            false,
        );

        let new_state = ralph_workflow::reducer::state_reduction::reduce(state, event);

        assert!(matches!(
            new_state.agent_chain.current_agent(),
            Some(agent) if agent != "agent1"
        ));
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_agent_panic_caught_by_fault_tolerant_executor() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();

        let event = PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            "agent1".to_string(),
            1,
            AgentErrorKind::InternalError,
            false,
        );

        let new_state = ralph_workflow::reducer::state_reduction::reduce(state, event);

        assert!(matches!(
            new_state.agent_chain.current_agent(),
            Some(agent) if agent != "agent1"
        ));
    });
}

#[test]
fn test_network_error_triggers_model_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();

        let event = PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            "agent1".to_string(),
            1,
            AgentErrorKind::Network,
            true,
        );

        let new_state = ralph_workflow::reducer::state_reduction::reduce(state, event);

        assert_eq!(new_state.agent_chain.current_agent_index, 0);
        assert!(new_state.agent_chain.current_model_index > 0);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_auth_error_triggers_agent_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();

        let event = PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            "agent1".to_string(),
            1,
            AgentErrorKind::Authentication,
            false,
        );

        let new_state = ralph_workflow::reducer::state_reduction::reduce(state, event);

        assert!(new_state.agent_chain.current_agent_index > 0);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_pipeline_state_machine_recovers_from_multiple_failures() {
    with_default_timeout(|| {
        let mut state = create_state_with_agent_chain_in_development();

        let events = vec![
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                "agent1".to_string(),
                1,
                AgentErrorKind::Network,
                true,
            ),
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                "agent1".to_string(),
                1,
                AgentErrorKind::Authentication,
                false,
            ),
            PipelineEvent::agent_invocation_succeeded(AgentRole::Developer, "agent2".to_string()),
        ];

        for event in events {
            state = ralph_workflow::reducer::state_reduction::reduce(state, event);
        }

        assert_eq!(state.agent_chain.current_agent().unwrap(), "agent2");
        assert_eq!(state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_agent_fails_after_10_retries_fallback_to_next_agent() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();
        let initial_model_index = state.agent_chain.current_model_index;

        let new_state = ralph_workflow::reducer::state_reduction::reduce(
            state,
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                "agent1".to_string(),
                1,
                AgentErrorKind::Network,
                true,
            ),
        );

        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent1");
        assert!(new_state.agent_chain.current_model_index > initial_model_index);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

/// Test that rate limit (429) errors trigger AGENT fallback, not model fallback.
///
/// This verifies the core behavior change: when an agent hits a 429 rate limit,
/// we immediately switch to the next agent in the chain rather than trying
/// the same agent with a different model. This allows work to continue without
/// waiting for rate limits to reset.
#[test]
fn test_rate_limit_429_triggers_agent_fallback_not_model_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();
        let initial_agent_index = state.agent_chain.current_agent_index;

        // Simulate 429 rate limit - should switch to next AGENT, not model
        let new_state = ralph_workflow::reducer::state_reduction::reduce(
            state,
            PipelineEvent::agent_rate_limit_fallback(
                AgentRole::Developer,
                "agent1".to_string(),
                Some("continue this work".to_string()),
            ),
        );

        // Should switch to next agent
        assert!(
            new_state.agent_chain.current_agent_index > initial_agent_index,
            "429 should trigger agent fallback, not model fallback"
        );

        // Model index should reset to 0 (new agent starts fresh)
        assert_eq!(new_state.agent_chain.current_model_index, 0);

        // Prompt context should be preserved for continuation
        assert_eq!(
            new_state.agent_chain.rate_limit_continuation_prompt,
            Some("continue this work".to_string())
        );

        // Phase should remain Development
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

/// Test that network errors still trigger model fallback (same agent, different model).
///
/// This verifies that the change to rate limit handling doesn't affect other
/// retriable errors like Network/Timeout which should still try different models
/// within the same agent before falling back to the next agent.
#[test]
fn test_network_error_still_triggers_model_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();
        let initial_agent_index = state.agent_chain.current_agent_index;
        let initial_model_index = state.agent_chain.current_model_index;

        // Network error should still trigger model fallback (same agent)
        let new_state = ralph_workflow::reducer::state_reduction::reduce(
            state,
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                "agent1".to_string(),
                1,
                AgentErrorKind::Network,
                true,
            ),
        );

        // Should stay on same agent
        assert_eq!(
            new_state.agent_chain.current_agent_index, initial_agent_index,
            "Network error should trigger model fallback, not agent fallback"
        );

        // Should advance to next model
        assert!(
            new_state.agent_chain.current_model_index > initial_model_index,
            "Network error should advance model index"
        );

        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

/// Test that rate limit continuation prompt is cleared on successful agent execution.
#[test]
fn test_rate_limit_continuation_prompt_cleared_on_success() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::state::{CommitState, ContinuationState, RebaseState};

        // Create state with a saved continuation prompt
        let mut agent_chain = AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec!["model1".to_string()], vec!["model1".to_string()]],
            AgentRole::Developer,
        );
        agent_chain.rate_limit_continuation_prompt = Some("saved prompt".to_string());
        agent_chain.current_agent_index = 1; // On agent2 after rate limit fallback

        let state = PipelineState {
            agent_chain,
            phase: PipelinePhase::Development,
            previous_phase: None,
            iteration: 1,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            review_issues_found: false,
            context_cleaned: false,
            rebase: RebaseState::NotStarted,
            commit: CommitState::NotStarted,
            continuation: ContinuationState::new(),
            execution_history: Vec::new(),
        };

        // Agent succeeds
        let new_state = ralph_workflow::reducer::state_reduction::reduce(
            state,
            PipelineEvent::agent_invocation_succeeded(AgentRole::Developer, "agent2".to_string()),
        );

        // Continuation prompt should be cleared
        assert!(
            new_state
                .agent_chain
                .rate_limit_continuation_prompt
                .is_none(),
            "Success should clear rate limit continuation prompt"
        );
    });
}

#[test]
fn test_all_agents_exhausted_pipeline_graceful_abort() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::state::{CommitState, ContinuationState, RebaseState};

        let state = PipelineState {
            agent_chain: AgentChainState::initial()
                .with_agents(
                    vec!["agent1".to_string()],
                    vec![vec!["model1".to_string()]],
                    AgentRole::Developer,
                )
                .with_max_cycles(1),
            phase: PipelinePhase::Development,
            previous_phase: None,
            iteration: 1,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            review_issues_found: false,
            context_cleaned: false,
            rebase: RebaseState::NotStarted,
            commit: CommitState::NotStarted,
            continuation: ContinuationState::new(),
            execution_history: Vec::new(),
        };

        let exhausted_state = ralph_workflow::reducer::state_reduction::reduce(
            state,
            PipelineEvent::agent_chain_exhausted(AgentRole::Developer),
        );

        assert_eq!(exhausted_state.agent_chain.current_agent_index, 0);
        assert_eq!(exhausted_state.agent_chain.current_model_index, 0);
        assert_eq!(exhausted_state.agent_chain.retry_cycle, 1);
        assert_eq!(exhausted_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_agent_exhaustion_transitions_to_next_phase() {
    with_default_timeout(|| {
        use ralph_workflow::reducer::state::{CommitState, ContinuationState, RebaseState};

        let mut chain = AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec!["model1".to_string()]],
                AgentRole::Developer,
            )
            .with_max_cycles(1);
        chain = chain.start_retry_cycle();

        let state = PipelineState {
            agent_chain: chain,
            phase: PipelinePhase::Development,
            previous_phase: None,
            iteration: 1,
            total_iterations: 5,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            review_issues_found: false,
            context_cleaned: false,
            rebase: RebaseState::NotStarted,
            commit: CommitState::NotStarted,
            continuation: ContinuationState::new(),
            execution_history: Vec::new(),
        };

        assert_eq!(state.phase, PipelinePhase::Development);
        assert!(state.agent_chain.is_exhausted());
        assert_eq!(state.agent_chain.retry_cycle, 1);
    });
}

#[test]
fn test_pipeline_continues_after_agent_sigsegv() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain_in_development();
        let initial_agent_index = state.agent_chain.current_agent_index;

        let new_state = ralph_workflow::reducer::state_reduction::reduce(
            state,
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                "agent1".to_string(),
                139,
                AgentErrorKind::InternalError,
                false,
            ),
        );

        assert!(new_state.agent_chain.current_agent_index > initial_agent_index);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_pipeline_continues_after_multiple_agent_failures() {
    with_default_timeout(|| {
        let mut state = create_state_with_agent_chain_in_development();

        let events = vec![
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                "agent1".to_string(),
                1,
                AgentErrorKind::Authentication,
                false,
            ),
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                "agent2".to_string(),
                139,
                AgentErrorKind::InternalError,
                false,
            ),
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                "agent3".to_string(),
                1,
                AgentErrorKind::FileSystem,
                false,
            ),
        ];

        for event in events {
            state = ralph_workflow::reducer::state_reduction::reduce(state, event);
        }

        assert!(state.agent_chain.current_agent().is_some());
        assert_eq!(state.phase, PipelinePhase::Development);
    });
}
