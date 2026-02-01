// NOTE: split from reducer/state_reduction.rs.

use crate::reducer::event::*;
use crate::reducer::state::*;

pub(super) fn reduce_agent_event(state: PipelineState, event: AgentEvent) -> PipelineState {
    match event {
        AgentEvent::InvocationStarted { .. } => state,
        // Clear continuation prompt on success
        AgentEvent::InvocationSucceeded { .. } => PipelineState {
            agent_chain: state.agent_chain.clear_continuation_prompt(),
            ..state
        },
        // Rate limit (429): immediate agent fallback, preserve prompt context
        // Unlike other retriable errors, rate limits indicate the provider is
        // temporarily exhausted, so we switch to the next agent immediately
        // to continue work without delay.
        AgentEvent::RateLimitFallback { prompt_context, .. } => PipelineState {
            agent_chain: state
                .agent_chain
                .switch_to_next_agent_with_prompt(prompt_context)
                .clear_session_id(),
            ..state
        },
        // Auth failure (401/403): immediate agent fallback, clear session
        // Unlike rate limits, auth failures indicate credential issues with
        // the current agent, so we don't preserve prompt context - the next
        // agent may have different (valid) credentials.
        AgentEvent::AuthFallback { .. } => PipelineState {
            agent_chain: state
                .agent_chain
                .switch_to_next_agent()
                .clear_session_id()
                .clear_continuation_prompt(),
            ..state
        },
        // Timeout (idle): immediate agent fallback, clear session
        // Unlike rate limits, timeouts may indicate the agent is stuck or the
        // task is too complex for it. Retrying the same agent would likely
        // hit the same timeout, so switch to a different agent. We don't
        // preserve prompt context since the previous execution may have made
        // partial progress that is difficult to resume cleanly.
        AgentEvent::TimeoutFallback { .. } => PipelineState {
            agent_chain: state
                .agent_chain
                .switch_to_next_agent()
                .clear_session_id()
                .clear_continuation_prompt(),
            ..state
        },
        // Other retriable errors (Network, ModelUnavailable): try next model
        AgentEvent::InvocationFailed {
            retriable: true, ..
        } => PipelineState {
            agent_chain: state.agent_chain.advance_to_next_model(),
            ..state
        },
        // Non-retriable errors: switch agent and clear session
        AgentEvent::InvocationFailed {
            retriable: false, ..
        } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent().clear_session_id(),
            ..state
        },
        AgentEvent::FallbackTriggered { .. } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent().clear_session_id(),
            ..state
        },
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

        // Template variables invalid: switch to next agent (different agent may have different templates)
        // This is treated as a non-retriable error since the template system itself failed.
        AgentEvent::TemplateVariablesInvalid { .. } => PipelineState {
            agent_chain: state.agent_chain.switch_to_next_agent().clear_session_id(),
            ..state
        },
    }
}
