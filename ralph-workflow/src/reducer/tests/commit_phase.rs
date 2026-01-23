//! Tests for commit phase events (generation, validation, agent fallback).
//!
//! These tests validate the critical commit validation agent fallback fix
//! that prevents infinite loops when validation fails.

use super::*;
use crate::agents::AgentRole;
use crate::reducer::event::CheckpointTrigger;
use crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS;

#[test]
fn test_commit_generation_started_is_noop() {
    let state = create_test_state();
    let new_state = reduce(state.clone(), PipelineEvent::CommitGenerationStarted);

    assert_eq!(new_state.phase, state.phase);
}

#[test]
fn test_commit_message_generated_sets_commit_to_generated() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::CommitMessageGenerated {
            message: "feat: add feature".to_string(),
            attempt: 1,
        },
    );

    assert!(matches!(new_state.commit, CommitState::Generated { .. }));
}

#[test]
fn test_commit_message_generated_stores_message() {
    let state = create_test_state();
    let message = "fix: resolve bug".to_string();
    let new_state = reduce(
        state,
        PipelineEvent::CommitMessageGenerated {
            message: message.clone(),
            attempt: 1,
        },
    );

    if let CommitState::Generated {
        message: stored_msg,
    } = new_state.commit
    {
        assert_eq!(stored_msg, message);
    } else {
        panic!("Expected CommitState::Generated");
    }
}

#[test]
fn test_commit_created_sets_commit_to_committed() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::CommitCreated {
            hash: "abc123".to_string(),
            message: "feat: test".to_string(),
        },
    );

    assert!(matches!(new_state.commit, CommitState::Committed { .. }));
}

#[test]
fn test_commit_created_transitions_to_final_validation() {
    let state = create_state_in_phase(PipelinePhase::CommitMessage);
    let new_state = reduce(
        state,
        PipelineEvent::CommitCreated {
            hash: "abc123".to_string(),
            message: "feat: test".to_string(),
        },
    );

    assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
}

#[test]
fn test_commit_created_stores_hash() {
    let state = create_test_state();
    let hash = "abc123def456".to_string();
    let new_state = reduce(
        state,
        PipelineEvent::CommitCreated {
            hash: hash.clone(),
            message: "test".to_string(),
        },
    );

    if let CommitState::Committed { hash: stored_hash } = new_state.commit {
        assert_eq!(stored_hash, hash);
    } else {
        panic!("Expected CommitState::Committed");
    }
}

#[test]
fn test_commit_message_validation_failed_retries() {
    let state = PipelineState {
        commit: CommitState::Generated {
            message: "test".to_string(),
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::CommitMessageValidationFailed {
            reason: "Invalid format".to_string(),
            attempt: 1,
        },
    );

    // Should retry with incremented attempt
    assert!(matches!(
        new_state.commit,
        CommitState::Generating { attempt: 2, .. }
    ));
}

#[test]
fn test_commit_message_validation_failed_exhausts_attempts_with_more_agents() {
    // Setup: 3 commit agents available
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state.agent_chain.with_agents(
            vec![
                "commit-agent-1".to_string(),
                "commit-agent-2".to_string(),
                "commit-agent-3".to_string(),
            ],
            vec![vec![], vec![], vec![]],
            AgentRole::Commit,
        ),
        commit: CommitState::Generating {
            attempt: MAX_VALIDATION_RETRY_ATTEMPTS,
            max_attempts: MAX_VALIDATION_RETRY_ATTEMPTS,
        },
        ..base_state
    };

    let new_state = reduce(
        state,
        PipelineEvent::CommitMessageValidationFailed {
            reason: "Invalid format".to_string(),
            attempt: MAX_VALIDATION_RETRY_ATTEMPTS,
        },
    );

    // Should advance to next agent and reset to attempt 1
    assert_eq!(new_state.agent_chain.current_agent_index, 1);
    assert!(matches!(
        new_state.commit,
        CommitState::Generating { attempt: 1, .. }
    ));
}

#[test]
fn test_commit_message_validation_failed_exhausts_all_agents() {
    // Setup: On last agent (index 2 of 3 agents)
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state
            .agent_chain
            .with_agents(
                vec![
                    "commit-agent-1".to_string(),
                    "commit-agent-2".to_string(),
                    "commit-agent-3".to_string(),
                ],
                vec![vec![], vec![], vec![]],
                AgentRole::Commit,
            )
            .switch_to_next_agent()
            .switch_to_next_agent(), // Move to last agent (index 2)
        commit: CommitState::Generating {
            attempt: MAX_VALIDATION_RETRY_ATTEMPTS,
            max_attempts: MAX_VALIDATION_RETRY_ATTEMPTS,
        },
        ..base_state
    };

    // Verify we're on the last agent and retry_cycle is 0
    assert_eq!(state.agent_chain.current_agent_index, 2);
    assert_eq!(state.agent_chain.retry_cycle, 0);

    let new_state = reduce(
        state,
        PipelineEvent::CommitMessageValidationFailed {
            reason: "Invalid format".to_string(),
            attempt: MAX_VALIDATION_RETRY_ATTEMPTS,
        },
    );

    // When we try to advance from last agent, switch_to_next_agent() wraps around:
    // - Index wraps back to 0
    // - Retry cycle increments to 1
    assert_eq!(new_state.agent_chain.current_agent_index, 0);
    assert_eq!(new_state.agent_chain.retry_cycle, 1);

    // Since we wrapped around (exhausted all agents in this cycle), should give up
    assert!(matches!(new_state.commit, CommitState::NotStarted));
}

#[test]
fn test_commit_message_validation_failed_with_single_agent() {
    // Setup: Only 1 commit agent
    let base_state = create_test_state();
    let state = PipelineState {
        agent_chain: base_state.agent_chain.with_agents(
            vec!["commit-agent-1".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        commit: CommitState::Generating {
            attempt: MAX_VALIDATION_RETRY_ATTEMPTS,
            max_attempts: MAX_VALIDATION_RETRY_ATTEMPTS,
        },
        ..base_state
    };

    let new_state = reduce(
        state,
        PipelineEvent::CommitMessageValidationFailed {
            reason: "Invalid format".to_string(),
            attempt: MAX_VALIDATION_RETRY_ATTEMPTS,
        },
    );

    // No more agents to fallback to - should give up
    assert!(matches!(new_state.commit, CommitState::NotStarted));
}

#[test]
fn test_commit_skipped_sets_commit_to_skipped() {
    let state = create_test_state();
    let new_state = reduce(
        state,
        PipelineEvent::CommitSkipped {
            reason: "No changes".to_string(),
        },
    );

    assert!(matches!(new_state.commit, CommitState::Skipped));
}

#[test]
fn test_commit_skipped_transitions_to_final_validation() {
    let state = create_state_in_phase(PipelinePhase::CommitMessage);
    let new_state = reduce(
        state,
        PipelineEvent::CommitSkipped {
            reason: "No changes".to_string(),
        },
    );

    assert_eq!(new_state.phase, PipelinePhase::FinalValidation);
}

#[test]
fn test_commit_generation_failed_resets_commit_to_not_started() {
    let state = PipelineState {
        commit: CommitState::Generating {
            attempt: 5,
            max_attempts: 10,
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state,
        PipelineEvent::CommitGenerationFailed {
            reason: "Agent failed to generate valid commit message".to_string(),
        },
    );

    assert!(matches!(new_state.commit, CommitState::NotStarted));
}

#[test]
fn test_checkpoint_saved_preserves_all_state() {
    let state = PipelineState {
        phase: PipelinePhase::Development,
        iteration: 3,
        reviewer_pass: 1,
        commit: CommitState::Generated {
            message: "test".to_string(),
        },
        ..create_test_state()
    };
    let new_state = reduce(
        state.clone(),
        PipelineEvent::CheckpointSaved {
            trigger: CheckpointTrigger::PhaseTransition,
        },
    );

    assert_eq!(new_state.phase, state.phase);
    assert_eq!(new_state.iteration, state.iteration);
    assert_eq!(new_state.reviewer_pass, state.reviewer_pass);
    assert!(matches!(new_state.commit, CommitState::Generated { .. }));
}

#[test]
fn test_checkpoint_saved_with_different_triggers() {
    let state = PipelineState {
        phase: PipelinePhase::Review,
        iteration: 2,
        ..create_test_state()
    };

    // Test each checkpoint trigger type
    for trigger in [
        CheckpointTrigger::PhaseTransition,
        CheckpointTrigger::IterationComplete,
        CheckpointTrigger::BeforeRebase,
        CheckpointTrigger::Interrupt,
    ] {
        let new_state = reduce(state.clone(), PipelineEvent::CheckpointSaved { trigger });

        assert_eq!(new_state.phase, state.phase);
        assert_eq!(new_state.iteration, state.iteration);
    }
}

#[test]
fn test_commit_message_generated_increments_attempt() {
    let state = create_test_state();

    // Generate first message (attempt 1)
    let new_state = reduce(
        state,
        PipelineEvent::CommitMessageGenerated {
            message: "first".to_string(),
            attempt: 1,
        },
    );

    assert!(matches!(new_state.commit, CommitState::Generated { .. }));
    if let CommitState::Generated { message } = &new_state.commit {
        assert_eq!(message, "first");
    }

    // Generate second message (attempt 2) - overwrites previous
    let new_state2 = reduce(
        new_state,
        PipelineEvent::CommitMessageGenerated {
            message: "second".to_string(),
            attempt: 2,
        },
    );

    assert!(matches!(new_state2.commit, CommitState::Generated { .. }));
    if let CommitState::Generated { message } = &new_state2.commit {
        assert_eq!(message, "second");
    }
}
