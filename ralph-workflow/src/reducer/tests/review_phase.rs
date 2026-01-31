//! Tests for review phase events (review passes, fix attempts).
//!
//! These tests validate the critical review_issues_found flag behavior that was
//! one of the 7 bugs we fixed in the reducer.

use super::*;

#[test]
fn test_review_phase_started_sets_review_phase() {
    let state = create_test_state();
    let new_state = reduce(state, PipelineEvent::review_phase_started());

    assert_eq!(new_state.phase, PipelinePhase::Review);
}

#[test]
fn test_review_phase_started_resets_reviewer_pass_to_zero() {
    let state = PipelineState {
        reviewer_pass: 5,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::review_phase_started());

    assert_eq!(new_state.reviewer_pass, 0);
}

#[test]
fn test_review_phase_started_clears_issues_flag() {
    let state = PipelineState {
        review_issues_found: true,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::review_phase_started());

    assert!(!new_state.review_issues_found);
}

#[test]
fn test_review_pass_started_sets_pass_and_resets_agent_chain() {
    let base_state = create_test_state();
    // Setup with non-zero agent chain state AND review_issues_found = true
    let mut agent_chain = base_state.agent_chain.with_agents(
        vec!["agent1".to_string(), "agent2".to_string()],
        vec![vec!["model1".to_string()], vec!["model2".to_string()]],
        crate::agents::AgentRole::Reviewer,
    );
    agent_chain = agent_chain.switch_to_next_agent(); // Move to agent 1
    agent_chain.retry_cycle = 2; // Manually set retry_cycle to verify preservation

    let state = PipelineState {
        review_issues_found: true, // Should be cleared by ReviewPassStarted
        agent_chain,
        ..base_state
    };

    assert_eq!(state.agent_chain.current_agent_index, 1);
    assert_eq!(state.agent_chain.retry_cycle, 2);
    assert!(state.review_issues_found);

    let new_state = reduce(state, PipelineEvent::review_pass_started(2));

    // Pass should be set
    assert_eq!(new_state.reviewer_pass, 2);

    // CRITICAL: review_issues_found should be reset to false
    assert!(!new_state.review_issues_found);

    // Agent chain should be reset (indices to 0, but retry_cycle preserved)
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
    assert_eq!(new_state.agent_chain.retry_cycle, 2); // Preserved, not reset
}

#[test]
fn test_review_pass_started_clears_issues_flag() {
    let state = PipelineState {
        review_issues_found: true,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::review_pass_started(0));

    assert!(!new_state.review_issues_found);
}

#[test]
fn test_review_completed_with_no_issues_increments_pass() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 3,
        review_issues_found: false,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::review_completed(0, false));

    assert_eq!(new_state.reviewer_pass, 1);
    assert!(!new_state.review_issues_found);
}

#[test]
fn test_review_completed_with_issues_stays_on_same_pass() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 3,
        review_issues_found: false,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::review_completed(0, true));

    // Should stay on pass 0 to allow fix attempt
    assert_eq!(new_state.reviewer_pass, 0);
    assert!(new_state.review_issues_found);
    assert_eq!(new_state.phase, PipelinePhase::Review);
}

#[test]
fn test_review_completed_on_last_pass_with_no_issues_transitions_to_commit() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 1,
        total_reviewer_passes: 2,
        review_issues_found: false,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::review_completed(1, false));

    // 1 + 1 = 2, 2 >= 2, should transition to CommitMessage
    assert_eq!(new_state.reviewer_pass, 2);
    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
}

#[test]
fn test_review_completed_on_last_pass_with_issues_stays_in_review() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 1,
        total_reviewer_passes: 2,
        review_issues_found: false,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::review_completed(1, true));

    // Should stay on pass 1 for fix attempt
    assert_eq!(new_state.reviewer_pass, 1);
    assert_eq!(new_state.phase, PipelinePhase::Review);
    assert!(new_state.review_issues_found);
}

#[test]
fn test_fix_attempt_started_resets_agent_chain() {
    let base_state = create_test_state();
    // Setup with non-zero agent chain state and retry_cycle
    let mut agent_chain = base_state.agent_chain.with_agents(
        vec!["agent1".to_string(), "agent2".to_string()],
        vec![vec!["model1".to_string()], vec!["model2".to_string()]],
        crate::agents::AgentRole::Reviewer,
    );
    agent_chain = agent_chain.switch_to_next_agent(); // Move to agent 1
    agent_chain.retry_cycle = 3; // Manually set retry_cycle to verify reset

    let state = PipelineState {
        review_issues_found: true,
        agent_chain,
        ..base_state
    };

    assert_eq!(state.agent_chain.current_agent_index, 1);
    assert_eq!(state.agent_chain.retry_cycle, 3);
    assert!(state.review_issues_found);

    let new_state = reduce(state.clone(), PipelineEvent::fix_attempt_started(0));

    // Phase and reviewer pass should be preserved
    assert_eq!(new_state.phase, state.phase);
    assert_eq!(new_state.reviewer_pass, state.reviewer_pass);

    // CRITICAL: review_issues_found should be preserved (not reset)
    assert!(new_state.review_issues_found);

    // Agent chain should be reset for developer role
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
    assert_eq!(new_state.agent_chain.retry_cycle, 0);
    assert_eq!(
        new_state.agent_chain.current_role,
        crate::agents::AgentRole::Developer
    );
    assert!(
        new_state.agent_chain.agents.is_empty(),
        "Expected agent chain to be cleared for re-initialization"
    );
}

#[test]
fn test_fix_attempt_completed_clears_issues_flag() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        review_issues_found: true,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::fix_attempt_completed(0, true));

    assert!(!new_state.review_issues_found);
}

#[test]
fn test_fix_attempt_completed_on_mid_pass_increments_and_stays_in_review() {
    // Fix attempt -> CommitMessage -> back to Review (more passes to do)
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        review_issues_found: true,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::fix_attempt_completed(0, true));

    // After fix, go to CommitMessage (don't increment yet)
    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    assert_eq!(new_state.reviewer_pass, 0);
    assert!(!new_state.review_issues_found); // Flag cleared after fix

    // After commit, go back to Review and increment
    let new_state = reduce(
        new_state,
        PipelineEvent::commit_created("abc".to_string(), "fix".to_string()),
    );

    assert_eq!(new_state.phase, PipelinePhase::Review);
    assert_eq!(new_state.reviewer_pass, 1);
}

#[test]
fn test_fix_attempt_completed_on_last_pass_transitions_to_commit() {
    // Last fix attempt -> CommitMessage -> FinalValidation (all passes done)
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 1,
        total_reviewer_passes: 2,
        review_issues_found: true,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::fix_attempt_completed(1, true));

    // After fix, go to CommitMessage (don't increment yet)
    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
    assert_eq!(new_state.reviewer_pass, 1);

    // After commit, all passes done -> go to FinalValidation
    let new_state = reduce(
        new_state,
        PipelineEvent::commit_created("abc".to_string(), "fix".to_string()),
    );

    assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
    assert_eq!(new_state.reviewer_pass, 2);
}

#[test]
fn test_review_phase_completed_transitions_to_commit_message() {
    let state = create_state_in_phase(PipelinePhase::Review);
    let new_state = reduce(state, PipelineEvent::review_phase_completed(false));

    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
}

#[test]
fn test_review_phase_completed_with_early_exit_transitions_to_commit_message() {
    let state = create_state_in_phase(PipelinePhase::Review);
    let new_state = reduce(state, PipelineEvent::review_phase_completed(true));

    // Even with early_exit, should still transition to CommitMessage
    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
}

#[test]
fn test_review_pass_started_with_large_pass_number() {
    let state = create_test_state();
    let new_state = reduce(state, PipelineEvent::review_pass_started(999));

    assert_eq!(new_state.reviewer_pass, 999);
}

#[test]
fn test_review_completed_increments_large_pass_number() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 999,
        total_reviewer_passes: 1001,
        review_issues_found: false,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::review_completed(999, false));

    // Should increment to 1000
    assert_eq!(new_state.reviewer_pass, 1000);
    assert_eq!(new_state.phase, PipelinePhase::Review); // Not done yet (1000 < 1001)
}

// =========================================================================
// PassCompletedClean and OutputValidationFailed event tests
// =========================================================================

#[test]
fn test_review_pass_completed_clean_increments_pass() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 3,
        review_issues_found: false,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::review_pass_completed_clean(0));

    assert_eq!(new_state.reviewer_pass, 1);
    assert!(!new_state.review_issues_found);
}

#[test]
fn test_review_pass_completed_clean_on_last_pass_transitions_to_commit() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 2,
        total_reviewer_passes: 3,
        review_issues_found: false,
        ..create_test_state()
    };
    let new_state = reduce(state, PipelineEvent::review_pass_completed_clean(2));

    // 2 + 1 = 3, 3 >= 3, should transition to CommitMessage
    assert_eq!(new_state.reviewer_pass, 3);
    assert_eq!(new_state.phase, PipelinePhase::CommitMessage);
}

#[test]
fn test_review_output_validation_failed_retries_within_limit() {
    let agent_chain = crate::reducer::state::AgentChainState::initial().with_agents(
        vec!["agent1".to_string(), "agent2".to_string()],
        vec![vec![], vec![]],
        crate::agents::AgentRole::Reviewer,
    );
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        agent_chain,
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::review_output_validation_failed(0, 0));

    assert_eq!(new_state.phase, PipelinePhase::Review);
    // Should stay on same agent when within retry limit
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
}

#[test]
fn test_review_output_validation_failed_switches_agent_at_limit() {
    let agent_chain = crate::reducer::state::AgentChainState::initial().with_agents(
        vec!["agent1".to_string(), "agent2".to_string()],
        vec![vec![], vec![]],
        crate::agents::AgentRole::Reviewer,
    );
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        agent_chain,
        ..create_test_state()
    };

    // MAX_REVIEW_INVALID_OUTPUT_RERUNS is 2, so at attempt 2 we should switch
    let new_state = reduce(state, PipelineEvent::review_output_validation_failed(0, 2));

    assert_eq!(new_state.phase, PipelinePhase::Review);
    // Should switch to next agent
    assert_eq!(
        new_state.agent_chain.current_agent_index, 1,
        "Should switch to next agent after max invalid output attempts"
    );
}

#[test]
fn test_review_output_validation_failed_stays_in_review_phase() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 1,
        ..create_test_state()
    };

    let new_state = reduce(state, PipelineEvent::review_output_validation_failed(1, 0));

    assert_eq!(
        new_state.phase,
        PipelinePhase::Review,
        "Should stay in Review phase for retry"
    );
    assert_eq!(new_state.reviewer_pass, 1, "Should preserve reviewer_pass");
}

#[test]
fn test_review_output_validation_event_sequence_retry_then_success() {
    let agent_chain = crate::reducer::state::AgentChainState::initial().with_agents(
        vec!["agent1".to_string(), "agent2".to_string()],
        vec![vec![], vec![]],
        crate::agents::AgentRole::Reviewer,
    );
    let mut state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        agent_chain,
        ..create_test_state()
    };

    // First validation failure - retry
    state = reduce(state, PipelineEvent::review_output_validation_failed(0, 0));
    assert_eq!(state.phase, PipelinePhase::Review);
    assert_eq!(state.agent_chain.current_agent_index, 0);

    // Second validation failure - retry
    state = reduce(state, PipelineEvent::review_output_validation_failed(0, 1));
    assert_eq!(state.phase, PipelinePhase::Review);
    assert_eq!(state.agent_chain.current_agent_index, 0);

    // Success after retries
    state = reduce(state, PipelineEvent::review_completed(0, false));
    assert_eq!(state.phase, PipelinePhase::Review);
    assert_eq!(state.reviewer_pass, 1);
}

// =========================================================================
// Agent chain clearing on phase transition tests (BUG: agent chain not cleared)
// =========================================================================

/// Test that verifies the agent chain is cleared when transitioning from Development
/// to Review phase via CommitCreated.
///
/// This is a regression test for the bug where the developer agent chain was carried
/// over to the Review phase, causing the wrong agent to be used for review.
#[test]
fn test_commit_created_clears_agent_chain_when_transitioning_to_review() {
    use crate::reducer::orchestration::determine_next_effect;

    // Setup: state in Development phase with developer agent chain
    let developer_chain = crate::reducer::state::AgentChainState::initial()
        .with_agents(
            vec!["dev-agent-1".to_string(), "dev-agent-2".to_string()],
            vec![vec![], vec![]],
            crate::agents::AgentRole::Developer,
        )
        .with_max_cycles(3);

    let mut state = PipelineState {
        phase: PipelinePhase::Development,
        previous_phase: Some(PipelinePhase::Development),
        iteration: 4, // Last iteration (total is 5, so 4 is index of 5th iteration)
        total_iterations: 5,
        total_reviewer_passes: 2,
        agent_chain: developer_chain,
        commit: CommitState::Generated {
            message: "test commit".to_string(),
        },
        ..create_test_state()
    };

    // Verify developer chain is populated
    assert!(!state.agent_chain.agents.is_empty());
    assert_eq!(
        state.agent_chain.current_agent(),
        Some(&"dev-agent-1".to_string())
    );
    assert_eq!(
        state.agent_chain.current_role,
        crate::agents::AgentRole::Developer
    );

    // Simulate commit created after last development iteration
    state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string()),
    );

    // After commit on last iteration, should transition to Review phase
    assert_eq!(
        state.phase,
        PipelinePhase::Review,
        "Should transition to Review phase after last development iteration commit"
    );

    // CRITICAL: The agent chain should be cleared so orchestration emits InitializeAgentChain
    assert!(
        state.agent_chain.agents.is_empty(),
        "Agent chain should be cleared when transitioning from Development to Review"
    );

    // Orchestration should now emit InitializeAgentChain for Reviewer
    let effect = determine_next_effect(&state);
    assert!(
        matches!(
            effect,
            crate::reducer::effect::Effect::InitializeAgentChain {
                role: crate::agents::AgentRole::Reviewer
            }
        ),
        "Orchestration should emit InitializeAgentChain for Reviewer, got {:?}",
        effect
    );
}

/// Test that orchestration uses the correct agent from state.agent_chain for review,
/// not ctx.reviewer_agent.
///
/// This test simulates the full flow:
/// 1. Initialize reviewer agent chain with specific agents
/// 2. Verify that RunReviewPass is emitted (not InitializeAgentChain)
/// 3. Verify the first agent in the chain is used (not a fallback)
#[test]
fn test_review_uses_agent_from_state_chain_not_context() {
    use crate::reducer::orchestration::determine_next_effect;

    // Setup: state in Review phase with reviewer agent chain already initialized
    let reviewer_chain = crate::reducer::state::AgentChainState::initial()
        .with_agents(
            vec![
                "codex".to_string(),
                "opencode".to_string(),
                "claude".to_string(),
            ],
            vec![vec![], vec![], vec![]],
            crate::agents::AgentRole::Reviewer,
        )
        .with_max_cycles(3);

    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        agent_chain: reviewer_chain,
        ..create_test_state()
    };

    // Verify chain is populated with correct first agent
    assert!(!state.agent_chain.agents.is_empty());
    assert_eq!(
        state.agent_chain.current_agent(),
        Some(&"codex".to_string()),
        "First agent should be 'codex' (from fallback chain), not 'claude'"
    );
    assert_eq!(state.agent_chain.current_agent_index, 0);

    // Orchestration should emit RunReviewPass (since chain is populated)
    let effect = determine_next_effect(&state);
    assert!(
        matches!(
            effect,
            crate::reducer::effect::Effect::RunReviewPass { pass: 0 }
        ),
        "Orchestration should emit RunReviewPass, got {:?}",
        effect
    );
}

#[test]
fn test_fix_attempt_reinitializes_chain_for_developer_role() {
    use crate::reducer::orchestration::determine_next_effect;

    let reviewer_chain = crate::reducer::state::AgentChainState::initial().with_agents(
        vec!["reviewer-1".to_string(), "reviewer-2".to_string()],
        vec![vec![], vec![]],
        crate::agents::AgentRole::Reviewer,
    );

    let state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 1,
        review_issues_found: true,
        agent_chain: reviewer_chain,
        ..create_test_state()
    };

    let effect = determine_next_effect(&state);
    assert!(
        matches!(
            effect,
            crate::reducer::effect::Effect::InitializeAgentChain {
                role: crate::agents::AgentRole::Developer
            }
        ),
        "Expected InitializeAgentChain for Developer before fix attempt, got {:?}",
        effect
    );
}

/// Test that auth failure during review advances the agent chain via events.
#[test]
fn test_auth_failure_during_review_advances_agent_chain() {
    use crate::reducer::event::AgentErrorKind;

    // Setup: state in Review phase with reviewer agent chain
    let reviewer_chain = crate::reducer::state::AgentChainState::initial()
        .with_agents(
            vec![
                "codex".to_string(),
                "opencode".to_string(),
                "claude".to_string(),
            ],
            vec![vec![], vec![], vec![]],
            crate::agents::AgentRole::Reviewer,
        )
        .with_max_cycles(3);

    let mut state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        agent_chain: reviewer_chain,
        ..create_test_state()
    };

    // Verify starting with first agent
    assert_eq!(
        state.agent_chain.current_agent(),
        Some(&"codex".to_string())
    );
    assert_eq!(state.agent_chain.current_agent_index, 0);

    // Simulate auth failure - should advance to next agent
    state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            crate::agents::AgentRole::Reviewer,
            "codex".to_string(),
            1,
            AgentErrorKind::Authentication,
            false, // Not retriable - switch to next agent
        ),
    );

    // Should have advanced to next agent
    assert_eq!(
        state.agent_chain.current_agent(),
        Some(&"opencode".to_string()),
        "Should advance to next agent after auth failure"
    );
    assert_eq!(state.agent_chain.current_agent_index, 1);

    // Simulate another auth failure
    state = reduce(
        state,
        PipelineEvent::agent_invocation_failed(
            crate::agents::AgentRole::Reviewer,
            "opencode".to_string(),
            1,
            AgentErrorKind::Authentication,
            false,
        ),
    );

    // Should have advanced to third agent
    assert_eq!(
        state.agent_chain.current_agent(),
        Some(&"claude".to_string()),
        "Should advance to third agent after second auth failure"
    );
    assert_eq!(state.agent_chain.current_agent_index, 2);
}

/// Test that after ChainInitialized, the handler can read the correct agent from state.
///
/// This test simulates what the handler does when calling run_review_pass:
/// it reads state.agent_chain.current_agent() to get the active reviewer agent.
#[test]
fn test_handler_reads_correct_agent_from_state_after_chain_initialized() {
    // Simulate the state after ChainInitialized event is processed
    let state = reduce(
        PipelineState {
            phase: PipelinePhase::Review,
            reviewer_pass: 0,
            total_reviewer_passes: 2,
            ..create_test_state()
        },
        PipelineEvent::agent_chain_initialized(
            crate::agents::AgentRole::Reviewer,
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

    // This is what the handler does: read current_agent() and pass it to run_review_pass
    let review_agent = state.agent_chain.current_agent().cloned();

    // CRITICAL: review_agent must be Some("codex"), NOT None
    assert!(
        review_agent.is_some(),
        "Handler should get Some(agent) from state.agent_chain.current_agent(), got None"
    );
    assert_eq!(
        review_agent,
        Some("codex".to_string()),
        "Handler should pass 'codex' to run_review_pass, not '{:?}'",
        review_agent
    );

    // Verify the chain is properly populated
    assert_eq!(state.agent_chain.agents.len(), 3);
    assert_eq!(state.agent_chain.current_agent_index, 0);
}

/// Test that the full pipeline flow uses the correct reviewer agent order.
///
/// This is an end-to-end test of the Development -> Review transition to verify
/// the reviewer agent chain is properly initialized.
#[test]
fn test_full_pipeline_flow_uses_correct_reviewer_agent() {
    use crate::reducer::orchestration::determine_next_effect;

    // Start with a state that simulates post-development with dev agent chain
    let dev_chain = crate::reducer::state::AgentChainState::initial()
        .with_agents(
            vec!["claude".to_string()], // Developer uses "claude"
            vec![vec![]],
            crate::agents::AgentRole::Developer,
        )
        .with_max_cycles(3);

    let mut state = PipelineState {
        phase: PipelinePhase::Development,
        previous_phase: Some(PipelinePhase::Development),
        iteration: 4, // Last iteration
        total_iterations: 5,
        total_reviewer_passes: 2,
        agent_chain: dev_chain,
        commit: CommitState::Generated {
            message: "test".to_string(),
        },
        ..create_test_state()
    };

    // Create commit to transition to Review
    state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "test".to_string()),
    );
    assert_eq!(state.phase, PipelinePhase::Review);

    // Orchestration should request agent chain initialization
    let effect = determine_next_effect(&state);
    assert!(
        matches!(
            effect,
            crate::reducer::effect::Effect::InitializeAgentChain {
                role: crate::agents::AgentRole::Reviewer
            }
        ),
        "Should request reviewer chain initialization, got {:?}",
        effect
    );

    // Simulate initializing the reviewer chain with different agents
    state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            crate::agents::AgentRole::Reviewer,
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

    // Verify reviewer chain is now populated with correct agents
    assert_eq!(
        state.agent_chain.current_agent(),
        Some(&"codex".to_string()),
        "First reviewer agent should be 'codex', not 'claude'"
    );
    assert_eq!(
        state.agent_chain.current_role,
        crate::agents::AgentRole::Reviewer
    );

    // Now orchestration should emit RunReviewPass
    let effect = determine_next_effect(&state);
    assert!(
        matches!(
            effect,
            crate::reducer::effect::Effect::RunReviewPass { pass: 0 }
        ),
        "Should emit RunReviewPass, got {:?}",
        effect
    );
}

/// Test that simulates the exact event loop behavior to verify handler state consistency.
///
/// This test simulates:
/// 1. State after ChainInitialized is processed and stored in handler
/// 2. Orchestration returns RunReviewPass
/// 3. Handler reads current_agent() from its state
///
/// The handler should have the updated state with populated agent chain.
#[test]
fn test_event_loop_state_consistency_for_review_agent() {
    use crate::reducer::orchestration::determine_next_effect;

    // === ITERATION N: InitializeAgentChain ===
    // State before InitializeAgentChain effect
    let mut state = PipelineState {
        phase: PipelinePhase::Review,
        reviewer_pass: 0,
        total_reviewer_passes: 2,
        ..create_test_state()
    };

    // Verify agent chain is empty (as it would be after dev->review transition)
    assert!(
        state.agent_chain.agents.is_empty(),
        "Agent chain should be empty before initialization"
    );

    // Orchestration should emit InitializeAgentChain
    let effect = determine_next_effect(&state);
    assert!(
        matches!(
            effect,
            crate::reducer::effect::Effect::InitializeAgentChain {
                role: crate::agents::AgentRole::Reviewer
            }
        ),
        "Expected InitializeAgentChain, got {:?}",
        effect
    );

    // Handler executes InitializeAgentChain, emits ChainInitialized event
    // (simulating what handler.initialize_agent_chain does)
    let event = PipelineEvent::agent_chain_initialized(
        crate::agents::AgentRole::Reviewer,
        vec![
            "codex".to_string(),
            "opencode".to_string(),
            "claude".to_string(),
        ],
        3,
        1000,
        2.0,
        60000,
    );

    // Event loop: reduce state with the event
    state = reduce(state, event);

    // Event loop: handler.state = new_state.clone() (simulating event loop line 194)
    // In real code, handler.state would be updated here
    let handler_state = state.clone();

    // === ITERATION N+1: RunReviewPass ===
    // Orchestration determines next effect based on updated state
    let effect = determine_next_effect(&state);
    assert!(
        matches!(
            effect,
            crate::reducer::effect::Effect::RunReviewPass { pass: 0 }
        ),
        "Expected RunReviewPass, got {:?}",
        effect
    );

    // Handler executes RunReviewPass, reads current_agent from its state
    // This is exactly what handler.run_review_pass does at line 618
    let review_agent = handler_state.agent_chain.current_agent().cloned();

    // CRITICAL ASSERTION: review_agent must be Some("codex")
    assert!(
        review_agent.is_some(),
        "Handler should get Some(agent) from state.agent_chain.current_agent(), got None. \
        This means the agent chain was not properly populated before RunReviewPass."
    );
    assert_eq!(
        review_agent,
        Some("codex".to_string()),
        "Handler should pass 'codex' to run_review_pass, got {:?}. \
        This means the wrong agent is being used.",
        review_agent
    );

    // Verify chain state is correct
    assert_eq!(handler_state.agent_chain.agents.len(), 3);
    assert_eq!(handler_state.agent_chain.current_agent_index, 0);
    assert_eq!(
        handler_state.agent_chain.current_role,
        crate::agents::AgentRole::Reviewer
    );
}

/// Full integration test: Development -> CommitMessage -> Review
///
/// This test simulates the complete flow from development through commit creation
/// to review phase, verifying that the agent chain is correctly initialized.
#[test]
fn test_complete_flow_dev_commit_review_uses_correct_reviewer_agent() {
    use crate::reducer::orchestration::determine_next_effect;

    // Start with development phase, last iteration, with developer agent chain
    let dev_chain = crate::reducer::state::AgentChainState::initial()
        .with_agents(
            vec!["claude".to_string()], // Developer uses "claude"
            vec![vec![]],
            crate::agents::AgentRole::Developer,
        )
        .with_max_cycles(3);

    let mut state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 4, // Last iteration (0-indexed, total is 5)
        total_iterations: 5,
        total_reviewer_passes: 2,
        agent_chain: dev_chain.clone(),
        ..create_test_state()
    };

    // === STEP 1: Development completes successfully ===
    state = reduce(
        state,
        PipelineEvent::development_iteration_completed(4, true),
    );
    assert_eq!(
        state.phase,
        PipelinePhase::CommitMessage,
        "Should transition to CommitMessage after successful dev iteration"
    );
    assert_eq!(
        state.previous_phase,
        Some(PipelinePhase::Development),
        "previous_phase should be Development"
    );
    // Agent chain should still have developer agents at this point
    assert!(!state.agent_chain.agents.is_empty());

    // === STEP 2: Commit message generated ===
    state = reduce(
        state,
        PipelineEvent::commit_message_generated("test commit".to_string(), 0),
    );
    assert_eq!(state.phase, PipelinePhase::CommitMessage);
    assert!(matches!(
        state.commit,
        crate::reducer::state::CommitState::Generated { .. }
    ));

    // === STEP 3: Commit created ===
    state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "test commit".to_string()),
    );

    // After commit on last iteration, should transition to Review
    assert_eq!(
        state.phase,
        PipelinePhase::Review,
        "Should transition to Review after last dev iteration commit"
    );

    // CRITICAL: Agent chain should be CLEARED to force reinitialization
    assert!(
        state.agent_chain.agents.is_empty(),
        "Agent chain should be empty after dev->review transition, got {:?}",
        state.agent_chain.agents
    );

    // === STEP 4: Orchestration cleans continuation context if needed ===
    let mut effect = determine_next_effect(&state);
    if matches!(
        effect,
        crate::reducer::effect::Effect::CleanupContinuationContext
    ) {
        state = reduce(
            state,
            PipelineEvent::development_continuation_context_cleaned(),
        );
        effect = determine_next_effect(&state);
    }

    assert!(
        matches!(
            effect,
            crate::reducer::effect::Effect::InitializeAgentChain {
                role: crate::agents::AgentRole::Reviewer
            }
        ),
        "Orchestration should request reviewer chain initialization, got {:?}",
        effect
    );

    // === STEP 5: Agent chain initialized with reviewer agents ===
    state = reduce(
        state,
        PipelineEvent::agent_chain_initialized(
            crate::agents::AgentRole::Reviewer,
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

    // Verify reviewer chain is populated
    assert!(!state.agent_chain.agents.is_empty());
    assert_eq!(
        state.agent_chain.current_agent(),
        Some(&"codex".to_string()),
        "First reviewer agent should be 'codex'"
    );
    assert_eq!(
        state.agent_chain.current_role,
        crate::agents::AgentRole::Reviewer
    );

    // === STEP 6: Orchestration requests review pass ===
    let effect = determine_next_effect(&state);
    assert!(
        matches!(
            effect,
            crate::reducer::effect::Effect::RunReviewPass { pass: 0 }
        ),
        "Should request review pass, got {:?}",
        effect
    );

    // === STEP 7: Simulate what handler does ===
    // Handler reads current_agent from state to pass to run_review_pass
    let review_agent = state.agent_chain.current_agent().cloned();
    assert_eq!(
        review_agent,
        Some("codex".to_string()),
        "Handler should pass 'codex' to run_review_pass"
    );
}
