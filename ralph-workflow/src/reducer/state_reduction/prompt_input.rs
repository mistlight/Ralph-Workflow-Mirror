use crate::reducer::event::PromptInputEvent;
use crate::reducer::state::{
    MaterializedCommitInputs, MaterializedDevelopmentInputs, MaterializedPlanningInputs,
    MaterializedReviewInputs, MaterializedXsdRetryLastOutput, PipelineState,
};

pub fn reduce_prompt_input_event(state: PipelineState, event: PromptInputEvent) -> PipelineState {
    match event {
        PromptInputEvent::OversizeDetected { .. } => state,
        PromptInputEvent::PlanningInputsMaterialized { iteration, prompt } => PipelineState {
            prompt_inputs: crate::reducer::state::PromptInputsState {
                planning: Some(MaterializedPlanningInputs { iteration, prompt }),
                ..state.prompt_inputs
            },
            ..state
        },
        PromptInputEvent::DevelopmentInputsMaterialized {
            iteration,
            prompt,
            plan,
        } => PipelineState {
            prompt_inputs: crate::reducer::state::PromptInputsState {
                development: Some(MaterializedDevelopmentInputs {
                    iteration,
                    prompt,
                    plan,
                }),
                ..state.prompt_inputs
            },
            ..state
        },
        PromptInputEvent::ReviewInputsMaterialized { pass, plan, diff } => PipelineState {
            prompt_inputs: crate::reducer::state::PromptInputsState {
                review: Some(MaterializedReviewInputs { pass, plan, diff }),
                ..state.prompt_inputs
            },
            ..state
        },
        PromptInputEvent::CommitInputsMaterialized { attempt, diff } => PipelineState {
            prompt_inputs: crate::reducer::state::PromptInputsState {
                commit: Some(MaterializedCommitInputs { attempt, diff }),
                ..state.prompt_inputs
            },
            ..state
        },
        PromptInputEvent::XsdRetryLastOutputMaterialized {
            phase,
            scope_id,
            last_output,
        } => PipelineState {
            prompt_inputs: crate::reducer::state::PromptInputsState {
                xsd_retry_last_output: Some(MaterializedXsdRetryLastOutput {
                    phase,
                    scope_id,
                    last_output,
                }),
                ..state.prompt_inputs
            },
            ..state
        },
        PromptInputEvent::HandlerError { error, .. } => super::error::reduce_error(&state, &error),

        PromptInputEvent::PromptPermissionsLocked { warning } => PipelineState {
            prompt_permissions: crate::reducer::state::PromptPermissionsState {
                locked: true,
                restore_needed: true,
                last_warning: warning,
                ..state.prompt_permissions
            },
            ..state
        },
    }
}
