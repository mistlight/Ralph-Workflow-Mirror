// NOTE: split from reducer/state_reduction.rs.

use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_commit_event(state: PipelineState, event: CommitEvent) -> PipelineState {
    match event {
        CommitEvent::GenerationStarted => PipelineState {
            commit: CommitState::Generating {
                attempt: 1,
                max_attempts: crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS,
            },
            commit_prompt_prepared: false,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            ..state
        },
        CommitEvent::DiffPrepared {
            empty,
            content_id_sha256,
        } => PipelineState {
            commit_diff_prepared: true,
            commit_diff_empty: empty,
            commit_diff_content_id_sha256: Some(content_id_sha256),
            // If the diff is (re)prepared, any previously materialized commit inputs
            // are potentially stale (the diff file was rewritten). Force rematerialization.
            prompt_inputs: PromptInputsState {
                commit: None,
                ..state.prompt_inputs.clone()
            },
            ..state
        },
        CommitEvent::DiffFailed { .. } => PipelineState {
            phase: crate::reducer::event::PipelinePhase::Interrupted,
            commit_diff_prepared: false,
            commit_diff_empty: false,
            commit_diff_content_id_sha256: None,
            ..state
        },
        CommitEvent::DiffInvalidated { .. } => PipelineState {
            commit_diff_prepared: false,
            commit_diff_empty: false,
            commit_diff_content_id_sha256: None,
            commit_prompt_prepared: false,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            prompt_inputs: PromptInputsState {
                commit: None,
                ..state.prompt_inputs.clone()
            },
            ..state
        },
        CommitEvent::PromptPrepared { .. } => PipelineState {
            commit: match state.commit {
                CommitState::NotStarted => CommitState::Generating {
                    attempt: 1,
                    max_attempts: crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS,
                },
                _ => state.commit.clone(),
            },
            commit_prompt_prepared: true,
            continuation: crate::reducer::state::ContinuationState {
                xsd_retry_pending: false,
                xsd_retry_session_reuse_pending: state.continuation.xsd_retry_pending,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation
            },
            ..state
        },
        CommitEvent::AgentInvoked { .. } => PipelineState {
            commit_agent_invoked: true,
            continuation: crate::reducer::state::ContinuationState {
                xsd_retry_pending: false,
                xsd_retry_session_reuse_pending: false,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                last_xsd_error: None,
                ..state.continuation
            },
            ..state
        },
        CommitEvent::CommitXmlCleaned { .. } => PipelineState {
            commit_xml_cleaned: true,
            ..state
        },
        CommitEvent::CommitXmlExtracted { .. } => PipelineState {
            commit_xml_extracted: true,
            ..state
        },
        CommitEvent::CommitXmlMissing { attempt } => PipelineState {
            commit_xml_extracted: true,
            commit_validated_outcome: Some(crate::reducer::state::CommitValidatedOutcome {
                attempt,
                message: None,
                reason: Some("Commit XML missing".to_string()),
            }),
            ..state
        },
        CommitEvent::CommitXmlValidated { message, attempt } => PipelineState {
            commit_validated_outcome: Some(crate::reducer::state::CommitValidatedOutcome {
                attempt,
                message: Some(message),
                reason: None,
            }),
            ..state
        },
        CommitEvent::CommitXmlValidationFailed { reason, attempt } => PipelineState {
            commit_validated_outcome: Some(crate::reducer::state::CommitValidatedOutcome {
                attempt,
                message: None,
                reason: Some(reason),
            }),
            ..state
        },
        CommitEvent::CommitXmlArchived { .. } => PipelineState {
            commit_xml_archived: true,
            ..state
        },
        CommitEvent::MessageGenerated { message, .. } => PipelineState {
            commit: CommitState::Generated { message },
            ..state
        },
        CommitEvent::Created { hash, .. } => {
            let (next_phase, next_iter, next_reviewer_pass) =
                compute_post_commit_transition(&state);
            // When transitioning from Development to Review, clear the agent chain
            // so orchestration will emit InitializeAgentChain for Reviewer role.
            // This ensures the reviewer fallback chain is used, not the developer chain.
            let agent_chain = if next_phase == crate::reducer::event::PipelinePhase::Review
                && state.previous_phase == Some(crate::reducer::event::PipelinePhase::Development)
            {
                crate::reducer::state::AgentChainState::initial()
                    .with_max_cycles(state.agent_chain.max_cycles)
                    .with_backoff_policy(
                        state.agent_chain.retry_delay_ms,
                        state.agent_chain.backoff_multiplier,
                        state.agent_chain.max_backoff_ms,
                    )
                    .reset_for_role(crate::agents::AgentRole::Reviewer)
            } else {
                state.agent_chain.clone()
            };

            let continuation = if next_phase == crate::reducer::event::PipelinePhase::Planning {
                ContinuationState {
                    invalid_output_attempts: 0,
                    ..state.continuation
                }
            } else {
                state.continuation.clone()
            };
            PipelineState {
                commit: CommitState::Committed { hash },
                phase: next_phase,
                previous_phase: None,
                iteration: next_iter,
                reviewer_pass: next_reviewer_pass,
                context_cleaned: false,
                commit_xml_cleaned: false,
                agent_chain,
                continuation,
                ..state
            }
        }
        CommitEvent::GenerationFailed { .. } => PipelineState {
            commit: CommitState::NotStarted,
            commit_prompt_prepared: false,
            commit_diff_prepared: false,
            commit_diff_empty: false,
            commit_diff_content_id_sha256: None,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            ..state
        },
        CommitEvent::Skipped { .. } => {
            let (next_phase, next_iter, next_reviewer_pass) =
                compute_post_commit_transition(&state);
            // When transitioning from Development to Review, clear the agent chain
            // so orchestration will emit InitializeAgentChain for Reviewer role.
            // This ensures the reviewer fallback chain is used, not the developer chain.
            let agent_chain = if next_phase == crate::reducer::event::PipelinePhase::Review
                && state.previous_phase == Some(crate::reducer::event::PipelinePhase::Development)
            {
                crate::reducer::state::AgentChainState::initial()
                    .with_max_cycles(state.agent_chain.max_cycles)
                    .with_backoff_policy(
                        state.agent_chain.retry_delay_ms,
                        state.agent_chain.backoff_multiplier,
                        state.agent_chain.max_backoff_ms,
                    )
                    .reset_for_role(crate::agents::AgentRole::Reviewer)
            } else {
                state.agent_chain.clone()
            };

            let continuation = if next_phase == crate::reducer::event::PipelinePhase::Planning {
                ContinuationState {
                    invalid_output_attempts: 0,
                    ..state.continuation
                }
            } else {
                state.continuation.clone()
            };
            PipelineState {
                commit: CommitState::Skipped,
                phase: next_phase,
                previous_phase: None,
                iteration: next_iter,
                reviewer_pass: next_reviewer_pass,
                commit_prompt_prepared: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                context_cleaned: false,
                agent_chain,
                continuation,
                ..state
            }
        }
        CommitEvent::MessageValidationFailed { attempt, reason } => {
            reduce_commit_validation_failed(state, attempt, reason)
        }
    }
}

/// Compute phase transition after a commit (used by CommitCreated and CommitSkipped).
fn compute_post_commit_transition(
    state: &PipelineState,
) -> (crate::reducer::event::PipelinePhase, u32, u32) {
    match state.previous_phase {
        Some(crate::reducer::event::PipelinePhase::Development) => {
            let next_iter = state.iteration + 1;
            if next_iter >= state.total_iterations {
                if state.total_reviewer_passes == 0 {
                    (
                        crate::reducer::event::PipelinePhase::FinalValidation,
                        next_iter,
                        state.reviewer_pass,
                    )
                } else {
                    (
                        crate::reducer::event::PipelinePhase::Review,
                        next_iter,
                        state.reviewer_pass,
                    )
                }
            } else {
                (
                    crate::reducer::event::PipelinePhase::Planning,
                    next_iter,
                    state.reviewer_pass,
                )
            }
        }
        Some(crate::reducer::event::PipelinePhase::Review) => {
            let next_pass = state.reviewer_pass + 1;
            if next_pass >= state.total_reviewer_passes {
                (
                    crate::reducer::event::PipelinePhase::FinalValidation,
                    state.iteration,
                    next_pass,
                )
            } else {
                (
                    crate::reducer::event::PipelinePhase::Review,
                    state.iteration,
                    next_pass,
                )
            }
        }
        _ => (
            crate::reducer::event::PipelinePhase::FinalValidation,
            state.iteration,
            state.reviewer_pass,
        ),
    }
}

/// Handle commit message validation failure with XSD retry logic.
///
/// This now integrates with the XSD retry tracking in ContinuationState
/// for uniformity with other phases.
fn reduce_commit_validation_failed(
    state: PipelineState,
    attempt: u32,
    reason: String,
) -> PipelineState {
    let new_xsd_count = state.continuation.xsd_retry_count + 1;
    let max_attempts = crate::reducer::state::MAX_VALIDATION_RETRY_ATTEMPTS;

    // Check if XSD retries are exhausted (configured limit) or global safety limit hit.
    //
    // NOTE: Commit XSD retries intentionally reuse the same commit attempt number so we
    // can safely reuse attempt-scoped materialized inputs (diff, references, etc.).
    if new_xsd_count >= state.continuation.max_xsd_retry_count || new_xsd_count >= max_attempts {
        // XSD retries exhausted - switch to next agent
        let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();

        // Check if we successfully advanced to next agent
        let advanced = new_agent_chain.current_agent_index != state.agent_chain.current_agent_index
            && new_agent_chain.retry_cycle == state.agent_chain.retry_cycle;

        if advanced {
            // Reset for new agent
            PipelineState {
                agent_chain: new_agent_chain,
                commit: CommitState::Generating {
                    attempt: 1,
                    max_attempts,
                },
                commit_prompt_prepared: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                continuation: crate::reducer::state::ContinuationState {
                    xsd_retry_count: 0,
                    xsd_retry_pending: false,
                    xsd_retry_session_reuse_pending: false,
                    same_agent_retry_count: 0,
                    same_agent_retry_pending: false,
                    same_agent_retry_reason: None,
                    last_xsd_error: None,
                    ..state.continuation
                },
                ..state
            }
        } else {
            // All agents exhausted - reset so orchestration can handle
            PipelineState {
                agent_chain: new_agent_chain,
                commit: CommitState::NotStarted,
                commit_prompt_prepared: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                continuation: crate::reducer::state::ContinuationState {
                    xsd_retry_count: 0,
                    xsd_retry_pending: false,
                    xsd_retry_session_reuse_pending: false,
                    same_agent_retry_count: 0,
                    same_agent_retry_pending: false,
                    same_agent_retry_reason: None,
                    last_xsd_error: None,
                    ..state.continuation
                },
                ..state
            }
        }
    } else {
        // Set XSD retry pending - orchestration will trigger retry with same agent/session
        PipelineState {
            commit: CommitState::Generating {
                attempt,
                max_attempts,
            },
            commit_prompt_prepared: false,
            commit_agent_invoked: false,
            commit_xml_cleaned: false,
            commit_xml_extracted: false,
            commit_validated_outcome: None,
            commit_xml_archived: false,
            continuation: crate::reducer::state::ContinuationState {
                xsd_retry_count: new_xsd_count,
                xsd_retry_pending: true,
                xsd_retry_session_reuse_pending: false,
                last_xsd_error: Some(reason),
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation
            },
            ..state
        }
    }
}
