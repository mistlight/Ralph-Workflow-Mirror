// Commit phase orchestration tests.
//
// Tests for commit phase: agent chain initialization, diff checking,
// prompt preparation, and commit creation.

use super::*;

#[test]
fn test_commit_empty_chain_initializes_agent_chain() {
    // When agent chain is empty, commit phase should request initialization
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: crate::reducer::state::CommitState::NotStarted,
        agent_chain: crate::reducer::state::AgentChainState::initial(),
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
fn test_commit_not_started_prepares_prompt() {
    // With initialized agent chain, commit phase should prepare prompt
    use crate::reducer::state::AgentChainState;
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: crate::reducer::state::CommitState::NotStarted,
        agent_chain: AgentChainState::initial().with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..create_test_state()
    };
    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::CheckCommitDiff));
}

#[test]
fn test_commit_checks_diff_before_prompt() {
    use crate::reducer::state::AgentChainState;
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: crate::reducer::state::CommitState::NotStarted,
        agent_chain: AgentChainState::initial().with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..create_test_state()
    };

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::CheckCommitDiff));
}

#[test]
fn test_commit_skips_when_diff_empty() {
    use crate::reducer::state::AgentChainState;
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: crate::reducer::state::CommitState::NotStarted,
        commit_diff_prepared: true,
        commit_diff_empty: true,
        agent_chain: AgentChainState::initial().with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..create_test_state()
    };

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::SkipCommit { .. }));
}

#[test]
fn test_commit_does_not_apply_outcome_without_xml_extracted() {
    use crate::reducer::state::{AgentChainState, CommitValidatedOutcome};
    let mut state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: crate::reducer::state::CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        },
        commit_diff_prepared: true,
        commit_diff_empty: false,
        commit_prompt_prepared: true,
        commit_xml_cleaned: true,
        commit_agent_invoked: true,
        commit_xml_extracted: false,
        commit_validated_outcome: Some(CommitValidatedOutcome {
            attempt: 1,
            message: Some("msg".to_string()),
            reason: None,
        }),
        prompt_inputs: crate::reducer::state::PromptInputsState {
            commit: Some(crate::reducer::state::MaterializedCommitInputs {
                attempt: 1,
                diff: crate::reducer::state::MaterializedPromptInput {
                    kind: crate::reducer::state::PromptInputKind::Diff,
                    content_id_sha256: "id".to_string(),
                    consumer_signature_sha256: String::new(),
                    original_bytes: 1,
                    final_bytes: 1,
                    model_budget_bytes: None,
                    inline_budget_bytes: None,
                    representation: crate::reducer::state::PromptInputRepresentation::Inline,
                    reason: crate::reducer::state::PromptMaterializationReason::WithinBudgets,
                },
            }),
            ..Default::default()
        },
        agent_chain: AgentChainState::initial().with_agents(
            vec!["commit-agent".to_string()],
            vec![vec![]],
            AgentRole::Commit,
        ),
        ..create_test_state()
    };

    let sig = state.agent_chain.consumer_signature_sha256();
    state
        .prompt_inputs
        .commit
        .as_mut()
        .unwrap()
        .diff
        .consumer_signature_sha256 = sig;

    let effect = determine_next_effect(&state);
    assert!(matches!(effect, Effect::ExtractCommitXml));
}

#[test]
fn test_commit_generated_creates_commit() {
    use crate::reducer::state::AgentChainState;
    let state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: crate::reducer::state::CommitState::Generated {
            message: "test commit message".to_string(),
        },
        commit_xml_archived: true,
        agent_chain: AgentChainState::initial().with_agents(
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
fn test_commit_created_transitions_to_final_validation() {
    let mut state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: crate::reducer::state::CommitState::Generated {
            message: "test".to_string(),
        },
        ..create_test_state()
    };

    // Create commit
    state = reduce(
        state,
        PipelineEvent::commit_created("abc123".to_string(), "test".to_string()),
    );

    assert_eq!(state.phase, PipelinePhase::FinalValidation);
    assert!(matches!(
        state.commit,
        crate::reducer::state::CommitState::Committed { .. }
    ));
}
