// NOTE: Review constructors split from constructors.rs

impl PipelineEvent {
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
    pub fn review_issues_xml_missing(
        pass: u32,
        attempt: u32,
        error_detail: Option<String>,
    ) -> Self {
        Self::Review(ReviewEvent::IssuesXmlMissing {
            pass,
            attempt,
            error_detail,
        })
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

    pub fn fix_result_xml_missing(pass: u32, attempt: u32, error_detail: Option<String>) -> Self {
        Self::Review(ReviewEvent::FixResultXmlMissing {
            pass,
            attempt,
            error_detail,
        })
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
    pub fn review_output_validation_failed(
        pass: u32,
        attempt: u32,
        error_detail: Option<String>,
    ) -> Self {
        Self::Review(ReviewEvent::OutputValidationFailed {
            pass,
            attempt,
            error_detail,
        })
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
    pub fn fix_output_validation_failed(
        pass: u32,
        attempt: u32,
        error_detail: Option<String>,
    ) -> Self {
        Self::Review(ReviewEvent::FixOutputValidationFailed {
            pass,
            attempt,
            error_detail,
        })
    }
}
