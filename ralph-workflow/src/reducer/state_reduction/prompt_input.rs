use crate::reducer::event::PromptInputEvent;
use crate::reducer::state::{
    MaterializedCommitInputs, MaterializedDevelopmentInputs, MaterializedPlanningInputs,
    MaterializedReviewInputs, PipelineState,
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
    }
}
