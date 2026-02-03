// NOTE: Rebase, Commit, and miscellaneous constructors split from constructors.rs

impl PipelineEvent {
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
    pub fn commit_diff_prepared(empty: bool, content_id_sha256: String) -> Self {
        Self::Commit(CommitEvent::DiffPrepared {
            empty,
            content_id_sha256,
        })
    }

    /// Create a CommitDiffFailed event.
    pub fn commit_diff_failed(error: String) -> Self {
        Self::Commit(CommitEvent::DiffFailed { error })
    }

    pub fn commit_diff_invalidated(reason: String) -> Self {
        Self::Commit(CommitEvent::DiffInvalidated { reason })
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
