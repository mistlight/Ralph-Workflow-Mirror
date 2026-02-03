// NOTE: split from reducer/state_reduction.rs.

use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_agent_event(state: PipelineState, event: AgentEvent) -> PipelineState {
    match event {
        // Clear any saved continuation prompt when an invocation starts.
        // This makes prompt consumption reducer-driven (handlers must not mutate state).
        AgentEvent::InvocationStarted { .. } => PipelineState {
            agent_chain: state.agent_chain.clear_continuation_prompt(),
            ..state
        },
        // Clear continuation prompt on success
        AgentEvent::InvocationSucceeded { .. } => PipelineState {
            agent_chain: state.agent_chain.clear_continuation_prompt(),
            continuation: ContinuationState {
                same_agent_retry_count: 0,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation
            },
            ..state
        },
        // Rate limit (429): immediate agent switch, preserve prompt context.
        AgentEvent::RateLimited { prompt_context, .. } => {
            let state = reset_phase_xml_cleanup_for_retry(state);
            PipelineState {
                agent_chain: state
                    .agent_chain
                    .switch_to_next_agent_with_prompt(prompt_context)
                    .clear_session_id(),
                continuation: ContinuationState {
                    xsd_retry_count: 0,
                    xsd_retry_pending: false,
                    same_agent_retry_count: 0,
                    same_agent_retry_pending: false,
                    same_agent_retry_reason: None,
                    ..state.continuation
                },
                ..state
            }
        }
        // Auth failure (401/403): immediate agent switch, clear session and prompt context.
        AgentEvent::AuthFailed { .. } => {
            let state = reset_phase_xml_cleanup_for_retry(state);
            PipelineState {
                agent_chain: state
                    .agent_chain
                    .switch_to_next_agent()
                    .clear_session_id()
                    .clear_continuation_prompt(),
                continuation: ContinuationState {
                    xsd_retry_count: 0,
                    xsd_retry_pending: false,
                    same_agent_retry_count: 0,
                    same_agent_retry_pending: false,
                    same_agent_retry_reason: None,
                    ..state.continuation
                },
                ..state
            }
        }
        // Timeout: retry same agent first; fall back only after retry budget exhaustion.
        AgentEvent::TimedOut { .. } => {
            reduce_same_agent_retryable_failure(state, SameAgentRetryableFailure::Timeout)
        }
        // Other retriable errors (Network, ModelUnavailable): try next model
        AgentEvent::InvocationFailed {
            retriable: true, ..
        } => {
            let state = reset_phase_xml_cleanup_for_retry(state);
            PipelineState {
                agent_chain: state.agent_chain.advance_to_next_model(),
                continuation: ContinuationState {
                    xsd_retry_count: 0,
                    xsd_retry_pending: false,
                    same_agent_retry_count: 0,
                    same_agent_retry_pending: false,
                    same_agent_retry_reason: None,
                    ..state.continuation
                },
                ..state
            }
        }
        AgentEvent::InvocationFailed {
            retriable: false,
            error_kind,
            ..
        } => {
            let state = reset_phase_xml_cleanup_for_retry(state);
            match error_kind {
                // Authentication and rate limit failures: immediate agent switch.
                // These may arrive as InvocationFailed for legacy callers; prefer AuthFailed/RateLimited.
                AgentErrorKind::Authentication | AgentErrorKind::RateLimit => PipelineState {
                    agent_chain: state
                        .agent_chain
                        .switch_to_next_agent()
                        .clear_session_id()
                        .clear_continuation_prompt(),
                    continuation: ContinuationState {
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        same_agent_retry_count: 0,
                        same_agent_retry_pending: false,
                        same_agent_retry_reason: None,
                        ..state.continuation
                    },
                    ..state
                },
                // Internal/unknown: retry same agent first; fall back after budget exhaustion.
                AgentErrorKind::InternalError => reduce_same_agent_retryable_failure(
                    state,
                    SameAgentRetryableFailure::InternalError,
                ),
                // Defensive: treat explicit Timeout similarly if it arrives here.
                AgentErrorKind::Timeout => {
                    reduce_same_agent_retryable_failure(state, SameAgentRetryableFailure::Timeout)
                }
                // Other non-retriable errors: retry same agent first; only fall back after budget.
                _ => reduce_same_agent_retryable_failure(
                    state,
                    SameAgentRetryableFailure::OtherNonRetriable,
                ),
            }
        }
        AgentEvent::FallbackTriggered {
            from_agent: _,
            to_agent,
            ..
        } => {
            let state = reset_phase_xml_cleanup_for_retry(state);
            PipelineState {
                agent_chain: state
                    .agent_chain
                    .switch_to_agent_named(&to_agent)
                    .clear_session_id()
                    .clear_continuation_prompt(),
                continuation: ContinuationState {
                    xsd_retry_count: 0,
                    xsd_retry_pending: false,
                    same_agent_retry_count: 0,
                    same_agent_retry_pending: false,
                    same_agent_retry_reason: None,
                    ..state.continuation
                },
                ..state
            }
        }
        AgentEvent::ChainExhausted { .. } => PipelineState {
            agent_chain: state.agent_chain.start_retry_cycle(),
            ..state
        },
        AgentEvent::ModelFallbackTriggered { .. } => PipelineState {
            agent_chain: state.agent_chain.advance_to_next_model(),
            ..state
        },
        AgentEvent::RetryCycleStarted { .. } => PipelineState {
            agent_chain: state.agent_chain.clear_backoff_pending(),
            ..state
        },
        AgentEvent::ChainInitialized {
            role,
            agents,
            max_cycles,
            retry_delay_ms,
            backoff_multiplier,
            max_backoff_ms,
        } => {
            let models_per_agent = agents.iter().map(|_| vec![]).collect();
            PipelineState {
                agent_chain: state
                    .agent_chain
                    .with_agents(agents, models_per_agent, role)
                    .with_max_cycles(max_cycles)
                    .with_backoff_policy(retry_delay_ms, backoff_multiplier, max_backoff_ms)
                    .reset_for_role(role),
                ..state
            }
        }
        // Session established: store session ID for potential XSD retry
        AgentEvent::SessionEstablished { session_id, .. } => PipelineState {
            agent_chain: state.agent_chain.with_session_id(Some(session_id)),
            ..state
        },
        // XSD validation failed: trigger XSD retry via continuation state
        AgentEvent::XsdValidationFailed { .. } => PipelineState {
            continuation: state.continuation.trigger_xsd_retry(),
            ..state
        },

        // Template variables invalid: retry same agent first; only fall back after budget.
        AgentEvent::TemplateVariablesInvalid { .. } => {
            reduce_same_agent_retryable_failure(state, SameAgentRetryableFailure::OtherNonRetriable)
        }
    }
}

#[derive(Clone, Copy)]
enum SameAgentRetryableFailure {
    Timeout,
    InternalError,
    OtherNonRetriable,
}

fn reduce_same_agent_retryable_failure(
    state: PipelineState,
    failure: SameAgentRetryableFailure,
) -> PipelineState {
    let state = reset_phase_xml_cleanup_for_retry(state);
    // Keep agent selection reducer-driven and deterministic:
    // - Retry same agent first for timeouts/internal errors.
    // - Fall back to next agent only after exhausting the configured budget.
    let new_retry_count = state.continuation.same_agent_retry_count + 1;
    if new_retry_count >= state.continuation.max_same_agent_retry_count {
        PipelineState {
            agent_chain: state
                .agent_chain
                .switch_to_next_agent()
                .clear_session_id()
                .clear_continuation_prompt(),
            continuation: ContinuationState {
                xsd_retry_count: 0,
                xsd_retry_pending: false,
                same_agent_retry_count: 0,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation
            },
            ..state
        }
    } else {
        let reason = match failure {
            SameAgentRetryableFailure::Timeout => SameAgentRetryReason::Timeout,
            SameAgentRetryableFailure::InternalError => SameAgentRetryReason::InternalError,
            SameAgentRetryableFailure::OtherNonRetriable => SameAgentRetryReason::Other,
        };
        PipelineState {
            agent_chain: state
                .agent_chain
                .clear_session_id()
                .clear_continuation_prompt(),
            continuation: ContinuationState {
                same_agent_retry_count: new_retry_count,
                same_agent_retry_pending: true,
                same_agent_retry_reason: Some(reason),
                ..state.continuation
            },
            ..state
        }
    }
}

fn reset_phase_xml_cleanup_for_retry(state: PipelineState) -> PipelineState {
    match state.phase {
        PipelinePhase::Planning => PipelineState {
            planning_xml_cleaned_iteration: None,
            ..state
        },
        PipelinePhase::Development => PipelineState {
            development_xml_cleaned_iteration: None,
            ..state
        },
        PipelinePhase::Review => {
            if state.review_issues_found || state.continuation.fix_continue_pending {
                PipelineState {
                    fix_result_xml_cleaned_pass: None,
                    ..state
                }
            } else {
                PipelineState {
                    review_issues_xml_cleaned_pass: None,
                    ..state
                }
            }
        }
        PipelinePhase::CommitMessage => PipelineState {
            commit_xml_cleaned: false,
            ..state
        },
        _ => state,
    }
}
