//! Continuation and retry logic reducer
//!
//! Handles events related to:
//! - Continuation flow (ContinuationTriggered, ContinuationSucceeded, ContinuationBudgetExhausted)
//! - XSD retry logic (OutputValidationFailed, XmlMissing)
//! - Context management (ContinuationContextWritten, ContinuationContextCleaned)

use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_continuation_event(
    state: PipelineState,
    event: DevelopmentEvent,
) -> PipelineState {
    match event {
        DevelopmentEvent::ContinuationTriggered {
            iteration,
            status,
            summary,
            files_changed,
            next_steps,
        } => {
            // Trigger continuation with context from the previous attempt
            let old_attempt = state.continuation.continuation_attempt;
            let new_continuation =
                state
                    .continuation
                    .trigger_continuation(status, summary, files_changed, next_steps);
            let new_attempt = new_continuation.continuation_attempt;

            // Only increment metrics if the continuation counter actually incremented.
            // The defensive check in trigger_continuation may prevent the increment when
            // at the budget boundary, in which case metrics should also not increment.
            let metrics = if new_attempt > old_attempt {
                state.metrics.increment_dev_continuation_attempt()
            } else {
                state.metrics
            };

            PipelineState {
                iteration,
                continuation: new_continuation,
                development_context_prepared_iteration: None,
                development_prompt_prepared_iteration: None,
                development_xml_cleaned_iteration: None,
                development_agent_invoked_iteration: None,
                // IMPORTANT: analysis must run after EVERY development-agent invocation.
                // Reset this marker so the orchestrator will invoke analysis for the new
                // continuation attempt within the same iteration.
                analysis_agent_invoked_iteration: None,
                development_xml_extracted_iteration: None,
                development_validated_outcome: None,
                development_xml_archived_iteration: None,
                metrics,
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
                    ..state.continuation.reset()
                },
                development_context_prepared_iteration: None,
                development_prompt_prepared_iteration: None,
                development_xml_cleaned_iteration: None,
                development_agent_invoked_iteration: None,
                development_xml_extracted_iteration: None,
                development_validated_outcome: None,
                development_xml_archived_iteration: None,
                metrics: state.metrics.increment_dev_iterations_completed(),
                ..state
            }
        }
        DevelopmentEvent::OutputValidationFailed { iteration, attempt }
        | DevelopmentEvent::XmlMissing { iteration, attempt } => {
            // Policy: After configured XSD retries are exhausted, switch to next agent.
            // This keeps invalid output retry logic in the reducer, not the handler.
            let new_xsd_count = state.continuation.xsd_retry_count + 1;

            // Only increment metrics if we're actually retrying (not exhausted)
            let will_retry = new_xsd_count < state.continuation.max_xsd_retry_count;

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
                        xsd_retry_session_reuse_pending: false,
                        same_agent_retry_count: 0,
                        same_agent_retry_pending: false,
                        same_agent_retry_reason: None,
                        ..state.continuation
                    },
                    // IMPORTANT: XSD retry is for the analysis agent's XML output.
                    // Preserve developer-agent progress and retry analysis only.
                    development_context_prepared_iteration: state
                        .development_context_prepared_iteration,
                    development_prompt_prepared_iteration: state
                        .development_prompt_prepared_iteration,
                    development_xml_cleaned_iteration: state.development_xml_cleaned_iteration,
                    development_agent_invoked_iteration: state.development_agent_invoked_iteration,
                    analysis_agent_invoked_iteration: None,
                    development_xml_extracted_iteration: None,
                    development_validated_outcome: None,
                    development_xml_archived_iteration: None,
                    metrics: if will_retry {
                        state.metrics.increment_xsd_retry_development()
                    } else {
                        state.metrics
                    },
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
                        // Reuse last session id for analysis XSD retry when available.
                        xsd_retry_session_reuse_pending: true,
                        ..state.continuation
                    },
                    // Preserve developer-agent progress and retry analysis only.
                    development_context_prepared_iteration: state
                        .development_context_prepared_iteration,
                    development_prompt_prepared_iteration: state
                        .development_prompt_prepared_iteration,
                    development_xml_cleaned_iteration: state.development_xml_cleaned_iteration,
                    development_agent_invoked_iteration: state.development_agent_invoked_iteration,
                    analysis_agent_invoked_iteration: None,
                    development_xml_extracted_iteration: None,
                    development_validated_outcome: None,
                    development_xml_archived_iteration: None,
                    metrics: if will_retry {
                        state.metrics.increment_xsd_retry_development()
                    } else {
                        state.metrics
                    },
                    ..state
                }
            }
        }
        DevelopmentEvent::ContinuationBudgetExhausted {
            iteration,
            total_attempts: _,
            last_status,
        } => {
            // Policy: Switch to next agent when continuations exhausted.
            // CRITICAL: If all agents are exhausted AND work is incomplete (Failed/Partial status),
            // transition directly to AwaitingDevFix to emit completion marker and invoke dev-fix flow.
            // This ensures the pipeline NEVER exits early due to budget exhaustion - it always
            // continues through the configured remediation path (AwaitingDevFix -> TriggerDevFixFlow
            // -> completion marker write -> Interrupted -> SaveCheckpoint).
            let new_agent_chain = state.agent_chain.switch_to_next_agent().clear_session_id();

            // Check if all agents exhausted AND work incomplete
            if new_agent_chain.is_exhausted()
                && matches!(
                    last_status,
                    DevelopmentStatus::Failed | DevelopmentStatus::Partial
                )
            {
                // Transition to AwaitingDevFix for remediation attempt
                PipelineState {
                    phase: crate::reducer::event::PipelinePhase::AwaitingDevFix,
                    previous_phase: Some(crate::reducer::event::PipelinePhase::Development),
                    iteration,
                    agent_chain: new_agent_chain,
                    dev_fix_triggered: false, // CRITICAL: ensure TriggerDevFixFlow executes
                    continuation: ContinuationState {
                        continuation_attempt: 0,
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        xsd_retry_session_reuse_pending: false,
                        same_agent_retry_count: 0,
                        same_agent_retry_pending: false,
                        same_agent_retry_reason: None,
                        context_cleanup_pending: false, // No cleanup needed when transitioning to AwaitingDevFix
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
                // Otherwise, fall back to next agent (if available)
                PipelineState {
                    phase: crate::reducer::event::PipelinePhase::Development,
                    iteration,
                    agent_chain: new_agent_chain,
                    continuation: ContinuationState {
                        continuation_attempt: 0,
                        invalid_output_attempts: 0,
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        xsd_retry_session_reuse_pending: false,
                        same_agent_retry_count: 0,
                        same_agent_retry_pending: false,
                        same_agent_retry_reason: None,
                        context_cleanup_pending: true,
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
        // These events are handled by iteration_reducer
        _ => state,
    }
}
