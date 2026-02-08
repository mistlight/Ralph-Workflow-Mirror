//! Commit phase effect-to-event mapping.
//!
//! This module handles effect execution for the Commit phase of the pipeline.
//! Commit involves generating a commit message and creating the git commit.
//!
//! ## Commit Phase Flow
//!
//! 1. **CheckCommitDiff** - Verify there are changes to commit
//! 2. **MaterializeCommitInputs** - Prepare diff input for commit agent
//! 3. **PrepareCommitPrompt** - Generate commit prompt
//! 4. **InvokeCommitAgent** - Execute commit agent
//! 5. **CleanupCommitXml** - Clean any existing XML
//! 6. **ExtractCommitXml** - Extract XML from agent output
//! 7. **ValidateCommitXml** - Validate and parse commit message
//! 8. **ApplyCommitMessageOutcome** - Apply commit message to state
//! 9. **ArchiveCommitXml** - Archive XML
//! 10. **CreateCommit** or **SkipCommit** - Create git commit or skip if no changes
//!
//! ## Rebase Support
//!
//! Before commit, the pipeline may rebase onto a target branch:
//! - **RunRebase** - Rebase onto target branch
//! - **ResolveRebaseConflicts** - Resolve conflicts if rebase fails
//!
//! ## Mock Behavior
//!
//! - Mock always returns a valid commit message
//! - **CheckCommitDiff** can be configured to simulate empty diff (for testing skip logic)
//! - **CreateCommit** returns a fake commit hash
//! - **RunRebase** always succeeds with a fake head OID

use crate::reducer::effect::Effect;
use crate::reducer::event::{PipelineEvent, PipelinePhase};
use crate::reducer::state::{
    CommitState, MaterializedPromptInput, PromptInputKind, PromptInputRepresentation,
    PromptMaterializationReason,
};
use crate::reducer::ui_event::{UIEvent, XmlOutputType};

use super::super::MockEffectHandler;

impl MockEffectHandler {
    /// Handle commit phase effects.
    ///
    /// Returns appropriate mock events for each commit effect without
    /// performing real agent execution, XML validation, or git operations.
    pub(super) fn handle_commit_effect(
        &mut self,
        effect: Effect,
    ) -> Option<(PipelineEvent, Vec<UIEvent>)> {
        match effect {
            Effect::RunRebase {
                phase,
                target_branch: _,
            } => Some((
                PipelineEvent::rebase_succeeded(phase, "mock_head_abc123".to_string()),
                vec![],
            )),

            Effect::ResolveRebaseConflicts { strategy: _ } => {
                Some((PipelineEvent::rebase_conflict_resolved(vec![]), vec![]))
            }

            Effect::CheckCommitDiff => {
                let empty = self.simulate_empty_diff;
                Some((
                    PipelineEvent::commit_diff_prepared(empty, "id".to_string()),
                    vec![],
                ))
            }

            Effect::MaterializeCommitInputs { attempt } => Some((
                PipelineEvent::commit_inputs_materialized(
                    attempt,
                    MaterializedPromptInput {
                        kind: PromptInputKind::Diff,
                        content_id_sha256: "id".to_string(),
                        consumer_signature_sha256: self
                            .state
                            .agent_chain
                            .consumer_signature_sha256(),
                        original_bytes: 1,
                        final_bytes: 1,
                        model_budget_bytes: None,
                        inline_budget_bytes: None,
                        representation: PromptInputRepresentation::Inline,
                        reason: PromptMaterializationReason::WithinBudgets,
                    },
                ),
                vec![],
            )),

            Effect::PrepareCommitPrompt { prompt_mode: _ } => {
                let attempt = match self.state.commit {
                    CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                let ui = vec![UIEvent::PhaseTransition {
                    from: Some(self.state.phase),
                    to: PipelinePhase::CommitMessage,
                }];
                Some((PipelineEvent::commit_prompt_prepared(attempt), ui))
            }

            Effect::InvokeCommitAgent => {
                let attempt = match self.state.commit {
                    CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                Some((PipelineEvent::commit_agent_invoked(attempt), vec![]))
            }

            Effect::CleanupCommitXml => {
                let attempt = match self.state.commit {
                    CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                Some((PipelineEvent::commit_xml_cleaned(attempt), vec![]))
            }

            Effect::ExtractCommitXml => {
                let attempt = match self.state.commit {
                    CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                Some((PipelineEvent::commit_xml_extracted(attempt), vec![]))
            }

            Effect::ValidateCommitXml => {
                let attempt = match self.state.commit {
                    CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                let mock_commit_xml = r#"<ralph-commit>
<ralph-subject>feat: mock commit message for testing</ralph-subject>
<ralph-body>This is a mock commit body generated for testing purposes.

- Changed some files
- Added new features</ralph-body>
</ralph-commit>"#;
                let ui = vec![UIEvent::XmlOutput {
                    xml_type: XmlOutputType::CommitMessage,
                    content: mock_commit_xml.to_string(),
                    context: None,
                }];
                Some((
                    PipelineEvent::commit_xml_validated("mock commit message".to_string(), attempt),
                    ui,
                ))
            }

            Effect::ApplyCommitMessageOutcome => {
                let event = match self.state.commit_validated_outcome.as_ref() {
                    Some(outcome) => match (&outcome.message, &outcome.reason) {
                        (Some(message), _) => PipelineEvent::commit_message_generated(
                            message.clone(),
                            outcome.attempt,
                        ),
                        (None, Some(reason)) => PipelineEvent::commit_message_validation_failed(
                            reason.clone(),
                            outcome.attempt,
                        ),
                        _ => PipelineEvent::commit_generation_failed(
                            "Mock commit outcome missing message and reason".to_string(),
                        ),
                    },
                    None => PipelineEvent::commit_generation_failed(
                        "Mock commit outcome missing".to_string(),
                    ),
                };
                Some((event, vec![]))
            }

            Effect::ArchiveCommitXml => {
                let attempt = match self.state.commit {
                    CommitState::Generating { attempt, .. } => attempt,
                    _ => 1,
                };
                Some((PipelineEvent::commit_xml_archived(attempt), vec![]))
            }

            Effect::CreateCommit { message } => Some((
                PipelineEvent::commit_created("mock_commit_hash_abc123".to_string(), message),
                vec![],
            )),

            Effect::SkipCommit { reason } => Some((PipelineEvent::commit_skipped(reason), vec![])),

            _ => None,
        }
    }
}
