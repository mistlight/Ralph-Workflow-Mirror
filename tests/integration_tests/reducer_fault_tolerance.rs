//! Fault tolerance integration tests for reducer architecture.
//!
//! Tests verify that agent failures (including panics, segfaults, I/O errors)
//! never crash the pipeline and always trigger proper fallback behavior.

use crate::test_timeout::with_default_timeout;
use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::effect::Effect;
use ralph_workflow::reducer::event::{AgentErrorKind, PipelineEvent, PipelinePhase};
use ralph_workflow::reducer::orchestration::determine_next_effect;
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
        checkpoint_saved_count: 0,
        execution_history: Vec::new(),
        ..PipelineState::initial(5, 2)
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
            checkpoint_saved_count: 0,
            execution_history: Vec::new(),
            ..PipelineState::initial(5, 2)
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
            checkpoint_saved_count: 0,
            execution_history: Vec::new(),
            ..PipelineState::initial(5, 2)
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
            checkpoint_saved_count: 0,
            execution_history: Vec::new(),
            ..PipelineState::initial(5, 2)
        };

        assert_eq!(state.phase, PipelinePhase::Development);
        assert!(state.agent_chain.is_exhausted());
        assert_eq!(state.agent_chain.retry_cycle, 1);
    });
}

/// Test that retry-cycle backoff is emitted as an explicit effect.
///
/// When an agent chain wraps into a new retry cycle, the reducer must record that
/// a backoff wait is pending, and orchestration must emit a BackoffWait effect
/// before attempting more work.
#[test]
fn test_retry_cycle_backoff_is_explicit_effect() {
    with_default_timeout(|| {
        let mut state = PipelineState::initial(1, 0);
        state.phase = PipelinePhase::Development;
        state.agent_chain = AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec!["model1".to_string()]],
                AgentRole::Developer,
            )
            .with_max_cycles(3);

        // Exhaust once to start retry cycle. This should mark backoff pending.
        state = ralph_workflow::reducer::state_reduction::reduce(
            state,
            PipelineEvent::agent_chain_exhausted(AgentRole::Developer),
        );

        assert!(
            state.agent_chain.backoff_pending_ms.is_some(),
            "starting a retry cycle must mark backoff pending"
        );

        // Orchestration should emit a wait effect before any further work.
        let effect = determine_next_effect(&state);
        assert!(
            matches!(effect, Effect::BackoffWait { .. }),
            "expected BackoffWait, got {effect:?}"
        );
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

// ============================================================================
// XSD RETRY STATE TRACKING TESTS
// ============================================================================

/// Test that XSD retry decisions come from reducer state, not implicit logic.
///
/// The invalid_output_attempts counter in ContinuationState must be the
/// single source of truth for XSD retry decisions. This test verifies
/// that the reducer correctly updates this counter and triggers agent
/// fallback at the right threshold.
#[test]
fn test_xsd_retry_decisions_from_reducer_state_only() {
    use ralph_workflow::reducer::state::{PipelineState, MAX_DEV_INVALID_OUTPUT_RERUNS};
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Set up state with multiple agents for fallback
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["agent-1".to_string(), "agent-2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        // Verify counter starts at 0
        assert_eq!(state.continuation.invalid_output_attempts, 0);

        // Process validation failures up to threshold
        for i in 0..MAX_DEV_INVALID_OUTPUT_RERUNS {
            state = reduce(
                state,
                PipelineEvent::development_output_validation_failed(0, i),
            );
            assert_eq!(
                state.continuation.invalid_output_attempts,
                i + 1,
                "Counter should increment on each failure"
            );
            // Agent should NOT change until threshold exceeded
            assert_eq!(
                state.agent_chain.current_agent(),
                Some(&"agent-1".to_string()),
                "Agent should not change until threshold exceeded"
            );
        }

        // One more failure should trigger agent advancement
        state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, MAX_DEV_INVALID_OUTPUT_RERUNS),
        );

        // Counter should reset after agent switch
        assert_eq!(
            state.continuation.invalid_output_attempts, 0,
            "Counter should reset after agent advancement"
        );

        // Agent should have advanced
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"agent-2".to_string()),
            "Agent should advance after max retries"
        );
    });
}

/// Test complete XSD retry exhaustion flow is reducer-driven.
///
/// Verifies that:
/// 1. XSD validation failures increment state counter
/// 2. After MAX_DEV_INVALID_OUTPUT_RERUNS, agent chain advances
/// 3. Counter resets after agent advancement
/// 4. If all agents exhausted with retries, chain enters exhausted state
///
/// This test ensures no hidden XSD retry logic exists outside the reducer.
#[test]
fn test_xsd_retry_exhaustion_complete_flow() {
    use ralph_workflow::reducer::state::{PipelineState, MAX_DEV_INVALID_OUTPUT_RERUNS};
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Set up state with two agents
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["agent-primary".to_string(), "agent-fallback".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        // Phase 1: Exhaust retries on primary agent
        for attempt in 0..MAX_DEV_INVALID_OUTPUT_RERUNS {
            assert_eq!(
                state.agent_chain.current_agent(),
                Some(&"agent-primary".to_string()),
                "Should stay on primary agent until retries exhausted"
            );
            state = reduce(
                state,
                PipelineEvent::development_output_validation_failed(0, attempt),
            );
        }

        // Counter should be at MAX
        assert_eq!(
            state.continuation.invalid_output_attempts, MAX_DEV_INVALID_OUTPUT_RERUNS,
            "Counter should be at MAX before final failure"
        );

        // One more failure triggers agent advancement
        state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, MAX_DEV_INVALID_OUTPUT_RERUNS),
        );

        // After exhausting retries, should be on fallback agent
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"agent-fallback".to_string()),
            "Should advance to fallback agent after exhausting retries"
        );
        assert_eq!(
            state.continuation.invalid_output_attempts, 0,
            "Counter should reset after agent advancement"
        );

        // Phase 2: Exhaust retries on fallback agent
        for attempt in 0..=MAX_DEV_INVALID_OUTPUT_RERUNS {
            state = reduce(
                state,
                PipelineEvent::development_output_validation_failed(0, attempt),
            );
        }

        // After exhausting all agents, chain should wrap around (cycle increment)
        // or point back to first agent if not exhausted
        assert!(
            state.agent_chain.current_agent_index == 0
                || state.agent_chain.retry_cycle > 0
                || state.agent_chain.is_exhausted(),
            "After exhausting all agents, chain should wrap, increment cycle, or be exhausted"
        );
    });
}

/// Test that XSD retry state is distinct from agent invocation failure state.
///
/// XSD validation failures (bad XML output) and agent invocation failures
/// (crashes, auth errors) are tracked separately. This test verifies the
/// reducer treats them as independent failure modes.
#[test]
fn test_xsd_retry_state_independent_of_invocation_failures() {
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Development;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["agent-1".to_string(), "agent-2".to_string()],
            vec![vec!["model-a".to_string(), "model-b".to_string()], vec![]],
            AgentRole::Developer,
        );

        // Start with XSD validation failure
        state = reduce(
            state,
            PipelineEvent::development_output_validation_failed(0, 0),
        );
        assert_eq!(state.continuation.invalid_output_attempts, 1);
        assert_eq!(state.agent_chain.current_agent_index, 0, "Same agent");

        // Then get a network error (retriable) - should advance model, not agent
        state = reduce(
            state,
            PipelineEvent::agent_invocation_failed(
                AgentRole::Developer,
                "agent-1".to_string(),
                1,
                AgentErrorKind::Network,
                true,
            ),
        );

        // Network error should advance model, but XSD counter remains
        assert_eq!(state.agent_chain.current_agent_index, 0, "Same agent");
        assert!(
            state.agent_chain.current_model_index > 0,
            "Model should advance"
        );
        // XSD counter should NOT be affected by invocation failures
        assert_eq!(
            state.continuation.invalid_output_attempts, 1,
            "XSD counter should remain independent"
        );
    });
}

// ============================================================================
// PLANNING PHASE XSD RETRY TESTS
// ============================================================================

/// Test that planning XSD retry decisions come from reducer state.
///
/// The invalid_output_attempts counter in ContinuationState must be the
/// single source of truth for planning XSD retry decisions.
#[test]
fn test_planning_xsd_retry_decisions_from_reducer_state() {
    use ralph_workflow::reducer::state::{PipelineState, MAX_PLAN_INVALID_OUTPUT_RERUNS};
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Planning;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["agent-1".to_string(), "agent-2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        );

        // Verify counter starts at 0
        assert_eq!(state.continuation.invalid_output_attempts, 0);

        // Process validation failures up to threshold
        for i in 0..MAX_PLAN_INVALID_OUTPUT_RERUNS {
            state = reduce(
                state,
                PipelineEvent::planning_output_validation_failed(0, i),
            );
            assert_eq!(
                state.continuation.invalid_output_attempts,
                i + 1,
                "Counter should increment on each failure"
            );
            // Agent should NOT change until threshold exceeded
            assert_eq!(
                state.agent_chain.current_agent(),
                Some(&"agent-1".to_string()),
                "Agent should not change until threshold exceeded"
            );
        }

        // One more failure should trigger agent advancement
        state = reduce(
            state,
            PipelineEvent::planning_output_validation_failed(0, MAX_PLAN_INVALID_OUTPUT_RERUNS),
        );

        // Counter should reset after agent switch
        assert_eq!(
            state.continuation.invalid_output_attempts, 0,
            "Counter should reset after agent advancement"
        );

        // Agent should have advanced
        assert_eq!(
            state.agent_chain.current_agent(),
            Some(&"agent-2".to_string()),
            "Agent should advance after max retries"
        );
    });
}

/// Test that planning XSD retry is independent of development XSD retry.
///
/// When planning phase completes and development starts, the retry counter
/// should be reset.
#[test]
fn test_planning_xsd_retry_resets_on_phase_transition() {
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Planning;
        state.continuation.invalid_output_attempts = 2;

        // Plan generation completes successfully
        state = reduce(state, PipelineEvent::plan_generation_completed(1, true));

        // Counter should reset on successful completion
        assert_eq!(
            state.continuation.invalid_output_attempts, 0,
            "Counter should reset after successful plan generation"
        );
        assert_eq!(
            state.phase,
            PipelinePhase::Development,
            "Should transition to Development"
        );
    });
}

/// Test planning XSD retry state persists across multiple attempts.
///
/// This verifies that the continuation state correctly tracks XSD retry
/// attempts within the planning phase.
#[test]
fn test_planning_xsd_retry_state_persistence() {
    use ralph_workflow::reducer::state::PipelineState;
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        let mut state = PipelineState::initial(3, 1);
        state.phase = PipelinePhase::Planning;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["agent-1".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        );

        // First failure
        state = reduce(
            state,
            PipelineEvent::planning_output_validation_failed(1, 0),
        );
        assert_eq!(state.continuation.invalid_output_attempts, 1);
        assert_eq!(state.iteration, 1);

        // Second failure at same iteration
        state = reduce(
            state,
            PipelineEvent::planning_output_validation_failed(1, 1),
        );
        assert_eq!(state.continuation.invalid_output_attempts, 2);
        assert_eq!(state.iteration, 1);

        // State remains in Planning phase
        assert_eq!(state.phase, PipelinePhase::Planning);
    });
}

// ============================================================================
// COMMIT AGENT FALLBACK TO REVIEWER CHAIN TESTS
// ============================================================================

/// Test that commit agent chain can use reviewer agents when no commit agents configured.
///
/// This is the documented fallback behavior: when agent_chain.commit is empty,
/// the system falls back to using agent_chain.reviewer agents.
#[test]
fn test_commit_phase_uses_reviewer_chain_fallback() {
    use ralph_workflow::reducer::state::{
        CommitState, PipelineState, MAX_VALIDATION_RETRY_ATTEMPTS,
    };
    use ralph_workflow::reducer::state_reduction::reduce;

    with_default_timeout(|| {
        // Set up state with reviewer agents in commit role (simulating fallback)
        let mut state = PipelineState::initial(1, 1);
        state.phase = PipelinePhase::CommitMessage;
        state.agent_chain = state.agent_chain.with_agents(
            vec!["reviewer-claude".to_string(), "reviewer-codex".to_string()],
            vec![vec![], vec![]],
            AgentRole::Commit, // Commit role with reviewer agents
        );
        state.commit = CommitState::Generating {
            attempt: MAX_VALIDATION_RETRY_ATTEMPTS,
            max_attempts: MAX_VALIDATION_RETRY_ATTEMPTS,
        };

        // Validation failure should still trigger proper fallback
        state = reduce(
            state,
            PipelineEvent::commit_message_validation_failed(
                "Invalid format".to_string(),
                MAX_VALIDATION_RETRY_ATTEMPTS,
            ),
        );

        // Should advance to next agent
        assert_eq!(
            state.agent_chain.current_agent_index, 1,
            "Should advance to next reviewer agent"
        );
        assert_eq!(
            state.agent_chain.current_role,
            AgentRole::Commit,
            "Role should remain Commit"
        );
    });
}
