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
    agent_chain.retry_cycle = 3; // Manually set retry_cycle to verify preservation

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

    // Agent chain should be reset (indices to 0, but retry_cycle preserved)
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.current_model_index, 0);
    assert_eq!(new_state.agent_chain.retry_cycle, 3); // Preserved, not reset
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
