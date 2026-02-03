// NOTE: split from reducer/state_reduction.rs.

use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_development_event(
    state: PipelineState,
    event: DevelopmentEvent,
) -> PipelineState {
    match event {
        DevelopmentEvent::PhaseStarted => PipelineState {
            phase: crate::reducer::event::PipelinePhase::Development,
            continuation: crate::reducer::state::ContinuationState {
                context_write_pending: false,
                context_cleanup_pending: false,
                ..state.continuation
            },
            development_context_prepared_iteration: None,
            development_prompt_prepared_iteration: None,
            development_xml_cleaned_iteration: None,
            development_agent_invoked_iteration: None,
            development_xml_extracted_iteration: None,
            development_validated_outcome: None,
            development_xml_archived_iteration: None,
            ..state
        },
        DevelopmentEvent::IterationStarted { iteration } => PipelineState {
            iteration,
            agent_chain: state.agent_chain.reset(),
            // Reset continuation state when starting a new iteration
            continuation: crate::reducer::state::ContinuationState {
                context_cleanup_pending: true,
                ..state.continuation.reset()
            },
            development_context_prepared_iteration: None,
            development_prompt_prepared_iteration: None,
            development_xml_cleaned_iteration: None,
            development_agent_invoked_iteration: None,
            development_xml_extracted_iteration: None,
            development_validated_outcome: None,
            development_xml_archived_iteration: None,
            ..state
        },
        DevelopmentEvent::ContextPrepared { iteration } => PipelineState {
            development_context_prepared_iteration: Some(iteration),
            // Clear continue_pending to prevent infinite loop.
            // Once context is prepared, the continuation attempt has started,
            // so we should not re-derive PrepareDevelopmentContext.
            continuation: crate::reducer::state::ContinuationState {
                continue_pending: false,
                ..state.continuation
            },
            ..state
        },
        DevelopmentEvent::PromptPrepared { iteration } => PipelineState {
            development_prompt_prepared_iteration: Some(iteration),
            continuation: crate::reducer::state::ContinuationState {
                xsd_retry_pending: false,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation
            },
            ..state
        },
        DevelopmentEvent::XmlCleaned { iteration } => PipelineState {
            development_xml_cleaned_iteration: Some(iteration),
            ..state
        },
        DevelopmentEvent::AgentInvoked { iteration } => PipelineState {
            development_agent_invoked_iteration: Some(iteration),
            continuation: crate::reducer::state::ContinuationState {
                xsd_retry_pending: false,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation
            },
            ..state
        },
        DevelopmentEvent::XmlExtracted { iteration } => PipelineState {
            development_xml_extracted_iteration: Some(iteration),
            ..state
        },
        DevelopmentEvent::XmlValidated {
            iteration,
            status,
            summary,
            files_changed,
            next_steps,
        } => PipelineState {
            development_validated_outcome: Some(
                crate::reducer::state::DevelopmentValidatedOutcome {
                    iteration,
                    status,
                    summary,
                    files_changed,
                    next_steps,
                },
            ),
            ..state
        },
        DevelopmentEvent::XmlArchived { iteration } => PipelineState {
            development_xml_archived_iteration: Some(iteration),
            ..state
        },
        DevelopmentEvent::OutcomeApplied { iteration } => {
            let Some(outcome) = state
                .development_validated_outcome
                .as_ref()
                .filter(|o| o.iteration == iteration)
            else {
                return state;
            };

            let continuation_state = &state.continuation;
            let max_continuations = continuation_state.max_continue_count.saturating_sub(1);

            let next_event = if matches!(outcome.status, DevelopmentStatus::Completed) {
                if continuation_state.is_continuation() {
                    DevelopmentEvent::ContinuationSucceeded {
                        iteration,
                        total_continuation_attempts: continuation_state.continuation_attempt,
                    }
                } else {
                    DevelopmentEvent::IterationCompleted {
                        iteration,
                        output_valid: true,
                    }
                }
            } else if continuation_state.continuation_attempt > max_continuations
                || continuation_state.continuation_attempt + 1 > max_continuations
            {
                DevelopmentEvent::ContinuationBudgetExhausted {
                    iteration,
                    total_attempts: continuation_state.continuation_attempt,
                    last_status: outcome.status.clone(),
                }
            } else {
                DevelopmentEvent::ContinuationTriggered {
                    iteration,
                    status: outcome.status.clone(),
                    summary: outcome.summary.clone(),
                    files_changed: outcome.files_changed.clone(),
                    next_steps: outcome.next_steps.clone(),
                }
            };

            reduce_development_event(state, next_event)
        }
        DevelopmentEvent::IterationCompleted {
            iteration,
            output_valid,
        } => {
            if output_valid {
                // After a successful dev iteration, go to CommitMessage phase to create a commit.
                PipelineState {
                    phase: crate::reducer::event::PipelinePhase::CommitMessage,
                    previous_phase: Some(crate::reducer::event::PipelinePhase::Development),
                    iteration,
                    commit: crate::reducer::state::CommitState::NotStarted,
                    commit_prompt_prepared: false,
                    commit_diff_prepared: false,
                    commit_diff_empty: false,
                    commit_agent_invoked: false,
                    commit_xml_cleaned: false,
                    commit_xml_extracted: false,
                    commit_validated_outcome: None,
                    commit_xml_archived: false,
                    context_cleaned: false,
                    // Reset continuation state on successful completion
                    // Use reset() to preserve configured limits (max_xsd_retry_count, etc.)
                    continuation: ContinuationState {
                        context_cleanup_pending: true,
                        ..state.continuation.reset()
                    },
                    development_context_prepared_iteration: None,
                    development_prompt_prepared_iteration: None,
                    development_xml_cleaned_iteration: None,
                    development_agent_invoked_iteration: None,
                    development_xml_extracted_iteration: None,
                    development_validated_outcome: None,
                    development_xml_archived_iteration: None,
                    ..state
                }
            } else {
                // Output was not valid enough to proceed to commit; retry in Development.
                let invalid_output_attempts = state.continuation.invalid_output_attempts + 1;
                if invalid_output_attempts > crate::reducer::state::MAX_DEV_INVALID_OUTPUT_RERUNS {
                    let new_agent_chain = state.agent_chain.switch_to_next_agent();
                    let continuation = ContinuationState {
                        invalid_output_attempts: 0,
                        ..state.continuation
                    };

                    PipelineState {
                        phase: crate::reducer::event::PipelinePhase::Development,
                        iteration,
                        continuation,
                        agent_chain: new_agent_chain,
                        development_context_prepared_iteration: None,
                        development_prompt_prepared_iteration: None,
                        development_xml_cleaned_iteration: None,
                        development_agent_invoked_iteration: None,
                        development_xml_extracted_iteration: None,
                        development_validated_outcome: None,
                        development_xml_archived_iteration: None,
                        ..state
                    }
                } else {
                    let continuation = ContinuationState {
                        invalid_output_attempts,
                        ..state.continuation
                    };

                    PipelineState {
                        phase: crate::reducer::event::PipelinePhase::Development,
                        iteration,
                        continuation,
                        development_context_prepared_iteration: None,
                        development_prompt_prepared_iteration: None,
                        development_xml_cleaned_iteration: None,
                        development_agent_invoked_iteration: None,
                        development_xml_extracted_iteration: None,
                        development_validated_outcome: None,
                        development_xml_archived_iteration: None,
                        ..state
                    }
                }
            }
        }
        DevelopmentEvent::PhaseCompleted => PipelineState {
            phase: crate::reducer::event::PipelinePhase::Review,
            // Reset continuation state when phase completes
            continuation: ContinuationState::new(),
            development_context_prepared_iteration: None,
            development_prompt_prepared_iteration: None,
            development_xml_cleaned_iteration: None,
            development_agent_invoked_iteration: None,
            development_xml_extracted_iteration: None,
            development_validated_outcome: None,
            development_xml_archived_iteration: None,
            ..state
        },
        DevelopmentEvent::ContinuationTriggered {
            iteration,
            status,
            summary,
            files_changed,
            next_steps,
        } => {
            // Trigger continuation with context from the previous attempt
            PipelineState {
                iteration,
                continuation: state.continuation.trigger_continuation(
                    status,
                    summary,
                    files_changed,
                    next_steps,
                ),
                development_context_prepared_iteration: None,
                development_prompt_prepared_iteration: None,
                development_xml_cleaned_iteration: None,
                development_agent_invoked_iteration: None,
                development_xml_extracted_iteration: None,
                development_validated_outcome: None,
                development_xml_archived_iteration: None,
                ..state
            }
        }
        DevelopmentEvent::ContinuationSucceeded {
            iteration,
            total_continuation_attempts: _,
        } => {
            // Continuation succeeded; proceed to CommitMessage and reset continuation state.
            PipelineState {
                phase: crate::reducer::event::PipelinePhase::CommitMessage,
                previous_phase: Some(crate::reducer::event::PipelinePhase::Development),
                iteration,
                commit: crate::reducer::state::CommitState::NotStarted,
                commit_prompt_prepared: false,
                commit_diff_prepared: false,
                commit_diff_empty: false,
                commit_agent_invoked: false,
                commit_xml_cleaned: false,
                commit_xml_extracted: false,
                commit_validated_outcome: None,
                commit_xml_archived: false,
                context_cleaned: false,
                continuation: ContinuationState {
                    context_cleanup_pending: true,
                    ..ContinuationState::new()
                },
                development_context_prepared_iteration: None,
                development_prompt_prepared_iteration: None,
                development_xml_cleaned_iteration: None,
                development_agent_invoked_iteration: None,
                development_xml_extracted_iteration: None,
                development_validated_outcome: None,
                development_xml_archived_iteration: None,
                ..state
            }
        }
        DevelopmentEvent::OutputValidationFailed { iteration, attempt }
        | DevelopmentEvent::XmlMissing { iteration, attempt } => {
            // Policy: After configured XSD retries are exhausted, switch to next agent.
            // This keeps invalid output retry logic in the reducer, not the handler.
            let new_xsd_count = state.continuation.xsd_retry_count + 1;
            if new_xsd_count >= state.continuation.max_xsd_retry_count {
                // XSD retries exhausted - switch to next agent
                let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();
                PipelineState {
                    phase: crate::reducer::event::PipelinePhase::Development,
                    iteration,
                    agent_chain: new_agent_chain,
                    continuation: ContinuationState {
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        same_agent_retry_count: 0,
                        same_agent_retry_pending: false,
                        same_agent_retry_reason: None,
                        ..state.continuation
                    },
                    development_context_prepared_iteration: None,
                    development_prompt_prepared_iteration: None,
                    development_xml_cleaned_iteration: None,
                    development_agent_invoked_iteration: None,
                    development_xml_extracted_iteration: None,
                    development_validated_outcome: None,
                    development_xml_archived_iteration: None,
                    ..state
                }
            } else {
                // Stay in Development, increment attempt counters, set retry pending
                PipelineState {
                    phase: crate::reducer::event::PipelinePhase::Development,
                    iteration,
                    continuation: ContinuationState {
                        invalid_output_attempts: attempt + 1,
                        xsd_retry_count: new_xsd_count,
                        xsd_retry_pending: true,
                        ..state.continuation
                    },
                    development_context_prepared_iteration: None,
                    development_prompt_prepared_iteration: None,
                    development_xml_cleaned_iteration: None,
                    development_agent_invoked_iteration: None,
                    development_xml_extracted_iteration: None,
                    development_validated_outcome: None,
                    development_xml_archived_iteration: None,
                    ..state
                }
            }
        }
        DevelopmentEvent::ContinuationBudgetExhausted {
            iteration,
            total_attempts: _,
            last_status: _,
        } => {
            // Policy: Abort pipeline when continuations exhausted.
            // Future enhancement: Could try fallback agent instead.
            PipelineState {
                phase: crate::reducer::event::PipelinePhase::Interrupted,
                iteration,
                continuation: ContinuationState {
                    context_cleanup_pending: true,
                    ..ContinuationState::new()
                },
                development_context_prepared_iteration: None,
                development_prompt_prepared_iteration: None,
                development_xml_cleaned_iteration: None,
                development_agent_invoked_iteration: None,
                development_xml_extracted_iteration: None,
                development_validated_outcome: None,
                development_xml_archived_iteration: None,
                ..state
            }
        }
        DevelopmentEvent::ContinuationContextWritten {
            iteration,
            attempt: _,
        } => {
            // Context file was written, state remains unchanged.
            // The continuation state is already set by ContinuationTriggered.
            PipelineState {
                iteration,
                continuation: crate::reducer::state::ContinuationState {
                    context_write_pending: false,
                    ..state.continuation
                },
                ..state
            }
        }
        DevelopmentEvent::ContinuationContextCleaned => {
            // Context file was cleaned up, no state change needed.
            PipelineState {
                continuation: crate::reducer::state::ContinuationState {
                    context_cleanup_pending: false,
                    ..state.continuation
                },
                ..state
            }
        }
    }
}
