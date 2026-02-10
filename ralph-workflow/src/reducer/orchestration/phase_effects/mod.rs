//! Phase-specific effect orchestration.
//!
//! This module contains pure orchestration logic for determining the next effect
//! based on the current pipeline state. All functions are deterministic and perform
//! no I/O operations.
//!
//! # Architecture
//!
//! Each phase module implements a `determine_*_effect()` function that:
//! - Takes `&PipelineState` as input
//! - Returns an `Effect` to execute next
//! - Performs NO I/O or side effects
//! - Is purely deterministic
//!
//! # Priority Order
//!
//! The main `determine_next_effect_for_phase()` function is called by the
//! higher-level orchestration layer in `xsd_retry.rs`, which handles:
//!
//! 1. **Continuation cleanup** - Write pending continuation context
//! 2. **Retry logic** - Same-agent retry after timeout/failure
//! 3. **XSD retry** - Re-invoke agent after XSD validation failure
//! 4. **Continuation** - Re-invoke agent with continuation prompt
//! 5. **Normal progression** - Call phase-specific orchestration (this module)
//!
//! # Phase Modules
//!
//! - `planning` - Planning phase orchestration
//! - `development` - Development phase orchestration (including Analysis agent)
//! - `review` - Review phase orchestration (including Fix agent)
//! - `commit` - Commit phase orchestration
//!
//! # Special Cases
//!
//! - FinalValidation phase → ValidateFinalState effect
//! - Finalizing phase → RestorePromptPermissions effect
//! - AwaitingDevFix phase → TriggerDevFixFlow effect
//! - Complete/Interrupted phase → SaveCheckpoint effect

mod commit;
mod development;
mod planning;
mod review;

use crate::reducer::effect::Effect;
use crate::reducer::event::PipelinePhase;
use crate::reducer::state::PipelineState;

pub(in crate::reducer::orchestration) fn determine_next_effect_for_phase(
    state: &PipelineState,
) -> Effect {
    match state.phase {
        PipelinePhase::Planning => planning::determine_planning_effect(state),
        PipelinePhase::Development => development::determine_development_effect(state),
        PipelinePhase::Review => review::determine_review_effect(state),
        PipelinePhase::CommitMessage => commit::determine_commit_effect(state),
        PipelinePhase::FinalValidation => Effect::ValidateFinalState,
        PipelinePhase::Finalizing => Effect::RestorePromptPermissions,
        PipelinePhase::AwaitingDevFix => Effect::TriggerDevFixFlow {
            failed_phase: state.previous_phase.unwrap_or(PipelinePhase::Development),
            failed_role: state.agent_chain.current_role,
            retry_cycle: state.agent_chain.retry_cycle,
        },
        PipelinePhase::Complete | PipelinePhase::Interrupted => {
            use crate::reducer::event::CheckpointTrigger;

            // On Interrupted, check if restoration is pending before checkpoint
            // (This is the non-AwaitingDevFix path, e.g., user Ctrl+C)
            if state.phase == PipelinePhase::Interrupted
                && state.prompt_permissions.restore_needed
                && !state.prompt_permissions.restored
            {
                return Effect::RestorePromptPermissions;
            }

            Effect::SaveCheckpoint {
                trigger: CheckpointTrigger::Interrupt,
            }
        }
    }
}
