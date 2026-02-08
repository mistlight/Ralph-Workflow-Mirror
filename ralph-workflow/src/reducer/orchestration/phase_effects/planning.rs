//! Planning phase orchestration.
//!
//! Pure orchestration: State → Effect, no I/O.
//!
//! Planning phase workflow:
//! 1. Save checkpoint at iteration 0 (after rebase completes)
//! 2. Initialize agent chain (Developer role)
//! 3. Cleanup context (remove old PLAN.md from previous iteration)
//! 4. Materialize planning inputs (prompt template)
//! 5. Prepare planning prompt
//! 6. Cleanup planning XML
//! 7. Invoke planning agent
//! 8. Extract planning XML
//! 9. Validate planning XML
//! 10. Write planning markdown (PLAN.md)
//! 11. Archive planning XML
//! 12. Apply planning outcome (transition to Development)

use crate::agents::AgentRole;
use crate::reducer::effect::Effect;
use crate::reducer::event::CheckpointTrigger;
use crate::reducer::state::{PipelineState, PromptMode, RebaseState};

pub(super) fn determine_planning_effect(state: &PipelineState) -> Effect {
    if state.iteration == 0
        && state.checkpoint_saved_count == 0
        && matches!(
            state.rebase,
            RebaseState::Skipped | RebaseState::Completed { .. }
        )
    {
        return Effect::SaveCheckpoint {
            trigger: CheckpointTrigger::PhaseTransition,
        };
    }

    if state.agent_chain.agents.is_empty() || state.agent_chain.current_role != AgentRole::Developer
    {
        return Effect::InitializeAgentChain {
            role: AgentRole::Developer,
        };
    }

    let consumer_signature_sha256 = state.agent_chain.consumer_signature_sha256();

    // Clean up BEFORE planning to remove old PLAN.md from previous iteration
    if !state.context_cleaned {
        return Effect::CleanupContext;
    }

    if state.planning_prompt_prepared_iteration != Some(state.iteration) {
        let planning_inputs_materialized_for_iteration =
            state.prompt_inputs.planning.as_ref().is_some_and(|p| {
                p.iteration == state.iteration
                    && p.prompt.consumer_signature_sha256 == consumer_signature_sha256
            });
        if !planning_inputs_materialized_for_iteration {
            return Effect::MaterializePlanningInputs {
                iteration: state.iteration,
            };
        }
        return Effect::PreparePlanningPrompt {
            iteration: state.iteration,
            prompt_mode: PromptMode::Normal,
        };
    }

    if state.planning_xml_cleaned_iteration != Some(state.iteration) {
        return Effect::CleanupPlanningXml {
            iteration: state.iteration,
        };
    }

    if state.planning_agent_invoked_iteration != Some(state.iteration) {
        return Effect::InvokePlanningAgent {
            iteration: state.iteration,
        };
    }

    if state.planning_xml_extracted_iteration != Some(state.iteration) {
        return Effect::ExtractPlanningXml {
            iteration: state.iteration,
        };
    }

    let planning_validated_is_for_iteration = state
        .planning_validated_outcome
        .as_ref()
        .is_some_and(|o| o.iteration == state.iteration);
    if !planning_validated_is_for_iteration {
        return Effect::ValidatePlanningXml {
            iteration: state.iteration,
        };
    }

    if state.planning_markdown_written_iteration != Some(state.iteration) {
        return Effect::WritePlanningMarkdown {
            iteration: state.iteration,
        };
    }

    if state.planning_xml_archived_iteration != Some(state.iteration) {
        return Effect::ArchivePlanningXml {
            iteration: state.iteration,
        };
    }

    let outcome = state
        .planning_validated_outcome
        .as_ref()
        .expect("validated outcome should exist before applying planning outcome");
    Effect::ApplyPlanningOutcome {
        iteration: outcome.iteration,
        valid: outcome.valid,
    }
}
