// NOTE: Development constructors split from constructors.rs

impl PipelineEvent {
    // Development constructors
    /// Create a DevelopmentPhaseStarted event.
    pub fn development_phase_started() -> Self {
        Self::Development(DevelopmentEvent::PhaseStarted)
    }

    /// Create a DevelopmentIterationStarted event.
    pub fn development_iteration_started(iteration: u32) -> Self {
        Self::Development(DevelopmentEvent::IterationStarted { iteration })
    }

    /// Create a DevelopmentContextPrepared event.
    pub fn development_context_prepared(iteration: u32) -> Self {
        Self::Development(DevelopmentEvent::ContextPrepared { iteration })
    }

    /// Create a DevelopmentPromptPrepared event.
    pub fn development_prompt_prepared(iteration: u32) -> Self {
        Self::Development(DevelopmentEvent::PromptPrepared { iteration })
    }

    /// Create a DevelopmentAgentInvoked event.
    pub fn development_agent_invoked(iteration: u32) -> Self {
        Self::Development(DevelopmentEvent::AgentInvoked { iteration })
    }

    /// Create a DevelopmentXmlExtracted event.
    pub fn development_xml_extracted(iteration: u32) -> Self {
        Self::Development(DevelopmentEvent::XmlExtracted { iteration })
    }

    /// Create a DevelopmentXmlCleaned event.
    pub fn development_xml_cleaned(iteration: u32) -> Self {
        Self::Development(DevelopmentEvent::XmlCleaned { iteration })
    }

    /// Create a DevelopmentXmlMissing event.
    pub fn development_xml_missing(iteration: u32, attempt: u32) -> Self {
        Self::Development(DevelopmentEvent::XmlMissing { iteration, attempt })
    }

    /// Create a DevelopmentXmlValidated event.
    pub fn development_xml_validated(
        iteration: u32,
        status: DevelopmentStatus,
        summary: String,
        files_changed: Option<Vec<String>>,
        next_steps: Option<String>,
    ) -> Self {
        Self::Development(DevelopmentEvent::XmlValidated {
            iteration,
            status,
            summary,
            files_changed,
            next_steps,
        })
    }

    /// Create a DevelopmentOutcomeApplied event.
    pub fn development_outcome_applied(iteration: u32) -> Self {
        Self::Development(DevelopmentEvent::OutcomeApplied { iteration })
    }

    /// Create a DevelopmentXmlArchived event.
    pub fn development_xml_archived(iteration: u32) -> Self {
        Self::Development(DevelopmentEvent::XmlArchived { iteration })
    }

    /// Create a DevelopmentIterationCompleted event.
    pub fn development_iteration_completed(iteration: u32, output_valid: bool) -> Self {
        Self::Development(DevelopmentEvent::IterationCompleted {
            iteration,
            output_valid,
        })
    }

    /// Create a DevelopmentPhaseCompleted event.
    pub fn development_phase_completed() -> Self {
        Self::Development(DevelopmentEvent::PhaseCompleted)
    }

    /// Create a DevelopmentIterationContinuationTriggered event.
    pub fn development_iteration_continuation_triggered(
        iteration: u32,
        status: DevelopmentStatus,
        summary: String,
        files_changed: Option<Vec<String>>,
        next_steps: Option<String>,
    ) -> Self {
        Self::Development(DevelopmentEvent::ContinuationTriggered {
            iteration,
            status,
            summary,
            files_changed,
            next_steps,
        })
    }

    /// Create a DevelopmentIterationContinuationSucceeded event.
    pub fn development_iteration_continuation_succeeded(
        iteration: u32,
        total_continuation_attempts: u32,
    ) -> Self {
        Self::Development(DevelopmentEvent::ContinuationSucceeded {
            iteration,
            total_continuation_attempts,
        })
    }

    /// Create a DevelopmentOutputValidationFailed event.
    pub fn development_output_validation_failed(iteration: u32, attempt: u32) -> Self {
        Self::Development(DevelopmentEvent::OutputValidationFailed { iteration, attempt })
    }

    /// Create a DevelopmentContinuationBudgetExhausted event.
    pub fn development_continuation_budget_exhausted(
        iteration: u32,
        total_attempts: u32,
        last_status: DevelopmentStatus,
    ) -> Self {
        Self::Development(DevelopmentEvent::ContinuationBudgetExhausted {
            iteration,
            total_attempts,
            last_status,
        })
    }

    /// Create a DevelopmentContinuationContextWritten event.
    pub fn development_continuation_context_written(iteration: u32, attempt: u32) -> Self {
        Self::Development(DevelopmentEvent::ContinuationContextWritten { iteration, attempt })
    }

    /// Create a DevelopmentContinuationContextCleaned event.
    pub fn development_continuation_context_cleaned() -> Self {
        Self::Development(DevelopmentEvent::ContinuationContextCleaned)
    }
}
