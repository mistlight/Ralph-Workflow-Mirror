// NOTE: split from reducer/state_reduction.rs.

use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_agent_event(state: PipelineState, event: AgentEvent) -> PipelineState {
    match event {
        // Do NOT clear any saved continuation prompt on invocation start.
        //
        // Rationale: after a 429, we preserve prompt context so the next agent can continue the
        // same work. If the first post-rate-limit invocation fails (e.g., timeout/internal), we
        // must keep the continuation prompt available for retries until an invocation succeeds.
        AgentEvent::InvocationStarted { .. } => PipelineState {
            continuation: ContinuationState {
                xsd_retry_session_reuse_pending: false,
                ..state.continuation
            },
            ..state
        },
        // Clear continuation prompt on success
        AgentEvent::InvocationSucceeded { .. } => PipelineState {
            agent_chain: state.agent_chain.clear_continuation_prompt(),
            continuation: ContinuationState {
                same_agent_retry_count: 0,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                xsd_retry_session_reuse_pending: false,
                ..state.continuation
            },
            ..state
        },
        // Rate limit (429): immediate agent switch, preserve prompt context.
        AgentEvent::RateLimited {
            role,
            prompt_context,
            ..
        } => {
            let state = reset_phase_xml_cleanup_for_retry(state);
            PipelineState {
                agent_chain: state
                    .agent_chain
                    .switch_to_next_agent_with_prompt_for_role(role, prompt_context)
                    .clear_session_id(),
                continuation: ContinuationState {
                    xsd_retry_count: 0,
                    xsd_retry_pending: false,
                    xsd_retry_session_reuse_pending: false,
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
                    xsd_retry_session_reuse_pending: false,
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
                    xsd_retry_session_reuse_pending: false,
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
            role,
            error_kind,
            ..
        } => {
            let state = reset_phase_xml_cleanup_for_retry(state);
            match error_kind {
                // Authentication and rate limit failures: immediate agent switch.
                // These may arrive as InvocationFailed for legacy callers; prefer AuthFailed/RateLimited.
                AgentErrorKind::Authentication => PipelineState {
                    agent_chain: state
                        .agent_chain
                        .switch_to_next_agent()
                        .clear_session_id()
                        .clear_continuation_prompt(),
                    continuation: ContinuationState {
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        xsd_retry_session_reuse_pending: false,
                        same_agent_retry_count: 0,
                        same_agent_retry_pending: false,
                        same_agent_retry_reason: None,
                        ..state.continuation
                    },
                    ..state
                },
                AgentErrorKind::RateLimit => PipelineState {
                    // Legacy callers may report rate limit as InvocationFailed without prompt context.
                    // In that case, explicitly overwrite any saved continuation prompt to avoid
                    // reusing stale prompt context on the next invocation.
                    agent_chain: state
                        .agent_chain
                        .switch_to_next_agent_with_prompt_for_role(role, None)
                        .clear_session_id(),
                    continuation: ContinuationState {
                        xsd_retry_count: 0,
                        xsd_retry_pending: false,
                        xsd_retry_session_reuse_pending: false,
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
            let mut metrics = state.metrics.clone();
            metrics.agent_fallbacks_total += 1;

            PipelineState {
                agent_chain: state
                    .agent_chain
                    .switch_to_agent_named(&to_agent)
                    .clear_session_id()
                    .clear_continuation_prompt(),
                continuation: ContinuationState {
                    xsd_retry_count: 0,
                    xsd_retry_pending: false,
                    xsd_retry_session_reuse_pending: false,
                    same_agent_retry_count: 0,
                    same_agent_retry_pending: false,
                    same_agent_retry_reason: None,
                    ..state.continuation
                },
                metrics,
                ..state
            }
        }
        AgentEvent::ChainExhausted { .. } => PipelineState {
            agent_chain: state.agent_chain.start_retry_cycle(),
            ..state
        },
        AgentEvent::ModelFallbackTriggered { .. } => {
            let mut metrics = state.metrics.clone();
            metrics.model_fallbacks_total += 1;

            PipelineState {
                agent_chain: state.agent_chain.advance_to_next_model(),
                metrics,
                ..state
            }
        }
        AgentEvent::RetryCycleStarted { .. } => {
            let mut metrics = state.metrics.clone();
            metrics.retry_cycles_started_total += 1;

            PipelineState {
                agent_chain: state.agent_chain.clear_backoff_pending(),
                metrics,
                ..state
            }
        }
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
        AgentEvent::XsdValidationFailed { .. } => {
            let continuation = state.continuation.trigger_xsd_retry();
            let mut metrics = state.metrics.clone();

            metrics.xsd_retry_attempts_total += 1;

            // Increment per-phase counter based on current phase
            match state.phase {
                PipelinePhase::Planning => metrics.xsd_retry_planning += 1,
                PipelinePhase::Development => metrics.xsd_retry_development += 1,
                PipelinePhase::Review => {
                    // Distinguish review vs fix based on whether we're in fix flow
                    if state.fix_agent_invoked_pass.is_some()
                        && state.fix_result_xml_extracted_pass.is_none()
                    {
                        metrics.xsd_retry_fix += 1;
                    } else {
                        metrics.xsd_retry_review += 1;
                    }
                }
                PipelinePhase::CommitMessage => metrics.xsd_retry_commit += 1,
                _ => {}
            }

            PipelineState {
                continuation,
                metrics,
                ..state
            }
        }

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
    let mut metrics = state.metrics.clone();

    // Only increment metrics if we're actually retrying (not exhausted)
    let will_retry = new_retry_count < state.continuation.max_same_agent_retry_count;
    if will_retry {
        metrics.same_agent_retry_attempts_total += 1;
    }

    if new_retry_count >= state.continuation.max_same_agent_retry_count {
        PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent().clear_session_id(),
            continuation: ContinuationState {
                xsd_retry_count: 0,
                xsd_retry_pending: false,
                xsd_retry_session_reuse_pending: false,
                same_agent_retry_count: 0,
                same_agent_retry_pending: false,
                same_agent_retry_reason: None,
                ..state.continuation
            },
            metrics,
            ..state
        }
    } else {
        let reason = match failure {
            SameAgentRetryableFailure::Timeout => SameAgentRetryReason::Timeout,
            SameAgentRetryableFailure::InternalError => SameAgentRetryReason::InternalError,
            SameAgentRetryableFailure::OtherNonRetriable => SameAgentRetryReason::Other,
        };
        PipelineState {
            agent_chain: state.agent_chain.clear_session_id(),
            continuation: ContinuationState {
                same_agent_retry_count: new_retry_count,
                same_agent_retry_pending: true,
                same_agent_retry_reason: Some(reason),
                xsd_retry_session_reuse_pending: false,
                ..state.continuation
            },
            metrics,
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
