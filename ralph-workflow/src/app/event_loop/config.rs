//! Event loop configuration and initialization.
//!
//! This module defines configuration types and initialization logic for the
//! reducer-based event loop.

use crate::phases::PhaseContext;
use crate::reducer::event::PipelinePhase;
use crate::reducer::state::ContinuationState;
use crate::reducer::PipelineState;

/// Create initial pipeline state with continuation limits from config.
///
/// This function creates a `PipelineState` with XSD retry and continuation limits
/// loaded from the config, ensuring these values are available for the reducer
/// to make deterministic retry decisions.
pub(crate) fn create_initial_state_with_config(ctx: &PhaseContext<'_>) -> PipelineState {
    // Config semantics: max_dev_continuations counts continuation attempts *beyond*
    // the initial attempt. ContinuationState::max_continue_count semantics are
    // "maximum total attempts including initial".
    let max_dev_continuations = ctx.config.max_dev_continuations.unwrap_or(2);
    let max_continue_count = 1 + max_dev_continuations;

    let continuation = ContinuationState::with_limits(
        ctx.config.max_xsd_retries.unwrap_or(10),
        max_continue_count,
        ctx.config.max_same_agent_retries.unwrap_or(2),
    );
    let mut state = PipelineState::initial_with_continuation(
        ctx.config.developer_iters,
        ctx.config.reviewer_reviews,
        continuation,
    );

    // Inject a checkpoint-safe (redacted) view of runtime cloud config.
    // This ensures pure orchestration can derive cloud effects when enabled,
    // without ever storing secrets in reducer state.
    state.cloud_config = crate::config::CloudStateConfig::from(ctx.cloud_config);

    state
}

/// Maximum iterations for the main event loop to prevent infinite loops.
///
/// This is a safety limit - the pipeline should complete well before this limit
/// under normal circumstances. If reached, it indicates either a bug in the
/// reducer logic or an extremely complex project.
///
/// NOTE: Even 1_000_000 can still be too low for extremely slow-progress runs.
/// If this cap is hit in practice, prefer making it configurable and/or
/// investigating why the reducer is not converging.
pub const MAX_EVENT_LOOP_ITERATIONS: usize = 1_000_000;

/// Configuration for event loop.
#[derive(Clone, Debug)]
pub struct EventLoopConfig {
    /// Maximum number of iterations to prevent infinite loops.
    pub max_iterations: usize,
}

/// Result of event loop execution.
#[derive(Debug, Clone)]
pub struct EventLoopResult {
    /// Whether pipeline completed successfully.
    pub completed: bool,
    /// Total events processed.
    pub events_processed: usize,
    /// Final reducer phase when the loop stopped.
    pub final_phase: PipelinePhase,
    /// Final pipeline state (for metrics and summary).
    pub final_state: PipelineState,
}
