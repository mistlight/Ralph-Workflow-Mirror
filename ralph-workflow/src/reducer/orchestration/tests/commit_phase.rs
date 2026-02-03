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
    assert!(matches!(effect, Effect::MaterializeCommitInputs { .. }));
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
    assert!(matches!(effect, Effect::MaterializeCommitInputs { .. }));
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
fn test_determine_effect_commit_message_rematerializes_when_consumer_signature_changes() {
    // If the consumer set (agent chain + models + role) changes mid-attempt,
    // we must re-materialize commit inputs so model budget decisions stay safe.
    let mut state = PipelineState {
        phase: PipelinePhase::CommitMessage,
        commit: CommitState::Generating {
            attempt: 1,
            max_attempts: 3,
        },
        commit_diff_prepared: true,
        commit_diff_empty: false,
        commit_prompt_prepared: false,
        agent_chain: PipelineState::initial(5, 2).agent_chain.with_agents(
            vec!["commit-agent".to_string(), "fallback-agent".to_string()],
            vec![vec!["model-a".to_string()], vec!["model-b".to_string()]],
            AgentRole::Commit,
        ),
        prompt_inputs: crate::reducer::state::PromptInputsState {
            commit: Some(crate::reducer::state::MaterializedCommitInputs {
                attempt: 1,
                diff: crate::reducer::state::MaterializedPromptInput {
                    kind: crate::reducer::state::PromptInputKind::Diff,
                    content_id_sha256: "id".to_string(),
                    consumer_signature_sha256: "stale_sig".to_string(),
                    original_bytes: 1,
                    final_bytes: 1,
                    model_budget_bytes: Some(200_000),
                    inline_budget_bytes: Some(100_000),
                    representation: crate::reducer::state::PromptInputRepresentation::Inline,
                    reason: crate::reducer::state::PromptMaterializationReason::WithinBudgets,
                },
            }),
            ..Default::default()
        },
        ..create_test_state()
    };

    // Ensure the agent chain signature is different from the stored one.
    let expected_sig = state.agent_chain.consumer_signature_sha256();
    assert_ne!(
        expected_sig, "stale_sig",
        "test setup error: consumer signature should differ"
    );

    let effect = determine_next_effect(&state);
    assert!(
        matches!(effect, Effect::MaterializeCommitInputs { attempt: 1 }),
        "Expected re-materialization when consumer signature changes, got {:?}",
        effect
    );

    // Changing current agent/model indices should not change the signature and should not
    // force re-materialization once signatures match.
    state
        .prompt_inputs
        .commit
        .as_mut()
        .unwrap()
        .diff
        .consumer_signature_sha256 = expected_sig;
    state.agent_chain.current_agent_index = 1;
    let effect = determine_next_effect(&state);
    assert!(
        !matches!(effect, Effect::MaterializeCommitInputs { .. }),
        "Expected no re-materialization when only current agent index changes, got {:?}",
        effect
    );
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
