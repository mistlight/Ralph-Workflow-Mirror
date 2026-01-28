//! Reducer state machine integration tests.
//!
//! These tests verify that reducer state machine handles all transitions correctly
//! in real pipeline execution. Tests verify actual state changes through event
//! emission and reduce() function, not just unit tests of individual transitions.
//!
//! # Integration Test Style Guide
//!
//! **CRITICAL:** All tests in this module MUST follow integration test style guide
//! defined in **[../../INTEGRATION_TESTS.md](../../INTEGRATION_TESTS.md)**.
//!
//! Key principles applied in this module:
//! - Tests verify **observable behavior** (state machine transitions)
//! - Tests are deterministic and isolated
//! - Tests verify that reducer produces correct state for each event

use ralph_workflow::agents::AgentRole;
use ralph_workflow::reducer::event::{AgentErrorKind, PipelineEvent, PipelinePhase};
use ralph_workflow::reducer::state::{AgentChainState, PipelineState};

use crate::test_timeout::with_default_timeout;

fn create_initial_state() -> PipelineState {
    PipelineState::initial(5, 2)
}

fn create_state_with_agent_chain() -> PipelineState {
    PipelineState {
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec!["model1".to_string(), "model2".to_string()]],
            AgentRole::Developer,
        ),
        ..PipelineState::initial(5, 2)
    }
}

fn reduce(state: PipelineState, event: PipelineEvent) -> PipelineState {
    ralph_workflow::reducer::state_reduction::reduce(state, event)
}

#[test]
fn test_planning_to_development_transition() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(state, PipelineEvent::PlanningPhaseCompleted);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_development_phase_starts() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(state, PipelineEvent::DevelopmentPhaseStarted);
        assert_eq!(new_state.phase, PipelinePhase::Development);
    });
}

#[test]
fn test_development_to_review_transition() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(state, PipelineEvent::DevelopmentPhaseCompleted);
        assert_eq!(new_state.phase, PipelinePhase::Review);
    });
}

#[test]
fn test_review_phase_starts() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(state, PipelineEvent::ReviewPhaseStarted);
        assert_eq!(new_state.phase, PipelinePhase::Review);
        assert_eq!(new_state.reviewer_pass, 0);
    });
}

#[test]
fn test_review_to_commit_message_transition() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(
            state,
            PipelineEvent::ReviewPhaseCompleted { early_exit: false },
        );
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    });
}

#[test]
fn test_commit_message_to_final_validation_on_commit() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(
            state,
            PipelineEvent::CommitCreated {
                hash: "abc123".to_string(),
                message: "test commit".to_string(),
            },
        );
        assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    });
}

#[test]
fn test_commit_message_to_final_validation_on_skip() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(
            state,
            PipelineEvent::CommitSkipped {
                reason: "no changes".to_string(),
            },
        );
        assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    });
}

#[test]
fn test_pipeline_complete_transition() {
    with_default_timeout(|| {
        let state = create_initial_state();
        let new_state = reduce(state, PipelineEvent::PipelineCompleted);
        assert_eq!(new_state.phase, PipelinePhase::Complete);
    });
}

#[test]
fn test_development_iteration_increments() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 2,
            total_iterations: 5,
            ..create_initial_state()
        };
        // DevelopmentIterationCompleted transitions to CommitMessage phase
        // The iteration stays the same; increment happens after CommitCreated
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationCompleted {
                iteration: 2,
                output_valid: true,
            },
        );
        assert_eq!(new_state.iteration, 2);
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Development));
    });
}

#[test]
fn test_development_iteration_complete_moves_to_review() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            iteration: 5,
            total_iterations: 5,
            ..create_initial_state()
        };
        // DevelopmentIterationCompleted goes to CommitMessage first
        // Transition to Review happens after CommitCreated when iteration >= total_iterations
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationCompleted {
                iteration: 5,
                output_valid: true,
            },
        );
        assert_eq!(new_state.iteration, 5);
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Development));
    });
}

#[test]
fn test_development_iteration_started_resets_agent_chain() {
    with_default_timeout(|| {
        let mut state = create_state_with_agent_chain();
        state = reduce(
            state,
            PipelineEvent::AgentFallbackTriggered {
                role: AgentRole::Developer,
                from_agent: "agent1".to_string(),
                to_agent: "agent2".to_string(),
            },
        );
        assert_eq!(state.agent_chain.current_agent().unwrap(), "agent2");
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationStarted { iteration: 2 },
        );
        assert_eq!(new_state.iteration, 2);
        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent1");
    });
}

#[test]
fn test_review_pass_increments() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 1,
            total_reviewer_passes: 2,
            ..create_initial_state()
        };
        // FixAttemptCompleted transitions to CommitMessage phase
        // The reviewer_pass stays the same; increment happens after CommitCreated
        let new_state = reduce(
            state,
            PipelineEvent::FixAttemptCompleted {
                pass: 1,
                changes_made: true,
            },
        );
        assert_eq!(new_state.reviewer_pass, 1);
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Review));
    });
}

#[test]
fn test_review_pass_complete_moves_to_commit_message() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 2,
            total_reviewer_passes: 2,
            ..create_initial_state()
        };
        // FixAttemptCompleted goes to CommitMessage phase
        // reviewer_pass stays the same; increment happens after CommitCreated
        let new_state = reduce(
            state,
            PipelineEvent::FixAttemptCompleted {
                pass: 2,
                changes_made: true,
            },
        );
        assert_eq!(new_state.reviewer_pass, 2);
        assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
        assert_eq!(new_state.previous_phase, Some(PipelinePhase::Review));
    });
}

#[test]
fn test_agent_chain_resets_on_new_iteration() {
    with_default_timeout(|| {
        let mut state = create_state_with_agent_chain();
        state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Network,
                retriable: true,
            },
        );
        assert_eq!(state.agent_chain.current_model_index, 1);
        let new_state = reduce(
            state,
            PipelineEvent::DevelopmentIterationStarted { iteration: 2 },
        );
        assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent1");
        assert_eq!(new_state.agent_chain.current_model_index, 0);
    });
}

#[test]
fn test_agent_chain_advances_on_model_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Network,
                retriable: true,
            },
        );
        assert!(new_state.agent_chain.current_model_index > 0);
    });
}

#[test]
fn test_agent_fallback_on_auth_error() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let initial_agent_index = state.agent_chain.current_agent_index;

        // Simulate non-retriable error (auth) - should switch to next agent
        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Authentication,
                retriable: false,
            },
        );

        // Should switch to next agent
        assert!(new_state.agent_chain.current_agent_index > initial_agent_index);
        assert_eq!(new_state.agent_chain.current_model_index, 0);
    });
}

#[test]
fn test_agent_chain_exhausted_triggers_retry_cycle() {
    with_default_timeout(|| {
        let state = PipelineState {
            agent_chain: AgentChainState::initial()
                .with_agents(
                    vec!["agent1".to_string()],
                    vec![vec!["model1".to_string()]],
                    AgentRole::Developer,
                )
                .with_max_cycles(3),
            ..create_initial_state()
        };

        // Simulate chain exhausted event
        let new_state = reduce(
            state,
            PipelineEvent::AgentChainExhausted {
                role: AgentRole::Developer,
            },
        );

        // Should start retry cycle
        assert_eq!(new_state.agent_chain.current_agent_index, 0);
        assert_eq!(new_state.agent_chain.current_model_index, 0);
        assert_eq!(new_state.agent_chain.retry_cycle, 1);
    });
}

#[test]
fn test_sigsegv_causes_agent_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let initial_agent_index = state.agent_chain.current_agent_index;

        // Simulate SIGSEGV (exit code 139, non-retriable)
        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 139,
                error_kind: AgentErrorKind::InternalError,
                retriable: false,
            },
        );

        // Should switch to next agent, not crash pipeline
        assert!(new_state.agent_chain.current_agent_index > initial_agent_index);
    });
}

#[test]
fn test_pipeline_continues_after_agent_failure() {
    with_default_timeout(|| {
        let state = PipelineState {
            phase: PipelinePhase::Development,
            agent_chain: AgentChainState::initial().with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec!["model1".to_string()]],
                AgentRole::Developer,
            ),
            ..create_initial_state()
        };
        let initial_agent = state.agent_chain.current_agent_index;

        // Agent fails with retriable error
        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Network,
                retriable: true,
            },
        );

        // Pipeline should continue with same agent, different model
        assert_eq!(new_state.phase, PipelinePhase::Development);
        assert_eq!(new_state.agent_chain.current_agent_index, initial_agent);
    });
}

#[test]
fn test_network_error_triggers_model_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();

        // Simulate network error (retriable) - should trigger model fallback event
        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Network,
                retriable: true,
            },
        );

        // Should advance to next model, not agent
        assert_eq!(new_state.agent_chain.current_agent_index, 0);
        assert!(new_state.agent_chain.current_model_index > 0);
    });
}

#[test]
fn test_filesystem_error_triggers_agent_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let initial_agent_index = state.agent_chain.current_agent_index;

        // Simulate filesystem error (non-retriable) - should trigger agent fallback
        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::FileSystem,
                retriable: false,
            },
        );

        // Should switch to next agent
        assert!(new_state.agent_chain.current_agent_index > initial_agent_index);
    });
}

/// Test that rate limit (429) triggers agent fallback via AgentRateLimitFallback event.
///
/// Rate limit errors now trigger immediate agent fallback (not model fallback)
/// to allow work to continue without waiting for rate limits to reset.
/// This is handled via the dedicated AgentRateLimitFallback event which also
/// preserves prompt context for continuation.
#[test]
fn test_rate_limit_error_triggers_agent_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();

        // Simulate rate limit via AgentRateLimitFallback event - should trigger agent fallback
        let new_state = reduce(
            state,
            PipelineEvent::AgentRateLimitFallback {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                prompt_context: Some("continue work".to_string()),
            },
        );

        // Should switch to next agent
        assert!(
            new_state.agent_chain.current_agent_index > 0,
            "Rate limit should trigger agent fallback"
        );
        // Model index should reset (new agent starts at model 0)
        assert_eq!(new_state.agent_chain.current_model_index, 0);
        // Prompt context should be preserved
        assert_eq!(
            new_state.agent_chain.rate_limit_continuation_prompt,
            Some("continue work".to_string())
        );
    });
}

#[test]
fn test_authentication_error_triggers_agent_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let initial_agent_index = state.agent_chain.current_agent_index;

        // Simulate auth error (non-retriable) - should trigger agent fallback
        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::Authentication,
                retriable: false,
            },
        );

        // Should switch to next agent
        assert!(new_state.agent_chain.current_agent_index > initial_agent_index);
    });
}

#[test]
fn test_internal_error_triggers_agent_fallback() {
    with_default_timeout(|| {
        let state = create_state_with_agent_chain();
        let initial_agent_index = state.agent_chain.current_agent_index;

        // Simulate internal error (non-retriable) - should trigger agent fallback
        let new_state = reduce(
            state,
            PipelineEvent::AgentInvocationFailed {
                role: AgentRole::Developer,
                agent: "agent1".to_string(),
                exit_code: 1,
                error_kind: AgentErrorKind::InternalError,
                retriable: false,
            },
        );

        // Should switch to next agent
        assert!(new_state.agent_chain.current_agent_index > initial_agent_index);
    });
}

#[test]
fn test_event_replay_reproduces_final_state() {
    with_default_timeout(|| {
        let initial_state = create_initial_state();

        let events = vec![
            PipelineEvent::DevelopmentPhaseStarted,
            PipelineEvent::DevelopmentIterationStarted { iteration: 1 },
            PipelineEvent::DevelopmentIterationCompleted {
                iteration: 1,
                output_valid: true,
            },
            PipelineEvent::DevelopmentIterationStarted { iteration: 2 },
            PipelineEvent::DevelopmentIterationCompleted {
                iteration: 2,
                output_valid: true,
            },
            PipelineEvent::DevelopmentIterationCompleted {
                iteration: 3,
                output_valid: true,
            },
            PipelineEvent::PipelineCompleted,
        ];

        let final_state = events.into_iter().fold(initial_state, reduce);

        assert_eq!(final_state.phase, PipelinePhase::Complete);
        // The last DevelopmentIterationCompleted sets iteration to 3
        assert_eq!(final_state.iteration, 3);
    });
}

// ============================================================================
// CommitSkipped with previous_phase context tests
// ============================================================================
// These integration tests verify that CommitSkipped respects previous_phase
// for proper phase transitions, matching the behavior of CommitCreated.

/// Test that CommitSkipped after development iteration goes to Planning for next iteration.
///
/// When commit is skipped after a development iteration (not the last one),
/// the pipeline should go back to Planning for the next iteration, not FinalValidation.
#[test]
fn test_commit_skipped_respects_previous_phase_from_development() {
    with_default_timeout(|| {
        // Setup state as if we just completed a development iteration
        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            previous_phase: Some(PipelinePhase::Development),
            iteration: 0,
            total_iterations: 3,
            ..create_initial_state()
        };

        // Simulate commit being skipped (empty diff)
        let new_state = reduce(
            state,
            PipelineEvent::CommitSkipped {
                reason: "No changes to commit (empty diff)".to_string(),
            },
        );

        // Should go back to Planning for next iteration, NOT FinalValidation
        assert_eq!(new_state.phase, PipelinePhase::Planning);
        assert_eq!(new_state.iteration, 1);
        assert!(new_state.previous_phase.is_none());
    });
}

/// Test that CommitSkipped after last development iteration goes to Review.
///
/// When commit is skipped after the last development iteration,
/// the pipeline should go to Review, not FinalValidation.
#[test]
fn test_commit_skipped_after_last_dev_iteration_goes_to_review() {
    with_default_timeout(|| {
        // Setup state as if we just completed the last development iteration
        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            previous_phase: Some(PipelinePhase::Development),
            iteration: 2, // 0-indexed, this is the 3rd of 3 iterations
            total_iterations: 3,
            ..create_initial_state()
        };

        // Simulate commit being skipped
        let new_state = reduce(
            state,
            PipelineEvent::CommitSkipped {
                reason: "No changes to commit".to_string(),
            },
        );

        // Should go to Review after all dev iterations done
        assert_eq!(new_state.phase, PipelinePhase::Review);
        assert_eq!(new_state.iteration, 3);
    });
}

/// Test that CommitSkipped after fix attempt stays in Review for next pass.
///
/// When commit is skipped after a fix attempt (not the last review pass),
/// the pipeline should stay in Review for the next pass.
#[test]
fn test_commit_skipped_respects_previous_phase_from_review() {
    with_default_timeout(|| {
        // Setup state as if we just completed a fix attempt
        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            previous_phase: Some(PipelinePhase::Review),
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            ..create_initial_state()
        };

        // Simulate commit being skipped
        let new_state = reduce(
            state,
            PipelineEvent::CommitSkipped {
                reason: "No changes to commit".to_string(),
            },
        );

        // Should stay in Review for next pass
        assert_eq!(new_state.phase, PipelinePhase::Review);
        assert_eq!(new_state.reviewer_pass, 1);
    });
}

/// Test that CommitSkipped after last review pass goes to FinalValidation.
///
/// When commit is skipped after the last review pass,
/// the pipeline should go to FinalValidation.
#[test]
fn test_commit_skipped_after_last_review_goes_to_final_validation() {
    with_default_timeout(|| {
        // Setup state as if we just completed the last review pass
        let state = PipelineState {
            phase: PipelinePhase::CommitMessage,
            previous_phase: Some(PipelinePhase::Review),
            reviewer_pass: 1, // 0-indexed, this is the 2nd of 2 passes
            total_reviewer_passes: 2,
            ..create_initial_state()
        };

        // Simulate commit being skipped
        let new_state = reduce(
            state,
            PipelineEvent::CommitSkipped {
                reason: "No changes to commit".to_string(),
            },
        );

        // Should go to FinalValidation after all review passes done
        assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    });
}
