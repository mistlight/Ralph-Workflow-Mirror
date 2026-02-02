// NOTE: split from reducer/event.rs to keep the main file under line limits.
use crate::agents::AgentRole;
use serde::{Deserialize, Serialize};

/// Agent invocation and chain management events.
///
/// Events related to agent execution, fallback chains, model switching,
/// rate limiting, and retry cycles. The agent chain provides fault tolerance
/// through multiple fallback levels:
///
/// 1. Model level: Try different models for the same agent
/// 2. Agent level: Switch to a fallback agent
/// 3. Retry cycle: Start over with exponential backoff
///
/// # State Transitions
///
/// - `InvocationFailed(retriable=true)`: Advances to next model
/// - `InvocationFailed(retriable=false)`: Switches to next agent
/// - `RateLimitFallback`: Immediate agent switch with prompt preservation
/// - `ChainExhausted`: Starts new retry cycle
/// - `InvocationSucceeded`: Clears continuation prompt
#[derive(Clone, Serialize, Deserialize, Debug)]
pub enum AgentEvent {
    /// Agent invocation started.
    InvocationStarted {
        /// The role this agent is fulfilling.
        role: AgentRole,
        /// The agent being invoked.
        agent: String,
        /// The model being used, if specified.
        model: Option<String>,
    },
    /// Agent invocation succeeded.
    InvocationSucceeded {
        /// The role this agent fulfilled.
        role: AgentRole,
        /// The agent that succeeded.
        agent: String,
    },
    /// Agent invocation failed.
    InvocationFailed {
        /// The role this agent was fulfilling.
        role: AgentRole,
        /// The agent that failed.
        agent: String,
        /// The exit code from the agent process.
        exit_code: i32,
        /// The kind of error that occurred.
        error_kind: super::AgentErrorKind,
        /// Whether this error is retriable with the same agent.
        retriable: bool,
    },
    /// Fallback triggered to switch to a different agent.
    FallbackTriggered {
        /// The role being fulfilled.
        role: AgentRole,
        /// The agent being switched from.
        from_agent: String,
        /// The agent being switched to.
        to_agent: String,
    },
    /// Model fallback triggered within the same agent.
    ModelFallbackTriggered {
        /// The role being fulfilled.
        role: AgentRole,
        /// The agent whose model is changing.
        agent: String,
        /// The model being switched from.
        from_model: String,
        /// The model being switched to.
        to_model: String,
    },
    /// Retry cycle started (all agents exhausted, starting over).
    RetryCycleStarted {
        /// The role being retried.
        role: AgentRole,
        /// The cycle number starting.
        cycle: u32,
    },
    /// Agent chain exhausted (no more agents/models to try).
    ChainExhausted {
        /// The role whose chain is exhausted.
        role: AgentRole,
    },
    /// Agent chain initialized with available agents.
    ChainInitialized {
        /// The role this chain is for.
        role: AgentRole,
        /// The agents available in this chain.
        agents: Vec<String>,
        /// Maximum number of retry cycles allowed for this chain.
        max_cycles: u32,
        /// Base retry-cycle delay in milliseconds.
        retry_delay_ms: u64,
        /// Exponential backoff multiplier.
        backoff_multiplier: f64,
        /// Maximum backoff delay in milliseconds.
        max_backoff_ms: u64,
    },
    /// Agent hit rate limit (429) - should fallback immediately.
    ///
    /// Unlike other retriable errors (Network, Timeout), rate limits indicate
    /// the current provider is temporarily exhausted. Rather than waiting and
    /// retrying the same agent, we immediately switch to the next agent in the
    /// chain to continue work without delay.
    RateLimitFallback {
        /// The role being fulfilled.
        role: AgentRole,
        /// The agent that hit the rate limit.
        agent: String,
        /// The prompt that was being executed when rate limit was hit.
        /// This allows the next agent to continue the same work.
        prompt_context: Option<String>,
    },

    /// Agent hit authentication failure (401/403) - should fallback immediately.
    ///
    /// Unlike rate limits, auth failures indicate a credentials problem with
    /// the current agent/provider. We switch to the next agent without
    /// preserving prompt context since the issue is not transient exhaustion.
    AuthFallback {
        /// The role being fulfilled.
        role: AgentRole,
        /// The agent that failed authentication.
        agent: String,
    },

    /// Agent hit idle timeout - should fallback to a different agent.
    ///
    /// Unlike other retriable errors (Network, ModelUnavailable), idle timeouts
    /// indicate the agent may be stuck or the task is too complex for it.
    /// Retrying the same agent would likely hit the same timeout, so we switch
    /// to a different agent instead.
    ///
    /// Unlike `RateLimitFallback`, timeout fallback does not preserve prompt
    /// context since the previous execution may have made partial progress
    /// that is difficult to resume cleanly.
    TimeoutFallback {
        /// The role being fulfilled.
        role: AgentRole,
        /// The agent that timed out.
        agent: String,
    },

    /// Session established with agent.
    ///
    /// Emitted when an agent response includes a session ID that can be
    /// used for XSD retry continuation. This enables reusing the same
    /// session when retrying due to validation failures.
    SessionEstablished {
        /// The role this agent is fulfilling.
        role: AgentRole,
        /// The agent name.
        agent: String,
        /// The session ID returned by the agent.
        session_id: String,
    },

    /// XSD validation failed for agent output.
    ///
    /// Emitted when agent output cannot be parsed or fails XSD validation.
    /// Distinct from OutputValidationFailed events in phase-specific enums,
    /// this is the canonical XSD retry trigger that the reducer uses to
    /// decide whether to retry with the same agent/session or advance the chain.
    XsdValidationFailed {
        /// The role whose output failed validation.
        role: AgentRole,
        /// The artifact type that failed validation.
        artifact: crate::reducer::state::ArtifactType,
        /// Error message from validation.
        error: String,
        /// Current XSD retry count for this artifact.
        retry_count: u32,
    },

    /// Template rendering failed due to missing required variables or unresolved placeholders.
    ///
    /// Emitted when a prompt template cannot be rendered because required variables
    /// are missing or unresolved placeholders (e.g., `{{VAR}}`) remain in the output.
    /// The reducer decides fallback policy, typically switching to the next agent.
    TemplateVariablesInvalid {
        /// The role whose template failed to render.
        role: AgentRole,
        /// The name of the template that failed.
        template_name: String,
        /// Variables that were required but not provided.
        missing_variables: Vec<String>,
        /// Placeholder patterns that remain unresolved in the rendered output.
        unresolved_placeholders: Vec<String>,
    },
}
