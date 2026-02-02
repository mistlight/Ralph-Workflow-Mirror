use super::*;
use crate::agents::AgentRole;
use crate::reducer::event::AgentErrorKind;
use crate::reducer::event::PipelineEvent;
use crate::reducer::event::PipelinePhase;
use crate::reducer::event::RebasePhase;
use crate::reducer::state::AgentChainState;
use crate::reducer::state::CommitState;
use crate::reducer::state::ContinuationState;
use crate::reducer::state::PipelineState;
use crate::reducer::state::RebaseState;

fn create_test_state() -> PipelineState {
    PipelineState {
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec!["model1".to_string(), "model2".to_string()]],
            AgentRole::Developer,
        ),
        ..PipelineState::initial(5, 2)
    }
}

#[test]
fn test_review_phase_started_clears_agent_chain_for_reviewer_role() {
    use crate::reducer::orchestration::determine_next_effect;

    // Simulate typical state after Development where the agent chain is populated
    // for developer runs.
    let state = create_test_state();

    // Enter Review phase.
    let review_state = reduce(state, PipelineEvent::review_phase_started());

    // The reviewer phase must not reuse the developer chain.
    assert!(
        review_state.agent_chain.agents.is_empty(),
        "Review phase should clear populated agent_chain to force reviewer initialization"
    );
    assert_eq!(
        review_state.agent_chain.current_role,
        AgentRole::Reviewer,
        "Review phase should set agent_chain role to Reviewer"
    );

    // Orchestration should deterministically emit InitializeAgentChain for reviewers.
    let effect = determine_next_effect(&review_state);
    assert!(matches!(
        effect,
        crate::reducer::effect::Effect::InitializeAgentChain {
            role: AgentRole::Reviewer
        }
    ));
}

#[test]
fn test_review_phase_started_resets_continuation_state() {
    use crate::reducer::state::{ContinuationState, DevelopmentStatus};

    let state = PipelineState {
        continuation: ContinuationState {
            previous_status: Some(DevelopmentStatus::Partial),
            previous_summary: Some("prev summary".to_string()),
            previous_files_changed: Some(vec!["src/lib.rs".to_string()]),
            previous_next_steps: Some("next steps".to_string()),
            continuation_attempt: 2,
            invalid_output_attempts: 3,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let review_state = reduce(state, PipelineEvent::review_phase_started());

    assert_eq!(
        review_state.continuation,
        ContinuationState::new(),
        "Entering Review should reset continuation state to avoid cross-phase leakage"
    );
}

#[test]
fn test_review_phase_started_preserves_agent_chain_backoff_policy() {
    // Review phase resets the chain, but must preserve the configured
    // retry/backoff policy so behavior is consistent across phases.
    let mut state = create_test_state();
    state.agent_chain = state
        .agent_chain
        .with_max_cycles(7)
        .with_backoff_policy(1234, 3.5, 98765);

    let review_state = reduce(state.clone(), PipelineEvent::review_phase_started());

    assert_eq!(
        review_state.agent_chain.max_cycles,
        state.agent_chain.max_cycles
    );
    assert_eq!(
        review_state.agent_chain.retry_delay_ms,
        state.agent_chain.retry_delay_ms
    );
    assert_eq!(
        review_state.agent_chain.backoff_multiplier,
        state.agent_chain.backoff_multiplier
    );
    assert_eq!(
        review_state.agent_chain.max_backoff_ms,
        state.agent_chain.max_backoff_ms
    );
}

#[test]
fn test_reduce_pipeline_started() {
    let state = create_test_state();
    let new_state = reduce(state, PipelineEvent::pipeline_started());
    assert_eq!(new_state.phase, PipelinePhase::Planning);
}

#[test]
fn test_reduce_pipeline_completed() {
    let state = create_test_state();
    let new_state = reduce(state, PipelineEvent::pipeline_completed());
    assert_eq!(new_state.phase, PipelinePhase::Complete);
}

#[test]
fn test_reduce_development_iteration_completed() {
    // DevelopmentIterationCompleted transitions to CommitMessage phase
    // The iteration counter stays the same; it gets incremented by CommitCreated
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
    // Iteration stays at 2 (incremented by CommitCreated later)
    assert_eq!(new_state.iteration, 2);
    // Goes to CommitMessage phase to create a commit
    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    // Previous phase stored for return after commit
    assert_eq!(new_state.previous_phase, Some(PipelinePhase::Development));
}

#[test]
fn test_reduce_development_iteration_complete_goes_to_commit() {
    // Even on last iteration, DevelopmentIterationCompleted goes to CommitMessage
    // The transition to Review happens after CommitCreated
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 5,
        total_iterations: 5,
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::development_iteration_completed(5, true),
    );
    // Iteration stays at 5 (incremented by CommitCreated later)
    assert_eq!(new_state.iteration, 5);
    // Goes to CommitMessage phase first
    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
}

#[test]
fn test_plan_generation_completed_invalid_does_not_transition_to_development() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::plan_generation_completed(1, false));

    assert_eq!(
        new_state.phase,
        PipelinePhase::Planning,
        "Invalid plan should keep pipeline in Planning phase"
    );
}

#[test]
fn test_reduce_agent_fallback_to_next_model() {
    let state = create_test_state();
    let initial_agent = state.agent_chain.current_agent().unwrap().clone();
    let initial_model_index = state.agent_chain.current_model_index;

    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            initial_agent.clone(),
            1,
            AgentErrorKind::Network,
            true,
        ),
    );

    assert_ne!(
        new_state.agent_chain.current_model_index,
        initial_model_index
    );
}

#[test]
fn test_reduce_rebase_started() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::rebase_started(RebasePhase::Initial, "main".to_string()),
    );

    assert!(matches!(new_state.rebase, RebaseState::InProgress { .. }));
}

#[test]
fn test_reduce_rebase_succeeded() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::rebase_succeeded(RebasePhase::Initial, "abc123".to_string()),
    );

    assert!(matches!(new_state.rebase, RebaseState::Completed { .. }));
}

#[test]
fn test_reduce_commit_generation_started() {
    let state = PipelineState {
        commit_diff_prepared: true,
        commit_diff_empty: false,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::commit_generation_started());

    assert!(matches!(new_state.commit, CommitState::Generating { .. }));
    assert!(new_state.commit_diff_prepared);
    assert!(!new_state.commit_diff_empty);
}

#[test]
fn test_reduce_commit_diff_failed_interrupts_pipeline() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::commit_diff_failed("diff failed".to_string()),
    );

    assert_eq!(new_state.phase, PipelinePhase::Interrupted);
    assert!(!new_state.commit_diff_prepared);
    assert!(!new_state.commit_diff_empty);
}

#[test]
fn test_reduce_commit_created() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string()),
    );

    assert!(matches!(new_state.commit, CommitState::Committed { .. }));
    assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
}

#[test]
fn test_reduce_all_agent_failure_scenarios() {
    let state = create_test_state();
    let initial_agent_index = state.agent_chain.current_agent_index;
    let initial_model_index = state.agent_chain.current_model_index;
    let agent_name = state.agent_chain.current_agent().unwrap().clone();

    let network_error_state = reduce(
        state.clone(),
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            agent_name.clone(),
            1,
            AgentErrorKind::Network,
            true,
        ),
    );
    assert_eq!(
        network_error_state.agent_chain.current_agent_index,
        initial_agent_index
    );
    assert!(network_error_state.agent_chain.current_model_index > initial_model_index);

    let auth_error_state = reduce(
        state.clone(),
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            agent_name.clone(),
            1,
            AgentErrorKind::Authentication,
            false,
        ),
    );
    assert!(auth_error_state.agent_chain.current_agent_index > initial_agent_index);
    assert_eq!(
        auth_error_state.agent_chain.current_model_index,
        initial_model_index
    );

    let internal_error_state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            agent_name,
            139,
            AgentErrorKind::InternalError,
            false,
        ),
    );
    assert!(internal_error_state.agent_chain.current_agent_index > initial_agent_index);
}

#[test]
fn test_reduce_rebase_full_state_machine() {
    let mut state = create_test_state();

    state = reduce(
        state,
        PipelineEvent::rebase_started(RebasePhase::Initial, "main".to_string()),
    );
    assert!(matches!(state.rebase, RebaseState::InProgress { .. }));

    state = reduce(
        state,
        PipelineEvent::rebase_conflict_detected(vec![std::path::PathBuf::from("file1.txt")]),
    );
    assert!(matches!(state.rebase, RebaseState::Conflicted { .. }));

    state = reduce(
        state,
        PipelineEvent::rebase_conflict_resolved(vec![std::path::PathBuf::from("file1.txt")]),
    );
    assert!(matches!(state.rebase, RebaseState::InProgress { .. }));

    state = reduce(
        state,
        PipelineEvent::rebase_succeeded(RebasePhase::Initial, "def456".to_string()),
    );
    assert!(matches!(state.rebase, RebaseState::Completed { .. }));
}

#[test]
fn test_reduce_commit_full_state_machine() {
    let mut state = create_test_state();

    state = reduce(state, PipelineEvent::commit_generation_started());
    assert!(matches!(state.commit, CommitState::Generating { .. }));

    state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string()),
    );
    assert!(matches!(state.commit, CommitState::Committed { .. }));
}

#[test]
fn test_reduce_phase_transitions() {
    let mut state = create_test_state();

    state = reduce(state, PipelineEvent::planning_phase_completed());
    assert_eq!(state.phase, PipelinePhase::Development);

    state = reduce(state, PipelineEvent::development_phase_started());
    assert_eq!(state.phase, PipelinePhase::Development);

    state = reduce(state, PipelineEvent::development_phase_completed());
    assert_eq!(state.phase, PipelinePhase::Review);

    state = reduce(state, PipelineEvent::review_phase_started());
    assert_eq!(state.phase, PipelinePhase::Review);

    state = reduce(state, PipelineEvent::review_phase_completed(false));
    assert_eq!(state.phase, PipelinePhase::CommitMessage);
}

#[test]
fn test_reduce_agent_chain_exhaustion() {
    let state = PipelineState {
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string()],
                vec![vec!["model1".to_string()]],
                AgentRole::Developer,
            )
            .with_max_cycles(3),
        ..create_test_state()
    };

    let exhausted_state = reduce(
        state,
        PipelineEvent::agent_chain_exhausted(AgentRole::Developer),
    );

    assert_eq!(exhausted_state.agent_chain.current_agent_index, 0);
    assert_eq!(exhausted_state.agent_chain.current_model_index, 0);
    assert_eq!(exhausted_state.agent_chain.retry_cycle, 1);
}

#[test]
fn test_reduce_agent_fallback_triggers_fallback_event() {
    let state = create_test_state();
    let agent = state.agent_chain.current_agent().unwrap().clone();

    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            agent.clone(),
            1,
            AgentErrorKind::Authentication,
            false,
        ),
    );

    assert!(new_state.agent_chain.current_agent_index > 0);
}

#[test]
fn test_reduce_model_fallback_triggers_for_network_error() {
    let state = create_test_state();
    let initial_model_index = state.agent_chain.current_model_index;
    let agent_name = state.agent_chain.current_agent().unwrap().clone();

    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            agent_name,
            1,
            AgentErrorKind::Network,
            true,
        ),
    );

    assert!(new_state.agent_chain.current_model_index > initial_model_index);
}

#[test]
fn test_rate_limit_fallback_switches_agent() {
    let state = create_test_state();
    let initial_agent_index = state.agent_chain.current_agent_index;

    let new_state = reduce(
        state,
        PipelineEvent::agent_rate_limit_fallback(
            AgentRole::Developer,
            "agent1".to_string(),
            Some("test prompt".to_string()),
        ),
    );

    // Should switch to next agent
    assert!(
        new_state.agent_chain.current_agent_index > initial_agent_index,
        "Rate limit should trigger agent fallback, not model fallback"
    );
    // Should preserve prompt
    assert_eq!(
        new_state.agent_chain.rate_limit_continuation_prompt,
        Some("test prompt".to_string())
    );
}

#[test]
fn test_rate_limit_fallback_with_no_prompt_context() {
    let state = create_test_state();
    let initial_agent_index = state.agent_chain.current_agent_index;

    let new_state = reduce(
        state,
        PipelineEvent::agent_rate_limit_fallback(AgentRole::Developer, "agent1".to_string(), None),
    );

    // Should still switch to next agent
    assert!(new_state.agent_chain.current_agent_index > initial_agent_index);
    // Prompt context should be None
    assert!(new_state
        .agent_chain
        .rate_limit_continuation_prompt
        .is_none());
}

#[test]
fn test_success_clears_rate_limit_continuation_prompt() {
    let mut state = create_test_state();
    state.agent_chain.rate_limit_continuation_prompt = Some("old prompt".to_string());

    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_succeeded(AgentRole::Developer, "agent1".to_string()),
    );

    assert!(
        new_state
            .agent_chain
            .rate_limit_continuation_prompt
            .is_none(),
        "Success should clear rate limit continuation prompt"
    );
}

#[test]
fn test_auth_fallback_clears_session_and_advances_agent() {
    let mut chain = AgentChainState::initial()
        .with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        )
        .with_session_id(Some("session-123".to_string()));
    chain.rate_limit_continuation_prompt = Some("some saved prompt".to_string());

    let state = PipelineState {
        phase: PipelinePhase::Development,
        agent_chain: chain,
        ..PipelineState::initial(5, 2)
    };

    let new_state = reduce(
        state,
        PipelineEvent::agent_auth_fallback(AgentRole::Developer, "agent1".to_string()),
    );

    // Should advance to next agent
    assert_eq!(new_state.agent_chain.current_agent().unwrap(), "agent2");

    // Session should be cleared
    assert!(new_state.agent_chain.last_session_id.is_none());

    // Auth fallback semantics: switch agents WITHOUT prompt context.
    // Any previously-saved rate-limit continuation prompt must be cleared so we
    // don't accidentally carry prompt context across an auth fallback.
    assert!(
        new_state
            .agent_chain
            .rate_limit_continuation_prompt
            .is_none(),
        "AuthFallback must clear any existing continuation prompt"
    );
}

#[test]
fn test_rate_limit_fallback_clears_session_id() {
    // RateLimitFallback preserves prompt context, but MUST NOT preserve session IDs
    // across agents.
    let chain = AgentChainState::initial()
        .with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        )
        .with_session_id(Some("session-123".to_string()));

    let state = PipelineState {
        phase: PipelinePhase::Development,
        agent_chain: chain,
        ..PipelineState::initial(5, 2)
    };

    let new_state = reduce(
        state,
        PipelineEvent::agent_rate_limit_fallback(
            AgentRole::Developer,
            "agent1".to_string(),
            Some("preserved prompt".to_string()),
        ),
    );

    assert!(
        new_state.agent_chain.last_session_id.is_none(),
        "RateLimitFallback must clear session IDs when switching agents"
    );
    assert_eq!(
        new_state.agent_chain.rate_limit_continuation_prompt,
        Some("preserved prompt".to_string()),
        "RateLimitFallback should preserve prompt context"
    );
}

#[test]
fn test_auth_fallback_does_not_set_continuation_prompt() {
    // Setup: state with NO existing continuation prompt
    let chain = AgentChainState::initial().with_agents(
        vec!["agent1".to_string(), "agent2".to_string()],
        vec![vec![], vec![]],
        AgentRole::Developer,
    );

    let state = PipelineState {
        phase: PipelinePhase::Development,
        agent_chain: chain,
        ..PipelineState::initial(5, 2)
    };

    // Auth fallback should NOT set a continuation prompt
    let new_state = reduce(
        state,
        PipelineEvent::agent_auth_fallback(AgentRole::Developer, "agent1".to_string()),
    );

    // Key assertion: AuthFallback does NOT set prompt context
    // (unlike RateLimitFallback which preserves the prompt)
    assert!(
        new_state
            .agent_chain
            .rate_limit_continuation_prompt
            .is_none(),
        "AuthFallback should not set continuation prompt (only RateLimitFallback does)"
    );
}

#[test]
fn test_rate_limit_vs_auth_fallback_prompt_semantics() {
    // This test documents the key semantic difference between the two fallback types
    let base_chain = AgentChainState::initial().with_agents(
        vec![
            "agent1".to_string(),
            "agent2".to_string(),
            "agent3".to_string(),
        ],
        vec![vec![], vec![], vec![]],
        AgentRole::Developer,
    );

    // Test 1: RateLimitFallback preserves prompt
    let state1 = PipelineState {
        phase: PipelinePhase::Development,
        agent_chain: base_chain.clone(),
        ..PipelineState::initial(5, 2)
    };

    let after_rate_limit = reduce(
        state1,
        PipelineEvent::agent_rate_limit_fallback(
            AgentRole::Developer,
            "agent1".to_string(),
            Some("preserved prompt".to_string()),
        ),
    );

    assert_eq!(
        after_rate_limit.agent_chain.rate_limit_continuation_prompt,
        Some("preserved prompt".to_string()),
        "RateLimitFallback should preserve prompt context"
    );

    // Test 2: AuthFallback does NOT set prompt (credentials issue, not exhaustion)
    let state2 = PipelineState {
        phase: PipelinePhase::Development,
        agent_chain: base_chain,
        ..PipelineState::initial(5, 2)
    };

    let after_auth = reduce(
        state2,
        PipelineEvent::agent_auth_fallback(AgentRole::Developer, "agent1".to_string()),
    );

    assert!(
        after_auth
            .agent_chain
            .rate_limit_continuation_prompt
            .is_none(),
        "AuthFallback should not set prompt context (credentials issue, not exhaustion)"
    );
}

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
    let initial_state = PipelineState {
        phase: PipelinePhase::FinalValidation,
        ..PipelineState::initial(5, 2)
    };

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

// =========================================================================
// Continuation event handling tests
// =========================================================================

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

// =========================================================================
// OutputValidationFailed event tests
// =========================================================================

#[test]
fn test_output_validation_failed_retries_within_limit() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(0, 0),
    );
    assert_eq!(new_state.phase, PipelinePhase::Development);
    assert_eq!(new_state.continuation.invalid_output_attempts, 1);
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
}

#[test]
fn test_output_validation_failed_increments_attempt_counter() {
    let mut state = create_test_state();
    state.continuation.invalid_output_attempts = 1;

    let new_state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(0, 1),
    );
    assert_eq!(new_state.phase, PipelinePhase::Development);
    assert_eq!(new_state.continuation.invalid_output_attempts, 2);
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
}

#[test]
fn test_output_validation_failed_switches_agent_at_limit() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_count: 1,
            max_xsd_retry_count: 2,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(0, 0),
    );
    assert_eq!(new_state.continuation.invalid_output_attempts, 0);
    assert!(
        new_state.agent_chain.current_agent_index > 0,
        "Should switch to next agent after max invalid output attempts"
    );
}

#[test]
fn test_output_validation_failed_resets_counter_on_agent_switch() {
    use crate::reducer::state::ContinuationState;

    let mut state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_count: 1,
            max_xsd_retry_count: 2,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };
    state.continuation.invalid_output_attempts = 2;

    let new_state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(0, 0),
    );
    assert_eq!(
        new_state.continuation.invalid_output_attempts, 0,
        "Counter should reset when switching agents"
    );
}

#[test]
fn test_output_validation_failed_stays_in_development_phase() {
    let mut state = create_test_state();
    state.phase = PipelinePhase::Development;

    let new_state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(0, 0),
    );
    assert_eq!(
        new_state.phase,
        PipelinePhase::Development,
        "Should stay in Development phase for retry"
    );
}

#[test]
fn test_output_validation_failed_respects_configured_xsd_retry_limit() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 1,
        continuation: ContinuationState {
            xsd_retry_count: 1,
            max_xsd_retry_count: 5,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        ),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(1, 0),
    );

    assert_eq!(
        new_state.agent_chain.current_agent_index, 0,
        "Configured XSD retry limit should allow retries before agent fallback"
    );
    assert!(
        new_state.continuation.xsd_retry_pending,
        "Should request XSD retry while under configured limit"
    );
}

// =========================================================================
// Review output validation / clean pass tests
// =========================================================================

#[test]
fn test_review_output_validation_failed_increments_state_counter() {
    let mut state = create_test_state();
    state.phase = PipelinePhase::Review;
    state.reviewer_pass = 0;
    state.total_reviewer_passes = 2;

    let new_state = reduce(state, PipelineEvent::review_output_validation_failed(0, 0));

    assert_eq!(new_state.phase, PipelinePhase::Review);
    assert_eq!(new_state.reviewer_pass, 0);
    assert_eq!(new_state.continuation.invalid_output_attempts, 1);
}

#[test]
fn test_review_output_validation_failed_switches_agent_after_limit() {
    use crate::reducer::state::ContinuationState;

    let mut state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_count: 1,
            max_xsd_retry_count: 2,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };
    state.phase = PipelinePhase::Review;
    state.reviewer_pass = 0;
    state.total_reviewer_passes = 2;
    state.continuation.invalid_output_attempts = 2;

    let new_state = reduce(state, PipelineEvent::review_output_validation_failed(0, 0));

    assert_eq!(new_state.phase, PipelinePhase::Review);
    assert_eq!(new_state.reviewer_pass, 0);
    assert_eq!(
        new_state.continuation.invalid_output_attempts, 0,
        "Counter should reset when switching agents"
    );
    assert!(
        new_state.agent_chain.current_agent_index > 0,
        "Should switch to next agent after max invalid output attempts"
    );
}

#[test]
fn test_review_pass_completed_clean_exits_review_phase() {
    let mut state = create_test_state();
    state.phase = PipelinePhase::Review;
    state.reviewer_pass = 0;
    state.total_reviewer_passes = 2;

    let new_state = reduce(state, PipelineEvent::review_pass_completed_clean(0));

    assert_eq!(
        new_state.phase,
        PipelinePhase::Review,
        "Clean pass should not exit review when passes remain"
    );
    assert_eq!(new_state.reviewer_pass, 1);
    assert_eq!(new_state.review_issues_found, false);
}

#[test]
fn test_review_pass_completed_clean_on_last_pass_clears_previous_phase() {
    let mut state = create_test_state();
    state.phase = PipelinePhase::Review;
    state.reviewer_pass = 0;
    state.total_reviewer_passes = 1;
    state.previous_phase = Some(PipelinePhase::Development);

    let new_state = reduce(state, PipelineEvent::review_pass_completed_clean(0));

    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    assert_eq!(new_state.previous_phase, None);
    assert!(matches!(new_state.commit, CommitState::NotStarted));
}

#[test]
fn test_review_pass_started_does_not_reset_invalid_output_attempts_on_retry() {
    let mut state = create_test_state();
    state.phase = PipelinePhase::Review;
    state.reviewer_pass = 0;
    state.continuation.invalid_output_attempts = 1;

    let new_state = reduce(state, PipelineEvent::review_pass_started(0));

    assert_eq!(new_state.reviewer_pass, 0);
    assert_eq!(
        new_state.continuation.invalid_output_attempts, 1,
        "Retrying the same pass should not clear invalid output attempt counter"
    );
}

#[test]
fn test_review_pass_started_preserves_agent_chain_on_retry() {
    use crate::reducer::state::ContinuationState;

    let mut state = create_test_state();
    state.phase = PipelinePhase::Review;
    state.reviewer_pass = 0;
    state.total_reviewer_passes = 2;
    // Set xsd_retry_count to max-1 so next validation failure triggers agent switch.
    state.continuation = ContinuationState {
        xsd_retry_count: 1,
        max_xsd_retry_count: 2,
        ..ContinuationState::new()
    };

    // Simulate switching agents due to XSD retry limit reached.
    let state = reduce(state, PipelineEvent::review_output_validation_failed(0, 0));
    assert!(
        state.agent_chain.current_agent_index > 0,
        "Precondition: review_output_validation_failed should have switched agents when XSD retry limit reached"
    );

    // Orchestration can re-emit PassStarted for the same pass during retries.
    let new_state = reduce(state.clone(), PipelineEvent::review_pass_started(0));

    assert_eq!(
        new_state.agent_chain.current_agent_index, state.agent_chain.current_agent_index,
        "Retrying the same pass should preserve the current agent selection"
    );
}

#[test]
fn test_review_pass_started_resets_invalid_output_attempts_for_new_pass() {
    let mut state = create_test_state();
    state.phase = PipelinePhase::Review;
    state.reviewer_pass = 0;
    state.continuation.invalid_output_attempts = 2;

    let new_state = reduce(state, PipelineEvent::review_pass_started(1));

    assert_eq!(new_state.reviewer_pass, 1);
    assert_eq!(new_state.continuation.invalid_output_attempts, 0);
}

#[test]
fn test_review_phase_completed_resets_commit_state() {
    let mut state = create_test_state();
    state.phase = PipelinePhase::Review;
    state.commit = CommitState::Committed {
        hash: "abc123".to_string(),
    };

    let new_state = reduce(state, PipelineEvent::review_phase_completed(true));

    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    assert!(matches!(new_state.commit, CommitState::NotStarted));
    assert_eq!(new_state.previous_phase, None);
}

#[test]
fn test_review_completed_no_issues_on_last_pass_resets_commit_state() {
    let mut state = create_test_state();
    state.phase = PipelinePhase::Review;
    state.reviewer_pass = 0;
    state.total_reviewer_passes = 1;
    state.commit = CommitState::Committed {
        hash: "abc123".to_string(),
    };

    let new_state = reduce(state, PipelineEvent::review_completed(0, false));

    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    assert!(matches!(new_state.commit, CommitState::NotStarted));
}

// =========================================================================
// ContinuationBudgetExhausted event tests
// =========================================================================

#[test]
fn test_continuation_budget_exhausted_transitions_to_interrupted() {
    use crate::reducer::state::DevelopmentStatus;

    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::development_continuation_budget_exhausted(0, 3, DevelopmentStatus::Partial),
    );
    assert_eq!(
        new_state.phase,
        PipelinePhase::Interrupted,
        "Should transition to Interrupted when continuation budget exhausted"
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
        "Continuation state should be reset"
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

// =========================================================================
// Event sequence tests for determinism
// =========================================================================

#[test]
fn test_event_sequence_output_validation_retry_then_success() {
    use crate::reducer::state::ContinuationState;

    let mut state = PipelineState {
        continuation: ContinuationState {
            max_xsd_retry_count: 3,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };
    state.phase = PipelinePhase::Development;

    // Simulate: validation fail -> validation fail -> success
    state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(0, 0),
    );
    assert_eq!(state.continuation.invalid_output_attempts, 1);
    assert_eq!(state.phase, PipelinePhase::Development);

    state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(0, 1),
    );
    assert_eq!(state.continuation.invalid_output_attempts, 2);

    assert_eq!(
        state.agent_chain.current_agent_index, 0,
        "Should not switch agents yet"
    );

    // Now succeed
    state = reduce(
        state,
        PipelineEvent::development_iteration_completed(0, true),
    );
    assert_eq!(state.phase, PipelinePhase::CommitMessage);
}

#[test]
fn test_event_sequence_validation_failures_trigger_agent_switch() {
    use crate::reducer::state::ContinuationState;

    let mut state = PipelineState {
        continuation: ContinuationState {
            max_xsd_retry_count: 3,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };
    state.phase = PipelinePhase::Development;

    // First validation failure
    state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(0, 0),
    );

    // Second validation failure
    state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(0, 1),
    );

    // Third validation failure - should trigger agent switch
    state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(0, 2),
    );

    // After max failures, should switch agents and reset counter
    assert_eq!(
        state.continuation.invalid_output_attempts, 0,
        "Counter should reset"
    );
    assert!(
        state.agent_chain.current_agent_index > 0 || state.agent_chain.retry_cycle > 0,
        "Should have advanced to next agent or started retry cycle"
    );
}

#[test]
fn test_determinism_same_events_same_state() {
    use crate::reducer::state::DevelopmentStatus;

    // Create two identical initial states
    let state1 = create_test_state();
    let state2 = create_test_state();

    // Apply the same sequence of events
    let events = vec![
        PipelineEvent::development_iteration_started(0),
        PipelineEvent::development_output_validation_failed(0, 0),
        PipelineEvent::development_iteration_continuation_triggered(
            0,
            DevelopmentStatus::Partial,
            "Work".to_string(),
            None,
            None,
        ),
    ];

    let mut final1 = state1;
    let mut final2 = state2;

    for event in events {
        final1 = reduce(final1, event.clone());
        final2 = reduce(final2, event);
    }

    // States should be identical
    assert_eq!(final1.iteration, final2.iteration);
    assert_eq!(final1.phase, final2.phase);
    assert_eq!(
        final1.continuation.continuation_attempt,
        final2.continuation.continuation_attempt
    );
    assert_eq!(
        final1.continuation.invalid_output_attempts,
        final2.continuation.invalid_output_attempts
    );
}

// =========================================================================
// Dev->Review transition agent chain tests
// =========================================================================

/// When transitioning from Development to Review (via CommitCreated or CommitSkipped),
/// the agent chain must be cleared so that orchestration will emit InitializeAgentChain
/// for the Reviewer role. This ensures the reviewer fallback chain is used.
#[test]
fn test_commit_created_clears_agent_chain_when_dev_to_review() {
    let mut state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        previous_phase: Some(PipelinePhase::Development),
        iteration: 4, // Last iteration (will trigger transition to Review)
        total_iterations: 5,
        total_reviewer_passes: 2,
        ..create_test_state()
    };

    // Populate the agent chain as if it was used for development
    state.agent_chain = AgentChainState::initial()
        .with_agents(
            vec!["dev-agent-1".to_string(), "dev-agent-2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        )
        .with_max_cycles(3);
    state.agent_chain.current_agent_index = 1; // Simulate having advanced to second agent

    let new_state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string()),
    );

    // Should transition to Review
    assert_eq!(new_state.phase, PipelinePhase::Review);
    // Agent chain should be cleared (empty agents list) so orchestration
    // will initialize it for Reviewer role
    assert!(
        new_state.agent_chain.agents.is_empty(),
        "Agent chain should be cleared when transitioning from Dev to Review, got agents: {:?}",
        new_state.agent_chain.agents
    );
    assert_eq!(
        new_state.agent_chain.current_role,
        AgentRole::Reviewer,
        "Agent chain role should be set to Reviewer"
    );
    assert_eq!(
        new_state.agent_chain.current_agent_index, 0,
        "Agent chain index should be reset to 0"
    );
}

/// Same test for CommitSkipped - should also clear agent chain for dev->review transition
#[test]
fn test_commit_skipped_clears_agent_chain_when_dev_to_review() {
    let mut state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        previous_phase: Some(PipelinePhase::Development),
        iteration: 4, // Last iteration
        total_iterations: 5,
        total_reviewer_passes: 2,
        ..create_test_state()
    };

    state.agent_chain = AgentChainState::initial()
        .with_agents(
            vec!["dev-agent-1".to_string()],
            vec![vec![]],
            AgentRole::Developer,
        )
        .with_max_cycles(3);

    let new_state = reduce(
        state,
        PipelineEvent::commit_skipped("no changes".to_string()),
    );

    assert_eq!(new_state.phase, PipelinePhase::Review);
    assert!(
        new_state.agent_chain.agents.is_empty(),
        "Agent chain should be cleared when transitioning from Dev to Review via skip"
    );
    assert_eq!(new_state.agent_chain.current_role, AgentRole::Reviewer);
}

/// Verify that after ChainInitialized for Reviewer, the reducer correctly populates
/// state.agent_chain with the fallback agents in order.
#[test]
fn test_chain_initialized_populates_reviewer_chain() {
    let mut state = create_test_state();
    state.phase = PipelinePhase::Review;
    state.agent_chain = AgentChainState::initial().reset_for_role(AgentRole::Reviewer);

    let new_state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            AgentRole::Reviewer,
            vec![
                "codex".to_string(),
                "opencode".to_string(),
                "claude".to_string(),
            ],
            3,
            1000,
            2.0,
            60000,
        ),
    );

    assert_eq!(
        new_state.agent_chain.agents,
        vec![
            "codex".to_string(),
            "opencode".to_string(),
            "claude".to_string()
        ],
        "Reducer should store the exact fallback chain from ChainInitialized event"
    );
    assert_eq!(
        new_state.agent_chain.current_agent(),
        Some(&"codex".to_string()),
        "First agent in chain should be 'codex' (first fallback)"
    );
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_role, AgentRole::Reviewer);
}

/// Auth failure during review should advance the reducer's agent chain,
/// not just a local variable in review.rs
#[test]
fn test_auth_failure_during_review_advances_reducer_chain() {
    let mut state = create_test_state();
    state.phase = PipelinePhase::Review;
    state.agent_chain = AgentChainState::initial()
        .with_agents(
            vec![
                "codex".to_string(),
                "opencode".to_string(),
                "claude".to_string(),
            ],
            vec![vec![], vec![], vec![]],
            AgentRole::Reviewer,
        )
        .reset_for_role(AgentRole::Reviewer);

    assert_eq!(
        state.agent_chain.current_agent(),
        Some(&"codex".to_string()),
        "Precondition: current agent should be codex"
    );

    // Simulate auth failure - this should advance to next agent
    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            AgentRole::Reviewer,
            "codex".to_string(),
            1,
            AgentErrorKind::Authentication,
            false, // not retriable
        ),
    );

    assert_eq!(
        new_state.agent_chain.current_agent(),
        Some(&"opencode".to_string()),
        "Auth failure should advance reducer's agent chain to opencode"
    );
    assert_eq!(new_state.agent_chain.current_agent_index, 1);
}

/// Orchestration should emit InitializeAgentChain when entering Review phase
/// with an empty agent chain.
#[test]
fn test_orchestration_emits_init_chain_for_reviewer_after_dev_review_transition() {
    use crate::reducer::orchestration::determine_next_effect;

    let mut state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        ..create_test_state()
    };

    // Clear the agent chain as would happen after dev->review transition
    state.agent_chain = AgentChainState::initial().reset_for_role(AgentRole::Reviewer);

    let effect = determine_next_effect(&state);
    assert!(
        matches!(
            effect,
            crate::reducer::effect::Effect::InitializeAgentChain {
                role: AgentRole::Reviewer
            }
        ),
        "Orchestration should emit InitializeAgentChain for Reviewer when chain is empty, got {:?}",
        effect
    );
}

/// Verify that the agent chain used in review comes from reducer state,
/// not from local construction.
#[test]
fn test_review_phase_agent_selection_uses_reducer_state() {
    use crate::reducer::orchestration::determine_next_effect;

    let mut state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        ..create_test_state()
    };

    // Initialize the agent chain with specific agents
    state.agent_chain = AgentChainState::initial()
        .with_agents(
            vec![
                "codex".to_string(),
                "opencode".to_string(),
                "claude".to_string(),
            ],
            vec![vec![], vec![], vec![]],
            AgentRole::Reviewer,
        )
        .reset_for_role(AgentRole::Reviewer);

    // Verify the current agent is codex (first in chain)
    assert_eq!(
        state.agent_chain.current_agent(),
        Some(&"codex".to_string()),
        "Current agent should be 'codex' from reducer state"
    );

    // Advance to next agent (simulating auth failure)
    state.agent_chain = state.agent_chain.switch_to_next_agent();

    assert_eq!(
        state.agent_chain.current_agent(),
        Some(&"opencode".to_string()),
        "After advance, current agent should be 'opencode'"
    );

    // Orchestration should still work - it reads from state.agent_chain
    let effect = determine_next_effect(&state);

    // Should emit PrepareReviewContext, not InitializeAgentChain (chain is already populated)
    assert!(
        matches!(
            effect,
            crate::reducer::effect::Effect::PrepareReviewContext { .. }
        ),
        "Should emit PrepareReviewContext when chain is already initialized, got {:?}",
        effect
    );
}

// =========================================================================
// XSD retry state transitions
// =========================================================================

#[test]
fn test_development_output_validation_failed_sets_xsd_retry_pending() {
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 1,
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::development_output_validation_failed(1, 0),
    );

    assert!(
        new_state.continuation.xsd_retry_pending,
        "XSD retry should be pending after validation failure"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 1,
        "XSD retry count should be incremented"
    );
    assert_eq!(
        new_state.continuation.invalid_output_attempts, 1,
        "Invalid output attempts should be incremented"
    );
}

#[test]
fn test_development_output_validation_failed_exhausts_xsd_retries() {
    use crate::reducer::state::ContinuationState;

    // Create state with custom max_xsd_retry_count = 2
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 1,
        continuation: ContinuationState {
            xsd_retry_count: 1,
            max_xsd_retry_count: 2,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Developer,
        ),
        ..create_test_state()
    };

    let new_state = reduce(
        state.clone(),
        PipelineEvent::development_output_validation_failed(1, 0),
    );

    // XSD retries exhausted, should switch agent
    assert!(
        !new_state.continuation.xsd_retry_pending,
        "XSD retry should not be pending after exhaustion"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 0,
        "XSD retry count should be reset after agent switch"
    );
    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should have switched to next agent"
    );
}

#[test]
fn test_planning_output_validation_failed_sets_xsd_retry_pending() {
    let state = PipelineState {
        phase: PipelinePhase::Planning,
        iteration: 0,
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::planning_output_validation_failed(0, 0),
    );

    assert!(
        new_state.continuation.xsd_retry_pending,
        "XSD retry should be pending after validation failure"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 1,
        "XSD retry count should be incremented"
    );
}

#[test]
fn test_review_output_validation_failed_sets_xsd_retry_pending() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::review_output_validation_failed(0, 0));

    assert!(
        new_state.continuation.xsd_retry_pending,
        "XSD retry should be pending after validation failure"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 1,
        "XSD retry count should be incremented"
    );
}

#[test]
fn test_plan_generation_completed_clears_xsd_retry_state() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::Planning,
        continuation: ContinuationState {
            xsd_retry_count: 3,
            xsd_retry_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::plan_generation_completed(1, true));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "XSD retry pending should be cleared on success"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 0,
        "XSD retry count should be reset on success"
    );
}

#[test]
fn test_session_established_stores_session_id() {
    let state = create_test_state();

    let new_state = reduce(
        state,
        PipelineEvent::agent_session_established(
            AgentRole::Developer,
            "claude".to_string(),
            "ses_abc123".to_string(),
        ),
    );

    assert_eq!(
        new_state.agent_chain.last_session_id,
        Some("ses_abc123".to_string()),
        "Session ID should be stored"
    );
}

#[test]
fn test_agent_switch_clears_session_id() {
    let state = PipelineState {
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            )
            .with_session_id(Some("ses_abc123".to_string())),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            AgentRole::Developer,
            "agent1".to_string(),
            1,
            crate::reducer::event::AgentErrorKind::InternalError,
            false,
        ),
    );

    assert!(
        new_state.agent_chain.last_session_id.is_none(),
        "Session ID should be cleared when switching agents"
    );
}

// =========================================================================
// Tests for commit XSD retry
// =========================================================================

#[test]
fn test_commit_message_validation_failed_sets_xsd_retry_pending() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        },
        continuation: ContinuationState::new(),
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::commit_message_validation_failed("Invalid XML".to_string(), 1),
    );

    assert!(
        new_state.continuation.xsd_retry_pending,
        "XSD retry pending should be set"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 1,
        "XSD retry count should be incremented"
    );
}

#[test]
fn test_commit_xsd_retry_exhausted_switches_agent() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        },
        continuation: ContinuationState {
            xsd_retry_count: 9,
            max_xsd_retry_count: 10,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Commit,
        ),
        ..PipelineState::initial(5, 2)
    };

    let new_state = reduce(
        state,
        PipelineEvent::commit_message_validation_failed("Invalid XML".to_string(), 1),
    );

    // Should have switched to next agent
    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should switch to next agent"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 0,
        "XSD retry count should be reset"
    );
    assert!(
        !new_state.continuation.xsd_retry_pending,
        "XSD retry pending should be cleared"
    );
}

#[test]
fn test_planning_prompt_prepared_clears_xsd_retry_pending() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::planning_prompt_prepared(0));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Prompt preparation should clear xsd retry pending"
    );
}

#[test]
fn test_planning_agent_invoked_clears_xsd_retry_pending() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::planning_agent_invoked(0));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Agent invocation should clear xsd retry pending"
    );
}

#[test]
fn test_review_prompt_prepared_clears_xsd_retry_pending() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::review_prompt_prepared(0));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Prompt preparation should clear xsd retry pending"
    );
}

#[test]
fn test_review_agent_invoked_clears_xsd_retry_pending() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::review_agent_invoked(0));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Agent invocation should clear xsd retry pending"
    );
}

#[test]
fn test_commit_prompt_prepared_clears_xsd_retry_pending() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::commit_prompt_prepared(1));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Prompt preparation should clear xsd retry pending"
    );
}

#[test]
fn test_commit_agent_invoked_clears_xsd_retry_pending() {
    let state = PipelineState {
        continuation: ContinuationState {
            xsd_retry_pending: true,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::commit_agent_invoked(1));

    assert!(
        !new_state.continuation.xsd_retry_pending,
        "Agent invocation should clear xsd retry pending"
    );
}

#[test]
fn test_review_pass_completed_clean_resets_commit_diff_flags() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 1,
        commit_diff_prepared: true,
        commit_diff_empty: true,
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::review_pass_completed_clean(0));

    assert!(!new_state.commit_diff_prepared);
    assert!(!new_state.commit_diff_empty);
}

// =========================================================================
// Tests for fix continuation
// =========================================================================

#[test]
fn test_fix_continuation_triggered_sets_pending() {
    use crate::reducer::state::{ContinuationState, FixStatus};

    let state = PipelineState {
        phase: PipelinePhase::Review,
        review_issues_found: true,
        reviewer_pass: 0,
        continuation: ContinuationState {
            invalid_output_attempts: 3, // Set non-zero to verify reset
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::fix_continuation_triggered(
            0,
            FixStatus::IssuesRemain,
            Some("Fixed 2 of 5 issues".to_string()),
        ),
    );

    assert!(
        new_state.continuation.fix_continue_pending,
        "Fix continue pending should be set"
    );
    assert_eq!(
        new_state.continuation.fix_continuation_attempt, 1,
        "Fix continuation attempt should be incremented"
    );
    assert_eq!(
        new_state.continuation.fix_status,
        Some(FixStatus::IssuesRemain),
        "Fix status should be stored"
    );
    assert_eq!(
        new_state.continuation.invalid_output_attempts, 0,
        "Invalid output attempts should be reset for new continuation"
    );
}

#[test]
fn test_fix_continuation_succeeded_transitions_to_commit() {
    use crate::reducer::state::{ContinuationState, FixStatus};

    let state = PipelineState {
        phase: PipelinePhase::Review,
        review_issues_found: true,
        reviewer_pass: 0,
        continuation: ContinuationState {
            fix_continue_pending: true,
            fix_continuation_attempt: 2,
            fix_status: Some(FixStatus::IssuesRemain),
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::fix_continuation_succeeded(0, 2));

    assert_eq!(
        new_state.phase,
        PipelinePhase::CommitMessage,
        "Should transition to CommitMessage phase"
    );
    assert!(
        !new_state.continuation.fix_continue_pending,
        "Fix continue pending should be cleared"
    );
}

#[test]
fn test_fix_continuation_budget_exhausted_transitions_to_commit() {
    use crate::reducer::state::{ContinuationState, FixStatus};

    let state = PipelineState {
        phase: PipelinePhase::Review,
        review_issues_found: true,
        reviewer_pass: 0,
        continuation: ContinuationState {
            fix_continue_pending: true,
            fix_continuation_attempt: 3,
            fix_status: Some(FixStatus::IssuesRemain),
            max_fix_continue_count: 3,
            ..ContinuationState::new()
        },
        ..create_test_state()
    };

    let new_state = reduce(
        state,
        PipelineEvent::fix_continuation_budget_exhausted(0, 3, FixStatus::IssuesRemain),
    );

    assert_eq!(
        new_state.phase,
        PipelinePhase::CommitMessage,
        "Should transition to CommitMessage even when budget exhausted"
    );
}

// =========================================================================
// Tests for TEMPLATE_VARIABLES_INVALID
// =========================================================================

#[test]
fn test_template_variables_invalid_switches_agent() {
    let state = PipelineState {
        phase: PipelinePhase::Development,
        agent_chain: AgentChainState::initial()
            .with_agents(
                vec!["agent1".to_string(), "agent2".to_string()],
                vec![vec![], vec![]],
                AgentRole::Developer,
            )
            .with_session_id(Some("ses_abc123".to_string())),
        ..PipelineState::initial(5, 2)
    };

    let new_state = reduce(
        state,
        PipelineEvent::agent_template_variables_invalid(
            AgentRole::Developer,
            "dev_iteration".to_string(),
            vec!["PLAN".to_string()],
            vec!["{{XSD_ERROR}}".to_string()],
        ),
    );

    // Should switch to next agent
    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should switch to next agent on template failure"
    );
    // Session ID should be cleared
    assert!(
        new_state.agent_chain.last_session_id.is_none(),
        "Session ID should be cleared when switching agents"
    );
}

// =========================================================================
// Tests for fix output validation failed
// =========================================================================

#[test]
fn test_fix_output_validation_failed_sets_xsd_retry_pending() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::Review,
        review_issues_found: true,
        reviewer_pass: 0,
        continuation: ContinuationState::new(),
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::fix_output_validation_failed(0, 0));

    assert!(
        new_state.continuation.xsd_retry_pending,
        "XSD retry pending should be set"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 1,
        "XSD retry count should be incremented"
    );
}

#[test]
fn test_fix_output_validation_exhausted_switches_agent() {
    use crate::reducer::state::ContinuationState;

    let state = PipelineState {
        phase: PipelinePhase::Review,
        review_issues_found: true,
        reviewer_pass: 0,
        continuation: ContinuationState {
            xsd_retry_count: 9,
            max_xsd_retry_count: 10,
            ..ContinuationState::new()
        },
        agent_chain: AgentChainState::initial().with_agents(
            vec!["agent1".to_string(), "agent2".to_string()],
            vec![vec![], vec![]],
            AgentRole::Reviewer,
        ),
        ..PipelineState::initial(5, 2)
    };

    let new_state = reduce(state, PipelineEvent::fix_output_validation_failed(0, 2));

    // Should have switched to next agent
    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should switch to next agent when XSD retries exhausted"
    );
    assert_eq!(
        new_state.continuation.xsd_retry_count, 0,
        "XSD retry count should be reset"
    );
}
