// Commit phase tests.
//
// Tests for commit phase effect determination, agent chain states,
// diff checking, and commit message generation.

use super::*;

#[test]
fn test_determine_effect_commit_message_empty_chain() {
    // When agent chain is empty, commit phase should request initialization
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::NotStarted,
        agent_chain: AgentChainState::initial(),
        ..create_test_state()
    };
    let effect = determine_next_effect(&state);
    assert!(matches!(
        effect,
        Effect::InitializeAgentChain {
            role: AgentRole::Commit
        }
    ));
}

#[test]
fn test_determine_effect_commit_message_not_started() {
    // With initialized agent chain and diff prepared, commit phase should prepare prompt
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::NotStarted,
        commit_diff_prepared: true, // Diff already done
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..create_test_state()
    };
    let effect = determine_next_effect(&state);
    // Since diff is prepared but prompt is not, next step is to prepare prompt
    assert!(matches!(
        effect,
        Effect::PrepareCommitPrompt {
            prompt_mode: PromptMode::Normal
        }
    ));
}

#[test]
fn test_determine_effect_commit_message_ignores_stale_validated_outcome() {
    // Stale outcome (attempt 1) should be ignored when current attempt is 2
    // Should proceed to prepare prompt instead of applying stale outcome
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 2,
            max_attempts: 5,
        },
        commit_diff_prepared: true, // Diff already done
        commit_prompt_prepared: false,
        commit_agent_invoked: false,
        commit_xml_extracted: false,
        commit_validated_outcome: Some(crate::reducer::state::CommitValidatedOutcome {
            attempt: 1, // Stale: from attempt 1, not current attempt 2
            message: Some("stale message".to_string()),
            reason: None,
        }),
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..create_test_state()
    };

    let effect = determine_next_effect(&state);
    // Stale outcome is ignored, so proceeds to prepare prompt
    assert!(matches!(
        effect,
        Effect::PrepareCommitPrompt {
            prompt_mode: PromptMode::Normal
        }
    ));
}

#[test]
fn test_determine_effect_commit_message_generated() {
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generated {
            message: "test commit message".to_string(),
        },
        commit_xml_archived: true,
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..create_test_state()
    };
    let effect = determine_next_effect(&state);
    match effect {
        Effect::CreateCommit { message } => {
            assert_eq!(message, "test commit message");
        }
        _ => panic!("Expected CreateCommit effect, got {:?}", effect),
    }
}

#[test]
fn test_determine_effect_final_validation() {
    let state = PipelineState {
        phase: PipelinePhase::FinalValidation,
        ..create_test_state()
    };
    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::ValidateFinalState));
}

#[test]
fn test_determine_effect_complete() {
    let state = PipelineState {
        phase: PipelinePhase::Complete,
        ..create_test_state()
    };
    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::SaveCheckpoint { .. }));
}
