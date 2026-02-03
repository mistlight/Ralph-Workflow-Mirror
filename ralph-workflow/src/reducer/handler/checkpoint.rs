use super::MainEffectHandler;
use crate::checkpoint::{
    save_checkpoint_with_workspace, CheckpointBuilder, PipelinePhase as CheckpointPhase,
};
use crate::phases::PhaseContext;
use crate::reducer::effect::EffectResult;
use crate::reducer::event::{CheckpointTrigger, PipelineEvent, PipelinePhase};
use crate::reducer::state::PipelineState;
use anyhow::Result;

impl MainEffectHandler {
    pub(super) fn save_checkpoint(
        &mut self,
        ctx: &mut PhaseContext<'_>,
        trigger: CheckpointTrigger,
    ) -> Result<EffectResult> {
        if ctx.config.features.checkpoint_enabled {
            let _ = save_checkpoint_from_state(&self.state, ctx);
        }

        let mut result = EffectResult::event(PipelineEvent::checkpoint_saved(trigger));

        // If the pipeline reaches a phase boundary but checkpoint writing is disabled (or the
        // checkpoint file write is skipped), orchestration can repeatedly derive the
        // phase-transition checkpoint effect without making progress.
        //
        // Emit the phase completion event as a separate reducer event so the state machine
        // always advances past the boundary.
        if trigger == CheckpointTrigger::PhaseTransition {
            match self.state.phase {
                PipelinePhase::Review
                    if self.state.reviewer_pass >= self.state.total_reviewer_passes =>
                {
                    result = result.with_additional_event(PipelineEvent::review_phase_completed(
                        /* early_exit */ false,
                    ));
                }
                _ => {}
            }
        }

        Ok(result)
    }
}

fn save_checkpoint_from_state(
    state: &PipelineState,
    ctx: &mut PhaseContext<'_>,
) -> anyhow::Result<()> {
    let builder = CheckpointBuilder::new()
        .phase(
            map_to_checkpoint_phase(state.phase),
            state.iteration,
            state.total_iterations,
        )
        .reviewer_pass(state.reviewer_pass, state.total_reviewer_passes)
        .capture_from_context(
            ctx.config,
            ctx.registry,
            ctx.developer_agent,
            ctx.reviewer_agent,
            ctx.logger,
            &ctx.run_context,
        )
        .with_executor_from_context(std::sync::Arc::clone(&ctx.executor_arc))
        .with_execution_history(ctx.execution_history.clone())
        .with_prompt_history(ctx.clone_prompt_history())
        .with_prompt_inputs(state.prompt_inputs.clone());

    if let Some(checkpoint) = builder.build_with_workspace(ctx.workspace) {
        let _ = save_checkpoint_with_workspace(ctx.workspace, &checkpoint);
    }

    Ok(())
}

fn map_to_checkpoint_phase(phase: crate::reducer::event::PipelinePhase) -> CheckpointPhase {
    match phase {
        crate::reducer::event::PipelinePhase::Planning => CheckpointPhase::Planning,
        crate::reducer::event::PipelinePhase::Development => CheckpointPhase::Development,
        crate::reducer::event::PipelinePhase::Review => CheckpointPhase::Review,
        crate::reducer::event::PipelinePhase::CommitMessage => CheckpointPhase::CommitMessage,
        crate::reducer::event::PipelinePhase::FinalValidation => CheckpointPhase::FinalValidation,
        crate::reducer::event::PipelinePhase::Finalizing => CheckpointPhase::FinalValidation,
        crate::reducer::event::PipelinePhase::Complete => CheckpointPhase::Complete,
        crate::reducer::event::PipelinePhase::Interrupted => CheckpointPhase::Interrupted,
    }
}
