//! Event loop for the reducer-based pipeline architecture.
//!
//! This module implements the main event loop that coordinates the reducer,
//! effect handlers, and orchestration logic. It provides a unified way to
//! run the pipeline using the event-sourced architecture from RFC-004.

use crate::phases::PhaseContext;
use crate::reducer::{
    determine_next_effect, reduce, EffectHandler, MainEffectHandler, PipelineState,
};
use anyhow::Result;

/// Configuration for the event loop.
#[derive(Clone, Debug)]
pub struct EventLoopConfig {
    /// Maximum number of iterations to prevent infinite loops.
    pub max_iterations: usize,
    /// Whether to enable checkpointing during the event loop.
    pub enable_checkpointing: bool,
}

impl Default for EventLoopConfig {
    fn default() -> Self {
        Self {
            max_iterations: 1000,
            enable_checkpointing: true,
        }
    }
}

/// Result of running the event loop.
#[derive(Debug)]
pub struct EventLoopResult {
    /// Final pipeline state.
    pub final_state: PipelineState,
    /// Number of events processed.
    pub events_processed: usize,
    /// Whether the loop completed successfully.
    pub completed: bool,
}

/// Run the pipeline using the reducer-based event loop.
///
/// This function:
/// 1. Creates the initial pipeline state
/// 2. Loops: determine next effect → execute effect → apply event to state
/// 3. Stops when pipeline reaches Complete/Interrupted phase
///
/// # Arguments
///
/// * `phase_ctx` - Phase context containing all runtime dependencies
/// * `initial_state` - Optional initial state (for resume scenarios)
/// * `config` - Event loop configuration
///
/// # Returns
///
/// Returns event loop result with final state and statistics.
pub fn run_event_loop<'a>(
    phase_ctx: &'a mut PhaseContext<'a>,
    initial_state: Option<PipelineState>,
    config: EventLoopConfig,
) -> Result<EventLoopResult> {
    // Initialize state from checkpoint or create new initial state
    let mut state = initial_state.unwrap_or_else(|| {
        PipelineState::initial(
            phase_ctx.config.developer_iters,
            phase_ctx.config.reviewer_reviews,
        )
    });

    // Event log for debugging
    let mut event_log = Vec::new();
    let mut events_processed = 0;

    // Main event loop
    while events_processed < config.max_iterations {
        // Check if we've reached a terminal state
        if is_terminal_state(&state) {
            return Ok(EventLoopResult {
                final_state: state,
                events_processed,
                completed: true,
            });
        }

        // Determine next effect based on current state
        let effect = determine_next_effect(&state);

        // Execute effect and get event
        let event = {
            let mut handler = MainEffectHandler::new(state.clone());
            handler.execute(effect, phase_ctx)?
        };

        // Apply event to state (pure reduction)
        state = reduce(state, event.clone());

        // Log event
        event_log.push(event.clone());
        events_processed += 1;

        // Handle checkpointing if enabled (phase_ctx is available again)
        if config.enable_checkpointing {
            if let Err(e) = handle_checkpoint_trigger(&event, phase_ctx, &state) {
                // Log checkpoint failure but continue
                eprintln!("Checkpoint failed: {e}");
            }
        }
    }

    // If we exit the loop due to max_iterations, something went wrong
    Err(anyhow::anyhow!(
        "Event loop exceeded maximum iterations ({})",
        config.max_iterations
    ))
}

/// Check if the pipeline state is terminal (Complete or Interrupted).
fn is_terminal_state(state: &PipelineState) -> bool {
    matches!(
        state.phase,
        crate::reducer::event::PipelinePhase::Complete
            | crate::reducer::event::PipelinePhase::Interrupted
    )
}

/// Handle checkpoint triggers from events.
///
/// This function saves checkpoints when triggered by specific events
/// like phase transitions or interruption.
fn handle_checkpoint_trigger(
    event: &crate::reducer::PipelineEvent,
    phase_ctx: &mut PhaseContext<'_>,
    state: &PipelineState,
) -> Result<()> {
    use crate::checkpoint::{save_checkpoint, CheckpointBuilder};

    match event {
        crate::reducer::PipelineEvent::CheckpointSaved { .. } => {
            // Continue to save checkpoint
        }
        _ => return Ok(()),
    };

    if !phase_ctx.config.features.checkpoint_enabled {
        return Ok(());
    }

    let builder = CheckpointBuilder::new()
        .phase(
            map_to_checkpoint_phase(state.phase),
            state.iteration,
            state.total_iterations,
        )
        .reviewer_pass(state.reviewer_pass, state.total_reviewer_passes)
        .capture_from_context(
            phase_ctx.config,
            phase_ctx.registry,
            phase_ctx.developer_agent,
            phase_ctx.reviewer_agent,
            phase_ctx.logger,
            &phase_ctx.run_context,
        )
        .with_execution_history(phase_ctx.execution_history.clone())
        .with_prompt_history(phase_ctx.clone_prompt_history());

    if let Some(checkpoint) = builder.build() {
        save_checkpoint(&checkpoint)?;
    }

    Ok(())
}

/// Map reducer phase to checkpoint phase.
fn map_to_checkpoint_phase(
    phase: crate::reducer::event::PipelinePhase,
) -> crate::checkpoint::PipelinePhase {
    match phase {
        crate::reducer::event::PipelinePhase::Planning => {
            crate::checkpoint::PipelinePhase::Planning
        }
        crate::reducer::event::PipelinePhase::Development => {
            crate::checkpoint::PipelinePhase::Development
        }
        crate::reducer::event::PipelinePhase::Review => crate::checkpoint::PipelinePhase::Review,
        crate::reducer::event::PipelinePhase::CommitMessage => {
            crate::checkpoint::PipelinePhase::CommitMessage
        }
        crate::reducer::event::PipelinePhase::FinalValidation => {
            crate::checkpoint::PipelinePhase::FinalValidation
        }
        crate::reducer::event::PipelinePhase::Complete => {
            crate::checkpoint::PipelinePhase::Complete
        }
        crate::reducer::event::PipelinePhase::Interrupted => {
            crate::checkpoint::PipelinePhase::Complete
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_event_loop_config_default() {
        let config = EventLoopConfig::default();
        assert_eq!(config.max_iterations, 1000);
        assert!(config.enable_checkpointing);
    }

    #[test]
    fn test_is_terminal_state_complete() {
        let state = PipelineState {
            phase: crate::reducer::event::PipelinePhase::Complete,
            ..PipelineState::initial(5, 2)
        };
        assert!(is_terminal_state(&state));
    }

    #[test]
    fn test_is_terminal_state_interrupted() {
        let state = PipelineState {
            phase: crate::reducer::event::PipelinePhase::Interrupted,
            ..PipelineState::initial(5, 2)
        };
        assert!(is_terminal_state(&state));
    }

    #[test]
    fn test_is_terminal_state_running() {
        let state = PipelineState::initial(5, 2);
        assert!(!is_terminal_state(&state));
    }
}
