//! Event loop iteration control logic.
//!
//! This module contains functions that determine when the event loop should
//! exit based on the current pipeline state. The iteration control ensures:
//! - Terminal states cause loop exit
//! - Special cases (checkpoints, dev-fix) get their required iterations
//! - Defensive completion markers are written before exit

use crate::reducer::event::PipelinePhase;
use crate::reducer::state::PipelineState;

/// Determine if we should exit the loop BEFORE executing the next effect.
///
/// Returns true if the state is already complete, with exceptions for:
/// - Interrupted from AwaitingDevFix without checkpoint (need SaveCheckpoint)
/// - AwaitingDevFix without dev_fix_triggered (need TriggerDevFixFlow)
///
/// # Rationale
///
/// When resuming from an Interrupted checkpoint, the state is already complete
/// but we still need to allow one iteration to execute any pending SaveCheckpoint
/// effect. Similarly, when entering AwaitingDevFix, we must execute TriggerDevFixFlow
/// to write the completion marker before exiting.
///
/// # Example
///
/// ```ignore
/// if should_exit_before_effect(&state) {
///     break; // State is terminal and no pending work
/// }
/// // Otherwise, execute the next effect
/// ```
pub(super) fn should_exit_before_effect(state: &PipelineState) -> bool {
    if !state.is_complete() {
        return false;
    }

    let should_allow_checkpoint_save = matches!(state.phase, PipelinePhase::Interrupted)
        && matches!(state.previous_phase, Some(PipelinePhase::AwaitingDevFix))
        && state.checkpoint_saved_count == 0;

    let is_awaiting_dev_fix_not_triggered =
        matches!(state.phase, PipelinePhase::AwaitingDevFix) && !state.dev_fix_triggered;

    !should_allow_checkpoint_save && !is_awaiting_dev_fix_not_triggered
}

/// Determine if we should exit the loop AFTER executing an effect.
///
/// Similar logic to should_exit_before_effect, but checks after state transitions.
/// This ensures that transitions to terminal phases (e.g., Interrupted) have a
/// chance to save their checkpoint before the loop exits.
///
/// # Example
///
/// ```ignore
/// let new_state = reduce(state, event);
/// if should_exit_after_effect(&new_state) {
///     break; // State became terminal after this effect
/// }
/// ```
pub(super) fn should_exit_after_effect(state: &PipelineState) -> bool {
    should_exit_before_effect(state)
}
