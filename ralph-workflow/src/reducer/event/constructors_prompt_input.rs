impl PipelineEvent {
    pub fn prompt_input_oversize_detected(
        phase: PipelinePhase,
        kind: PromptInputKind,
        content_id_sha256: String,
        size_bytes: u64,
        limit_bytes: u64,
        policy: String,
    ) -> Self {
        PipelineEvent::PromptInput(PromptInputEvent::OversizeDetected {
            phase,
            kind,
            content_id_sha256,
            size_bytes,
            limit_bytes,
            policy,
        })
    }

    pub fn planning_inputs_materialized(iteration: u32, prompt: MaterializedPromptInput) -> Self {
        PipelineEvent::PromptInput(PromptInputEvent::PlanningInputsMaterialized {
            iteration,
            prompt,
        })
    }

    pub fn development_inputs_materialized(
        iteration: u32,
        prompt: MaterializedPromptInput,
        plan: MaterializedPromptInput,
    ) -> Self {
        PipelineEvent::PromptInput(PromptInputEvent::DevelopmentInputsMaterialized {
            iteration,
            prompt,
            plan,
        })
    }

    pub fn review_inputs_materialized(
        pass: u32,
        plan: MaterializedPromptInput,
        diff: MaterializedPromptInput,
    ) -> Self {
        PipelineEvent::PromptInput(PromptInputEvent::ReviewInputsMaterialized { pass, plan, diff })
    }

    pub fn commit_inputs_materialized(attempt: u32, diff: MaterializedPromptInput) -> Self {
        PipelineEvent::PromptInput(PromptInputEvent::CommitInputsMaterialized { attempt, diff })
    }
}
