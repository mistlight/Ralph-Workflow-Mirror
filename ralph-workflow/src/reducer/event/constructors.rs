// NOTE: split from reducer/event.rs to keep the facade small.
use super::*;

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

    // Review constructors
    /// Create a ReviewPhaseStarted event.
    pub fn review_phase_started() -> Self {
        Self::Review(ReviewEvent::PhaseStarted)
    }

    /// Create a ReviewPassStarted event.
    pub fn review_pass_started(pass: u32) -> Self {
        Self::Review(ReviewEvent::PassStarted { pass })
    }

    /// Create a ReviewContextPrepared event.
    pub fn review_context_prepared(pass: u32) -> Self {
        Self::Review(ReviewEvent::ContextPrepared { pass })
    }

    /// Create a ReviewPromptPrepared event.
    pub fn review_prompt_prepared(pass: u32) -> Self {
        Self::Review(ReviewEvent::PromptPrepared { pass })
    }

    /// Create a ReviewAgentInvoked event.
    pub fn review_agent_invoked(pass: u32) -> Self {
        Self::Review(ReviewEvent::AgentInvoked { pass })
    }

    /// Create a ReviewIssuesXmlExtracted event.
    pub fn review_issues_xml_extracted(pass: u32) -> Self {
        Self::Review(ReviewEvent::IssuesXmlExtracted { pass })
    }

    /// Create a ReviewIssuesXmlCleaned event.
    pub fn review_issues_xml_cleaned(pass: u32) -> Self {
        Self::Review(ReviewEvent::IssuesXmlCleaned { pass })
    }

    /// Create a ReviewIssuesXmlMissing event.
    pub fn review_issues_xml_missing(pass: u32, attempt: u32) -> Self {
        Self::Review(ReviewEvent::IssuesXmlMissing { pass, attempt })
    }

    /// Create a ReviewIssuesXmlValidated event.
    pub fn review_issues_xml_validated(
        pass: u32,
        issues_found: bool,
        clean_no_issues: bool,
        issues: Vec<String>,
        no_issues_found: Option<String>,
    ) -> Self {
        Self::Review(ReviewEvent::IssuesXmlValidated {
            pass,
            issues_found,
            clean_no_issues,
            issues,
            no_issues_found,
        })
    }

    pub fn review_issues_markdown_written(pass: u32) -> Self {
        Self::Review(ReviewEvent::IssuesMarkdownWritten { pass })
    }

    pub fn review_issue_snippets_extracted(pass: u32) -> Self {
        Self::Review(ReviewEvent::IssueSnippetsExtracted { pass })
    }

    pub fn review_issues_xml_archived(pass: u32) -> Self {
        Self::Review(ReviewEvent::IssuesXmlArchived { pass })
    }

    pub fn fix_prompt_prepared(pass: u32) -> Self {
        Self::Review(ReviewEvent::FixPromptPrepared { pass })
    }

    pub fn fix_agent_invoked(pass: u32) -> Self {
        Self::Review(ReviewEvent::FixAgentInvoked { pass })
    }

    pub fn fix_result_xml_extracted(pass: u32) -> Self {
        Self::Review(ReviewEvent::FixResultXmlExtracted { pass })
    }

    pub fn fix_result_xml_missing(pass: u32, attempt: u32) -> Self {
        Self::Review(ReviewEvent::FixResultXmlMissing { pass, attempt })
    }

    pub fn fix_result_xml_validated(
        pass: u32,
        status: crate::reducer::state::FixStatus,
        summary: Option<String>,
    ) -> Self {
        Self::Review(ReviewEvent::FixResultXmlValidated {
            pass,
            status,
            summary,
        })
    }

    pub fn fix_result_xml_cleaned(pass: u32) -> Self {
        Self::Review(ReviewEvent::FixResultXmlCleaned { pass })
    }

    pub fn fix_outcome_applied(pass: u32) -> Self {
        Self::Review(ReviewEvent::FixOutcomeApplied { pass })
    }

    pub fn fix_result_xml_archived(pass: u32) -> Self {
        Self::Review(ReviewEvent::FixResultXmlArchived { pass })
    }

    /// Create a ReviewCompleted event.
    pub fn review_completed(pass: u32, issues_found: bool) -> Self {
        Self::Review(ReviewEvent::Completed { pass, issues_found })
    }

    /// Create a FixAttemptStarted event.
    pub fn fix_attempt_started(pass: u32) -> Self {
        Self::Review(ReviewEvent::FixAttemptStarted { pass })
    }

    /// Create a FixAttemptCompleted event.
    pub fn fix_attempt_completed(pass: u32, changes_made: bool) -> Self {
        Self::Review(ReviewEvent::FixAttemptCompleted { pass, changes_made })
    }

    /// Create a ReviewPhaseCompleted event.
    pub fn review_phase_completed(early_exit: bool) -> Self {
        Self::Review(ReviewEvent::PhaseCompleted { early_exit })
    }

    /// Create a ReviewPassCompletedClean event.
    pub fn review_pass_completed_clean(pass: u32) -> Self {
        Self::Review(ReviewEvent::PassCompletedClean { pass })
    }

    /// Create a ReviewOutputValidationFailed event.
    pub fn review_output_validation_failed(pass: u32, attempt: u32) -> Self {
        Self::Review(ReviewEvent::OutputValidationFailed { pass, attempt })
    }

    /// Create a FixContinuationTriggered event.
    pub fn fix_continuation_triggered(
        pass: u32,
        status: crate::reducer::state::FixStatus,
        summary: Option<String>,
    ) -> Self {
        Self::Review(ReviewEvent::FixContinuationTriggered {
            pass,
            status,
            summary,
        })
    }

    /// Create a FixContinuationSucceeded event.
    pub fn fix_continuation_succeeded(pass: u32, total_attempts: u32) -> Self {
        Self::Review(ReviewEvent::FixContinuationSucceeded {
            pass,
            total_attempts,
        })
    }

    /// Create a FixContinuationBudgetExhausted event.
    pub fn fix_continuation_budget_exhausted(
        pass: u32,
        total_attempts: u32,
        last_status: crate::reducer::state::FixStatus,
    ) -> Self {
        Self::Review(ReviewEvent::FixContinuationBudgetExhausted {
            pass,
            total_attempts,
            last_status,
        })
    }

    /// Create a FixOutputValidationFailed event.
    pub fn fix_output_validation_failed(pass: u32, attempt: u32) -> Self {
        Self::Review(ReviewEvent::FixOutputValidationFailed { pass, attempt })
    }

    // Agent constructors
    /// Create an AgentInvocationStarted event.
    pub fn agent_invocation_started(role: AgentRole, agent: String, model: Option<String>) -> Self {
        Self::Agent(AgentEvent::InvocationStarted { role, agent, model })
    }

    /// Create an AgentInvocationSucceeded event.
    pub fn agent_invocation_succeeded(role: AgentRole, agent: String) -> Self {
        Self::Agent(AgentEvent::InvocationSucceeded { role, agent })
    }

    /// Create an AgentInvocationFailed event.
    pub fn agent_invocation_failed(
        role: AgentRole,
        agent: String,
        exit_code: i32,
        error_kind: AgentErrorKind,
        retriable: bool,
    ) -> Self {
        Self::Agent(AgentEvent::InvocationFailed {
            role,
            agent,
            exit_code,
            error_kind,
            retriable,
        })
    }

    /// Create an AgentFallbackTriggered event.
    pub fn agent_fallback_triggered(role: AgentRole, from_agent: String, to_agent: String) -> Self {
        Self::Agent(AgentEvent::FallbackTriggered {
            role,
            from_agent,
            to_agent,
        })
    }

    /// Create an AgentModelFallbackTriggered event.
    pub fn agent_model_fallback_triggered(
        role: AgentRole,
        agent: String,
        from_model: String,
        to_model: String,
    ) -> Self {
        Self::Agent(AgentEvent::ModelFallbackTriggered {
            role,
            agent,
            from_model,
            to_model,
        })
    }

    /// Create an AgentRetryCycleStarted event.
    pub fn agent_retry_cycle_started(role: AgentRole, cycle: u32) -> Self {
        Self::Agent(AgentEvent::RetryCycleStarted { role, cycle })
    }

    /// Create an AgentChainExhausted event.
    pub fn agent_chain_exhausted(role: AgentRole) -> Self {
        Self::Agent(AgentEvent::ChainExhausted { role })
    }

    /// Create an AgentChainInitialized event.
    pub fn agent_chain_initialized(
        role: AgentRole,
        agents: Vec<String>,
        max_cycles: u32,
        retry_delay_ms: u64,
        backoff_multiplier: f64,
        max_backoff_ms: u64,
    ) -> Self {
        Self::Agent(AgentEvent::ChainInitialized {
            role,
            agents,
            max_cycles,
            retry_delay_ms,
            backoff_multiplier,
            max_backoff_ms,
        })
    }

    /// Create an AgentRateLimitFallback event.
    pub fn agent_rate_limit_fallback(
        role: AgentRole,
        agent: String,
        prompt_context: Option<String>,
    ) -> Self {
        Self::Agent(AgentEvent::RateLimitFallback {
            role,
            agent,
            prompt_context,
        })
    }

    /// Create an AgentAuthFallback event.
    pub fn agent_auth_fallback(role: AgentRole, agent: String) -> Self {
        Self::Agent(AgentEvent::AuthFallback { role, agent })
    }

    /// Create an AgentTimeoutFallback event.
    ///
    /// Used when an agent hits an idle timeout and should fallback to
    /// a different agent. Unlike rate limit fallback, this does not
    /// preserve prompt context.
    pub fn agent_timeout_fallback(role: AgentRole, agent: String) -> Self {
        Self::Agent(AgentEvent::TimeoutFallback { role, agent })
    }

    /// Create an AgentSessionEstablished event.
    pub fn agent_session_established(role: AgentRole, agent: String, session_id: String) -> Self {
        Self::Agent(AgentEvent::SessionEstablished {
            role,
            agent,
            session_id,
        })
    }

    /// Create an AgentXsdValidationFailed event.
    pub fn agent_xsd_validation_failed(
        role: AgentRole,
        artifact: crate::reducer::state::ArtifactType,
        error: String,
        retry_count: u32,
    ) -> Self {
        Self::Agent(AgentEvent::XsdValidationFailed {
            role,
            artifact,
            error,
            retry_count,
        })
    }

    /// Create an AgentTemplateVariablesInvalid event.
    pub fn agent_template_variables_invalid(
        role: AgentRole,
        template_name: String,
        missing_variables: Vec<String>,
        unresolved_placeholders: Vec<String>,
    ) -> Self {
        Self::Agent(AgentEvent::TemplateVariablesInvalid {
            role,
            template_name,
            missing_variables,
            unresolved_placeholders,
        })
    }

    // Rebase constructors
    /// Create a RebaseStarted event.
    pub fn rebase_started(phase: RebasePhase, target_branch: String) -> Self {
        Self::Rebase(RebaseEvent::Started {
            phase,
            target_branch,
        })
    }

    /// Create a RebaseConflictDetected event.
    pub fn rebase_conflict_detected(files: Vec<PathBuf>) -> Self {
        Self::Rebase(RebaseEvent::ConflictDetected { files })
    }

    /// Create a RebaseConflictResolved event.
    pub fn rebase_conflict_resolved(files: Vec<PathBuf>) -> Self {
        Self::Rebase(RebaseEvent::ConflictResolved { files })
    }

    /// Create a RebaseSucceeded event.
    pub fn rebase_succeeded(phase: RebasePhase, new_head: String) -> Self {
        Self::Rebase(RebaseEvent::Succeeded { phase, new_head })
    }

    /// Create a RebaseFailed event.
    pub fn rebase_failed(phase: RebasePhase, reason: String) -> Self {
        Self::Rebase(RebaseEvent::Failed { phase, reason })
    }

    /// Create a RebaseAborted event.
    pub fn rebase_aborted(phase: RebasePhase, restored_to: String) -> Self {
        Self::Rebase(RebaseEvent::Aborted { phase, restored_to })
    }

    /// Create a RebaseSkipped event.
    pub fn rebase_skipped(phase: RebasePhase, reason: String) -> Self {
        Self::Rebase(RebaseEvent::Skipped { phase, reason })
    }

    // Commit constructors
    /// Create a CommitGenerationStarted event.
    pub fn commit_generation_started() -> Self {
        Self::Commit(CommitEvent::GenerationStarted)
    }

    /// Create a CommitDiffPrepared event.
    pub fn commit_diff_prepared(empty: bool) -> Self {
        Self::Commit(CommitEvent::DiffPrepared { empty })
    }

    /// Create a CommitDiffFailed event.
    pub fn commit_diff_failed(error: String) -> Self {
        Self::Commit(CommitEvent::DiffFailed { error })
    }

    /// Create a CommitPromptPrepared event.
    pub fn commit_prompt_prepared(attempt: u32) -> Self {
        Self::Commit(CommitEvent::PromptPrepared { attempt })
    }

    /// Create a CommitAgentInvoked event.
    pub fn commit_agent_invoked(attempt: u32) -> Self {
        Self::Commit(CommitEvent::AgentInvoked { attempt })
    }

    /// Create a CommitXmlExtracted event.
    pub fn commit_xml_extracted(attempt: u32) -> Self {
        Self::Commit(CommitEvent::CommitXmlExtracted { attempt })
    }

    /// Create a CommitXmlMissing event.
    pub fn commit_xml_missing(attempt: u32) -> Self {
        Self::Commit(CommitEvent::CommitXmlMissing { attempt })
    }

    /// Create a CommitXmlValidated event.
    pub fn commit_xml_validated(message: String, attempt: u32) -> Self {
        Self::Commit(CommitEvent::CommitXmlValidated { message, attempt })
    }

    /// Create a CommitXmlValidationFailed event.
    pub fn commit_xml_validation_failed(reason: String, attempt: u32) -> Self {
        Self::Commit(CommitEvent::CommitXmlValidationFailed { reason, attempt })
    }

    /// Create a CommitXmlArchived event.
    pub fn commit_xml_archived(attempt: u32) -> Self {
        Self::Commit(CommitEvent::CommitXmlArchived { attempt })
    }

    pub fn commit_xml_cleaned(attempt: u32) -> Self {
        Self::Commit(CommitEvent::CommitXmlCleaned { attempt })
    }

    /// Create a CommitMessageGenerated event.
    pub fn commit_message_generated(message: String, attempt: u32) -> Self {
        Self::Commit(CommitEvent::MessageGenerated { message, attempt })
    }

    /// Create a CommitMessageValidationFailed event.
    pub fn commit_message_validation_failed(reason: String, attempt: u32) -> Self {
        Self::Commit(CommitEvent::MessageValidationFailed { reason, attempt })
    }

    /// Create a CommitCreated event.
    pub fn commit_created(hash: String, message: String) -> Self {
        Self::Commit(CommitEvent::Created { hash, message })
    }

    /// Create a CommitGenerationFailed event.
    pub fn commit_generation_failed(reason: String) -> Self {
        Self::Commit(CommitEvent::GenerationFailed { reason })
    }

    /// Create a CommitSkipped event.
    pub fn commit_skipped(reason: String) -> Self {
        Self::Commit(CommitEvent::Skipped { reason })
    }

    // Miscellaneous constructors
    /// Create a ContextCleaned event.
    pub fn context_cleaned() -> Self {
        Self::ContextCleaned
    }

    /// Create a CheckpointSaved event.
    pub fn checkpoint_saved(trigger: CheckpointTrigger) -> Self {
        Self::CheckpointSaved { trigger }
    }

    /// Create a FinalizingStarted event.
    pub fn finalizing_started() -> Self {
        Self::FinalizingStarted
    }

    /// Create a PromptPermissionsRestored event.
    pub fn prompt_permissions_restored() -> Self {
        Self::PromptPermissionsRestored
    }
}
