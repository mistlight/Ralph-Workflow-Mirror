// NOTE: Lifecycle and Planning constructors split from constructors.rs

impl PipelineEvent {
    // Lifecycle constructors
    /// Create a PipelineStarted event.
    pub fn pipeline_started() -> Self {
        Self::Lifecycle(LifecycleEvent::Started)
    }

    /// Create a PipelineResumed event.
    pub fn pipeline_resumed(from_checkpoint: bool) -> Self {
        Self::Lifecycle(LifecycleEvent::Resumed { from_checkpoint })
    }

    /// Create a PipelineCompleted event.
    pub fn pipeline_completed() -> Self {
        Self::Lifecycle(LifecycleEvent::Completed)
    }

    /// Create a PipelineAborted event.
    pub fn pipeline_aborted(reason: String) -> Self {
        Self::Lifecycle(LifecycleEvent::Aborted { reason })
    }

    // Planning constructors
    /// Create a PlanningPhaseStarted event.
    pub fn planning_phase_started() -> Self {
        Self::Planning(PlanningEvent::PhaseStarted)
    }

    /// Create a PlanningPhaseCompleted event.
    pub fn planning_phase_completed() -> Self {
        Self::Planning(PlanningEvent::PhaseCompleted)
    }

    /// Create a PlanningPromptPrepared event.
    pub fn planning_prompt_prepared(iteration: u32) -> Self {
        Self::Planning(PlanningEvent::PromptPrepared { iteration })
    }

    /// Create a PlanningAgentInvoked event.
    pub fn planning_agent_invoked(iteration: u32) -> Self {
        Self::Planning(PlanningEvent::AgentInvoked { iteration })
    }

    /// Create a PlanningXmlCleaned event.
    pub fn planning_xml_cleaned(iteration: u32) -> Self {
        Self::Planning(PlanningEvent::PlanXmlCleaned { iteration })
    }

    /// Create a PlanningXmlExtracted event.
    pub fn planning_xml_extracted(iteration: u32) -> Self {
        Self::Planning(PlanningEvent::PlanXmlExtracted { iteration })
    }

    /// Create a PlanningXmlMissing event.
    pub fn planning_xml_missing(iteration: u32, attempt: u32) -> Self {
        Self::Planning(PlanningEvent::PlanXmlMissing { iteration, attempt })
    }

    /// Create a PlanningXmlValidated event.
    pub fn planning_xml_validated(iteration: u32, valid: bool, markdown: Option<String>) -> Self {
        Self::Planning(PlanningEvent::PlanXmlValidated {
            iteration,
            valid,
            markdown,
        })
    }

    /// Create a PlanningMarkdownWritten event.
    pub fn planning_markdown_written(iteration: u32) -> Self {
        Self::Planning(PlanningEvent::PlanMarkdownWritten { iteration })
    }

    /// Create a PlanningXmlArchived event.
    pub fn planning_xml_archived(iteration: u32) -> Self {
        Self::Planning(PlanningEvent::PlanXmlArchived { iteration })
    }

    /// Create a PlanGenerationCompleted event.
    pub fn plan_generation_completed(iteration: u32, valid: bool) -> Self {
        Self::Planning(PlanningEvent::GenerationCompleted { iteration, valid })
    }

    /// Create a PlanningOutputValidationFailed event.
    pub fn planning_output_validation_failed(iteration: u32, attempt: u32) -> Self {
        Self::Planning(PlanningEvent::OutputValidationFailed { iteration, attempt })
    }
}
